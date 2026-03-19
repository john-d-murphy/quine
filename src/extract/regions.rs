use crate::errors::LifeError;
use crate::types::{NodePath, ProseBlock, Region};

// @region region-parser
//| This is the region parser. It scans code files for @region/@end
//| markers and extracts prose blocks (lines starting with |).
//|
//| The self-referential moment: this file itself contains @region
//| markers. When `life collect` processes the life source code,
//| this function parses its own annotations. The quine eats its
//| own tail.
//|
//| Design rules enforced here:
//| - Regions cannot nest (NestedRegion error).
//| - Region names must be unique per file (DuplicateRegion error).
//| - Prose blocks are contiguous runs of //| lines. A regular
//|   comment or code line breaks the prose continuity, starting
//|   a new block.

/// Extract regions and prose from file contents.
///
/// Scans for `@region <name>` and `@end <name>` markers in comments.
/// Within a region, lines matching `<comment_prefix>|` are collected
/// as prose blocks.
///
/// Errors:
/// - Nested regions: `@region a` inside an open `@region b` → error.
/// - Duplicate names: two `@region foo` in the same file → error.
///
/// The `comment_prefix` determines what constitutes a marker line.
/// In freetext mode (markdown), there's no comment prefix — regions
/// are not supported (markdown files don't have comment syntax for
/// @region markers). Returns empty vec for freetext.
pub fn extract_regions(
    file_path: &NodePath,
    content: &str,
    comment_prefix: Option<&str>,
) -> Result<Vec<Region>, LifeError> {
    let prefix = match comment_prefix {
        Some(p) => p,
        None => return Ok(Vec::new()), // Freetext (markdown) — no regions.
    };

    let mut regions = Vec::new();
    let mut open: Option<OpenRegion> = None;
    let mut seen_names = std::collections::HashSet::new();

    for (line_idx, line) in content.lines().enumerate() {
        let line_num = (line_idx + 1) as u32;
        let trimmed = line.trim();

        // Check if this line is a comment.
        let comment_body = extract_comment_body(trimmed, prefix);

        if let Some(body) = comment_body {
            let body = body.trim();

            // Check for @region <name>
            if let Some(name) = parse_region_start(body) {
                // Error: nested region.
                if let Some(ref current) = open {
                    return Err(LifeError::NestedRegion {
                        file: file_path.clone(),
                        outer: current.name.clone(),
                        inner: name,
                    });
                }

                // Error: duplicate name.
                if seen_names.contains(&name) {
                    return Err(LifeError::DuplicateRegion {
                        file: file_path.clone(),
                        name,
                    });
                }

                seen_names.insert(name.clone());
                open = Some(OpenRegion {
                    name,
                    start_line: line_num,
                    prose_blocks: Vec::new(),
                    current_prose: None,
                });
                continue;
            }

            // Check for @end <name>
            if let Some(end_name) = parse_region_end(body) {
                if let Some(mut current) = open.take() {
                    // Flush any open prose block.
                    if let Some(prose) = current.current_prose.take() {
                        current.prose_blocks.push(prose);
                    }

                    // Verify the @end name matches the @region name.
                    // If it doesn't, we still close — but this could
                    // be a future warning.
                    let _ = end_name; // used for matching check if needed

                    regions.push(Region {
                        file: file_path.clone(),
                        name: current.name,
                        start_line: current.start_line,
                        end_line: line_num,
                        prose: current.prose_blocks,
                    });
                }
                // If no open region, @end is a no-op (orphaned end marker).
                continue;
            }

            // Check for prose marker: <prefix>| (pipe after comment prefix).
            if let Some(prose_text) = parse_prose_line(body) {
                if let Some(ref mut current) = open {
                    match current.current_prose {
                        Some(ref mut block) => {
                            // Continue existing prose block.
                            block.content.push('\n');
                            block.content.push_str(prose_text);
                        }
                        None => {
                            // Start new prose block.
                            current.current_prose = Some(ProseBlock {
                                start_line: line_num,
                                content: prose_text.to_string(),
                            });
                        }
                    }
                }
                continue;
            }

            // Regular comment inside a region — break prose continuity.
            if let Some(ref mut current) = open {
                if let Some(prose) = current.current_prose.take() {
                    current.prose_blocks.push(prose);
                }
            }
        } else {
            // Non-comment line inside a region — break prose continuity.
            if let Some(ref mut current) = open {
                if let Some(prose) = current.current_prose.take() {
                    current.prose_blocks.push(prose);
                }
            }
        }
    }

    // If a region was never closed, we still include it (open-ended).
    // The end_line will be the last line of the file.
    if let Some(mut current) = open.take() {
        if let Some(prose) = current.current_prose.take() {
            current.prose_blocks.push(prose);
        }
        let last_line = content.lines().count() as u32;
        regions.push(Region {
            file: file_path.clone(),
            name: current.name,
            start_line: current.start_line,
            end_line: last_line,
            prose: current.prose_blocks,
        });
    }

    Ok(regions)
}

