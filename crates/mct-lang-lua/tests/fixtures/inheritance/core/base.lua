local Base = {}
Base.__index = Base

function Base.new(name)
  return setmetatable({ name = name }, Base)
end

function Base:describe()
  return self.name
end

return Base
