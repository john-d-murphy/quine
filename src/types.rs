use std::fmt;
use std::path::{Path, PathBuf};

// @region node-identity
//| Every entity in the system is a file. Identity is its absolute path.
//| NodePath is the core type — a newtype around PathBuf that guarantees
//| the path is absolute and tilde-expanded. This prevents accidental use
//| of raw, unvalidated paths where a node identity is expected.
//|
//| The type system enforces design principle #1: no file, no node.
//| If you can't construct a NodePath, the thing doesn't exist in the graph.

/// A validated, tilde-expanded absolute path identifying a node in the graph.
/// Newtype prevents accidental use of raw PathBuf where a node identity is expected.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodePath(PathBuf);

impl NodePath {
    /// Create a NodePath from a path string that may start with `~`.
    /// Returns None if the path is relative without `~` prefix.
    /// Use `from_cwd` for relative paths.
    pub fn new(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();
        let expanded = if let Ok(stripped) = path.strip_prefix("~") {
            let home = dirs_home()?;
            home.join(stripped)
        } else if path.is_absolute() {
            path.to_path_buf()
        } else {
            return None;
        };

        Some(NodePath(clean_path(&expanded)))
    }

    /// Create a NodePath from a potentially relative path by resolving
    /// against the current working directory. Handles `..` and `.` components.
    pub fn from_cwd(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();

        // Try tilde expansion first.
        if path.starts_with("~") {
            return Self::new(path);
        }

        // Already absolute.
        if path.is_absolute() {
            return Some(NodePath(clean_path(path)));
        }

        // Relative — resolve against cwd.
        let cwd = std::env::current_dir().ok()?;
        let resolved = cwd.join(path);
        Some(NodePath(clean_path(&resolved)))
    }

    /// Create a NodePath from a path already known to be absolute.
    /// Used when reading paths back from the DB.
    pub fn from_absolute(path: PathBuf) -> Option<Self> {
        if path.is_absolute() {
            Some(NodePath(path))
        } else {
            None
        }
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn to_path_buf(&self) -> PathBuf {
        self.0.clone()
    }

    /// Returns the path as a string for DB storage.
    pub fn as_str(&self) -> &str {
        self.0.to_str().expect("NodePath should be valid UTF-8")
    }

    /// Check if this path starts with the given prefix.
    pub fn starts_with(&self, prefix: &NodePath) -> bool {
        self.0.starts_with(&prefix.0)
    }

    /// Join a child component onto this path, returning a new NodePath.
    pub fn join(&self, child: impl AsRef<Path>) -> Self {
        NodePath(self.0.join(child))
    }
}

impl fmt::Display for NodePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}
// @end node-identity

// @region graph-primitives
//| The graph has three primitives: edges, regions, and attributes.
//| An edge is a [[link]] between two files. A region is a named span
//| of code with optional prose commentary. An attribute is a frontmatter
//| key-value pair. These three types, plus NodePath, are the complete
//| vocabulary of the IR.

/// An edge in the knowledge graph — a link from one file to another,
/// optionally targeting a named region via fragment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    pub source: NodePath,
    pub target: NodePath,
    pub fragment: Option<String>,
}

/// A named region within a source file, delimited by @region/@end markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub file: NodePath,
    pub name: String,
    pub start_line: u32,
    pub end_line: u32,
    pub prose: Vec<ProseBlock>,
}

/// A contiguous block of prose comments (//| lines) within a region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProseBlock {
    pub start_line: u32,
    pub content: String, // raw markdown
}

/// A frontmatter key-value pair extracted from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub file: NodePath,
    pub key: String,
    pub value: String, // always string, projectors interpret
}
// @end graph-primitives

// @region discovery-types
//| The discovery tree is formed by definition files (life.yaml).
//| A Root is a directory that claims its subtree. Refs are edges
//| in the discovery tree — they tell the walker where else to look.
//| This is separate from the knowledge graph: discovery is
//| infrastructure, the knowledge graph is the thing you query.

/// A root directory — a directory containing a life.yaml.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    pub path: NodePath,
    pub name: String,
    pub refs: Vec<NodePath>,
}

/// A file discovered during phase 1 walk.
#[derive(Debug, Clone)]
pub struct WalkedFile {
    pub path: NodePath,
    pub hash: String,
    pub size: u64,
    pub modified: f64,
    pub root: NodePath,
}

/// Output of phase 2 extraction for a single file.
#[derive(Debug, Clone, Default)]
pub struct Extracted {
    pub edges: Vec<Edge>,
    pub regions: Vec<Region>,
    pub attributes: Vec<Attribute>,
}

/// Diff between previous DB state and current walk.
#[derive(Debug, Default)]
pub struct WalkDiff {
    pub added: Vec<WalkedFile>,
    pub removed: Vec<NodePath>,
    pub changed: Vec<WalkedFile>,  // hash differs from DB
    pub unchanged: Vec<NodePath>,
}

/// The definition file (life.yaml) structure.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DefinitionFile {
    pub name: String,
    #[serde(default)]
    pub refs: Vec<RefEntry>,
}

/// A ref entry in life.yaml.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RefEntry {
    pub path: String,
}
// @end discovery-types

/// Clean a path by resolving `.` and `..` components without touching
/// the filesystem (no symlink resolution). This is a pure string operation.
fn clean_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            c => components.push(c),
        }
    }
    components.iter().collect()
}

/// Get the user's home directory.
fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodepath_rejects_relative() {
        assert!(NodePath::new("relative/path").is_none());
    }

    #[test]
    fn nodepath_accepts_absolute() {
        let np = NodePath::new("/absolute/path").unwrap();
        assert_eq!(np.as_str(), "/absolute/path");
    }

    #[test]
    fn nodepath_expands_tilde() {
        let np = NodePath::new("~/something").unwrap();
        assert!(np.as_path().is_absolute());
        assert!(!np.as_str().contains('~'));
    }

    #[test]
    fn nodepath_starts_with() {
        let parent = NodePath::new("/home/murphy/notes").unwrap();
        let child = NodePath::new("/home/murphy/notes/fix.md").unwrap();
        let other = NodePath::new("/home/murphy/src/foo.rs").unwrap();
        assert!(child.starts_with(&parent));
        assert!(!other.starts_with(&parent));
    }

    #[test]
    fn nodepath_from_cwd_resolves_relative() {
        let np = NodePath::from_cwd(".").unwrap();
        assert!(np.as_path().is_absolute());
    }

    #[test]
    fn nodepath_from_cwd_handles_absolute() {
        let np = NodePath::from_cwd("/absolute/path").unwrap();
        assert_eq!(np.as_str(), "/absolute/path");
    }

    #[test]
    fn nodepath_from_cwd_handles_tilde() {
        let np = NodePath::from_cwd("~/notes").unwrap();
        assert!(np.as_path().is_absolute());
        assert!(!np.as_str().contains('~'));
    }

    #[test]
    fn clean_path_resolves_dotdot() {
        let cleaned = clean_path(Path::new("/a/b/../c"));
        assert_eq!(cleaned, PathBuf::from("/a/c"));
    }

    #[test]
    fn clean_path_resolves_dot() {
        let cleaned = clean_path(Path::new("/a/./b/./c"));
        assert_eq!(cleaned, PathBuf::from("/a/b/c"));
    }

    #[test]
    fn nodepath_join() {
        let parent = NodePath::new("/home/murphy").unwrap();
        let child = parent.join("notes/file.md");
        assert_eq!(child.as_str(), "/home/murphy/notes/file.md");
    }
}
