use std::path::Path;

use crate::db::Db;
use crate::errors::Result;
use crate::extract;
use crate::walk;

// @region collect-pipeline
//| The collect command is the full pipeline: phase 1 (walk), diff,
//| phase 2 (extract), and DB update — all in one transaction.
//|
//| This is the compiler in action: files on disk are lexed (walked
//| and hashed), parsed (links, regions, and frontmatter extracted),
//| and stored in the IR (SQLite). The diff step makes it incremental
//| — only added and changed files are re-extracted.
//|
//| The pipeline is the same every time, for every directory, regardless
//| of what's in the files. The extractors handle format differences.
//| The pipeline just orchestrates.

/// Report from a collect run.
#[derive(Debug)]
pub struct CollectReport {
    pub roots_discovered: usize,
    pub files_added: usize,
    pub files_removed: usize,
    pub files_changed: usize,
    pub files_unchanged: usize,
    pub edges_added: usize,
    pub warnings: usize,
    pub broken_edges: usize,
}

/// Run the collect pipeline: walk from seed, diff, extract, store.
pub fn run(seed: &Path, db_path: &Path) -> Result<CollectReport> {
    let db = Db::open(db_path)?;

    // ---- Phase 1: Walk ----
    let walk_result = walk::walk_seed(seed)?;

    let mut total_warnings = walk_result.warnings.len();
    for w in &walk_result.warnings {
        eprintln!("warning: {}", w);
    }

    // ---- Diff ----
    let previous_hashes = db.all_node_hashes()?;
    let previously_resolved = db.resolved_edge_targets()?;
    let previous_roots = db.all_root_paths()?;
    let diff = walk::diff(&previous_hashes, &walk_result.files);

    // Check for broken edges.
    let mut broken_count = 0;
    for removed_path in &diff.removed {
        if previously_resolved.contains(removed_path.as_str()) {
            eprintln!(
                "error: BROKEN — {} was removed but has incoming edges",
                removed_path
            );
            broken_count += 1;
        }
    }

    // ---- Phase 2: Extract (added + changed files only) ----
    let files_to_extract: Vec<_> = diff
        .added
        .iter()
        .chain(diff.changed.iter())
        .cloned()
        .collect();

    let extraction = extract::run(&files_to_extract)?;
    total_warnings += extraction.warnings.len();
    for w in &extraction.warnings {
        eprintln!("warning: {}", w);
    }

    // Flatten all extracted data.
    let mut all_edges = Vec::new();
    let mut all_regions = Vec::new();
    let mut all_attributes = Vec::new();
    for ext in &extraction.extracted {
        all_edges.extend(ext.edges.iter().cloned());
        all_regions.extend(ext.regions.iter().cloned());
        all_attributes.extend(ext.attributes.iter().cloned());
    }
    let edges_added = all_edges.len();

    // ---- Write to DB ----
    db.transaction(|| {
        // Remove deleted files (node + edges + regions + attributes).
        for path in &diff.removed {
            db.remove_file(path)?;
        }

        // Re-insert changed files (remove old data, insert new).
        for file in &diff.changed {
            db.remove_file(&file.path)?;
            db.upsert_node(file)?;
        }

        // Insert new files.
        for file in &diff.added {
            db.upsert_node(file)?;
        }

        // Insert edges from extraction.
        for edge in &all_edges {
            db.insert_edge(edge)?;
        }

        // Insert regions from extraction.
        for region in &all_regions {
            db.insert_region(region)?;
        }

        // Insert attributes from extraction.
        for attr in &all_attributes {
            db.insert_attribute(attr)?;
        }

        // Update roots.
        let current_root_paths: std::collections::HashSet<String> = walk_result
            .roots
            .iter()
            .map(|r| r.path.as_str().to_string())
            .collect();

        for root in &walk_result.roots {
            db.upsert_root(root)?;
        }

        // Remove stale roots.
        for old_root in &previous_roots {
            if !current_root_paths.contains(old_root) {
                db.remove_root(old_root)?;
            }
        }

        // Append to changelog.
        db.append_changelog(&diff)?;

        Ok(())
    })?;

    Ok(CollectReport {
        roots_discovered: walk_result.roots.len(),
        files_added: diff.added.len(),
        files_removed: diff.removed.len(),
        files_changed: diff.changed.len(),
        files_unchanged: diff.unchanged.len(),
        edges_added,
        warnings: total_warnings,
        broken_edges: broken_count,
    })
}
// @end collect-pipeline
