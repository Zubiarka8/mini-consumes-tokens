--- Token-bucket rate limiting for the warehouse API gateway, per client id,
-- backed by an OpenResty shared dictionary so every worker sees the same
-- buckets. Limits follow docs/api/reference.md: 600 reads and 120 writes per
-- minute, higher for partners.
--
-- @module gateway.ratelimit

local cjson = require("cjson.safe")
local lock_mod = require("resty.lock")

local _M = { _VERSION = "1.4.0" }

local DEFAULT_LIMITS = {
  read = { capacity = 600, per_second = 10 },
  write = { capacity = 120, per_second = 2 },
}

local PARTNER_LIMITS = {
  ["partner-acme"] = { read = { capacity = 3000, per_second = 50 }, write = { capacity = 600, per_second = 10 } },
  ["partner-globex"] = { read = { capacity = 1800, per_second = 30 }, write = { capacity = 300, per_second = 5 } },
}

local Limiter = {}
Limiter.__index = Limiter

-- ---------------------------------------------------------------------------
-- Helpers
-- ---------------------------------------------------------------------------

local function classify(method)
  if method == "GET" or method == "HEAD" or method == "OPTIONS" then
    return "read"
  end
  return "write"
end

local function limits_for(client_id, class)
  local partner = PARTNER_LIMITS[client_id]
  if partner and partner[class] then
    return partner[class]
  end
  return DEFAULT_LIMITS[class]
end

local function bucket_key(client_id, class)
  return "rl:" .. class .. ":" .. client_id
end

local function decode_bucket(raw, capacity, now)
  if not raw then
    return { tokens = capacity, updated = now }
  end
  local bucket = cjson.decode(raw)
  if not bucket then
    return { tokens = capacity, updated = now }
  end
  return bucket
end

local refill = function(bucket, limits, now)
  local elapsed = math.max(0, now - bucket.updated)
  bucket.tokens = math.min(limits.capacity, bucket.tokens + elapsed * limits.per_second)
  bucket.updated = now
  return bucket
end

-- ---------------------------------------------------------------------------
-- Limiter
-- ---------------------------------------------------------------------------

--- Creates a limiter over the shared dict `dict_name`.
function _M.new(dict_name, opts)
  opts = opts or {}
  local dict = ngx.shared[dict_name]
  if not dict then
    return nil, "no shared dict " .. dict_name
  end
  return setmetatable({
    dict = dict,
    lock_dict = opts.lock_dict or "locks",
    clock = opts.clock or ngx.now,
    rejected = 0,
    allowed = 0,
  }, Limiter)
end

--- Takes one token for `client_id`. Returns true, or false and the number of
-- seconds after which a retry may succeed.
function Limiter:take(client_id, method)
  local class = classify(method)
  local limits = limits_for(client_id, class)
  local key = bucket_key(client_id, class)
  local lock, lock_err = lock_mod:new(self.lock_dict, { timeout = 0.05 })
  if not lock then
    return nil, "cannot create lock: " .. tostring(lock_err)
  end
  local elapsed, err = lock:lock(key)
  if not elapsed then
    -- Fail open: a lock timeout must not turn into an outage.
    ngx.log(ngx.WARN, "rate limit lock failed, allowing: ", err)
    return true
  end
  local now = self.clock()
  local bucket = refill(decode_bucket(self.dict:get(key), limits.capacity, now), limits, now)
  local allowed = bucket.tokens >= 1
  if allowed then
    bucket.tokens = bucket.tokens - 1
    self.allowed = self.allowed + 1
  else
    self.rejected = self.rejected + 1
  end
  self.dict:set(key, cjson.encode(bucket), 3600)
  lock:unlock()
  if allowed then
    return true
  end
  local retry_after = math.ceil((1 - bucket.tokens) / limits.per_second)
  return false, retry_after
end

--- Remaining tokens for a client, for the `X-RateLimit-Remaining` header.
function Limiter:remaining(client_id, method)
  local class = classify(method)
  local limits = limits_for(client_id, class)
  local now = self.clock()
  local bucket = refill(decode_bucket(self.dict:get(bucket_key(client_id, class)), limits.capacity, now), limits, now)
  return math.floor(bucket.tokens), limits.capacity
end

function Limiter:reset(client_id)
  for class in pairs(DEFAULT_LIMITS) do
    self.dict:delete(bucket_key(client_id, class))
  end
end

function Limiter:stats()
  return { allowed = self.allowed, rejected = self.rejected }
end

-- ---------------------------------------------------------------------------
-- Request phase
-- ---------------------------------------------------------------------------

