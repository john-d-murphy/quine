-- quine/regions.lua — Parse @region markers in the current buffer.
--
-- Standalone Lua implementation; does not call into Python.
-- Used by split.lua and jump.lua for cursor-position awareness.

local M = {}

local COMMENT_PREFIXES = {
  python = "#", ruby = "#", sh = "#", bash = "#",
  yaml = "#", toml = "#", r = "#",
  cpp = "//", c = "//", java = "//", javascript = "//",
  typescript = "//", rust = "//", go = "//", swift = "//",
  supercollider = "//",
  lua = "--", sql = "--", haskell = "--",
}

local function prefix(ft)
  return COMMENT_PREFIXES[ft] or "//"
end

local function escape_prefix(p)
  return p:gsub("[/%-]", "%%%1")
end

-- Return the name of the @region the cursor is inside, or nil.
function M.region_name_at_cursor(buf, row)
  buf = buf or vim.api.nvim_get_current_buf()
  row = row or vim.api.nvim_win_get_cursor(0)[1]  -- 1-indexed

  local ft    = vim.bo[buf].filetype
  local pre   = escape_prefix(prefix(ft))
  local start_pat = "^%s*" .. pre .. "%s*@region%s+(%S+)"
  local end_pat   = "@endregion"

  local lines = vim.api.nvim_buf_get_lines(buf, 0, row, false)

  -- Walk backward from cursor
  for i = #lines, 1, -1 do
    local name = lines[i]:match(start_pat)
    if name then return name end
    if lines[i]:match(end_pat) then return nil end
  end
  return nil
end

-- Return all regions in a buffer as a list of {name, start_row, end_row}.
-- Rows are 1-indexed.
function M.all_regions(buf)
  buf = buf or vim.api.nvim_get_current_buf()

  local ft      = vim.bo[buf].filetype
  local pre     = escape_prefix(prefix(ft))
  local start_pat = "^%s*" .. pre .. "%s*@region%s+(%S+)"
  local end_pat   = "^%s*" .. pre .. "%s*@endregion"

  local lines   = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  local regions = {}
  local stack   = {}

  for i, line in ipairs(lines) do
    local name = line:match(start_pat)
    if name then
      table.insert(stack, { name = name, start_row = i })
    elseif line:match(end_pat) and #stack > 0 then
      local r = table.remove(stack)
      table.insert(regions, { name = r.name, start_row = r.start_row, end_row = i })
    end
  end

  return regions
end

-- List all directives in a markdown buffer.
-- Returns list of {name, row}.
function M.all_directives(buf)
  buf = buf or vim.api.nvim_get_current_buf()
  local lines  = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  local result = {}
  for i, line in ipairs(lines) do
    for name in line:gmatch("{@region:%s*([^}]+)}") do
      name = name:gsub("^%s+", ""):gsub("%s+$", "")
      table.insert(result, { name = name, row = i })
    end
  end
  return result
end

return M
