use std::path::Path;

use crate::db::Db;
use crate::errors::Result;

/// Search the graph for nodes matching a query string.
/// Prints matching paths to stdout, one per line.
/// Designed for piping to fzf.
pub fn run(query: &str, db_path: &Path) -> Result<()> {
    let db = Db::open(db_path)?;
    let results = db.find_nodes(query)?;

    for path in results {
        println!("{}", path);
    }

    Ok(())
}
