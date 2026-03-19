-- quine/wrap.lua — Wrap selections or functions as named regions.
--
-- :QuineWrap  (visual)  — wraps selected lines in @region/@endregion
-- :QuineTag   (normal)  — wraps function under cursor (treesitter)

local M = {}

local COMMENT_STYLES = {
  python = "#", ruby = "#", sh = "#", bash = "#",
  yaml = "#", toml = "#", r = "#",
  cpp = "//", c = "//", java = "//", javascript = "//",
  typescript = "//", rust = "//", go = "//", swift = "//",
  supercollider = "//",
  lua = "--", sql = "--", haskell = "--",
}

local function comment_prefix(ft)
  return COMMENT_STYLES[ft] or "//"
end

local function indent_of(line)
  return line:match("^(%s*)") or ""
end

local function insert_region_markers(start_row, end_row, name)
  local ft     = vim.bo.filetype
  local prefix = comment_prefix(ft)
  local lines  = vim.api.nvim_buf_get_lines(0, start_row - 1, end_row, false)
  local indent = indent_of(lines[1] or "")

  local start_marker = indent .. prefix .. " @region " .. name
  local end_marker   = indent .. prefix .. " @endregion"

  -- Insert end marker after selection, start marker before
  vim.api.nvim_buf_set_lines(0, end_row,     end_row,     false, { end_marker })
  vim.api.nvim_buf_set_lines(0, start_row-1, start_row-1, false, { start_marker })

  -- Optionally offer to create directive in life.md
  M._offer_directive(name)
end

function M._offer_directive(name)
  vim.ui.input(
    { prompt = "Add {@region: " .. name .. "} to life.md? [y/n] " },
    function(input)
      if not input or input:lower():sub(1, 1) ~= "y" then return end

      local db      = require("quine.db")
      local watches = db.all_watches()
      if not watches or #watches == 0 then return end

      -- Find life.md in first watch (or default watch)
      local life_md
      for _, w in ipairs(watches) do
        local candidate = w.path .. "/life.md"
        if vim.fn.filereadable(candidate) == 1 then
          life_md = candidate
          break
        end
      end
      if not life_md then
        life_md = watches[1].path .. "/life.md"
      end

      -- Open life.md in a split and append the directive
      vim.cmd("split " .. vim.fn.fnameescape(life_md))
      local buf   = vim.api.nvim_get_current_buf()
      local count = vim.api.nvim_buf_line_count(buf)
      vim.api.nvim_buf_set_lines(buf, count, count, false, {
        "",
        "{@region: " .. name .. "}",
        "",
      })
      vim.api.nvim_win_set_cursor(0, { count + 2, 0 })
    end
  )
end

function M.wrap_selection()
  -- Get visual selection range
  local start_row = vim.fn.line("'<")
  local end_row   = vim.fn.line("'>")

  vim.ui.input({ prompt = "Region name: " }, function(name)
    if not name or name == "" then return end
    name = name:gsub("%s+", "_"):lower()
    insert_region_markers(start_row, end_row, name)
  end)
end

function M.tag_function()
  -- Use treesitter to find function bounds
  local ok, ts = pcall(require, "nvim-treesitter.ts_utils")
  if not ok then
    vim.notify("Quine: nvim-treesitter required for :QuineTag", vim.log.levels.ERROR)
    return
  end

  local node = ts.get_node_at_cursor()
  while node do
    local t = node:type()
    if t:find("function") or t:find("method") or t == "block" then
      break
    end
    node = node:parent()
  end

  if not node then
    vim.notify("Quine: no function found at cursor", vim.log.levels.WARN)
    return
  end

  local sr, _, er, _ = node:range()  -- 0-indexed
  local start_row    = sr + 1
  local end_row      = er + 1

  -- Suggest the function name as default
  local name_node = node:child_by_field_name("name")
  local default   = name_node and vim.treesitter.get_node_text(name_node, 0) or ""

  vim.ui.input(
    { prompt = "Region name: ", default = default },
    function(name)
      if not name or name == "" then return end
      name = name:gsub("%s+", "_"):lower()
      insert_region_markers(start_row, end_row, name)
    end
  )
end

return M
