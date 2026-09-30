-- Lua build helper: `mct-lang-lua` is registered in the production registry,
-- so this module's symbols and calls must reach the index.
local M = {}

function M.build(target)
  return tostring(target)
end

return M