struct OpenRegion {
    name: String,
    start_line: u32,
    prose_blocks: Vec<ProseBlock>,
    current_prose: Option<ProseBlock>,
}

/// Extract the comment body from a line, given a prefix.
/// Returns None if the line doesn't start with the prefix (after trimming).
fn extract_comment_body<'a>(trimmed: &'a str, prefix: &str) -> Option<&'a str> {
    if trimmed.starts_with(prefix) {
        Some(&trimmed[prefix.len()..])
    } else {
        None
    }
}

/// Parse `@region <name>` from comment body text.
fn parse_region_start(body: &str) -> Option<String> {
    let body = body.trim();
    if let Some(rest) = body.strip_prefix("@region") {
        let name = rest.trim();
        if !name.is_empty() {
            Some(name.to_string())
        } else {
            None
        }
    } else {
        None
    }
}

/// Parse `@end <name>` from comment body text.
fn parse_region_end(body: &str) -> Option<String> {
    let body = body.trim();
    if let Some(rest) = body.strip_prefix("@end") {
        let name = rest.trim();
        if !name.is_empty() {
            Some(name.to_string())
        } else {
            None
        }
    } else {
        None
    }
}

/// Parse a prose marker line. The convention is `|` immediately after
/// the comment prefix. Returns the prose text (after the pipe).
fn parse_prose_line(body: &str) -> Option<&str> {
    if body.starts_with('|') {
        Some(body[1..].trim_start_matches(' '))
    } else {
        None
    }
}
// @end region-parser

#[cfg(test)]
mod tests {
    use super::*;

    fn np(s: &str) -> NodePath {
        NodePath::new(s).unwrap()
    }

    /// Build test content from lines. This avoids raw strings where
    /// "// @region" on its own line would be picked up by the extractor
    /// when it indexes this file (the quine bootstrap problem).
    fn tc(lines: &[&str]) -> String {
        lines.join("\n")
    }

