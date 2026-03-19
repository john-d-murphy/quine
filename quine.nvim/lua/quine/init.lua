-- quine/init.lua — Setup and user commands
--
-- Usage in init.lua / lazy.nvim:
--   require("quine").setup({
--     config = "~/.config/quine/config.yml",  -- or per-repo .quine.yml
--     db     = nil,   -- override DB path (normally read from config)
--     keymaps = true, -- set default keymaps
--   })

local M = {}

M.config = {
  config_file = nil,   -- resolved at setup time
  db_path     = nil,   -- resolved at setup time
  keymaps     = true,
}

-- Find config file by walking up from cwd
local function find_config()
  local names = { "config.yml", ".quine.yml", "quine.yml" }
  local dir   = vim.fn.getcwd()
  while dir ~= "/" do
    for _, name in ipairs(names) do
      local candidate = dir .. "/" .. name
      if vim.fn.filereadable(candidate) == 1 then
        return candidate
      end
    end
    dir = vim.fn.fnamemodify(dir, ":h")
  end
  return nil
end

-- Read db path from config file (minimal YAML parse — just greps for db:)
local function db_from_config(config_path)
  if not config_path then return nil end
  for line in io.lines(config_path) do
    local db = line:match("^%s*db%s*:%s*(.+)$")
    if db then
      db = db:gsub("%s*#.*", ""):gsub("^%s+", ""):gsub("%s+$", "")
      if db:sub(1, 1) ~= "/" then
        local dir = vim.fn.fnamemodify(config_path, ":h")
        db = dir .. "/" .. db
      end
      return db
    end
  end
  return nil
end

function M.setup(opts)
  opts = opts or {}
  M.config = vim.tbl_deep_extend("force", M.config, opts)

  -- Resolve config file
  if not M.config.config_file then
    M.config.config_file = find_config()
  end

  -- Resolve DB path
  if not M.config.db_path then
    M.config.db_path = db_from_config(M.config.config_file)
  end

  -- Register user commands
  vim.api.nvim_create_user_command("QuineCatalog", function()
    require("quine.catalog").open()
  end, { desc = "Browse tag catalog and insert {@region:} directive" })

  vim.api.nvim_create_user_command("QuineJump", function()
    require("quine.jump").jump()
  end, { desc = "Jump between region in source and directive in prose" })

  vim.api.nvim_create_user_command("QuineWrap", function()
    require("quine.wrap").wrap_selection()
  end, { range = true, desc = "Wrap selection in @region/@endregion markers" })

  vim.api.nvim_create_user_command("QuineTag", function()
    require("quine.wrap").tag_function()
  end, { desc = "Tag function under cursor as a region" })

  vim.api.nvim_create_user_command("QuineOpen", function()
    require("quine.split").open()
  end, { desc = "Open prose/source split pane" })

  vim.api.nvim_create_user_command("QuineClose", function()
    require("quine.split").close()
  end, { desc = "Close split pane" })

  -- Default keymaps (opt out with keymaps = false)
  if M.config.keymaps then
    local map = function(mode, lhs, rhs, desc)
      vim.keymap.set(mode, lhs, rhs, { silent = true, desc = "Quine: " .. desc })
    end
    map("n", "<leader>qc", "<cmd>QuineCatalog<cr>",  "catalog picker")
    map("n", "<leader>qj", "<cmd>QuineJump<cr>",     "jump source↔prose")
    map("n", "<leader>qo", "<cmd>QuineOpen<cr>",     "open split")
    map("v", "<leader>qw", ":QuineWrap<cr>",         "wrap as region")
    map("n", "<leader>qt", "<cmd>QuineTag<cr>",      "tag function")
  end

  -- Autocommands: refresh diagnostics on save
  local grp = vim.api.nvim_create_augroup("Quine", { clear = true })
  vim.api.nvim_create_autocmd("BufWritePost", {
    group   = grp,
    pattern = { "*.md", "*.py", "*.scd", "*.js", "*.ts", "*.cpp", "*.rs", "*.go" },
    callback = function()
      require("quine.drift").refresh_diagnostics()
    end,
    desc = "Quine: refresh drift diagnostics on save",
  })
end

function M.db_path()
  return M.config.db_path
end

function M.config_file()
  return M.config.config_file
end

return M
