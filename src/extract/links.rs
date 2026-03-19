// @region link-parser
//| The link parser finds [[path]] and [[path#fragment]] in text.
//| It's a pure text scanner — no regex, no parser combinator,
//| just a byte-by-byte walk looking for [[ and ]].
//|
//| Links cannot span lines. Empty links are skipped. The #
//| character splits path from fragment. This is the one piece
//| of syntax the system adds to files — everything else is
//| standard markdown or standard code comments.

/// A raw parsed link from source text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLink {
    /// The path portion of the link (before #).
    pub path: String,
    /// The fragment portion (after #), if any.
    pub fragment: Option<String>,
    /// Line number where the link was found (1-indexed).
    pub line: u32,
    /// Byte offset of `[[` within the line.
    pub col: usize,
}

/// Extract all `[[...]]` links from a string.
///
/// This is a pure text operation — it doesn't validate paths or
/// resolve tilde. It finds every `[[...]]` pair and parses the
/// contents into path and optional fragment.
///
/// Rules:
/// - `[[` opens a link, `]]` closes it.
/// - Nesting is not supported; `[[` inside an open link is ignored.
/// - Links cannot span multiple lines.
/// - Empty links `[[]]` are skipped.
/// - The `#` character splits path from fragment.
/// - Whitespace is trimmed from path and fragment.
pub fn extract_links(text: &str) -> Vec<RawLink> {
    let mut links = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let line_num = (line_idx + 1) as u32;
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut i = 0;

        while i + 1 < len {
            // Look for [[
            if bytes[i] == b'[' && bytes[i + 1] == b'[' {
                let open = i;
                i += 2; // skip past [[

                // Scan for ]] on the same line.
                let mut close = None;
                let mut j = i;
                while j + 1 < len {
                    if bytes[j] == b']' && bytes[j + 1] == b']' {
                        close = Some(j);
                        break;
                    }
                    j += 1;
                }

                if let Some(close_pos) = close {
                    let inner = &line[i..close_pos];
                    let inner = inner.trim();

                    if !inner.is_empty() {
                        let (path, fragment) = match inner.find('#') {
                            Some(hash_pos) => {
                                let p = inner[..hash_pos].trim();
                                let f = inner[hash_pos + 1..].trim();
                                (
                                    p.to_string(),
                                    if f.is_empty() { None } else { Some(f.to_string()) },
                                )
                            }
                            None => (inner.to_string(), None),
                        };

                        if !path.is_empty() {
                            links.push(RawLink {
                                path,
                                fragment,
                                line: line_num,
                                col: open,
                            });
                        }
                    }

                    i = close_pos + 2; // skip past ]]
                } else {
                    // No closing ]] on this line — not a link.
                    i += 2;
                }
            } else {
                i += 1;
            }
        }
    }

    links
}
// @end link-parser

// @region comment-extraction
//| In code files, [[links]] only count if they're in comments.
//| The system needs to distinguish "something a human wrote for
//| the graph" from "code that happens to contain brackets."
//|
//| Two modes: line comments (// or #, including trailing comments
//| on code lines) and block comments (/* */). The freetext mode
//| in the engine bypasses this entirely — in markdown, everything
//| is human-written text.

/// Extract links only from comment portions of lines.
///
/// Given text and a line-comment prefix (e.g. "//" or "#"),
/// find the comment prefix on each line and extract `[[...]]`
/// links from the text after it.
///
/// Handles both full-line comments (`// see [[link]]`) and
/// trailing comments (`int x = 5; // see [[link]]`).
pub fn extract_links_from_comments(text: &str, line_comment: &str) -> Vec<RawLink> {
    let mut links = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        let line_num = (line_idx + 1) as u32;

        // Find the comment prefix in the line.
        // Use the first occurrence. This is imperfect (could match
        // inside a string literal like "http://"), but [[]] links
        // inside string literals are unlikely.
        if let Some(comment_start) = line.find(line_comment) {
            let comment_body = &line[comment_start + line_comment.len()..];
            let line_links = extract_links(comment_body);
            for mut link in line_links {
                link.line = line_num;
                link.col += comment_start + line_comment.len();
                links.push(link);
            }
        }
    }

    links
}

