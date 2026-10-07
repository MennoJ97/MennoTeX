-----------------------------------------------------------------------
--         FILE:  luaotfload-main.lua (MennoTeX overlay, written by mtx)
--  DESCRIPTION:  luaotfload's entry point, plus on-demand fonts by name
-----------------------------------------------------------------------
-- The LaTeX kernel loads luaotfload with require('luaotfload-main'), and
-- luaotfload's own luaotfload-main.lua is just `return require'luaotfload'`.
-- This copy, in a tree searched before texmf-dist (TEXMFAUXTREES), does
-- the same and then changes one thing: a `name:` request (fontspec's
-- \setmainfont{TeX Gyre Pagella}) that misses the names database is first
-- resolved without luaotfload's once-per-run database reload. That pass
-- ends in a kpathsea font-metric probe, where MennoTeX's kpathsea installs
-- the package providing the font. The second pass reloads the database,
-- which then finds the new font, so the first run works.

local luaotfload_module = require'luaotfload'

local main = luaotfload.main
luaotfload.main = function (...)
  main(...)
  local resolvers = luaotfload.resolvers
  local db = config and config.luaotfload and config.luaotfload.db
  local by_name = resolvers and resolvers.name
  if not (by_name and db) then return end
  resolvers.name = function (specification)
    local live = db.update_live
    db.update_live = false
    local ok, file, sub = pcall(by_name, specification)
    db.update_live = live
    if ok and file then return file, sub end
    return by_name(specification)
  end
  -- fonts.definers.resolvers wraps luaotfload.resolvers lazily; drop a
  -- wrapper made before this point.
  local defined = fonts and fonts.definers and fonts.definers.resolvers
  if defined then rawset(defined, "name", nil) end
end

return luaotfload_module