--- Extracts the client id from the bearer token's `azp` claim, already
-- validated by the auth phase and stored in `ngx.ctx`.
local function client_id_of(ctx)
  if ctx.claims and ctx.claims.azp then
    return ctx.claims.azp
  end
  return ngx.var.remote_addr
end

--- Writes the problem-details body of a 429 response.
local function reject(retry_after)
  ngx.status = 429
  ngx.header["Content-Type"] = "application/problem+json"
  ngx.header["Retry-After"] = tostring(retry_after)
  ngx.say(cjson.encode({
    type = "https://warehouse.example.com/problems/rate-limited",
    title = "Too many requests",
    status = 429,
    detail = "retry after " .. retry_after .. " second(s)",
  }))
  return ngx.exit(429)
end

--- The access-phase handler: `access_by_lua_block { ratelimit.access(lim) }`.
function _M.access(limiter)
  local ctx = ngx.ctx
  local client_id = client_id_of(ctx)
  local method = ngx.req.get_method()
  local ok, retry_after = limiter:take(client_id, method)
  if ok == nil then
    ngx.log(ngx.ERR, "rate limiter error: ", retry_after)
    return
  end
  local remaining, capacity = limiter:remaining(client_id, method)
  ngx.header["X-RateLimit-Limit"] = capacity
  ngx.header["X-RateLimit-Remaining"] = remaining
  if not ok then
    return reject(retry_after)
  end
end

--- Returns the effective limits table, for the admin endpoint.
function _M.describe(client_id)
  local out = {}
  for class in pairs(DEFAULT_LIMITS) do
    local limits = limits_for(client_id, class)
    out[class] = { per_minute = limits.per_second * 60, burst = limits.capacity }
  end
  return out
end

-- ---------------------------------------------------------------------------
-- Concurrency limiting
-- ---------------------------------------------------------------------------

--- Caps in-flight requests per client, independently of the token bucket:
-- a slow client cannot hold every upstream connection. `enter` in the
-- access phase, `leave` in the log phase.
local Concurrency = {}
Concurrency.__index = Concurrency

function _M.new_concurrency(dict_name, max_in_flight)
  local dict = ngx.shared[dict_name]
  if not dict then
    return nil, "no shared dict " .. dict_name
  end
  return setmetatable({ dict = dict, max = max_in_flight or 20 }, Concurrency)
end

local function inflight_key(client_id)
  return "inflight:" .. client_id
end

function Concurrency:enter(client_id)
  local key = inflight_key(client_id)
  local count, err = self.dict:incr(key, 1, 0, 60)
  if not count then
    ngx.log(ngx.WARN, "concurrency counter failed, allowing: ", err)
    return true
  end
  if count > self.max then
    self.dict:incr(key, -1)
    return false, count - 1
  end
  ngx.ctx.inflight_client = client_id
  return true, count
end

function Concurrency:leave()
  local client_id = ngx.ctx.inflight_client
  if not client_id then
    return
  end
  local count = self.dict:incr(inflight_key(client_id), -1)
  if count and count < 0 then
    self.dict:set(inflight_key(client_id), 0)
  end
  ngx.ctx.inflight_client = nil
end

function Concurrency:reset(client_id)
  self.dict:set(inflight_key(client_id), 0)
end

function Concurrency:in_flight(client_id)
  return self.dict:get(inflight_key(client_id)) or 0
end

--- The access-phase handler combining both limits.
function _M.access_all(limiter, concurrency)
  local ok, current = concurrency:enter(client_id_of(ngx.ctx))
  if not ok then
    ngx.log(ngx.WARN, "too many in-flight requests: ", current)
    return reject(1)
  end
  return _M.access(limiter)
end

_M.Concurrency = Concurrency

-- ---------------------------------------------------------------------------
-- Testing hooks
-- ---------------------------------------------------------------------------

--- A fake shared dict with the subset of the API the limiter uses.
function _M.fake_dict()
  local store = {}
  local dict = {}
  function dict:get(key)
    return store[key]
  end
  function dict:set(key, value)
    store[key] = value
    return true
  end
  function dict:delete(key)
    store[key] = nil
  end
  return dict
end

--- A controllable clock for tests: `clock.advance(seconds)`.
function _M.fake_clock(start)
  local now = start or 0
  local clock = setmetatable({}, {
    __call = function()
      return now
    end,
  })
  clock.advance = function(seconds)
    now = now + seconds
  end
  return clock
end

_M.classify = classify
_M.limits_for = limits_for

return _M
