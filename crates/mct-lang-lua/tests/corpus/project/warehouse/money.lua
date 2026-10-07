--- Money for the warehouse gateway: integer minor units plus an explicit
-- currency (ADR-005). Amounts are immutable tables with a shared metatable,
-- so `a + b`, `a == b`, `a < b` and `tostring(a)` work as expected and mixing
-- currencies raises an error.
--
-- @module warehouse.money

local M = {}

local floor = math.floor
local format = string.format
local setmetatable = setmetatable

--- Minor units per currency.
M.SCALE = {
  EUR = 2,
  USD = 2,
  GBP = 2,
  JPY = 0,
}

M.SYMBOLS = {
  EUR = "€",
  USD = "$",
  GBP = "£",
  JPY = "¥",
}

local Money = {}
Money.__index = Money

--- Errors raised by this module carry a `kind` so callers can match them.
local function raise(kind, message, level)
  error({ kind = kind, message = message }, (level or 1) + 1)
end

local function is_money(value)
  return getmetatable(value) == Money
end

local function require_currency(code)
  if M.SCALE[code] == nil then
    raise("unsupported_currency", "unsupported currency: " .. tostring(code), 3)
  end
  return code
end

local function require_same(a, b)
  if not is_money(a) or not is_money(b) then
    raise("not_money", "both operands must be money", 3)
  end
  if a.currency ~= b.currency then
    raise("currency_mismatch", format("currency mismatch: %s vs %s", a.currency, b.currency), 3)
  end
end

--- Banker's rounding to the nearest integer.
function M.round_half_even(x)
  local f = floor(x)
  local diff = x - f
  if diff > 0.5 then
    return f + 1
  elseif diff < 0.5 then
    return f
  end
  if f % 2 == 0 then
    return f
  end
  return f + 1
end

-- ---------------------------------------------------------------------------
-- Construction
-- ---------------------------------------------------------------------------

--- Builds an amount from minor units (cents).
function M.of_minor(minor, currency)
  require_currency(currency)
  if minor ~= floor(minor) then
    raise("not_integer", "minor units must be an integer: " .. tostring(minor), 2)
  end
  return setmetatable({ minor = minor, currency = currency }, Money)
end

--- Builds an amount from a decimal number or string.
function M.of(amount, currency)
  require_currency(currency)
  local value = tonumber(amount)
  if value == nil then
    raise("bad_amount", "not a number: " .. tostring(amount), 2)
  end
  return M.of_minor(M.round_half_even(value * 10 ^ M.SCALE[currency]), currency)
end

function M.zero(currency)
  return M.of_minor(0, currency)
end

--- Parses "24.90 EUR" or "EUR 24.90".
function M.parse(text)
  local a, b = text:match("^%s*(%S+)%s+(%S+)%s*$")
  if not a then
    raise("bad_format", "expected an amount and a currency: " .. text, 2)
  end
  if a:match("^%u%u%u$") then
    return M.of(b, a)
  end
  return M.of(a, b)
end

--- Decodes the API's `{ amount = "24.90", currency = "EUR" }` shape.
function M.from_json(json)
  return M.of(json.amount, json.currency)
end

-- ---------------------------------------------------------------------------
-- Methods
-- ---------------------------------------------------------------------------

function Money:amount()
  return self.minor / 10 ^ M.SCALE[self.currency]
end

function Money:is_zero()
  return self.minor == 0
end

function Money:is_negative()
  return self.minor < 0
end

function Money:plus(other)
  require_same(self, other)
  return M.of_minor(self.minor + other.minor, self.currency)
end

function Money:minus(other)
  require_same(self, other)
  return M.of_minor(self.minor - other.minor, self.currency)
end

function Money:times(factor)
  return M.of_minor(M.round_half_even(self.minor * factor), self.currency)
end

function Money:percent(rate)
  return self:times(rate / 100)
end

function Money:negate()
  return M.of_minor(-self.minor, self.currency)
end

