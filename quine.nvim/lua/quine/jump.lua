-- quine/jump.lua — Bidirectional navigation between source regions and prose.
--
-- From source: cursor inside @region/@endregion block
--   → find all prose files with {@region: name} and jump to the first,
--     or show a picker if multiple exist.
--
-- From prose: cursor on {@region: name} directive
--   → find the source file and jump to the @region marker.

local M  = {}
local db = require("quine.db")

-- @region patterns (must match regions.py logic)
local REGION_START = "^%s*[/][/]%s*@region%s+(%S+)"
local REGION_START_HASH = "^%s*#%s*@region%s+(%S+)"
local REGION_START_DASH = "^%s*%-%-.*@region%s+(%S+)"
local REGION_END   = "@endregion"

-- Prose directive pattern
local DIRECTIVE_PAT = "{@region:%s*([^}]+)}"

local function detect_comment_style(ft)
  local styles = {
    python    = "#",  ruby      = "#",  bash     = "#",
    sh        = "#",  yaml      = "#",  toml     = "#",
    cpp       = "//", c         = "//", java     = "//",
    javascript= "//", typescript= "//", rust     = "//",
    go        = "//", swift     = "//",
    supercollider = "//",
    lua       = "--", sql       = "--", haskell  = "--",
  }
  return styles[ft] or "//"
end

local function region_pattern_for(ft)
  local style = detect_comment_style(ft)
  if style == "#" then
    return "^%s*#%s*@region%s+(%S+)"
  elseif style == "--" then
    return "^%s*%-%-.*@region%s+(%S+)"
  else
    return "^%s*[/][/]%s*@region%s+(%S+)"
  end
end

-- Get region name from current line or anywhere in surrounding block
local function region_name_at_cursor()
  local ft    = vim.bo.filetype
  local pat   = region_pattern_for(ft)
  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  local row   = vim.api.nvim_win_get_cursor(0)[1]  -- 1-indexed

  -- Scan upward from cursor for @region marker
  for i = row, 1, -1 do
    local name = lines[i]:match(pat)
    if name then
      return name
    end
    -- Hit @endregion going up → not in a region
    if lines[i]:match(REGION_END) then
      break
    end
  end
  return nil
end

-- Get directive name from current line (prose file)
local function directive_at_cursor()
  local line = vim.api.nvim_get_current_line()
  local col  = vim.api.nvim_win_get_cursor(0)[2] + 1  -- 1-indexed

  -- Find all directives in line, pick the one the cursor is inside
  for raw, s, e in line:gmatch("(){@region:%s*([^}]+)}()") do
    local start_pos = s - 1  -- approximate; good enough
  end

  -- Simpler: just match anywhere on the line
  return line:match(DIRECTIVE_PAT)
end

local function open_file_at_pattern(filepath, pattern)
  vim.cmd("edit " .. vim.fn.fnameescape(filepath))
  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  for i, line in ipairs(lines) do
    if line:match(pattern) then
      vim.api.nvim_win_set_cursor(0, { i, 0 })
      vim.cmd("normal! zz")
      return true
    end
  end
  return false
end

function M.jump()
  local buf = vim.api.nvim_get_current_buf()
  local ft  = vim.bo[buf].filetype

  if ft == "markdown" then
    -- Prose → source
    local name = directive_at_cursor()
    if not name then
      vim.notify("Quine: no {@region:} directive under cursor", vim.log.levels.WARN)
      return
    end
    name = name:gsub("^%s+", ""):gsub("%s+$", "")

    -- Qualified name?
    local repo_id, bare_name
    if name:find("/") then
      repo_id, bare_name = name:match("^([^/]+)/(.+)$")
    else
      bare_name = name
    end

    local rows, err
    if repo_id then
      rows, err = db.query("SELECT * FROM tags WHERE id = ?", repo_id .. "/" .. bare_name)
    else
      rows, err = db.candidates_for_name(bare_name)
    end

    if not rows or #rows == 0 then
      vim.notify("Quine: tag '" .. name .. "' not found in DB", vim.log.levels.WARN)
      return
    end

    local function jump_to_tag(tag)
      local watch, _ = db.watch_by_id(tag.repo_id)
      if not watch then
        vim.notify("Quine: watch not found for " .. tag.repo_id, vim.log.levels.ERROR)
        return
      end
      local filepath = watch.path .. "/" .. tag.source_file
      local ft_src   = vim.filetype.match({ filename = filepath }) or "text"
      local pat      = region_pattern_for(ft_src):gsub("%(%%S%+%)", vim.pesc(tag.name))
      -- Simpler pattern for search
      local search_pat = "@region%s+" .. vim.pesc(tag.name)
      open_file_at_pattern(filepath, search_pat)
    end

    if #rows == 1 then
      jump_to_tag(rows[1])
    else
      -- Multiple candidates — pick
      local labels = vim.tbl_map(function(t)
        return t.id .. "  (" .. t.source_file .. ")"
      end, rows)
      vim.ui.select(labels, { prompt = "Jump to region:" }, function(_, idx)
        if idx then jump_to_tag(rows[idx]) end
      end)
    end

  else
    -- Source → prose
    local name = region_name_at_cursor()
    if not name then
      vim.notify("Quine: cursor is not inside a @region block", vim.log.levels.WARN)
      return
    end

    -- Find prose files that reference this name
    local refs, err = db.query(
      "SELECT prose_file FROM tag_refs WHERE name = ? AND drifted = 0 ORDER BY prose_file",
      name
    )

    if not refs or #refs == 0 then
      -- No existing reference — offer to create one in life.md
      local watches, _ = db.all_watches()
      if watches and #watches > 0 then
        local life_md = watches[1].path .. "/life.md"
        vim.notify(
          "Quine: no prose reference for '" .. name .. "' — open life.md? (yy/n)",
          vim.log.levels.INFO
        )
        vim.ui.input({ prompt = "Open life.md? [y/n] " }, function(input)
          if input and input:lower():sub(1, 1) == "y" then
            vim.cmd("edit " .. vim.fn.fnameescape(life_md))
          end
        end)
      end
      return
    end

    if #refs == 1 then
      local prose_file = refs[1].prose_file
      open_file_at_pattern(prose_file, "{@region:%s*" .. vim.pesc(name))
    else
      local labels = vim.tbl_map(function(r)
        return vim.fn.fnamemodify(r.prose_file, ":~:.")
      end, refs)
      vim.ui.select(labels, { prompt = "Jump to prose:" }, function(_, idx)
        if idx then
          open_file_at_pattern(refs[idx].prose_file, "{@region:%s*" .. vim.pesc(name))
        end
      end)
    end
  end
end

return M
