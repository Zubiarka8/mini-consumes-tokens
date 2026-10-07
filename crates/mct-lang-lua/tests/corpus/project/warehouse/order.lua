--- The order aggregate as the gateway sees it: enough of the state machine
-- to validate requests before forwarding them, and the event log the
-- gateway replays to the back office over server-sent events.
--
-- @module warehouse.order

local money = require("warehouse.money")
local cjson = require("cjson.safe")

local Order = {}
Order.__index = Order

local M = { Order = Order }

-- ---------------------------------------------------------------------------
-- States
-- ---------------------------------------------------------------------------

M.STATES = {
  NEW = "NEW",
  RESERVED = "RESERVED",
  PAID = "PAID",
  PICKED = "PICKED",
  SHIPPED = "SHIPPED",
  INVOICED = "INVOICED",
  CANCELLED = "CANCELLED",
}

local S = M.STATES

local NEXT = {
  [S.NEW] = { [S.RESERVED] = true, [S.CANCELLED] = true },
  [S.RESERVED] = { [S.PAID] = true, [S.CANCELLED] = true },
  [S.PAID] = { [S.PICKED] = true, [S.CANCELLED] = true },
  [S.PICKED] = { [S.SHIPPED] = true, [S.CANCELLED] = true },
  [S.SHIPPED] = { [S.INVOICED] = true },
  [S.INVOICED] = {},
  [S.CANCELLED] = {},
}

function M.can_move_to(from, to)
  local allowed = NEXT[from]
  return allowed ~= nil and allowed[to] == true
end

function M.is_terminal(state)
  return next(NEXT[state] or {}) == nil
end

-- ---------------------------------------------------------------------------
-- Lines
-- ---------------------------------------------------------------------------

--- Validates and normalises one line of a place-order request.
local function new_line(number, raw, currency)
  if type(raw.sku) ~= "string" or not raw.sku:match("^sku%-%d%d%d%d") then
    return nil, "lines[" .. number .. "].sku is not a SKU"
  end
  local quantity = tonumber(raw.quantity)
  if not quantity or quantity <= 0 or quantity ~= math.floor(quantity) then
    return nil, "lines[" .. number .. "].quantity must be a positive integer"
  end
  local price = raw.unit_price and money.from_json(raw.unit_price) or money.zero(currency)
  return { number = number, sku = raw.sku, quantity = quantity, unit_price = price }
end

local subtotal = function(line)
  return line.unit_price:times(line.quantity)
end

M.subtotal = subtotal

-- ---------------------------------------------------------------------------
-- Aggregate
-- ---------------------------------------------------------------------------