--- Splits into `parts` shares that add up exactly.
function Money:split(parts)
  assert(parts > 0 and parts == floor(parts), "parts must be a positive integer")
  local share = floor(self.minor / parts)
  local remainder = self.minor - share * parts
  local shares = {}
  for i = 1, parts do
    local extra = 0
    if i <= remainder then
      extra = 1
    end
    shares[i] = M.of_minor(share + extra, self.currency)
  end
  return shares
end

--- Splits proportionally to the given weights.
function Money:allocate(weights)
  local total = 0
  for _, w in ipairs(weights) do
    assert(w >= 0, "negative weight")
    total = total + w
  end
  assert(total > 0, "weights add up to zero")
  local shares, given = {}, 0
  for i, w in ipairs(weights) do
    shares[i] = floor(self.minor * w / total)
    given = given + shares[i]
  end
  local i = 1
  while given < self.minor do
    shares[i] = shares[i] + 1
    given = given + 1
    i = i % #shares + 1
  end
  for k, minor in ipairs(shares) do
    shares[k] = M.of_minor(minor, self.currency)
  end
  return shares
end

function Money:max(other)
  if self < other then
    return other
  end
  return self
end

function Money:format(with_symbol)
  local scale = M.SCALE[self.currency]
  local text = format("%." .. scale .. "f", self:amount())
  if with_symbol then
    return text .. " " .. M.SYMBOLS[self.currency]
  end
  return text .. " " .. self.currency
end

function Money:to_json()
  return { amount = format("%." .. M.SCALE[self.currency] .. "f", self:amount()), currency = self.currency }
end

-- ---------------------------------------------------------------------------
-- Metamethods
-- ---------------------------------------------------------------------------

Money.__add = function(a, b)
  return a:plus(b)
end

Money.__sub = function(a, b)
  return a:minus(b)
end

Money.__unm = function(a)
  return a:negate()
end

Money.__mul = function(a, b)
  if is_money(a) then
    return a:times(b)
  end
  return b:times(a)
end

Money.__eq = function(a, b)
  return a.currency == b.currency and a.minor == b.minor
end

Money.__lt = function(a, b)
  require_same(a, b)
  return a.minor < b.minor
end

Money.__le = function(a, b)
  require_same(a, b)
  return a.minor <= b.minor
end

Money.__tostring = function(a)
  return a:format(false)
end

-- ---------------------------------------------------------------------------
-- Collections
-- ---------------------------------------------------------------------------

--- Sums a list of amounts of one currency.
function M.sum(amounts, currency)
  local total = M.zero(currency)
  for _, amount in ipairs(amounts) do
    total = total + amount
  end
  return total
end

--- The VAT of a net amount at a percentage rate.
function M.vat(net, rate_percent)
  return net:percent(rate_percent)
end

function M.gross(net, rate_percent)
  return net + M.vat(net, rate_percent)
end

--- Distributes a discount over subtotals, proportionally.
function M.distribute_discount(subtotals, discount)
  if #subtotals == 0 then
    return {}
  end
  local weights = {}
  for i, subtotal in ipairs(subtotals) do
    weights[i] = subtotal.minor
  end
  return discount:allocate(weights)
end

--- Converts between currencies with a rates table quoted per euro.
function M.convert(amount, to, rates_per_euro)
  if amount.currency == to then
    return amount
  end
  local from_rate = rates_per_euro[amount.currency]
  local to_rate = rates_per_euro[to]
  if not from_rate or not to_rate then
    raise("unknown_rate", format("no rate %s->%s", amount.currency, to), 2)
  end
  return M.of(amount:amount() * to_rate / from_rate, to)
end

--- Wraps a function so money errors come back as `nil, err` instead of
-- raising — the style the OpenResty handlers prefer.
function M.protect(fn)
  return function(...)
    local ok, result = pcall(fn, ...)
    if ok then
      return result
    end
    if type(result) == "table" and result.kind then
      return nil, result
    end
    error(result, 0)
  end
end

M.Money = Money
M.is_money = is_money

return M