    #[test]
    fn simple_region() {
        let content = tc(&[
            "",
            "// @region session-handler",
            "void onLogon() {}",
            "// @end session-handler",
            "",
        ]);
        let regions = extract_regions(&np("/test.cpp"), &content, Some("//")).unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].name, "session-handler");
        assert_eq!(regions[0].start_line, 2);
        assert_eq!(regions[0].end_line, 4);
        assert!(regions[0].prose.is_empty());
    }

    #[test]
    fn region_with_prose() {
        let content = tc(&[
            "// @region pmm-routing",
            "//| The PMM routing strategy cycles through eligible market makers",
            "//| in round-robin order.",
            "void routePMM() {",
            "    // grab the cycle",
            "    auto& cycle = getCycle();",
            "    //| When unavailable, skip to the next.",
            "    auto pmm = cycle.next();",
            "}",
            "// @end pmm-routing",
        ]);
        let regions = extract_regions(&np("/test.cpp"), &content, Some("//")).unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].name, "pmm-routing");

        // Two prose blocks: one before the code, one inline.
        assert_eq!(regions[0].prose.len(), 2);
        assert!(regions[0].prose[0]
            .content
            .contains("cycles through eligible market makers"));
        assert!(regions[0].prose[1].content.contains("When unavailable"));
    }

    #[test]
    fn python_region() {
        let content = tc(&[
            "",
            "# @region fee-calculation",
            "#| Fee calculation follows a most-specific-match rule.",
            "def calculate_fees(execution):",
            "    # look up the fee schedule",
            "    schedule = find_schedule(execution)",
            "# @end fee-calculation",
            "",
        ]);
        let regions = extract_regions(&np("/test.py"), &content, Some("#")).unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].name, "fee-calculation");
        assert_eq!(regions[0].prose.len(), 1);
        assert!(regions[0].prose[0]
            .content
            .contains("most-specific-match"));
    }

    #[test]
    fn nested_region_errors() {
        let content = tc(&[
            "",
            "// @region outer",
            "// @region inner",
            "// @end inner",
            "// @end outer",
            "",
        ]);
        let result = extract_regions(&np("/test.cpp"), &content, Some("//"));
        assert!(result.is_err());
        match result.unwrap_err() {
            LifeError::NestedRegion { outer, inner, .. } => {
                assert_eq!(outer, "outer");
                assert_eq!(inner, "inner");
            }
            e => panic!("expected NestedRegion, got {:?}", e),
        }
    }

    #[test]
    fn duplicate_region_name_errors() {
        let content = tc(&[
            "",
            "// @region handler",
            "void a() {}",
            "// @end handler",
            "// @region handler",
            "void b() {}",
            "// @end handler",
            "",
        ]);
        let result = extract_regions(&np("/test.cpp"), &content, Some("//"));
        assert!(result.is_err());
        match result.unwrap_err() {
            LifeError::DuplicateRegion { name, .. } => {
                assert_eq!(name, "handler");
            }
            e => panic!("expected DuplicateRegion, got {:?}", e),
        }
    }

    #[test]
    fn multiple_regions() {
        let content = tc(&[
            "",
            "// @region alpha",
            "int a = 1;",
            "// @end alpha",
            "",
            "// @region beta",
            "int b = 2;",
            "// @end beta",
            "",
        ]);
        let regions = extract_regions(&np("/test.cpp"), &content, Some("//")).unwrap();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].name, "alpha");
        assert_eq!(regions[1].name, "beta");
    }

    #[test]
    fn freetext_returns_empty() {
        let content = tc(&[
            "# @region this-wont-work",
            "some text",
            "# @end this-wont-work",
        ]);
        let regions = extract_regions(&np("/test.md"), &content, None).unwrap();
        assert!(regions.is_empty());
    }

    #[test]
    fn unclosed_region_includes_to_eof() {
        let content = tc(&[
            "",
            "// @region dangling",
            "//| This region is never closed.",
            "void something() {}",
            "",
        ]);
        let regions = extract_regions(&np("/test.cpp"), &content, Some("//")).unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].name, "dangling");
        assert_eq!(regions[0].end_line, 4); // last line of content
    }

    #[test]
    fn prose_continuity_broken_by_code() {
        let content = tc(&[
            "// @region test",
            "//| First block of prose.",
            "//| Continues here.",
            "void code() {}",
            "//| Second block after code.",
            "// @end test",
        ]);
        let regions = extract_regions(&np("/test.cpp"), &content, Some("//")).unwrap();
        assert_eq!(regions[0].prose.len(), 2);
        assert!(regions[0].prose[0].content.contains("First block"));
        assert!(regions[0].prose[0].content.contains("Continues here"));
        assert!(regions[0].prose[1].content.contains("Second block"));
    }

    #[test]
    fn prose_continuity_broken_by_regular_comment() {
        let content = tc(&[
            "// @region test",
            "//| Prose block one.",
            "// just a regular comment",
            "//| Prose block two.",
            "// @end test",
        ]);
        let regions = extract_regions(&np("/test.cpp"), &content, Some("//")).unwrap();
        assert_eq!(regions[0].prose.len(), 2);
    }
}
