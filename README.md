# quine

A literate programming environment for your own work. Source code and prose live side by side, connected by named tags. The site renders itself — it's a quine in the sense that the engine that builds it is also one of the things being written about.

---

## The idea

Most documentation systems treat prose as secondary — you write code, then you write about it separately, and the two drift apart. Quine inverts this. A **tag** is a named region in source code that can be pulled into any prose document. The prose is the primary artifact; the code is referenced from it, not the other way around.

```python
# @region resolve_tag
# Resolution works by proximity first — if the prose file lives inside
# a watched repo, tags from that repo take precedence. This means you
# can use bare names in most cases and only qualify when you mean a
# specific repo explicitly.
def resolve_tag(con, name, *, prose_file=None, default_repo_id=None):
    ...
# @endregion
```

In a prose document:

```markdown
The resolver walks outward from the prose file's location:

{@region: resolve_tag}

This means journal entries written inside a repo automatically reference
that repo's tags without any qualification.
```

When rendered, `{@region: resolve_tag}` expands to the prose extracted from the leading comments, followed by the code block, followed by a download link. The source file is never modified.

---

## Structure

```
quine/
  quine.py               ← entry point
  config.yml             ← watches, paths, site settings

  core/
    db.py                ← SQLite schema + query helpers
    regions.py           ← @region extraction from source files
    directives.py        ← {@region:} expansion in prose

  collect/
    git.py               ← git commits → items table
    tags.py              ← @region markers → tags table
                            {@region:} directives → tag_refs table

  render/
    site.py              ← orchestrator
    pages/
      project.py         ← /project/{id}/ — life.md + commit log
      catalog.py         ← /tags/ — the shared vocabulary

  templates/             ← Jinja2 HTML templates
  static/                ← CSS
```

---

## Getting started

**Dependencies**

```bash
uv sync
```

**Configure**

```bash
cp config.example.yml config.yml
```

Edit `config.yml`:

```yaml
system:
  db:   quine.db
  www:  www

site:
  title:        "Your Site"
  default_repo: "your-repo"  # fallback for bare tag names

watches:
  - id:         your-repo
    path:       /path/to/your/repo
    name:       "Your Repo"
    visibility: public
    default:    true
```

**Run**

```bash
uv run python quine.py --action collect --config config.yml
uv run python quine.py --action render  --config config.yml
```

Open `www/` in a browser. You should see:
- `/project/your-repo/` — your `life.md` rendered with code regions expanded
- `/tags/` — every named region across all watched repos

---

## Tags

A **tag** is a named region in a source file, delimited by comment markers:

```python
# @region my_tag
# Prose here — leading comments are extracted as the narrative text.
# They render as a paragraph before the code block.
def my_function():
    ...
# @endregion
```

Comment styles are detected by file extension. All of these work:

```javascript
// @region my_tag
```
```python
# @region my_tag
```
```lua
-- @region my_tag
```

A region can be all prose (no code after the comments), all code (no leading comments), or both. The renderer handles each case.

---

## Directives

In any markdown file, `{@region: name}` pulls a tag into the prose:

```markdown
{@region: my_tag}
```

Three resolution forms:

| Form | Example | Resolution |
|---|---|---|
| Bare name | `{@region: my_tag}` | Proximity → default repo → singleton |
| Qualified | `{@region: repo/my_tag}` | Always unambiguous |
| Explicit file | `{@region: src/foo.py#my_tag}` | Bypasses the registry |

**Proximity resolution** means a prose file inside `/projects/myapp/` will automatically resolve bare tag names to `myapp` tags first, without any qualification. You only need the qualified form when you're referencing code from a different repo — for example, comparing two implementations in a single document:

```markdown
The trading system approach:
{@region: sor/error_handling}

The quine approach:
{@region: quine/error_handling}
```

---

## The tag catalog

Every named region across all watched repos is listed at `/tags/`. Each entry shows:

- The region name and source file
- The prose extracted from its leading comments
- Every prose document that references it
- Any drifted references (prose that points at a tag that no longer exists)

The catalog is the shared vocabulary. When you're writing and want to reference a piece of code, this is where you look — or use `:QuineCatalog` in neovim to fuzzy-search it without leaving the editor.

---

## Drift detection

When a region is renamed or deleted, any prose that references it becomes **drifted**. The collect step detects this and reports it:

```
collect.tags: drifted {@region: old_name} in notes/journal.md
```

Drifted references also appear at the top of the `/tags/` page and as diagnostic warnings in the neovim plugin. Drift isn't always wrong — a journal entry referencing deleted code is historically accurate. You decide whether to update the prose, update the tag name, or leave it as a record.

---

## Multiple prose documents

The same tag can be referenced from any number of prose files:

- `life.md` — the living technical explanation
- `journal/2025-03-15.md` — "I rewrote this today because..."
- `notes/patterns.md` — "this is an instance of X"

Each document has its own purpose and timestamp. The tag is a stable anchor they all point at. The catalog shows all of them.

---

## life.md

By convention, each watched repo can have a `life.md` at its root. This is the primary prose document for that repo — it's what renders at `/project/{id}/`. It has a front matter key `annotates:` that names the primary source file for bare tag resolution without a DB:

```yaml
---
annotates: core/regions.py
visibility: public
---

The region extractor is the foundation of the whole system.

{@region: extract_regions}

The split logic separates leading comments from the code body:

{@region: _split_region}
```

The quine's own `life.md` uses this to describe the engine that renders it — a site describing its own construction in the code that does the constructing.

---

## neovim plugin

See [`quine.nvim/README.md`](../quine.nvim/README.md) for full details.

| Command | Description |
|---|---|
| `:QuineCatalog` | Fuzzy-find all tags, insert `{@region:}` at cursor |
| `:QuineJump` | Jump between `@region` in source and `{@region:}` in prose |
| `:QuineWrap` | Wrap visual selection as a named region |
| `:QuineTag` | Wrap function under cursor as a named region |
| `:QuineOpen` | Open source/prose split pane |

The plugin reads the same SQLite database the collect step builds. No language server — just `sqlite3` CLI queries against the DB.

---

## Config reference

```yaml
system:
  db:        quine.db      # SQLite database path
  www:       www           # rendered HTML output
  templates: templates     # Jinja2 templates
  static:    static        # CSS and assets

site:
  title:        "My Site"
  base_url:     ""          # https://yoursite.com — empty for local
  default_repo: "repo-id"  # fallback repo for bare tag names in prose
                            # that doesn't live inside any watched repo

render:
  target: public            # public | private

watches:
  - id:         repo-id
    path:       /path/to/repo
    name:       "Human Name"
    visibility: public      # public | unlisted | private
    default:    true        # one watch can be the default for bare names

# Extra directories to scan for prose files.
# All watch paths are always scanned.
# prose_roots:
#   - ~/notes
```

---

## Python dependencies

```
pyyaml      — config and front matter parsing
jinja2      — HTML templates
markdown    — markdown → HTML conversion
Pygments    — syntax highlighting (optional, used by codehilite extension)
```

Install:

```bash
uv sync
```

---

## How the quine closes

The site engine is itself a watched repo. `life.md` at the root has `annotates: core/regions.py` and pulls `{@region: extract_regions}` and `{@region: _split_region}` — the code that does the pulling, describing itself in the same system it enables. Running `collect` then `render` produces a page about the engine, built by the engine, containing the engine's own source.

That's the quine.
