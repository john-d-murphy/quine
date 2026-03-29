# quine

A personal knowledge graph that compiles files into a queryable SQLite database.

quine walks your source trees, extracts named regions and prose annotations,
resolves `[[links]]` between files, and builds a graph you can query, render,
and wire into your editor. The engine writes to SQLite; everything else —
renderers, editor plugins, document weavers — reads from it.

The name is literal: quine indexes its own source code. The `@region` markers
and `//|` prose annotations in the Rust source are extracted by the same engine
that processes any other codebase. The documentation is stitched from the code
that produces it.

## Quick start

```sh
cargo build --release
cd ~/some-project
quine init                  # creates quine.yaml
quine collect .             # walks, hashes, extracts, builds the graph
quine find "pattern"        # search nodes, pipe to fzf
quine roots --db quine.db   # show discovery tree
```

## How it works

Every directory with a `quine.yaml` is a **root**. The walker descends from
a seed directory, hashing files, extracting `@region`/`@end` markers, parsing
`//|` prose annotations, and resolving `[[wiki-style links]]` into typed edges.
Changed files are detected by SHA-256 diffing against the previous collection —
only modified files get re-extracted.

The result is a SQLite database containing nodes (files), edges (links between
files), regions (named code spans), and prose blocks (inline documentation).

### Annotations

```rust
// @region fee-calculation
//| The fee engine uses a most-specific-match strategy.
//| Spread orders check the spread table first, then fall
//| through to the per-leg rate. See [[billing/rates.md]].

fn calculate_fee(order: &Order) -> f64 {
    // ...
}
// @end fee-calculation
```

Regions are named spans. Prose blocks (`//|` lines) are contiguous runs of
documentation attached to a region. Links (`[[path]]` or `[[path#region]]`)
create edges in the graph. The extraction engine is parameterized by comment
syntax — adding a new language means adding a definition, not new code.

### Exclusion

Three mechanisms prevent walking unwanted directories:

- `.git` is always skipped (hardcoded).
- `exclude` patterns in `quine.yaml` support glob syntax (`target`, `*.db`).
- `.quine-stop` sentinel files halt descent into a specific directory.

```yaml
# quine.yaml
name: "my-project"
refs: []
exclude:
  - target
  - node_modules
  - "*.db"
```

### Multi-root graphs

Roots can reference other roots via `refs` in `quine.yaml`. The walker follows
refs to discover the full graph. Cycle detection prevents infinite loops.

```yaml
name: "notes"
refs:
  - path: ~/Dropbox/GitHub/quine
  - path: ~/Dropbox/GitHub/vesplatform
```

Local mode (single root, no refs) is self-contained and suitable for CI.
Global mode (multi-root, refs wired together) builds the full personal graph.

## Architecture

SQLite is the contract between the engine and everything downstream.

**Engine (Rust):** `collect`, `find`, `init`, `stop`, `unstop`, `roots` — writes to the DB.

**Projectors (Python, Lua, anything):** renderers, weavers, editor plugins — read from the DB. Projectors never write to the DB. The engine never renders output. New output formats are new projectors, not changes to the binary.

**Collectors (bash, Python):** transform external data (taskwarrior tasks, git commits, newsboat bookmarks) into markdown files with YAML frontmatter. The engine collects these files like any other — no plugin system, no special handling.

See [design_addendum.md](design_addendum.md) for the five load-bearing architecture decisions.

## Building

```sh
cargo build --release    # target/release/quine
cargo test               # 74 tests
cargo clippy             # no warnings (enforced in CI)
cargo fmt -- --check     # enforced in CI
```

## License

Personal project. Not yet licensed for distribution.
