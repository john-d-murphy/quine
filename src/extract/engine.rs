use crate::errors::LifeError;
use crate::extract::config::ExtractorDef;
use crate::extract::frontmatter;
use crate::extract::links::{self, RawLink};
use crate::extract::regions;
use crate::types::*;

// @region extraction-engine
//| This is the one extraction engine. It's parameterized by an
//| ExtractorDef — a declaration of comment syntax, not code.
//| Adding a new language means adding a definition (a few lines of
//| config), not changing this function.
//|
//| The engine runs three passes over the file:
//| 1. Link extraction: find [[path]] and [[path#fragment]]
//| 2. Region extraction: find @region/@end markers and //| prose
//| 3. Frontmatter extraction: parse YAML frontmatter (markdown only)
//|
//| In freetext mode (markdown), the entire content is scanned for
//| links. In comment mode (code), only comments are scanned. This
//| is the boundary between "what the human wrote for the graph" and
//| "what the compiler sees."

/// Run extraction on a single file's contents.
pub fn extract(
    file_path: &NodePath,
    content: &str,
    def: &ExtractorDef,
) -> Result<Extracted, LifeError> {
    let mut extracted = Extracted::default();

    // ---- Link extraction ----
    let raw_links = if def.freetext {
        links::extract_links(content)
    } else {
        let mut all_links = Vec::new();

        if let Some(ref prefix) = def.line_comment {
            all_links.extend(links::extract_links_from_comments(content, prefix));
        }

        if let Some((ref open, ref close)) = def.block_comment {
            all_links.extend(links::extract_links_from_block_comments(content, open, close));
        }

        dedup_links(all_links)
    };

    // Convert raw links to edges.
    for raw in raw_links {
        let target = if let Some(np) = NodePath::new(&raw.path) {
            np
        } else {
            continue; // Skip relative/invalid paths.
        };

        extracted.edges.push(Edge {
            source: file_path.clone(),
            target,
            fragment: raw.fragment,
        });
    }

    // ---- Region extraction ----
    let comment_prefix = def.line_comment.as_deref();
    extracted.regions = regions::extract_regions(file_path, content, comment_prefix)?;

    // ---- Frontmatter extraction (freetext/markdown only) ----
    if def.freetext {
        extracted.attributes = frontmatter::extract_frontmatter(file_path, content);
    }

    Ok(extracted)
}
// @end extraction-engine
/// Deduplicate links by (path, fragment, line).
fn dedup_links(mut links: Vec<RawLink>) -> Vec<RawLink> {
    links.sort_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(a.col.cmp(&b.col))
            .then(a.path.cmp(&b.path))
    });
    links.dedup_by(|a, b| a.path == b.path && a.fragment == b.fragment && a.line == b.line);
    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::config::ExtractorDef;

    fn markdown_def() -> ExtractorDef {
        ExtractorDef {
            name: "markdown".into(),
            patterns: vec!["*.md".into()],
            line_comment: None,
            block_comment: None,
            freetext: true,
        }
    }

    fn cpp_def() -> ExtractorDef {
        ExtractorDef {
            name: "c_family".into(),
            patterns: vec!["*.cpp".into()],
            line_comment: Some("//".into()),
            block_comment: Some(("/*".into(), "*/".into())),
            freetext: false,
        }
    }

    fn python_def() -> ExtractorDef {
        ExtractorDef {
            name: "python".into(),
            patterns: vec!["*.py".into()],
            line_comment: Some("#".into()),
            block_comment: None,
            freetext: false,
        }
    }

    #[test]
    fn markdown_extracts_all_links() {
        let file = NodePath::new("/notes/test.md").unwrap();
        let content = r#"
# My Note

See [[~/src/vesplatform/sor/strategy.cpp]] for the implementation.

```
This [[~/notes/other.md]] is in a code block but still a link.
```

Also related: [[~/music/supercollider/ovalprocess/main.scd#granular-voice]]
"#;
        let result = extract(&file, content, &markdown_def()).unwrap();
        assert_eq!(result.edges.len(), 3);
        assert_eq!(
            result.edges[0].target.as_str(),
            &*NodePath::new("~/src/vesplatform/sor/strategy.cpp")
                .unwrap()
                .as_str()
                .to_string()
        );
        assert_eq!(result.edges[1].target, NodePath::new("~/notes/other.md").unwrap());
        assert_eq!(
            result.edges[2].fragment,
            Some("granular-voice".to_string())
        );
    }

    #[test]
    fn cpp_only_extracts_from_comments() {
        let file = NodePath::new("/src/test.cpp").unwrap();
        let content = r#"
#include <iostream>

// See [[~/notes/fix-protocol.md]] for the FIX spec.
void handler() {
    auto s = "[[~/not/a/link.md]]"; // not in comment context
    // Related: [[~/src/other.cpp#session-handler]]
    /* Block comment with [[~/notes/design.md]] */
}
"#;
        let result = extract(&file, content, &cpp_def()).unwrap();
        assert_eq!(result.edges.len(), 3);

        let paths: Vec<&str> = result.edges.iter().map(|e| e.target.as_str()).collect();
        assert!(paths.iter().any(|p| p.ends_with("fix-protocol.md")));
        assert!(paths.iter().any(|p| p.ends_with("other.cpp")));
        assert!(paths.iter().any(|p| p.ends_with("design.md")));
        // The string literal link should NOT be extracted.
        assert!(!paths.iter().any(|p| p.ends_with("not/a/link.md")));
    }

    #[test]
    fn python_only_extracts_from_hash_comments() {
        let file = NodePath::new("/src/test.py").unwrap();
        let content = r#"
# See [[~/notes/billing.md]] for the fee schedule.
def calculate():
    s = "[[~/not/a/link.md]]"
    # Also [[~/src/transforms.py#fee-calculation]]
    pass
"#;
        let result = extract(&file, content, &python_def()).unwrap();
        assert_eq!(result.edges.len(), 2);
        assert!(result.edges[0]
            .target
            .as_str()
            .ends_with("billing.md"));
        assert_eq!(
            result.edges[1].fragment,
            Some("fee-calculation".to_string())
        );
    }

    #[test]
    fn relative_links_are_skipped() {
        let file = NodePath::new("/notes/test.md").unwrap();
        let content = "See [[relative/path.md]] — this is not valid.";
        let result = extract(&file, content, &markdown_def()).unwrap();
        assert_eq!(result.edges.len(), 0);
    }

    #[test]
    fn source_is_always_the_file() {
        let file = NodePath::new("/notes/test.md").unwrap();
        let content = "[[~/notes/a.md]] and [[~/notes/b.md]]";
        let result = extract(&file, content, &markdown_def()).unwrap();
        for edge in &result.edges {
            assert_eq!(edge.source, file);
        }
    }

    #[test]
    fn no_links_returns_empty() {
        let file = NodePath::new("/src/boring.cpp").unwrap();
        let content = "int main() { return 0; }";
        let result = extract(&file, content, &cpp_def()).unwrap();
        assert!(result.edges.is_empty());
    }

    #[test]
    fn trailing_line_comment_extracted() {
        // A line like `int x = 5; // see [[~/link.md]]` should work
        // because we find the comment prefix anywhere in the line.
        let file = NodePath::new("/src/test.cpp").unwrap();
        let content = "int x = 5; // see [[~/notes/fix.md]]";
        let result = extract(&file, content, &cpp_def()).unwrap();
        assert_eq!(result.edges.len(), 1);
        assert!(result.edges[0].target.as_str().ends_with("fix.md"));
    }
}
