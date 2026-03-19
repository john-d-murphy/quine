-- quine/drift.lua — Show drifted {@region:} references as diagnostics.
--
-- When a prose file contains {@region: name} and 'name' no longer exists
-- in the DB, show a warning diagnostic on that line. Refreshed on save.

local M   = {}
local ns  = vim.api.nvim_create_namespace("quine_drift")

function M.refresh_diagnostics()
  local db       = require("quine.db")
  local buf      = vim.api.nvim_get_current_buf()
  local filepath = vim.api.nvim_buf_get_name(buf)

  if not filepath:match("%.md$") then return end

  -- Clear existing quine diagnostics in this buffer
  vim.diagnostic.reset(ns, buf)

  -- Check this file's refs in the DB
  local refs, _ = db.query(
    "SELECT name, drifted FROM tag_refs WHERE prose_file = ?",
    filepath
  )
  if not refs then return end

  local drifted = {}
  for _, r in ipairs(refs) do
    if r.drifted == 1 then
      drifted[r.name] = true
    end
  end

  if vim.tbl_isempty(drifted) then return end

  -- Scan buffer for the drifted directives and attach diagnostics
  local lines = vim.api.nvim_buf_get_lines(buf, 0, -1, false)
  local diags = {}

  for i, line in ipairs(lines) do
    for name in line:gmatch("{@region:%s*([^}]+)}") do
      name = name:gsub("^%s+", ""):gsub("%s+$", "")
      -- Strip repo_id/ prefix for lookup
      local bare = name:match("^[^/]+/(.+)$") or name
      if drifted[bare] or drifted[name] then
        local col = line:find("{@region:", 1, true) or 1
        table.insert(diags, {
          lnum     = i - 1,
          col      = col - 1,
          end_col  = col - 1 + #("{@region: " .. name .. "}"),
          severity = vim.diagnostic.severity.WARN,
          message  = "Drifted tag: '" .. name .. "' not found in source",
          source   = "quine",
        })
      end
    end
  end

  if #diags > 0 then
    vim.diagnostic.set(ns, buf, diags)
  end
end

return M
