# Design Addendum — Architecture Decisions

These decisions were made in March 2026 and form the contract for
all quine development. They are not guidelines — they are load-bearing.


## Architecture boundary

SQLite is the contract. The Rust binary (`quine`) is the engine —
it writes to the DB. Everything else reads from it.

- **Engine (Rust):** collect, find, validate, mv, log
- **Projectors (Python, Lua, anything):** render, weave, context, quine.nvim

Projectors never write to the DB. The engine never renders output.
If a new output format is needed, it's a new projector, not a change
to the Rust binary.


## Path identity

Paths are root-relative, not absolute. A node's identity is
`root_name:relative/path`. The DB stores the root name and the
relative path separately. Links use the same scheme:

    [[src/sor/strategy.cpp#pmm-routing]]       proximity (same root)
    [[vesplatform:src/sor/strategy.cpp]]       qualified (any root)

The only absolute paths are in the machine-local seed config
(~/quine.yaml) which maps root names to directories on disk.
This file is not committed to any repo.


## Collection modes

**Local mode:** `quine collect . --db .quine.db`
Self-contained. One root, no refs, no external dependencies.
Used for per-repo docs and CI builds.

**Global mode:** `quine collect ~/quine.yaml --db ~/.quine.db`
Multi-root. Refs wire roots together. One DB, full graph.
Used for the personal knowledge graph.

Same binary, same schema, same resolution rules. The difference
is scope.


## Sink principle

A root in local mode is a sink — it can be referenced by other
roots in the global graph, but it does not reference outward.
Cross-root links belong in personal prose and notes, not inside
a repo's own source. This keeps local collections self-contained
(no external dependencies to break a CI build) and confines
cross-root topology to the global graph.


## Non-file sources

External data (tasks, commits, bookmarks) enters the graph as
files. Collector scripts transform source formats into markdown
with YAML frontmatter, written to ~/.cache/quine/. The engine
never knows or cares where the files came from — it just reads
files.

Collectors are file-writers, not DB-writers. They run before
`quine collect`, produce human-readable markdown, and accrete
history over time (a task file grows as annotations and status
changes accumulate). The file tells the story.

Pattern:
  1. Collector script reads source (taskwarrior JSON, git log, etc.)
  2. Writes/updates markdown files in ~/.cache/quine/<source>/
  3. quine.yaml at the cache root declares it as a collectible root
  4. `quine collect` picks them up like any other file

No synthetic paths, no special DB handling, no plugin system in
the engine. New sources require a new collector script, never a
change to the Rust binary.


## Database location

The DB lives next to the seed that produced it. `quine collect`
writes `.quine.db` alongside the `quine.yaml` it collected from.
Tools find the DB by walking up the directory tree.

No global default path, no config option needed in the common
case. The DB is a build artifact of its root.


## Collection schedule

`quine collect` runs on cron. The graph is ambient infrastructure,
not a manual step. Recommended interval: every 5-10 minutes for
the global graph, triggered by CI for local/repo graphs.

The engine already handles incremental collection (hash-based
diffing), so frequent runs are cheap — only changed files get
re-extracted. SQLite handles concurrent reads (projectors) with
a single writer (collect) without issue.

Collector scripts (taskwarrior, bookmarks, git) run in the same
cron job, before `quine collect`:

    */5 * * * * quine-collect-tasks && quine-collect-bookmarks && quine collect ~/quine.yaml
