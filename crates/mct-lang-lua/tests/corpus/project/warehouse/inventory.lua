--- A cache of stock levels kept in an OpenResty shared dictionary, so the
-- gateway can answer `GET /stock` without hitting the service, plus the
-- pre-checks done before forwarding a place-order request.
--
-- @module warehouse.inventory

local order_mod = require("warehouse.order")
local cjson = require("cjson.safe")

local M = {}

local DEFAULT_TTL = 15

local Cache = {}
Cache.__index = Cache

-- ---------------------------------------------------------------------------
-- Locations
-- ---------------------------------------------------------------------------

--- Parses "A-03-2" into { zone = "A", aisle = 3, level = 2 }.
function M.parse_location(code)
  local zone, aisle, level = code:match("^(%u+)-(%d+)-(%d+)$")
  if not zone then
    zone, aisle = code:match("^(Z%-BULK)-(%d+)$")
    if not zone then
      return nil, "bad location: " .. code
    end
    return { zone = zone, aisle = tonumber(aisle), level = 0 }
  end
  return { zone = zone, aisle = tonumber(aisle), level = tonumber(level) }
end

function M.format_location(loc)
  if loc.level == 0 then
    return string.format("%s-%d", loc.zone, loc.aisle)
  end
  return string.format("%s-%02d-%d", loc.zone, loc.aisle, loc.level)
end

local function compare_locations(a, b)
  if a.zone ~= b.zone then
    return a.zone < b.zone
  end
  if a.aisle ~= b.aisle then
    return a.aisle < b.aisle
  end
  if a.aisle % 2 == 0 then
    return a.level > b.level
  end
  return a.level < b.level
end

--- Sorts locations into a walking route.
function M.picking_route(locations)
  local route = { unpack(locations) }
  table.sort(route, compare_locations)
  return route
end

-- ---------------------------------------------------------------------------
-- Cache
-- ---------------------------------------------------------------------------

--- Creates a cache over a shared dict and a loader `fn(sku) -> levels, err`.
function M.new_cache(dict, loader, ttl)
  return setmetatable({ dict = dict, loader = loader, ttl = ttl or DEFAULT_TTL, hits = 0, misses = 0 }, Cache)
end

function Cache:key(sku)
  return "stock:" .. sku
end

function Cache:get(sku)
  local raw = self.dict:get(self:key(sku))
  if raw then
    self.hits = self.hits + 1
    return cjson.decode(raw)
  end
  self.misses = self.misses + 1
  local levels, err = self.loader(sku)
  if not levels then
    return nil, err
  end
  local ok, set_err = self.dict:set(self:key(sku), cjson.encode(levels), self.ttl)
  if not ok then
    ngx.log(ngx.WARN, "stock cache set failed: ", set_err)
  end
  return levels
end

function Cache:invalidate(sku)
  if sku then
    self.dict:delete(self:key(sku))
  else
    self.dict:flush_all()
  end
end

function Cache:available(sku)
  local levels, err = self:get(sku)
  if not levels then
    return nil, err
  end
  local total = 0
  for _, level in ipairs(levels) do
    total = total + (level.on_hand - level.reserved)
  end
  return total
end

function Cache:stats()
  local total = self.hits + self.misses
  local ratio = 0
  if total > 0 then
    ratio = self.hits / total
  end
  return { hits = self.hits, misses = self.misses, hit_ratio = ratio }
end

-- ---------------------------------------------------------------------------
-- Pre-checks
-- ---------------------------------------------------------------------------

