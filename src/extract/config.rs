// @region extractor-config
//| Extractor definitions parameterize the extraction engine.
//| Each definition declares comment syntax for a file format —
//| nothing else. The engine uses this to decide where to scan
//| for [[links]], @region markers, and //| prose.
//|
//| Adding a new language means adding a definition here, not
//| writing new extraction code. The engine is one function;
//| these are its inputs.

/// An extractor definition describes how to parse a file format.
///
/// The extraction engine is one piece of code. These definitions
/// parameterize it. Adding a new language is a new definition,
/// not new code.
#[derive(Debug, Clone)]
pub struct ExtractorDef {
    pub name: String,
    pub patterns: Vec<String>,
    pub line_comment: Option<String>,
    pub block_comment: Option<(String, String)>,
    pub freetext: bool,
}

/// The built-in extractor definitions.
///
/// These are compiled into the binary. A future version could also
/// load from a YAML config file and merge with these defaults.
pub fn builtin_extractors() -> Vec<ExtractorDef> {
    vec![
        ExtractorDef {
            name: "markdown".into(),
            patterns: vec!["*.md".into()],
            line_comment: None,
            block_comment: None,
            freetext: true,
        },
        ExtractorDef {
            name: "c_family".into(),
            patterns: vec![
                "*.c".into(),
                "*.h".into(),
                "*.cpp".into(),
                "*.hpp".into(),
                "*.cc".into(),
                "*.hh".into(),
            ],
            line_comment: Some("//".into()),
            block_comment: Some(("/*".into(), "*/".into())),
            freetext: false,
        },
        ExtractorDef {
            name: "rust".into(),
            patterns: vec!["*.rs".into()],
            line_comment: Some("//".into()),
            block_comment: Some(("/*".into(), "*/".into())),
            freetext: false,
        },
        ExtractorDef {
            name: "python".into(),
            patterns: vec!["*.py".into()],
            line_comment: Some("#".into()),
            block_comment: None,
            freetext: false,
        },
        ExtractorDef {
            name: "supercollider".into(),
            patterns: vec!["*.scd".into(), "*.sc".into()],
            line_comment: Some("//".into()),
            block_comment: Some(("/*".into(), "*/".into())),
            freetext: false,
        },
        ExtractorDef {
            name: "shell".into(),
            patterns: vec!["*.sh".into(), "*.bash".into(), "*.zsh".into()],
            line_comment: Some("#".into()),
            block_comment: None,
            freetext: false,
        },
        ExtractorDef {
            name: "yaml".into(),
            patterns: vec!["*.yaml".into(), "*.yml".into()],
            line_comment: Some("#".into()),
            block_comment: None,
            freetext: false,
        },
        ExtractorDef {
            name: "lua".into(),
            patterns: vec!["*.lua".into()],
            line_comment: Some("--".into()),
            block_comment: Some(("--[[".into(), "]]".into())),
            freetext: false,
        },
        ExtractorDef {
            name: "javascript".into(),
            patterns: vec!["*.js".into(), "*.ts".into(), "*.jsx".into(), "*.tsx".into()],
            line_comment: Some("//".into()),
            block_comment: Some(("/*".into(), "*/".into())),
            freetext: false,
        },
    ]
}

/// Find the matching extractor for a file path based on extension.
/// Returns None if no extractor matches (file is a leaf).
pub fn find_extractor<'a>(
    path: &str,
    extractors: &'a [ExtractorDef],
) -> Option<&'a ExtractorDef> {
    let path_lower = path.to_lowercase();

    for ext in extractors {
        for pattern in &ext.patterns {
            // Simple glob: just check if the pattern's suffix matches.
            // Patterns are always "*.ext" so we match on the extension.
            let suffix = pattern.trim_start_matches('*');
            if path_lower.ends_with(suffix) {
                return Some(ext);
            }
        }
    }

    None
}
// @end extractor-config

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_markdown() {
        let extractors = builtin_extractors();
        let ext = find_extractor("/home/murphy/notes/fix.md", &extractors).unwrap();
        assert_eq!(ext.name, "markdown");
        assert!(ext.freetext);
    }

    #[test]
    fn find_cpp() {
        let extractors = builtin_extractors();
        let ext = find_extractor("/home/murphy/src/strategy.cpp", &extractors).unwrap();
        assert_eq!(ext.name, "c_family");
        assert!(!ext.freetext);
        assert_eq!(ext.line_comment.as_deref(), Some("//"));
    }

    #[test]
    fn find_python() {
        let extractors = builtin_extractors();
        let ext = find_extractor("/home/murphy/billing/transforms.py", &extractors).unwrap();
        assert_eq!(ext.name, "python");
    }

    #[test]
    fn find_supercollider() {
        let extractors = builtin_extractors();
        let ext = find_extractor("/home/murphy/music/ovalprocess/main.scd", &extractors).unwrap();
        assert_eq!(ext.name, "supercollider");
    }

    #[test]
    fn no_extractor_for_binary() {
        let extractors = builtin_extractors();
        assert!(find_extractor("/home/murphy/photo.jpg", &extractors).is_none());
    }

    #[test]
    fn no_extractor_for_unknown() {
        let extractors = builtin_extractors();
        assert!(find_extractor("/home/murphy/data.parquet", &extractors).is_none());
    }
}
