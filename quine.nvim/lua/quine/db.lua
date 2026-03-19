-- quine/db.lua — Read the quine SQLite database from Lua.
--
-- Uses the sqlite3 CLI rather than a Lua binding, so there's no
-- native dependency. Fast enough for interactive queries.
--
-- All queries return tables of rows (key/value), or nil + error string.

local M = {}

local function db_path()
  return require("quine").db_path()
end

-- Run a SELECT query, return list of row tables.
-- Each row is a {col = value} table.
function M.query(sql, ...)
  local path = db_path()
  if not path or vim.fn.filereadable(path) == 0 then
    return nil, "DB not found: " .. tostring(path)
  end

  -- Interpolate params (basic, for non-hostile inputs)
  local args = { ... }
  local i    = 0
  local q    = sql:gsub("%?", function()
    i = i + 1
    local v = args[i]
    if type(v) == "string" then
      return "'" .. v:gsub("'", "''") .. "'"
    end
    return tostring(v or "NULL")
  end)

  local cmd    = string.format("sqlite3 -separator '\x1f' -newline '\x1e' %q %q", path, q)
  local handle = io.popen(cmd)
  if not handle then return nil, "sqlite3 not available" end

  local output = handle:read("*a")
  handle:close()
  output = output:gsub("%s+$", "")
  if output == "" then return {} end

  -- Parse column names from first query (pragma table_info workaround:
  -- we use sqlite3's .headers mode via a wrapper query)
  -- Simpler: use -json mode if available, else parse manually.
  -- We use -json for reliability.
  local cmd_json = string.format("sqlite3 -json %q %q", path, q)
  local jh       = io.popen(cmd_json)
  if not jh then return nil, "sqlite3 -json not available" end

  local json_out = jh:read("*a")
  jh:close()
  json_out = json_out:gsub("%s+$", "")
  if json_out == "" or json_out == "[]" then return {} end

  local ok, rows = pcall(vim.json.decode, json_out)
  if not ok then return nil, "JSON parse error: " .. tostring(rows) end
  return rows
end


-- Convenience: all tags (for catalog picker)
function M.all_tags()
  return M.query([[
    SELECT t.id, t.repo_id, t.name, t.source_file, t.language, t.prose,
           COUNT(r.id) AS ref_count
    FROM tags t
    LEFT JOIN tag_refs r ON r.tag_id = t.id
    GROUP BY t.id
    ORDER BY t.name COLLATE NOCASE
  ]])
end

-- Tags in a specific repo
function M.tags_for_repo(repo_id)
  return M.query(
    "SELECT * FROM tags WHERE repo_id = ? ORDER BY name COLLATE NOCASE",
    repo_id
  )
end

-- Resolve a tag by name (proximity not available in Lua easily — just
-- return all candidates; the caller decides)
function M.candidates_for_name(name)
  return M.query("SELECT * FROM tags WHERE name = ?", name)
end

-- Tag by full id "repo_id/name"
function M.tag_by_id(tag_id)
  local rows, err = M.query("SELECT * FROM tags WHERE id = ?", tag_id)
  if not rows then return nil, err end
  return rows[1]
end

-- Watch by id
function M.watch_by_id(watch_id)
  local rows, err = M.query("SELECT * FROM watches WHERE id = ?", watch_id)
  if not rows then return nil, err end
  return rows[1]
end

-- Drifted refs (for diagnostics)
function M.drifted_refs()
  return M.query(
    "SELECT prose_file, name FROM tag_refs WHERE drifted = 1 ORDER BY prose_file, name"
  )
end

-- All watches
function M.all_watches()
  return M.query("SELECT * FROM watches ORDER BY name")
end

return M