--- Creates an order from a decoded place-order request.
function M.new(request)
  local currency = request.currency or "EUR"
  local self = setmetatable({
    id = request.id,
    number = request.number,
    customer_id = request.customer_id,
    currency = currency,
    state = S.NEW,
    lines = {},
    events = {},
  }, Order)
  for i, raw in ipairs(request.lines or {}) do
    local line, err = new_line(i, raw, currency)
    if not line then
      return nil, err
    end
    self.lines[i] = line
  end
  if #self.lines == 0 then
    return nil, "an order needs at least one line"
  end
  self:record("placed", { customer_id = self.customer_id, lines = #self.lines })
  return self
end

function Order:total()
  local subtotals = {}
  for i, line in ipairs(self.lines) do
    subtotals[i] = subtotal(line)
  end
  return money.sum(subtotals, self.currency)
end

function Order:is_large(threshold)
  return self:total() > threshold
end

function Order:record(event_type, data)
  local event = { type = event_type, order = self.number, at = ngx.now(), data = data or {} }
  self.events[#self.events + 1] = event
  return event
end

function Order:move_to(target)
  if not M.can_move_to(self.state, target) then
    return nil, string.format("cannot move an order from %s to %s", self.state, target)
  end
  self.state = target
  return true
end

function Order:reserve(reservation_ids)
  local ok, err = self:move_to(S.RESERVED)
  if not ok then
    return nil, err
  end
  return self:record("reserved", { reservations = reservation_ids })
end

function Order:pay(payment_id, amount)
  if amount ~= self:total() then
    return nil, "paid " .. tostring(amount) .. " for a total of " .. tostring(self:total())
  end
  local ok, err = self:move_to(S.PAID)
  if not ok then
    return nil, err
  end
  return self:record("paid", { payment = payment_id, amount = amount:to_json() })
end

function Order:ship(carrier, tracking_number)
  local ok, err = self:move_to(S.SHIPPED)
  if not ok then
    return nil, err
  end
  self.tracking_number = tracking_number
  return self:record("shipped", { carrier = carrier, tracking = tracking_number })
end

function Order:cancel(reason)
  local ok, err = self:move_to(S.CANCELLED)
  if not ok then
    return nil, err
  end
  return self:record("cancelled", { reason = reason })
end

--- Hands the pending events to `sink` and forgets them.
function Order:drain(sink)
  for _, event in ipairs(self.events) do
    sink(event)
  end
  self.events = {}
end

function Order:to_json()
  local lines = {}
  for i, line in ipairs(self.lines) do
    lines[i] = { number = line.number, sku = line.sku, quantity = line.quantity, unit_price = line.unit_price:to_json() }
  end
  return cjson.encode({
    id = self.id,
    number = self.number,
    customer_id = self.customer_id,
    currency = self.currency,
    state = self.state,
    lines = lines,
    total = self:total():to_json(),
  })
end

-- ---------------------------------------------------------------------------
-- Events
-- ---------------------------------------------------------------------------

local describers = {
  placed = function(e)
    return "placed with " .. e.data.lines .. " line(s)"
  end,
  reserved = function(e)
    return "reserved " .. #e.data.reservations .. " reservation(s)"
  end,
  paid = function(e)
    return "paid " .. e.data.amount.amount .. " " .. e.data.amount.currency
  end,
  shipped = function(e)
    return "shipped with " .. e.data.carrier .. " (" .. e.data.tracking .. ")"
  end,
  cancelled = function(e)
    return "cancelled: " .. e.data.reason
  end,
}

--- A one-line description of an event, for logs.
function M.describe(event)
  local describer = describers[event.type]
  if describer then
    return describer(event)
  end
  return event.type
end

--- Formats an event as a server-sent events frame.
function M.sse_frame(event)
  return "event: " .. event.type .. "\n" .. "data: " .. cjson.encode(event) .. "\n\n"
end

-- ---------------------------------------------------------------------------
-- Collections
-- ---------------------------------------------------------------------------

function M.count_by_state(orders)
  local counts = {}
  for _, order in ipairs(orders) do
    counts[order.state] = (counts[order.state] or 0) + 1
  end
  return counts
end

function M.largest(orders, n)
  local sorted = { unpack(orders) }
  table.sort(sorted, function(a, b)
    return a:total() > b:total()
  end)
  local top = {}
  for i = 1, math.min(n, #sorted) do
    top[i] = sorted[i]
  end
  return top
end

--- Renders a packing slip as plain text.
function M.packing_slip(order, width)
  width = width or 48
  local function rule()
    return string.rep("-", width)
  end
  local function row(left, right)
    return left .. string.rep(" ", width - #left - #right) .. right
  end
  local out = { rule(), row("Albaran", order.number), rule() }
  for _, line in ipairs(order.lines) do
    out[#out + 1] = row(line.quantity .. " x " .. line.sku, tostring(subtotal(line)))
  end
  out[#out + 1] = rule()
  out[#out + 1] = row("Total", tostring(order:total()))
  return table.concat(out, "\n")
end

--- Order numbers: `PED-<year>-<6-digit sequence>`, the sequence kept in a
-- shared dict so every worker draws from the same counter.
function M.next_number(dict, year)
  year = year or tonumber(os.date("%Y"))
  local seq, err = dict:incr("order-seq:" .. year, 1, 0)
  if not seq then
    return nil, "cannot allocate an order number: " .. tostring(err)
  end
  return string.format("PED-%d-%06d", year, seq)
end

function M.is_order_number(text)
  return type(text) == "string" and text:match("^PED%-%d%d%d%d%-%d%d%d%d%d%d$") ~= nil
end

--- Decodes and validates a place-order body; returns the order or an error
-- table suitable for a 422 problem response.
function M.from_request_body(body, number)
  local request, decode_err = cjson.decode(body)
  if not request then
    return nil, { status = 400, title = "Malformed JSON", detail = decode_err }
  end
  if type(request.customer_id) ~= "string" or request.customer_id == "" then
    return nil, { status = 422, title = "Validation failed", detail = "customer_id is required" }
  end
  request.number = number
  local order, err = M.new(request)
  if not order then
    return nil, { status = 422, title = "Validation failed", detail = err }
  end
  return order
end

return M
