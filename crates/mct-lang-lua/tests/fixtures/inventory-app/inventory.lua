local Logger = require("logger")

local Inventory = {}

function Inventory.new()
  local self = { items = {} }
  return self
end

function Inventory:addItem(item)
  table.insert(self.items, item)
  Logger.log("added item")
end

function Inventory:removeItem(item)
  table.remove(self.items)
  Logger.log("removed item")
end

return Inventory
