# Quine — Design Document

Context for Claude Code sessions. Read this before working on any part of the quine codebase.

---

## What this is

A literate programming environment for personal work. Source code and prose live side by side, connected by named **tags**. The site renders itself — the engine that builds it is one of the things being written about (the quine property).

This is not a documentation generator. The prose is the primary artifact. Code is referenced from prose, not the other way around.

---

## The core concept

A **tag** is a named region in a source file:

```python
# @region resolve_tag
# Resolution works by proximity first — if the prose file lives inside
# a watched repo, tags from that repo take precedence.
def resolve_tag(con, name, *, prose_file=None, default_repo_id=None):
    ...
# @endregion
```

Leading comments are extracted as prose. Everything after the first non-comment line is code. A region can be all prose, all code, or both.

In any markdown file, a **directive** pulls a tag into the document:

```markdown
{@region: resolve_tag}
```

This expands to: prose paragraph + attribution line + syntax-highlighted code block + download link. The source file is never modified.

---

## Repository structure

```
quine/                     ← site engine (Python)
  quine.py                 ← entry point: --action collect|render
  config.yml               ← watches, paths, site settings
  pyproject.toml           ← uv-managed dependencies

  core/
    db.py                  ← SQLite schema + all query helpers
    regions.py             ← @region/@endregion extraction
    directives.py          ← {@region:} expansion

  collect/
    git.py                 ← git commits → items table
    tags.py                ← source @regions → tags table
                              prose {@region:} → tag_refs table

  render/
    site.py                ← orchestrator, called by --action render
    pages/
      project.py           ← /project/{id}/ — life.md + commit log
      catalog.py           ← /tags/ — the shared vocabulary

  templates/               ← Jinja2 (.html.j2)
  static/                  ← site.css, code.css

quine.nvim/                ← neovim plugin (Lua)
  lua/quine/
    init.lua               ← setup(), user commands, keymaps
    db.lua                 ← sqlite3 CLI → Lua (no native deps)
    catalog.lua            ← :QuineCatalog — tag picker
    jump.lua               ← :QuineJump — prose↔source navigation
    wrap.lua               ← :QuineWrap, :QuineTag
    regions.lua            ← buffer-local @region parsing
    split.lua              ← :QuineOpen split pane (skeleton)
    drift.lua              ← diagnostic markers for drifted refs
```

---

## Database schema

Four tables. Schema defined in `core/db.py`.

```sql
watches    -- repos and directories being tracked
           -- id, path, name, visibility, default_repo

items      -- git commits and markdown files
           -- id, source, watch_id, ref, title, body,
           -- occurred_at, ingested_at, visibility, meta (JSON)

tags       -- named @region markers from source files
           -- id = "{repo_id}/{name}"  (unique across repos)
           -- repo_id, name, source_file, language, sha, prose

tag_refs   -- {@region:} directives found in prose files
           -- id = "{prose_file}#{name}"
           -- tag_id (NULL if drifted), prose_file, name,
           -- repo_id, drifted
```

Key constraint: tag identity is `{repo_id}/{name}`. Two repos can have a tag with the same name — they are different tags. Resolution handles this.

---

## Tag resolution

The most important logic in the system. Lives in `core/db.py:resolve_tag()`.

Four-step resolution for bare names (no `/`):

1. **Proximity** — tag in same repo as the prose file (path prefix match against watches table)
2. **Default repo** — watch with `default_repo=1`
3. **Singleton** — only one tag with that name across all repos
4. **Ambiguous** — return None, caller emits warning or renders error block

Qualified names (`repo_id/name`) bypass all four steps — always unambiguous.

Explicit file references (`path/to/file.py#name`) bypass the registry entirely.

This resolution order means:
- Most of the time you write bare names and it just works
- Journal entries inside a repo automatically resolve to that repo's tags
- Cross-repo comparisons use qualified names explicitly
- The system warns you when it can't resolve rather than silently picking wrong

---

## Drift detection

When a tag is renamed or deleted, prose directives that reference it become **drifted**.

Detection runs in two places:
1. `collect/tags.py` — at collect time, marks `tag_refs.drifted=1` for any ref whose tag no longer exists
2. `quine.nvim/lua/quine/drift.lua` — on BufWritePost, shows vim diagnostics for drifted refs in the current markdown buffer

Drift is reported but not automatically fixed. It's intentional that a journal entry referencing deleted code remains as a historical record. You decide whether to update the prose, update the tag name, or leave it.

---

## The pipeline

```
collect → render
```

No build phase yet. Collect writes to DB. Render reads DB and emits HTML.

**collect:**
1. Register watches from config.yml
2. Ingest git commits → items table
3. Walk source files → extract @regions → tags table
4. Walk prose files → find {@region:} directives → tag_refs table
5. Mark drifted refs

