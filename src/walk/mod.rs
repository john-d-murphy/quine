use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use crate::errors::{QuineError, QuineWarning, Result};
use crate::types::*;

pub const DEFINITION_FILE: &str = "quine.yaml";
pub const STOP_FILE: &str = ".quine-stop";

/// Result of walking from a seed.
#[derive(Debug, Default)]
pub struct WalkResult {
    pub files: Vec<WalkedFile>,
    pub roots: Vec<Root>,
    pub warnings: Vec<QuineWarning>,
}

// @region walk-seed
//| Collection starts here. The seed is a single directory path — the
//| CLI argument to `quine collect`. The walker reads its quine.yaml,
//| walks the subtree, follows refs to other roots, and recurses.
//| This is phase 1: walk, hash, diff. Same operation everywhere,
//| regardless of what's in the files.

/// Walk from a seed directory, discovering all roots and files.
/// The seed path can be relative — it will be resolved against cwd.
pub fn walk_seed(seed: &Path) -> Result<WalkResult> {
    let seed_path = NodePath::from_cwd(seed).ok_or_else(|| {
        QuineError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("cannot resolve seed path: {}", seed.display()),
        ))
    })?;

    let def_file = seed_path.as_path().join(DEFINITION_FILE);
    if !def_file.exists() {
        return Err(QuineError::NoSeed { path: seed_path });
    }

    let mut result = WalkResult::default();
    let mut seen_inodes: HashSet<u64> = HashSet::new();
    let mut visited_roots: HashSet<NodePath> = HashSet::new();

    walk_root(&seed_path, &mut result, &mut seen_inodes, &mut visited_roots)?;

    Ok(result)
}
// @end walk-seed

// @region walk-root
//| Each root is a directory with a quine.yaml. The walker reads the
//| definition file, registers the root, walks its subtree for files,
//| then follows refs to discover other roots. Refs are discovery,
//| not permission — they expand the scope of what's known.
//|
//| Cycle detection uses a visited set of NodePaths. If the discovery
//| tree has A -> B -> A, the second visit to A is a no-op.

/// Recursively walk a root directory.
fn walk_root(
    root_path: &NodePath,
    result: &mut WalkResult,
    seen_inodes: &mut HashSet<u64>,
    visited_roots: &mut HashSet<NodePath>,
) -> Result<()> {
    // Prevent cycles in the discovery tree.
    if visited_roots.contains(root_path) {
        return Ok(());
    }
    visited_roots.insert(root_path.clone());

    // Parse the definition file.
    let def_file_path = root_path.as_path().join(DEFINITION_FILE);
    let def_content = fs::read_to_string(&def_file_path).map_err(|e| {
        QuineError::Io(std::io::Error::new(
            e.kind(),
            format!("reading {}: {}", def_file_path.display(), e),
        ))
    })?;
    let def: DefinitionFile = serde_yaml::from_str(&def_content).map_err(|e| {
        QuineError::YamlParse {
            path: def_file_path.display().to_string(),
            msg: e.to_string(),
        }
    })?;

    // Build the Root, expanding ~ in ref paths.
    let refs: Vec<NodePath> = def
        .refs
        .iter()
        .filter_map(|r| NodePath::new(&r.path))
        .collect();

    let root = Root {
        path: root_path.clone(),
        name: def.name,
        refs: refs.clone(),
    };
    result.roots.push(root);

    // Walk the subtree.
    walk_subtree(root_path, root_path, &def.exclude, result, seen_inodes, visited_roots)?;

    // Follow refs to other roots.
    for ref_path in &refs {
        let ref_dir = ref_path.as_path();
        if !ref_dir.exists() {
            result.warnings.push(QuineWarning::DanglingRef {
                from: root_path.clone(),
                to: ref_path.clone(),
            });
            continue;
        }

        let ref_def = ref_dir.join(DEFINITION_FILE);
        if ref_def.exists() {
            walk_root(ref_path, result, seen_inodes, visited_roots)?;
        } else {
            result.warnings.push(QuineWarning::DanglingRef {
                from: root_path.clone(),
                to: ref_path.clone(),
            });
        }
    }

    Ok(())
}
// @end walk-root

