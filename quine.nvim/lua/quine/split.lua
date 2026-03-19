-- quine/split.lua — Split pane: source on left, prose on right.
--
-- :QuineOpen  — open split, show life.md aligned to cursor region
-- :QuineClose — close split, save prose changes
--
-- The right pane is an editable buffer backed by life.md (or the
-- appropriate prose file for the current repo). Cursor movement in
-- the source pane scrolls the prose pane to the region the cursor
-- is inside, if one exists.

local M = {}

-- Track state
local state = {
  source_win  = nil,
  prose_win   = nil,
  prose_buf   = nil,
  prose_file  = nil,
  autocmd_id  = nil,
}

local function find_life_md()
  local db      = require("quine.db")
  local watches = db.all_watches()
  if not watches then return nil end

  -- Use the watch whose path is a prefix of the current file
  local cur = vim.api.nvim_buf_get_name(0)
  for _, w in ipairs(watches) do
    if cur:sub(1, #w.path) == w.path then
      local life = w.path .. "/life.md"
      if vim.fn.filereadable(life) == 1 then
        return life
      end
    end
  end

  -- Fall back to first watch with a life.md
  for _, w in ipairs(watches) do
    local life = w.path .. "/life.md"
    if vim.fn.filereadable(life) == 1 then
      return life
    end
  end
  return nil
end

local function region_at_cursor()
  return require("quine.jump")  -- reuse detection logic
    and require("quine.regions").region_name_at_cursor()
end

local function scroll_prose_to(name)
  if not state.prose_win or not vim.api.nvim_win_is_valid(state.prose_win) then return end

  local lines = vim.api.nvim_buf_get_lines(state.prose_buf, 0, -1, false)
  local pat   = "{@region:%s*" .. vim.pesc(name)

  for i, line in ipairs(lines) do
    if line:match(pat) then
      vim.api.nvim_win_set_cursor(state.prose_win, { i, 0 })
      vim.api.nvim_win_call(state.prose_win, function()
        vim.cmd("normal! zz")
      end)
      return
    end
  end
end

function M.open()
  if state.source_win and vim.api.nvim_win_is_valid(state.source_win) then
    vim.notify("Quine: split already open", vim.log.levels.INFO)
    return
  end

  local life_md = find_life_md()
  if not life_md then
    vim.notify("Quine: no life.md found in any watched repo", vim.log.levels.WARN)
    return
  end

  state.source_win = vim.api.nvim_get_current_win()
  state.prose_file = life_md

  -- Open life.md in a vertical split on the right
  vim.cmd("vsplit " .. vim.fn.fnameescape(life_md))
  state.prose_win = vim.api.nvim_get_current_win()
  state.prose_buf = vim.api.nvim_get_current_buf()

  -- Return focus to source
  vim.api.nvim_set_current_win(state.source_win)

  -- Autocmd: scroll prose when cursor moves in source
  state.autocmd_id = vim.api.nvim_create_autocmd("CursorMoved", {
    buffer   = vim.api.nvim_get_current_buf(),
    callback = function()
      local name = require("quine.regions").region_name_at_cursor()
      if name then scroll_prose_to(name) end
    end,
    desc = "Quine: scroll prose to current region",
  })

  -- Initial scroll
  local name = require("quine.regions").region_name_at_cursor()
  if name then scroll_prose_to(name) end

  vim.notify("Quine: split open — " .. vim.fn.fnamemodify(life_md, ":~:."), vim.log.levels.INFO)
end

function M.close()
  -- Save prose buffer if modified
  if state.prose_buf and vim.api.nvim_buf_is_valid(state.prose_buf) then
    if vim.api.nvim_buf_get_option(state.prose_buf, "modified") then
      vim.api.nvim_buf_call(state.prose_buf, function()
        vim.cmd("write")
      end)
    end
  end

  -- Close prose window
  if state.prose_win and vim.api.nvim_win_is_valid(state.prose_win) then
    vim.api.nvim_win_close(state.prose_win, false)
  end

  -- Clear autocmd
  if state.autocmd_id then
    pcall(vim.api.nvim_del_autocmd, state.autocmd_id)
  end

  state = {
    source_win = nil, prose_win = nil,
    prose_buf  = nil, prose_file = nil,
    autocmd_id = nil,
  }

  vim.notify("Quine: split closed", vim.log.levels.INFO)
end

function M.is_open()
  return state.prose_win ~= nil and vim.api.nvim_win_is_valid(state.prose_win)
end

return M
