use rusqlite::Connection;

// @region db-schema
//| The SQLite schema is the IR contract. Projectors in any language
//| read these tables. The schema encodes the design doc's data model:
//|
//| - nodes: every file, identified by absolute path. Hash for diffing.
//| - edges: [[links]] between files. Source, target, optional fragment.
//| - regions: @region/@end spans with extracted prose.
//| - attributes: frontmatter key-value pairs. Always strings.
//| - roots: directories with life.yaml. Discovery tree structure.
//| - refs: discovery edges between roots.
//| - changelog: append-only log of node/edge adds and removes.
//|
//| Primary keys enforce design rules: (file, name) on regions prevents
//| duplicate region names. CHECK constraints on changelog close the
//| action/kind vocabulary. Indices on edges(target) make life mv fast.

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS nodes (
    path         TEXT PRIMARY KEY,
    hash         TEXT NOT NULL,
    size         INTEGER NOT NULL,
    modified     REAL NOT NULL,
    root         TEXT NOT NULL,
    collected_at REAL NOT NULL
);

CREATE TABLE IF NOT EXISTS edges (
    source   TEXT NOT NULL,
    target   TEXT NOT NULL,
    fragment TEXT,
    PRIMARY KEY (source, target, fragment)
);

CREATE TABLE IF NOT EXISTS regions (
    file       TEXT NOT NULL,
    name       TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line   INTEGER NOT NULL,
    prose      TEXT,
    PRIMARY KEY (file, name)
);

CREATE TABLE IF NOT EXISTS attributes (
    file  TEXT NOT NULL,
    key   TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (file, key)
);

CREATE TABLE IF NOT EXISTS roots (
    path TEXT PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS refs (
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    PRIMARY KEY (source, target)
);

CREATE TABLE IF NOT EXISTS changelog (
    timestamp REAL NOT NULL,
    action    TEXT NOT NULL CHECK(action IN ('added', 'removed')),
    kind      TEXT NOT NULL CHECK(kind IN ('node', 'edge')),
    path_a    TEXT NOT NULL,
    path_b    TEXT
);

CREATE INDEX IF NOT EXISTS idx_changelog_time ON changelog(timestamp);
CREATE INDEX IF NOT EXISTS idx_changelog_path ON changelog(path_a, timestamp);
CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target);
CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source);
CREATE INDEX IF NOT EXISTS idx_attributes_file ON attributes(file);
CREATE INDEX IF NOT EXISTS idx_nodes_root ON nodes(root);
"#;

pub fn migrate(conn: &Connection) -> std::result::Result<(), rusqlite::Error> {
    conn.execute_batch(SCHEMA)
}
// @end db-schema