// @region walk-subtree
//| The subtree walker is where boundaries are enforced. Three
//| mechanisms prevent descending into unwanted directories:
//|
//| 1. .git is hardcoded — never meaningful to walk.
//| 2. Directories listed in quine.yaml `exclude` are skipped.
//| 3. .quine-stop sentinel files halt descent.
//|
//| When the walker finds a quine.yaml in a subdirectory, that
//| directory is a separate root with its own identity. The walker
//| delegates to walk_root rather than descending — the sub-root
//| owns its own subtree.
//|
//| Inode tracking prevents symlink cycles: if a directory's inode
//| has been seen before, the walker stops. Files are hashed with
//| SHA-256 for content-addressed diffing.

/// Walk a subtree, registering all files. Respects .quine-stop and
/// sub-roots (quine.yaml in subdirectories).
fn walk_subtree(
    dir: &NodePath,
    owning_root: &NodePath,
    exclude: &[String],
    result: &mut WalkResult,
    seen_inodes: &mut HashSet<u64>,
    visited_roots: &mut HashSet<NodePath>,
) -> Result<()> {
    let dir_path = dir.as_path();

    let entries = match fs::read_dir(dir_path) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("warning: cannot read {}: {}", dir, e);
            return Ok(());
        }
    };

    // Collect and sort entries for deterministic ordering.
    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let entry_path = entry.path();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if metadata.is_dir() {
            let dir_name = entry.file_name();
            let dir_name_str = dir_name.to_string_lossy();

            // Always skip .git — never meaningful to walk.
            if dir_name_str == ".git" {
                continue;
            }

            // Skip directories listed in exclude.
            if exclude.iter().any(|e| e == dir_name_str.as_ref()) {
                continue;
            }

            // Inode cycle detection for directories.
            let inode = metadata.ino();
            if !seen_inodes.insert(inode) {
                continue;
            }

            // Check for .quine-stop — halt, don't descend.
            if entry_path.join(STOP_FILE).exists() {
                continue;
            }

            // Check for sub-root (quine.yaml in this directory).
            if entry_path.join(DEFINITION_FILE).exists() {
                if let Some(sub_root) = NodePath::new(&entry_path) {
                    if !visited_roots.contains(&sub_root) {
                        walk_root(&sub_root, result, seen_inodes, visited_roots)?;
                    }
                }
                // Don't descend — the sub-root owns this subtree.
                continue;
            }

            // Regular directory — descend.
            if let Some(sub_dir) = NodePath::new(&entry_path) {
                walk_subtree(&sub_dir, owning_root, exclude, result, seen_inodes, visited_roots)?;
            }
        } else if metadata.is_file() {
            if let Some(file_path) = NodePath::new(&entry_path) {
                let hash = hash_file(&entry_path)?;
                let walked = WalkedFile {
                    path: file_path,
                    hash,
                    size: metadata.len(),
                    modified: modified_time(&metadata),
                    root: owning_root.clone(),
                };
                result.files.push(walked);
            }
        }
        // Symlinks: entry.metadata() follows symlinks. The inode check
        // on directories prevents cycles. Symlinked files are followed
        // and registered normally.
    }

    Ok(())
}
// @end walk-subtree

/// Hash a file's contents using SHA-256.
fn hash_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let content = fs::read(path)?;
    let hash = Sha256::digest(&content);
    Ok(format!("{:x}", hash))
}

/// Extract modification time as f64 seconds since epoch.
fn modified_time(metadata: &fs::Metadata) -> f64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

// @region walk-diff
//| The diff function is the bridge between phase 1 (walk) and the DB.
//| It takes a hash map of previously-known paths and hashes, compares
//| against the current walk, and classifies every file as added,
//| removed, changed, or unchanged — all in a single pass.
//|
//| This is the make analogy: content hashes are timestamps, and a
//| changed hash means the file needs re-extraction. The DB is rebuilt
//| incrementally, not from scratch, because the diff tells us exactly
//| what to re-process.

