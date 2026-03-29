pub mod schema;
pub mod changelog;

use rusqlite::Connection;
use std::collections::HashSet;
use std::path::Path;

use crate::errors::Result;
use crate::types::*;

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open or create the database at the given path.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let db = Db { conn };
        schema::migrate(&db.conn)?;
        Ok(db)
    }

    /// Open an in-memory database (for testing).
    #[cfg(test)]
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        schema::migrate(&conn)?;
        Ok(Db { conn })
    }

    // ---- Transactions ----

    /// Run a closure inside a transaction. Commits on Ok, rolls back on Err.
    pub fn transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let tx = self.conn.unchecked_transaction()?;
        let result = f()?;
        tx.commit()?;
        Ok(result)
    }

    // ---- Node operations ----

    /// Get all node paths currently in the DB.
    #[allow(dead_code)]
    pub fn all_node_paths(&self) -> Result<Vec<NodePath>> {
        let mut stmt = self.conn.prepare("SELECT path FROM nodes")?;
        let paths = stmt
            .query_map([], |row| {
                let path: String = row.get(0)?;
                Ok(path)
            })?
            .filter_map(|r| r.ok())
            .filter_map(|p| NodePath::from_absolute(p.into()))
            .collect();
        Ok(paths)
    }

    /// Look up the stored hash for a node. Returns None if not in DB.
    #[allow(dead_code)]
    pub fn node_hash(&self, path: &NodePath) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT hash FROM nodes WHERE path = ?1")?;
        let hash = stmt
            .query_row([path.as_str()], |row| row.get::<_, String>(0))
            .ok();
        Ok(hash)
    }

    /// Get all node path -> hash pairs from the DB.
    /// Used by diff to determine added/changed/unchanged in one pass.
    pub fn all_node_hashes(&self) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare("SELECT path, hash FROM nodes")?;
        let map = stmt
            .query_map([], |row| {
                let path: String = row.get(0)?;
                let hash: String = row.get(1)?;
                Ok((path, hash))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(map)
    }

    /// Insert or update a node from a walked file.
    pub fn upsert_node(&self, file: &WalkedFile) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO nodes (path, hash, size, modified, root, collected_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                file.path.as_str(),
                file.hash,
                file.size,
                file.modified,
                file.root.as_str(),
                now(),
            ],
        )?;
        Ok(())
    }

    // ---- Edge operations ----

    /// Get all edge target paths that currently exist as nodes.
    pub fn resolved_edge_targets(&self) -> Result<HashSet<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT e.target FROM edges e
             INNER JOIN nodes n ON e.target = n.path",
        )?;
        let targets = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(targets)
    }

    /// Get all edges whose target matches a given path.
    #[allow(dead_code)]
    pub fn incoming_edges(&self, target: &NodePath) -> Result<Vec<(NodePath, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT source, fragment FROM edges WHERE target = ?1",
        )?;
        let edges = stmt
            .query_map([target.as_str()], |row| {
                let source: String = row.get(0)?;
                let fragment: Option<String> = row.get(1)?;
                Ok((source, fragment))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(s, f)| Some((NodePath::from_absolute(s.into())?, f)))
            .collect();
        Ok(edges)
    }

    /// Insert an edge.
    pub fn insert_edge(&self, edge: &Edge) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO edges (source, target, fragment)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![
                edge.source.as_str(),
                edge.target.as_str(),
                edge.fragment,
            ],
        )?;
        Ok(())
    }

    // ---- Region operations ----

    /// Insert a region.
    pub fn insert_region(&self, region: &Region) -> Result<()> {
        let prose_text: String = region
            .prose
            .iter()
            .map(|p| p.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        self.conn.execute(
            "INSERT OR REPLACE INTO regions (file, name, start_line, end_line, prose)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                region.file.as_str(),
                region.name,
                region.start_line,
                region.end_line,
                if prose_text.is_empty() { None } else { Some(prose_text) },
            ],
        )?;
        Ok(())
    }

    // ---- Attribute operations ----

    /// Insert an attribute.
    pub fn insert_attribute(&self, attr: &Attribute) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO attributes (file, key, value)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![attr.file.as_str(), attr.key, attr.value],
        )?;
        Ok(())
    }

    // ---- Root operations ----

    /// Get all root paths currently in the DB.
    pub fn all_root_paths(&self) -> Result<HashSet<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM roots")?;
        let paths = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    /// Insert or update a root and its refs.
    pub fn upsert_root(&self, root: &Root) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO roots (path, name) VALUES (?1, ?2)",
            rusqlite::params![root.path.as_str(), root.name],
        )?;
        // Clear old refs for this root and re-insert.
        self.conn.execute(
            "DELETE FROM refs WHERE source = ?1",
            [root.path.as_str()],
        )?;
        for r in &root.refs {
            self.conn.execute(
                "INSERT INTO refs (source, target) VALUES (?1, ?2)",
                rusqlite::params![root.path.as_str(), r.as_str()],
            )?;
        }
        Ok(())
    }

    /// Remove a root and its refs.
    pub fn remove_root(&self, path: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM roots WHERE path = ?1", [path])?;
        self.conn
            .execute("DELETE FROM refs WHERE source = ?1", [path])?;
        Ok(())
    }

    /// List all roots with their names, sorted by path.
    pub fn list_roots(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, path FROM roots ORDER BY path")?;
        let roots = stmt
            .query_map([], |row| {
                let name: String = row.get(0)?;
                let path: String = row.get(1)?;
                Ok((name, path))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(roots)
    }

    // ---- File removal (cascading) ----

    /// Remove all data for a given file path: node, outgoing edges,
    /// regions, and attributes.
    pub fn remove_file(&self, path: &NodePath) -> Result<()> {
        let p = path.as_str();
        self.conn.execute("DELETE FROM nodes WHERE path = ?1", [p])?;
        self.conn.execute("DELETE FROM edges WHERE source = ?1", [p])?;
        self.conn.execute("DELETE FROM regions WHERE file = ?1", [p])?;
        self.conn
            .execute("DELETE FROM attributes WHERE file = ?1", [p])?;
        Ok(())
    }

    // ---- Search ----

    /// Find nodes whose path contains the query string (case-insensitive).
    /// Returns matching paths, most recently modified first.
    pub fn find_nodes(&self, query: &str) -> Result<Vec<NodePath>> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.conn.prepare(
            "SELECT path FROM nodes WHERE path LIKE ?1
             ORDER BY modified DESC LIMIT 50",
        )?;
        let paths = stmt
            .query_map([&pattern], |row| {
                let path: String = row.get(0)?;
                Ok(path)
            })?
            .filter_map(|r| r.ok())
            .filter_map(|p| NodePath::from_absolute(p.into()))
            .collect();
        Ok(paths)
    }
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_memory_creates_tables() {
        let db = Db::open_memory().unwrap();
        // Should be able to query nodes without error.
        let paths = db.all_node_paths().unwrap();
        assert!(paths.is_empty());
    }

    #[test]
    fn upsert_and_query_node() {
        let db = Db::open_memory().unwrap();
        let file = WalkedFile {
            path: NodePath::new("/test/file.md").unwrap(),
            hash: "abc123".into(),
            size: 100,
            modified: 1.0,
            root: NodePath::new("/test").unwrap(),
        };
        db.upsert_node(&file).unwrap();

        let paths = db.all_node_paths().unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].as_str(), "/test/file.md");

        let hash = db.node_hash(&file.path).unwrap();
        assert_eq!(hash.as_deref(), Some("abc123"));
    }

    #[test]
    fn remove_file_cascades() {
        let db = Db::open_memory().unwrap();
        let path = NodePath::new("/test/file.md").unwrap();
        let file = WalkedFile {
            path: path.clone(),
            hash: "abc".into(),
            size: 10,
            modified: 1.0,
            root: NodePath::new("/test").unwrap(),
        };
        db.upsert_node(&file).unwrap();

        let edge = Edge {
            source: path.clone(),
            target: NodePath::new("/test/other.md").unwrap(),
            fragment: None,
        };
        db.insert_edge(&edge).unwrap();

        let attr = Attribute {
            file: path.clone(),
            key: "title".into(),
            value: "Test".into(),
        };
        db.insert_attribute(&attr).unwrap();

        db.remove_file(&path).unwrap();

        assert!(db.all_node_paths().unwrap().is_empty());
        assert!(db.incoming_edges(&NodePath::new("/test/other.md").unwrap()).unwrap().is_empty());
    }

    #[test]
    fn find_nodes_by_substring() {
        let db = Db::open_memory().unwrap();
        for name in &["fix-protocol.md", "fix-gateway.md", "routing.md"] {
            let file = WalkedFile {
                path: NodePath::new(format!("/notes/{}", name)).unwrap(),
                hash: "x".into(),
                size: 10,
                modified: 1.0,
                root: NodePath::new("/notes").unwrap(),
            };
            db.upsert_node(&file).unwrap();
        }

        let results = db.find_nodes("fix").unwrap();
        assert_eq!(results.len(), 2);
    }
}
