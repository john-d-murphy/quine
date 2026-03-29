# CLAUDE.md

## What this is

quine is a personal knowledge graph that compiles files into a queryable
SQLite database. The Rust binary (`quine`) is the engine — it collects,
indexes, and queries. Everything that reads the DB (rendering, editor
plugins, weaving) is a **projector** and lives outside the binary.

## Architecture contract

Read `design_addendum.md` — those five decisions are load-bearing.
The short version:

- **SQLite is the boundary.** Engine writes, projectors read. No exceptions.
- **Paths are root-relative.** Identity is `root_name:relative/path`.
- **Files are source of truth.** The DB is a rebuildable index.
- **Non-file sources** (taskwarrior, git, bookmarks) enter as collector-written
  markdown files with YAML frontmatter.
- **`quine.db` lives next to its seed** (`quine.yaml`).

## Repo layout

```
src/
  main.rs           CLI entry point (clap derive)
  types.rs          Node, Region, ProseBlock, Link — the graph primitives
  errors.rs         QuineError enum (thiserror)
  walk/mod.rs       Filesystem walker, .quine-stop, quine.yaml discovery
  extract/          Region parser, link extractor, frontmatter, config reader
  db/               SQLite schema, changelog, insert/query operations
  commands/         collect, find subcommands
quine.yaml          Seed config (maps root names to directories)
quine-project       Python projector (proof of concept)
quine-bookmark      Bash script (newsboat → quine bridge)
quine-weave.bak     Python weaver (expands {{region:}} directives)
```

## Building and testing

```sh
cargo build --release    # produces target/release/quine
cargo test               # 72 tests, all should pass
cargo clippy             # no warnings expected
cargo fmt -- --check     # enforced in CI
```

## Conventions

- `@region name` / `@end name` markers delimit named regions in source.
- `//|` lines are prose annotations (extracted as ProseBlock).
- Regions cannot nest. Names must be unique per file.
- The binary name is `quine` everywhere — not `life` (the old name).
