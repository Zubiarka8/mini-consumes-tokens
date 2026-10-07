--- Request routing of the warehouse API gateway: a table of routes whose
-- handlers validate and pre-check requests with the warehouse modules, then
-- proxy them to the service. Wired from nginx.conf with
-- `content_by_lua_block { require("gateway.router").handle() }`.
--
-- @module gateway.router

local cjson = require("cjson.safe")
local http = require("resty.http")
local money = require("warehouse.money")
local order = require("warehouse.order")
local inventory = require("warehouse.inventory")
local ratelimit = require("gateway.ratelimit")

local Router = {}
Router.__index = Router

local M = {}

local UPSTREAM = os.getenv("WAREHOUSE_UPSTREAM") or "http://warehouse-web:8080"
local LARGE_ORDER = money.of("5000", "EUR")

-- ---------------------------------------------------------------------------
-- Responses
-- ---------------------------------------------------------------------------

local function respond(status, body, content_type)
  ngx.status = status
  ngx.header["Content-Type"] = content_type or "application/json"
  if type(body) == "table" then
    body = cjson.encode(body)
  end
  ngx.say(body)
  return ngx.exit(status)
end

local function problem(status, title, detail)
  return respond(status, { type = "about:blank", title = title, status = status, detail = detail },
    "application/problem+json")
end

-- ---------------------------------------------------------------------------
-- Upstream
-- ---------------------------------------------------------------------------

local function proxy(method, path, body)
  local client = http.new()
  client:set_timeouts(1000, 5000, 5000)
  local res, err = client:request_uri(UPSTREAM .. path, {
    method = method,
    body = body,
    headers = {
      ["Content-Type"] = "application/json",
      ["Authorization"] = ngx.var.http_authorization,
      ["X-Request-Id"] = ngx.var.request_id,
    },
  })
  if not res then
    ngx.log(ngx.ERR, "upstream ", method, " ", path, " failed: ", err)
    return problem(502, "Bad gateway", "upstream unavailable")
  end
  return respond(res.status, res.body, res.headers["Content-Type"])
end

-- ---------------------------------------------------------------------------
-- Router
-- ---------------------------------------------------------------------------

function M.new()
  return setmetatable({ routes = {} }, Router)
end

