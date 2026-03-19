-- quine/catalog.lua — Tag catalog picker.
--
-- :QuineCatalog opens a fuzzy finder over all tags in the DB.
-- Selecting a tag inserts {@region: name} (or {@region: repo/name}
-- if the name is ambiguous across repos) at the cursor position.
--
-- Uses vim.ui.select if Telescope is not available.

local M  = {}
local db = require("quine.db")

local function format_entry(tag)
  local prose = (tag.prose or ""):gsub("\n", " "):sub(1, 60)
  if #prose > 0 then
    return string.format("%-30s  %-20s  %s", tag.name, tag.source_file, prose)
  end
  return string.format("%-30s  %s", tag.name, tag.source_file)
end

local function directive_for(tag, all_tags)
  -- Count how many tags share this name
  local count = 0
  for _, t in ipairs(all_tags) do
    if t.name == tag.name then count = count + 1 end
  end
  if count > 1 then
    return string.format("{@region: %s/%s}", tag.repo_id, tag.name)
  end
  return string.format("{@region: %s}", tag.name)
end

local function insert_at_cursor(text)
  local row, col = unpack(vim.api.nvim_win_get_cursor(0))
  local line     = vim.api.nvim_get_current_line()
  local new_line = line:sub(1, col) .. text .. line:sub(col + 1)
  vim.api.nvim_set_current_line(new_line)
  vim.api.nvim_win_set_cursor(0, { row, col + #text })
end

function M.open()
  local tags, err = db.all_tags()
  if not tags then
    vim.notify("Quine: " .. (err or "DB error"), vim.log.levels.ERROR)
    return
  end
  if #tags == 0 then
    vim.notify("Quine: no tags found — run collect first", vim.log.levels.WARN)
    return
  end

  -- Try Telescope first
  local ok_tel, tel = pcall(require, "telescope.pickers")
  if ok_tel then
    M._open_telescope(tags)
    return
  end

  -- Fall back to vim.ui.select
  local entries = vim.tbl_map(format_entry, tags)
  vim.ui.select(entries, {
    prompt = "Insert {@region:}",
  }, function(choice, idx)
    if not choice or not idx then return end
    local tag       = tags[idx]
    local directive = directive_for(tag, tags)
    insert_at_cursor(directive)
  end)
end

function M._open_telescope(tags)
  local pickers    = require("telescope.pickers")
  local finders    = require("telescope.finders")
  local conf       = require("telescope.config").values
  local actions    = require("telescope.actions")
  local state      = require("telescope.actions.state")

  pickers.new({}, {
    prompt_title = "Quine · Tag Catalog",
    finder = finders.new_table({
      results = tags,
      entry_maker = function(tag)
        return {
          value   = tag,
          display = format_entry(tag),
          ordinal = tag.name .. " " .. tag.source_file .. " " .. (tag.prose or ""),
        }
      end,
    }),
    sorter = conf.generic_sorter({}),
    attach_mappings = function(prompt_bufnr, map)
      -- <CR> inserts directive
      actions.select_default:replace(function()
        actions.close(prompt_bufnr)
        local sel       = state.get_selected_entry()
        if not sel then return end
        local directive = directive_for(sel.value, tags)
        insert_at_cursor(directive)
      end)
      -- <C-y> inserts qualified form regardless of ambiguity
      map("i", "<C-y>", function()
        actions.close(prompt_bufnr)
        local sel = state.get_selected_entry()
        if not sel then return end
        local dir = string.format("{@region: %s/%s}", sel.value.repo_id, sel.value.name)
        insert_at_cursor(dir)
      end)
      return true
    end,
    previewer = false,
  }):find()
end

return M