--- Checks every line of an order against cached stock. Returns a list of
-- shortfalls (empty when the order can probably be served).
function M.check_order(cache, order)
  local shortfalls = {}
  for _, line in ipairs(order.lines) do
    local free, err = cache:available(line.sku)
    if not free then
      ngx.log(ngx.ERR, "stock lookup failed for ", line.sku, ": ", err)
    elseif free < line.quantity then
      shortfalls[#shortfalls + 1] = { sku = line.sku, requested = line.quantity, available = free }
    end
  end
  return shortfalls
end

--- Formats shortfalls as a problem-details body for a 409 response.
function M.out_of_stock_problem(shortfalls, order_number)
  local parts = {}
  for i, s in ipairs(shortfalls) do
    parts[i] = string.format("%s has %d available, %d requested", s.sku, s.available, s.requested)
  end
  return {
    type = "https://warehouse.example.com/problems/out-of-stock",
    title = "Out of stock",
    status = 409,
    detail = table.concat(parts, "; "),
    instance = "/api/v1/orders/" .. (order_number or ""),
  }
end

-- ---------------------------------------------------------------------------
-- Background refresh
-- ---------------------------------------------------------------------------

--- Refreshes the hottest SKUs in the background with a timer, so the first
-- request after expiry does not pay the round trip. Uses a coroutine per
-- batch to yield between SKUs.
function M.start_refresher(cache, hot_skus, interval)
  interval = interval or 10
  local refresh_batch = coroutine.wrap(function()
    while true do
      for _, sku in ipairs(hot_skus) do
        cache:invalidate(sku)
        cache:get(sku)
        coroutine.yield(sku)
      end
    end
  end)
  local function tick(premature)
    if premature then
      return
    end
    for _ = 1, #hot_skus do
      refresh_batch()
    end
  end
  local ok, err = ngx.timer.every(interval, tick)
  if not ok then
    return nil, "cannot start refresher: " .. tostring(err)
  end
  return true
end

-- ---------------------------------------------------------------------------
-- Cycle counts
-- ---------------------------------------------------------------------------

function M.difference(count)
  return count.counted - count.expected
end

function M.describe_count(count)
  local where = count.sku .. " at " .. M.format_location(count.location)
  local diff = M.difference(count)
  if diff == 0 then
    return where .. ": ok"
  elseif diff > 0 then
    return where .. ": " .. diff .. " over"
  end
  return where .. ": " .. -diff .. " short (" .. (count.reason or "no reason") .. ")"
end

--- Splits counts into those that need a supervisor and the rest.
function M.partition_for_review(counts, tolerance)
  tolerance = tolerance or 2
  local review, fine = {}, {}
  for _, count in ipairs(counts) do
    local diff = M.difference(count)
    if math.abs(diff) > tolerance or (diff ~= 0 and not count.reason) then
      review[#review + 1] = count
    else
      fine[#fine + 1] = count
    end
  end
  return review, fine
end

-- ---------------------------------------------------------------------------
-- Reservations seen by the gateway
-- ---------------------------------------------------------------------------

--- Reservations of an order that expire within `within` seconds.
function M.expiring_soon(reservations, within, now)
  now = now or ngx.now()
  local soon = {}
  for _, r in ipairs(reservations) do
    if r.expires_at - now < within then
      soon[#soon + 1] = r
    end
  end
  return soon
end

--- True when an order has waited for payment past its reservation expiry.
function M.payment_overdue(order, placed_at, now, ttl)
  ttl = ttl or 30 * 60
  return order.state == order_mod.STATES.RESERVED and (now - placed_at) > ttl
end

--- Builds pick tasks for a wave of orders, grouped by zone.
function M.pick_tasks(orders, levels_by_sku)
  local by_zone = {}
  for _, order in ipairs(orders) do
    for _, line in ipairs(order.lines) do
      local best
      for _, level in ipairs(levels_by_sku[line.sku] or {}) do
        local free = level.on_hand - level.reserved
        if free >= line.quantity and (not best or free > best.free) then
          best = { location = level.location, free = free }
        end
      end
      if best then
        local loc = M.parse_location(best.location)
        local zone = loc and loc.zone or "?"
        by_zone[zone] = by_zone[zone] or {}
        table.insert(by_zone[zone], { order = order.number, sku = line.sku, location = loc, quantity = line.quantity })
      end
    end
  end
  return by_zone
end

--- Validates a stock transfer request before it is forwarded: both
-- locations must parse, differ, and the source must hold enough free stock.
function M.validate_transfer(cache, transfer)
  local from, from_err = M.parse_location(transfer.from or "")
  if not from then
    return nil, from_err
  end
  local to, to_err = M.parse_location(transfer.to or "")
  if not to then
    return nil, to_err
  end
  if M.format_location(from) == M.format_location(to) then
    return nil, "transfer to the same location"
  end
  local quantity = tonumber(transfer.quantity)
  if not quantity or quantity <= 0 then
    return nil, "quantity must be positive"
  end
  local levels, err = cache:get(transfer.sku)
  if not levels then
    return nil, err
  end
  for _, level in ipairs(levels) do
    if level.location == transfer.from then
      if level.on_hand - level.reserved < quantity then
        return nil, "only " .. (level.on_hand - level.reserved) .. " free at " .. transfer.from
      end
      return { sku = transfer.sku, from = from, to = to, quantity = quantity, crosses_zones = from.zone ~= to.zone }
    end
  end
  return nil, "no stock of " .. transfer.sku .. " at " .. transfer.from
end

M.Cache = Cache

return M
