# quine.nvim

Neovim plugin for the quine literate programming environment.

## Commands

| Command | Description |
|---|---|
| `:QuineCatalog` | Fuzzy-find all named tags, insert `{@region:}` at cursor |
| `:QuineJump` | Jump between `@region` in source and `{@region:}` in prose |
| `:QuineWrap` | Wrap visual selection in `@region`/`@endregion` markers |
| `:QuineTag` | Wrap function under cursor as a named region (treesitter) |
| `:QuineOpen` | Open source/prose split pane |
| `:QuineClose` | Close split pane, save prose |

## Default keymaps

| Key | Command |
|---|---|
| `<leader>qc` | `:QuineCatalog` |
| `<leader>qj` | `:QuineJump` |
| `<leader>qo` | `:QuineOpen` |
| `<leader>qw` | `:QuineWrap` (visual) |
| `<leader>qt` | `:QuineTag` |

Disable with `keymaps = false` in setup.

## Setup

```lua
-- lazy.nvim
{
  "you/quine.nvim",
  config = function()
    require("quine").setup({
      -- Path to config.yml (auto-detected by walking up from cwd if nil)
      config_file = nil,
      -- Override DB path (normally read from config.yml)
      db = nil,
      -- Set default keymaps
      keymaps = true,
    })
  end,
}
```

## Requirements

- `sqlite3` in PATH (used for DB queries)
- `telescope.nvim` (optional, falls back to `vim.ui.select`)
- `nvim-treesitter` (optional, required for `:QuineTag`)

## How it works

The plugin reads the same SQLite database that the quine collect step builds.
No language server, no process — just `sqlite3` CLI queries.

The DB path is read from `config.yml` (or `.quine.yml`) by walking up from
the current working directory. You can override it in setup.

## Tag resolution

Bare `{@region: name}` resolves by:
1. **Proximity** — tag in same repo as the prose file
2. **Default repo** — watch with `default: true` in config
3. **Singleton** — only one tag with that name across all repos
4. **Ambiguous** — shown as warning, use qualified form

Qualified form is always unambiguous: `{@region: repo_id/name}`