**render:**
1. Read DB
2. Render /project/{id}/ for each watch (life.md + commit log)
3. Render /tags/ catalog
4. Copy static assets
5. Report drift

---

## life.md

By convention, each watched repo has `life.md` at its root. This is the primary prose document for that repo — rendered at `/project/{id}/`.

Front matter:
```yaml
---
annotates: core/regions.py   # primary source file for bare tag resolution
visibility: public
---
```

The quine's own `life.md` pulls `{@region: extract_regions}` and `{@region: _split_region}` from `core/regions.py` and `{@region: resolve_tag}` from `core/db.py` — the engine describing itself in the code that does the describing.

---

## Config

```yaml
system:
  db:        quine.db
  www:       www
  templates: templates
  static:    static

site:
  title:        "Site title"
  base_url:     ""            # https://yoursite.com or empty for local
  default_repo: "repo-id"    # fallback for bare tag names

render:
  target: public              # public | private

watches:
  - id:         repo-id
    path:       /absolute/path/to/repo
    name:       "Human Name"
    visibility: public
    default:    true          # one watch is the default for bare names

# prose_roots:               # extra dirs to scan for .md files
#   - ~/notes
```

---

## neovim plugin

Plugin reads the same SQLite DB the collect step builds. No language server, no background process — `sqlite3` CLI queries.

**Commands:**
- `:QuineCatalog` — fuzzy-find all tags, insert `{@region:}` at cursor (Telescope or vim.ui.select)
- `:QuineJump` — cursor on `{@region: name}` in markdown → jump to source; cursor inside `@region` block in source → jump to prose
- `:QuineWrap` — visual selection → wrap in `@region`/`@endregion` with correct comment style for filetype
- `:QuineTag` — wrap function under cursor (treesitter), suggest function name as default
- `:QuineOpen` — vertical split, source left, life.md right, prose pane scrolls to current region
- `:QuineClose` — close split, save prose

**Setup:**
```lua
require("quine").setup({
  config_file = nil,  -- auto-detected by walking up from cwd
  db = nil,           -- override DB path
  keymaps = true,     -- <leader>qc/qj/qo/qw/qt
})
```

**Default keymaps:** `<leader>q{c,j,o,w,t}`

---

## Current state (as of initial build)

**Done:**
- Core DB schema with all four tables
- @region extraction (`core/regions.py`)
- Directive expansion with full resolution logic (`core/directives.py`)
- collect/git.py — git ingestion
- collect/tags.py — tag registry builder
- render/pages/project.py — project page with directive expansion
- render/pages/catalog.py — /tags/ catalog
- All templates (base, project, catalog)
- CSS (site.css, code.css)
- quine.nvim — all modules written, split.lua is a skeleton
- pyproject.toml with uv

**Not yet done (see GitHub issues):**
- #19 Wire render step end-to-end and verify
- #20 Write life.md for the quine itself
- #21 Configure quine repo to watch itself (close the quine)
- #3  Test collect + render with a real config
- #2  Test neovim plugin
- #23 Build index/stream page (no homepage yet)
- #22 Finish split pane outline view

**Known gaps:**
- render/site.py calls render_project and render_catalog but needs end-to-end verification with a real DB
- split.lua opens and scrolls but the outline/reorder view isn't built
- No index page — the site has /project/{id}/ and /tags/ but no homepage

---

## Design decisions and why

**Why SQLite and not files?**
The tag registry needs to be queryable — "all tags by name", "all refs for this tag", "drifted refs". Flat files make this awkward. SQLite is a single file, zero infrastructure, and the neovim plugin can query it directly with the sqlite3 CLI.

**Why is origin (source vs prose) not tracked?**
Early design considered tracking whether a tag originated in source or prose. Dropped because it doesn't matter — a tag is just a name both sides know. The catalog doesn't care where it came from, only that it exists and what it refers to.

**Why proximity resolution instead of always requiring qualified names?**
Because most of the time you're writing prose about the code you're sitting next to. Requiring `repo_id/name` everywhere adds friction with no benefit in the common case. The qualified form is there for the cases where you need it.

**Why is drift a warning and not an error?**
A journal entry from 2023 referencing code that was deleted in 2024 is historically accurate. Making drift an error would require updating history. It's surfaced visibly so you can make a conscious choice.

**Why does the split pane show narrative order on the right?**
The left pane (source) is in source order — determined by the code's dependency structure. The right pane (life.md) is in narrative order — determined by the story you're telling. They can legitimately differ. Reordering the right pane reorders directives in life.md, never the source.

**Why `sqlite3` CLI in the neovim plugin instead of a Lua SQLite binding?**
Zero native dependencies. The `sqlite3` binary is available everywhere. A Lua binding requires compilation and version management. The query latency is imperceptible for interactive use.
