# quine

A personal knowledge graph that compiles files into a queryable IR.

This document was stitched together from its own source code by `quine-weave`.
The prose sections below were extracted by `quine collect` from `//|` comments
in the Rust source. The code blocks are pulled live from the same files. If
the code changes, re-collect and re-render — the document updates itself.

{{stats}}

---

## The Graph Model

The knowledge graph is built from files on disk. Every piece of data has a
physical location. The type system encodes this constraint.

### Node Identity

{{region:node-identity}}

### Graph Primitives

{{region:graph-primitives}}

### The Discovery Tree

{{region:discovery-types}}

---

## Collection

Collection is the compiler pass. It walks the filesystem, hashes files,
diffs against the DB, extracts links and metadata, and stores the results.
The same pipeline runs every time, for every directory, regardless of what's
in the files.

### The Seed

{{region:walk-seed}}

### Root Discovery

{{region:walk-root}}

### Subtree Walking

{{region:walk-subtree}}

### Incremental Diffing

{{region:walk-diff}}

### The Pipeline

{{region:collect-pipeline}}

---

## Extraction

Phase 2 reads the content of changed files and extracts structure: links
between files, named code regions with prose, and YAML frontmatter. The
extraction engine is one function parameterized by a per-language definition.

### The Engine

{{region:extraction-engine}}

### Language Definitions

{{prose:extractor-config}}

### Link Parsing

{{prose:link-parser}}

### Comment Isolation

{{prose:comment-extraction}}

### Region Parsing

{{region:region-parser}}

### Frontmatter

{{prose:frontmatter-parser}}

---

## Infrastructure

The SQLite database is the intermediate representation. Projectors in any
language read these tables. The error system encodes severity as types.

### The Schema

{{prose:db-schema}}

### Error Severity

{{prose:severity-model}}

---

## The Quine

This document is itself a product of the system it describes.
The prose you've been reading was extracted by `quine collect` from the
tool's own source code. The region parser in `regions.rs` parsed its own
`@region region-parser` annotation. The extraction engine in `engine.rs`
extracted its own design documentation. The tool documented itself by
running on itself.

{{index}}

---

*To regenerate: `quine collect src --db quine-self.db && quine-weave quine-self.db README.md quine_readme.pdf`*
