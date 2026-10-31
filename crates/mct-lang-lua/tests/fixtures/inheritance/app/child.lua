local Base = require("core.base")

local Child = setmetatable({}, { __index = Base })
Child.__index = Child

function Child.new(name)
  local self = Base.new(name)
  return setmetatable(self, Child)
end

return Child