/// Compute the diff between previous DB state and current walk.
///
/// `previous_hashes` maps path -> hash from the DB. This lets diff
/// determine added/removed/changed/unchanged in one pass without
/// needing DB access.
pub fn diff(previous_hashes: &HashMap<String, String>, current: &[WalkedFile]) -> WalkDiff {
    let mut diff = WalkDiff::default();

    let mut seen_current: HashSet<&str> = HashSet::with_capacity(current.len());

    for file in current {
        let path_str = file.path.as_str();
        seen_current.insert(path_str);

        match previous_hashes.get(path_str) {
            None => {
                // New file.
                diff.added.push(file.clone());
            }
            Some(old_hash) => {
                if old_hash == &file.hash {
                    diff.unchanged.push(file.path.clone());
                } else {
                    diff.changed.push(file.clone());
                }
            }
        }
    }

    // Removed: in previous but not in current.
    for prev_path in previous_hashes.keys() {
        if !seen_current.contains(prev_path.as_str()) {
            if let Some(np) = NodePath::from_absolute(prev_path.into()) {
                diff.removed.push(np);
            }
        }
    }

    diff
}
// @end walk-diff

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_test_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();

        fs::write(
            dir.path().join("quine.yaml"),
            "name: \"test-root\"\nrefs: []\n",
        )
        .unwrap();

        fs::write(dir.path().join("hello.md"), "# Hello\n\nWorld.\n").unwrap();
        fs::write(dir.path().join("notes.md"), "Some notes.\n").unwrap();

        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/deep.md"), "Deep file.\n").unwrap();

        dir
    }

    #[test]
    fn walk_discovers_files() {
        let dir = setup_test_dir();
        let result = walk_seed(dir.path()).unwrap();

        assert_eq!(result.roots.len(), 1);
        assert_eq!(result.roots[0].name, "test-root");

        let names: Vec<&str> = result
            .files
            .iter()
            .map(|f| f.path.as_path().file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"quine.yaml"));
        assert!(names.contains(&"hello.md"));
        assert!(names.contains(&"notes.md"));
        assert!(names.contains(&"deep.md"));
        assert_eq!(result.files.len(), 4);
    }

    #[test]
    fn walk_respects_quine_stop() {
        let dir = setup_test_dir();
        fs::write(dir.path().join("sub/.quine-stop"), "").unwrap();

        let result = walk_seed(dir.path()).unwrap();

        let names: Vec<&str> = result
            .files
            .iter()
            .map(|f| f.path.as_path().file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(!names.contains(&"deep.md"));
    }

    #[test]
    fn walk_fails_without_seed() {
        let dir = tempfile::tempdir().unwrap();
        let result = walk_seed(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn walk_warns_on_dangling_ref() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("quine.yaml"),
            "name: \"test\"\nrefs:\n  - path: /nonexistent/path\n",
        )
        .unwrap();

        let result = walk_seed(dir.path()).unwrap();
        assert_eq!(result.warnings.len(), 1);
        assert!(matches!(
            &result.warnings[0],
            QuineWarning::DanglingRef { .. }
        ));
    }

    #[test]
    fn walk_discovers_sub_roots() {
        let dir = setup_test_dir();

        fs::create_dir(dir.path().join("subroot")).unwrap();
        fs::write(
            dir.path().join("subroot/quine.yaml"),
            "name: \"sub-root\"\nrefs: []\n",
        )
        .unwrap();
        fs::write(dir.path().join("subroot/file.md"), "Sub-root file.\n").unwrap();

        let result = walk_seed(dir.path()).unwrap();

        assert_eq!(result.roots.len(), 2);
        let names: Vec<&str> = result.roots.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"test-root"));
        assert!(names.contains(&"sub-root"));
    }

    #[test]
    fn walk_sub_root_files_owned_by_sub_root() {
        let dir = setup_test_dir();

        fs::create_dir(dir.path().join("subroot")).unwrap();
        fs::write(
            dir.path().join("subroot/quine.yaml"),
            "name: \"sub-root\"\nrefs: []\n",
        )
        .unwrap();
        fs::write(dir.path().join("subroot/file.md"), "Sub-root file.\n").unwrap();

        let result = walk_seed(dir.path()).unwrap();

        // Find the subroot file and check its owning root.
        let sub_file = result
            .files
            .iter()
            .find(|f| f.path.as_str().ends_with("subroot/file.md"))
            .unwrap();
        assert!(sub_file.root.as_str().ends_with("subroot"));
    }

    #[test]
    fn diff_with_hashes() {
        let mut prev = HashMap::new();
        prev.insert("/a".to_string(), "hash_a".to_string());
        prev.insert("/b".to_string(), "hash_b".to_string());
        prev.insert("/c".to_string(), "hash_c_old".to_string());

        let current = vec![
            WalkedFile {
                path: NodePath::new("/b").unwrap(),
                hash: "hash_b".into(), // unchanged
                size: 10,
                modified: 1.0,
                root: NodePath::new("/").unwrap(),
            },
            WalkedFile {
                path: NodePath::new("/c").unwrap(),
                hash: "hash_c_new".into(), // changed
                size: 20,
                modified: 2.0,
                root: NodePath::new("/").unwrap(),
            },
            WalkedFile {
                path: NodePath::new("/d").unwrap(),
                hash: "hash_d".into(), // added
                size: 30,
                modified: 3.0,
                root: NodePath::new("/").unwrap(),
            },
        ];

        let d = diff(&prev, &current);
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.added[0].path.as_str(), "/d");
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.removed[0].as_str(), "/a");
        assert_eq!(d.changed.len(), 1);
        assert_eq!(d.changed[0].path.as_str(), "/c");
        assert_eq!(d.unchanged.len(), 1);
        assert_eq!(d.unchanged[0].as_str(), "/b");
    }

    #[test]
    fn walk_handles_refs_between_roots() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();

        // dir2 is a separate root.
        fs::write(
            dir2.path().join("quine.yaml"),
            "name: \"external\"\nrefs: []\n",
        )
        .unwrap();
        fs::write(dir2.path().join("data.md"), "External data.\n").unwrap();

        // dir1 refs dir2.
        let ref_path = dir2.path().to_str().unwrap();
        fs::write(
            dir1.path().join("quine.yaml"),
            format!("name: \"main\"\nrefs:\n  - path: {}\n", ref_path),
        )
        .unwrap();
        fs::write(dir1.path().join("local.md"), "Local file.\n").unwrap();

        let result = walk_seed(dir1.path()).unwrap();

        assert_eq!(result.roots.len(), 2);
        let names: Vec<&str> = result.roots.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"main"));
        assert!(names.contains(&"external"));

        // Should have files from both roots.
        let has_local = result.files.iter().any(|f| f.path.as_str().ends_with("local.md"));
        let has_data = result.files.iter().any(|f| f.path.as_str().ends_with("data.md"));
        assert!(has_local);
        assert!(has_data);
    }

    #[test]
    fn walk_skips_dot_git() {
        let dir = setup_test_dir();

        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();

        let result = walk_seed(dir.path()).unwrap();

        let names: Vec<&str> = result
            .files
            .iter()
            .map(|f| f.path.as_path().file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(!names.contains(&"HEAD"));
    }

    #[test]
    fn walk_respects_exclude() {
        let dir = tempfile::tempdir().unwrap();

        fs::write(
            dir.path().join("quine.yaml"),
            "name: \"test\"\nrefs: []\nexclude:\n  - build\n",
        )
        .unwrap();

        fs::write(dir.path().join("hello.md"), "# Hello\n").unwrap();

        fs::create_dir(dir.path().join("build")).unwrap();
        fs::write(dir.path().join("build/output.bin"), "binary stuff").unwrap();

        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

        let result = walk_seed(dir.path()).unwrap();

        let names: Vec<&str> = result
            .files
            .iter()
            .map(|f| f.path.as_path().file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"hello.md"));
        assert!(names.contains(&"main.rs"));
        assert!(!names.contains(&"output.bin"));
    }
}
