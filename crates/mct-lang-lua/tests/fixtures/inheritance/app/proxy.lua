local Base = require("core.base")

local Proxy = {}

setmetatable(Proxy, {
  __index = function(_, key)
    return Base[key]
  end,
})

return Proxy
