use crate::types::NodePath;
use thiserror::Error;

// @region severity-model
//| The error system encodes the design doc's severity model as types.
//| Hard errors (LifeError) stop collection — they represent violations
//| of design rules: nested regions, duplicate region names, missing
//| required keys, broken references.
//|
//| Soft warnings (LifeWarning) let collection continue — they represent
//| expected conditions: dangling edges (target not yet created),
//| dangling refs (directory not yet created), files with no extractor
//| (binary leaves).
//|
//| The key distinction: a DanglingEdge (target never existed) is a
//| forward reference and gets a warning. A BrokenEdge (target previously
//| existed and was removed) means something broke and gets an error.
//| The diff step distinguishes them by comparing against the DB.

/// Hard errors — collection stops.
#[derive(Debug, Error)]
pub enum LifeError {
    #[error("nested region in {file}: '{inner}' inside '{outer}'")]
    NestedRegion {
        file: NodePath,
        outer: String,
        inner: String,
    },

    #[error("duplicate region name '{name}' in {file}")]
    DuplicateRegion { file: NodePath, name: String },

    #[error("missing required key '{key}' in {file} (lens: {lens})")]
    MissingRequiredKey {
        file: NodePath,
        lens: String,
        key: String,
    },

    #[error("broken edge: {from} -> {to} (target previously existed)")]
    BrokenEdge { from: NodePath, to: NodePath },

    #[error("broken ref: {from} -> {to} (target previously existed)")]
    BrokenRef { from: NodePath, to: NodePath },

    #[error("no life.yaml found at seed: {path}")]
    NoSeed { path: NodePath },

    #[error("{count} broken references detected")]
    BrokenReferences { count: usize },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error in {path}: {msg}")]
    YamlParse { path: String, msg: String },

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

impl From<serde_yaml::Error> for LifeError {
    fn from(e: serde_yaml::Error) -> Self {
        LifeError::YamlParse {
            path: String::new(),
            msg: e.to_string(),
        }
    }
}

/// Soft warnings — collection continues.
#[derive(Debug)]
pub enum LifeWarning {
    /// Edge target has never existed in the graph.
    DanglingEdge { from: NodePath, to: NodePath },

    /// Ref target directory does not exist or has no life.yaml.
    DanglingRef { from: NodePath, to: NodePath },

    /// No extractor matched this file; it's a leaf node.
    NoExtractor { file: NodePath },

    /// A previously-resolved edge target has been removed.
    BrokenEdge { from: NodePath, to: NodePath },
}

impl std::fmt::Display for LifeWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LifeWarning::DanglingEdge { from, to } => {
                write!(f, "dangling edge: {} -> {} (target not found)", from, to)
            }
            LifeWarning::DanglingRef { from, to } => {
                write!(f, "dangling ref: {} -> {} (directory not found)", from, to)
            }
            LifeWarning::NoExtractor { file } => {
                write!(f, "no extractor for {} (treated as leaf)", file)
            }
            LifeWarning::BrokenEdge { from, to } => {
                write!(
                    f,
                    "BROKEN: {} -> {} (target was removed without life mv)",
                    from, to
                )
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, LifeError>;
// @end severity-model
