use crate::db::Db;
use crate::errors::Result;
use crate::types::{NodePath, WalkDiff};

impl Db {
    /// Append node-level changelog entries for a walk diff.
    pub fn append_changelog(&self, diff: &WalkDiff) -> Result<()> {
        let now = now_ts();
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO changelog (timestamp, action, kind, path_a, path_b)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for file in &diff.added {
            stmt.execute(rusqlite::params![
                now,
                "added",
                "node",
                file.path.as_str(),
                None::<String>
            ])?;
        }

        for path in &diff.removed {
            stmt.execute(rusqlite::params![
                now,
                "removed",
                "node",
                path.as_str(),
                None::<String>
            ])?;
        }

        Ok(())
    }

    /// Append edge-level changelog entries.
    #[allow(dead_code)]
    pub fn append_edge_changelog(
        &self,
        added_edges: &[(NodePath, NodePath)],
        removed_edges: &[(NodePath, NodePath)],
    ) -> Result<()> {
        let now = now_ts();
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO changelog (timestamp, action, kind, path_a, path_b)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for (from, to) in added_edges {
            stmt.execute(rusqlite::params![
                now,
                "added",
                "edge",
                from.as_str(),
                Some(to.as_str())
            ])?;
        }

        for (from, to) in removed_edges {
            stmt.execute(rusqlite::params![
                now,
                "removed",
                "edge",
                from.as_str(),
                Some(to.as_str())
            ])?;
        }

        Ok(())
    }
}

fn now_ts() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}