/// Extract links from block comments.
///
/// Given text and block comment delimiters (e.g. "/*" and "*/"),
/// extract `[[...]]` links only from within block comments.
pub fn extract_links_from_block_comments(
    text: &str,
    open: &str,
    close: &str,
) -> Vec<RawLink> {
    let mut links = Vec::new();
    let mut in_block = false;
    let mut block_lines: Vec<(usize, &str)> = Vec::new();

    for (line_idx, line) in text.lines().enumerate() {
        if in_block {
            if let Some(pos) = line.find(close) {
                // End of block — include content before the close.
                let before_close = &line[..pos];
                block_lines.push((line_idx, before_close));

                // Extract from accumulated block.
                let block_text: String = block_lines
                    .iter()
                    .map(|(_, l)| *l)
                    .collect::<Vec<_>>()
                    .join("\n");
                let block_links = extract_links(&block_text);
                for mut link in block_links {
                    // Adjust line number to absolute.
                    let first_line = block_lines[0].0;
                    link.line = (first_line + link.line as usize) as u32;
                    links.push(link);
                }

                block_lines.clear();
                in_block = false;
            } else {
                block_lines.push((line_idx, line));
            }
        } else if let Some(pos) = line.find(open) {
            // Start of block comment.
            let after_open = &line[pos + open.len()..];
            // Check if the block closes on the same line.
            if let Some(close_pos) = after_open.find(close) {
                // Single-line block comment.
                let content = &after_open[..close_pos];
                let single_links = extract_links(content);
                for mut link in single_links {
                    link.line = (line_idx + 1) as u32;
                    links.push(link);
                }
            } else {
                in_block = true;
                block_lines.push((line_idx, after_open));
            }
        }
    }

    links
}
// @end comment-extraction

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_link() {
        let links = extract_links("see [[~/notes/fix.md]] here");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, "~/notes/fix.md");
        assert_eq!(links[0].fragment, None);
        assert_eq!(links[0].line, 1);
    }

    #[test]
    fn link_with_fragment() {
        let links = extract_links("see [[~/src/foo.cpp#pmm-routing]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, "~/src/foo.cpp");
        assert_eq!(links[0].fragment, Some("pmm-routing".to_string()));
    }

    #[test]
    fn multiple_links_one_line() {
        let links = extract_links("[[~/a.md]] and [[~/b.md]]");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].path, "~/a.md");
        assert_eq!(links[1].path, "~/b.md");
    }

    #[test]
    fn multiple_links_multiple_lines() {
        let text = "line one [[~/a.md]]\nline two\nline three [[~/b.md]] and [[~/c.md#frag]]";
        let links = extract_links(text);
        assert_eq!(links.len(), 3);
        assert_eq!(links[0].line, 1);
        assert_eq!(links[1].line, 3);
        assert_eq!(links[2].line, 3);
        assert_eq!(links[2].fragment, Some("frag".to_string()));
    }

    #[test]
    fn empty_link_skipped() {
        let links = extract_links("empty [[]] here");
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn unclosed_link_skipped() {
        let links = extract_links("unclosed [[~/no-close here");
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn link_does_not_span_lines() {
        let links = extract_links("start [[~/path/\nto/file.md]]");
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn whitespace_trimmed() {
        let links = extract_links("[[ ~/notes/fix.md ]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, "~/notes/fix.md");
    }

    #[test]
    fn fragment_whitespace_trimmed() {
        let links = extract_links("[[ ~/src/foo.cpp # pmm-routing ]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, "~/src/foo.cpp");
        assert_eq!(links[0].fragment, Some("pmm-routing".to_string()));
    }

    #[test]
    fn hash_only_fragment_is_none() {
        let links = extract_links("[[~/notes/fix.md#]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, "~/notes/fix.md");
        assert_eq!(links[0].fragment, None);
    }

    #[test]
    fn absolute_path_link() {
        let links = extract_links("[[/home/murphy/notes/fix.md]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, "/home/murphy/notes/fix.md");
    }

    #[test]
    fn link_in_markdown_code_block_still_extracted() {
        // Design decision: in freetext mode, every [[ ]] is a link,
        // even inside code blocks.
        let text = "```\n[[~/notes/fix.md]]\n```";
        let links = extract_links(text);
        assert_eq!(links.len(), 1);
    }

    #[test]
    fn adjacent_brackets_not_confused() {
        let links = extract_links("array[1][2] and [[~/real-link.md]]");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, "~/real-link.md");
    }

    // ---- Comment-mode tests ----

    #[test]
    fn line_comment_extraction() {
        let text = r#"
void main() {
    // see [[~/notes/fix.md]] for details
    int x = 5; // [[~/notes/other.md]]
    printf("[[~/not/a/link.md]]"); // not in a comment line
}
"#;
        let links = extract_links_from_comments(text, "//");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].path, "~/notes/fix.md");
        assert_eq!(links[1].path, "~/notes/other.md");
    }

    #[test]
    fn python_comment_extraction() {
        let text = r#"
def main():
    # see [[~/notes/fix.md]]
    x = 5  # [[~/notes/other.md]]
    print("[[~/not/a/link.md]]")
"#;
        let links = extract_links_from_comments(text, "#");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].path, "~/notes/fix.md");
        assert_eq!(links[1].path, "~/notes/other.md");
    }

    #[test]
    fn line_comment_no_links_in_code() {
        let text = r#"
auto path = "[[~/notes/fix.md]]";
// no link here
"#;
        let links = extract_links_from_comments(text, "//");
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn block_comment_extraction() {
        let text = r#"
/* This references [[~/notes/fix.md]] */
int x = 5;
/*
 * Multi-line block:
 * See [[~/src/strategy.cpp#pmm-routing]]
 */
"#;
        let links = extract_links_from_block_comments(text, "/*", "*/");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].path, "~/notes/fix.md");
        assert_eq!(links[1].path, "~/src/strategy.cpp");
        assert_eq!(links[1].fragment, Some("pmm-routing".to_string()));
    }

    #[test]
    fn block_comment_ignores_code() {
        let text = r#"
char* s = "[[~/not/a/link.md]]";
/* but [[~/real/link.md]] is */
"#;
        let links = extract_links_from_block_comments(text, "/*", "*/");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, "~/real/link.md");
    }
}