--- Registers `handler` for `method` and a path pattern with `:name` captures.
function Router:add(method, pattern, handler)
  local names = {}
  local lua_pattern = "^" .. pattern:gsub(":(%w+)", function(name)
    names[#names + 1] = name
    return "([^/]+)"
  end) .. "$"
  self.routes[#self.routes + 1] = { method = method, pattern = lua_pattern, names = names, handler = handler }
  return self
end

function Router:match(method, path)
  for _, route in ipairs(self.routes) do
    if route.method == method then
      local captures = { path:match(route.pattern) }
      if #captures > 0 or path:match(route.pattern) then
        local params = {}
        for i, name in ipairs(route.names) do
          params[name] = captures[i]
        end
        return route.handler, params
      end
    end
  end
  return nil
end

function Router:dispatch(method, path)
  local handler, params = self:match(method, path)
  if not handler then
    return problem(404, "Not found", path)
  end
  return handler(params)
end

-- ---------------------------------------------------------------------------
-- Handlers
-- ---------------------------------------------------------------------------

local stock_cache

local function cache()
  if not stock_cache then
    stock_cache = inventory.new_cache(ngx.shared.stock, function(sku)
      local client = http.new()
      local res, err = client:request_uri(UPSTREAM .. "/api/v1/stock/" .. sku .. "/locations")
      if not res or res.status ~= 200 then
        return nil, err or ("status " .. tostring(res and res.status))
      end
      return cjson.decode(res.body)
    end)
  end
  return stock_cache
end

local handlers = {
  get_stock = function(params)
    local free, err = cache():available(params.sku)
    if not free then
      return problem(502, "Stock unavailable", err)
    end
    return respond(200, { sku = params.sku, available = free })
  end,

  place_order = function()
    ngx.req.read_body()
    local o, err = order.from_request_body(ngx.req.get_body_data() or "", ngx.var.request_id)
    if not o then
      return problem(err.status, err.title, err.detail)
    end
    local shortfalls = inventory.check_order(cache(), o)
    if #shortfalls > 0 then
      return respond(409, inventory.out_of_stock_problem(shortfalls, o.number), "application/problem+json")
    end
    if o:is_large(LARGE_ORDER) then
      ngx.log(ngx.INFO, "large order ", o.number, " will need approval")
    end
    return proxy("POST", "/api/v1/orders", o:to_json())
  end,

  get_order = function(params)
    return proxy("GET", "/api/v1/orders/" .. params.number)
  end,

  cancel_order = function(params)
    ngx.req.read_body()
    return proxy("POST", "/api/v1/orders/" .. params.number .. "/cancel", ngx.req.get_body_data())
  end,
}

--- Admin endpoint: the effective rate limits of a client.
function handlers.get_limits(params)
  return respond(200, ratelimit.describe(params.client))
end

--- CORS preflight for the order endpoints; `cors` answers it before this
-- runs for allowed origins, so reaching it means the origin was refused.
function handlers.preflight()
  return problem(403, "Origin not allowed", ngx.var.http_origin or "no origin")
end

--- Health endpoint answered by the gateway itself.
function handlers.health()
  return respond(200, { status = "ok", cache = cache():stats() })
end

-- ---------------------------------------------------------------------------
-- Middleware
-- ---------------------------------------------------------------------------

local ALLOWED_ORIGINS = {
  ["https://backoffice.example.com"] = true,
  ["https://partners.example.com"] = true,
}

--- Answers CORS preflights and decorates actual responses.
local function cors(next_handler)
  return function(params)
    local origin = ngx.var.http_origin
    if origin and ALLOWED_ORIGINS[origin] then
      ngx.header["Access-Control-Allow-Origin"] = origin
      ngx.header["Vary"] = "Origin"
      if ngx.req.get_method() == "OPTIONS" then
        ngx.header["Access-Control-Allow-Methods"] = "GET, POST, PATCH, DELETE"
        ngx.header["Access-Control-Allow-Headers"] = "Authorization, Content-Type, X-Tenant"
        ngx.header["Access-Control-Max-Age"] = "3600"
        return ngx.exit(204)
      end
    end
    return next_handler(params)
  end
end

--- Rejects bodies larger than `limit` bytes before reading them.
local function max_body(limit, next_handler)
  return function(params)
    local length = tonumber(ngx.var.http_content_length or "0") or 0
    if length > limit then
      return problem(413, "Payload too large", "at most " .. limit .. " bytes")
    end
    return next_handler(params)
  end
end

--- Requires a tenant header on every write.
local function require_tenant(next_handler)
  return function(params)
    if ngx.req.get_method() ~= "GET" and not ngx.var.http_x_tenant then
      return problem(400, "Missing tenant", "X-Tenant is required on writes")
    end
    return next_handler(params)
  end
end

--- Composes middleware right to left: `chain(a, b, h)` is `a(b(h))`.
local function chain(...)
  local fns = { ... }
  local handler = fns[#fns]
  for i = #fns - 1, 1, -1 do
    handler = fns[i](handler)
  end
  return handler
end

M.chain = chain

-- ---------------------------------------------------------------------------
-- Wiring
-- ---------------------------------------------------------------------------

local router

local function build()
  local r = M.new()
  r:add("GET", "/api/v1/stock/:sku", handlers.get_stock)
  r:add("POST", "/api/v1/orders", chain(cors, require_tenant, function(h)
    return max_body(64 * 1024, h)
  end, handlers.place_order))
  r:add("GET", "/api/v1/orders/:number", handlers.get_order)
  r:add("POST", "/api/v1/orders/:number/cancel", handlers.cancel_order)
  r:add("GET", "/admin/limits/:client", handlers.get_limits)
  r:add("OPTIONS", "/api/v1/orders", cors(handlers.preflight))
  r:add("GET", "/health", handlers.health)
  return r
end

--- The content-phase entry point.
function M.handle()
  router = router or build()
  ngx.header["X-Gateway"] = "warehouse"
  local limiter = ratelimit.new("ratelimit")
  if limiter then
    ratelimit.access(limiter)
  end
  return router:dispatch(ngx.req.get_method(), ngx.var.uri)
end

--- Logs one line per request in the log phase.
function M.log()
  local status = tonumber(ngx.var.status) or 0
  local level = ngx.INFO
  if status >= 500 then
    level = ngx.ERR
  elseif status >= 400 then
    level = ngx.WARN
  end
  ngx.log(level, ngx.var.request_method, " ", ngx.var.uri, " ", status, " ", ngx.var.request_time, "s")
end

--- Formats a quote preview for the shop's basket page; used by the edge
-- includes, not by the API.
function M.quote_preview(lines, currency)
  local subtotals = {}
  for i, line in ipairs(lines) do
    subtotals[i] = money.of(line.price, currency):times(line.quantity)
  end
  local net = money.sum(subtotals, currency)
  return {
    net = net:to_json(),
    vat = money.vat(net, 21):to_json(),
    gross = money.gross(net, 21):to_json(),
  }
end

M.Router = Router
M.handlers = handlers

return M
