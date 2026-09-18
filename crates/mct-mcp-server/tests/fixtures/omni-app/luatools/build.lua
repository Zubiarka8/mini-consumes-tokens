-- Lua is implemented as a crate (`mct-lang-lua`) but deliberately NOT
-- registered in the production registry; this file exists to pin that.
local M = {}

function M.build(target)
  return target
end

return M
