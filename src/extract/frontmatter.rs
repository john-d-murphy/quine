use crate::types::{Attribute, NodePath};

// @region frontmatter-parser
//| Frontmatter extraction pulls YAML key-value pairs from the ---
//| delimited block at the top of markdown files. Values are always
//| stored as strings — the design says projectors interpret values,
//| not the graph.
//|
//| This is how lenses enforce schema: required_keys checks that
//| specific keys exist in a file's extracted attributes. The lens
//| doesn't know what the values mean. The projector does.

/// Extract YAML frontmatter key-value pairs from a markdown file.
///
/// Frontmatter is delimited by `---` at the start and end.
/// Only extracts top-level scalar values — nested structures are
/// stored as their YAML string representation.
///
/// Returns empty vec if no frontmatter is found.
pub fn extract_frontmatter(file_path: &NodePath, content: &str) -> Vec<Attribute> {
    let fm = match parse_frontmatter_block(content) {
        Some(fm) => fm,
        None => return Vec::new(),
    };

    // Parse as YAML.
    let yaml: serde_yaml::Value = match serde_yaml::from_str(&fm) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mapping = match yaml.as_mapping() {
        Some(m) => m,
        None => return Vec::new(),
    };

    let mut attributes = Vec::new();
    for (key, value) in mapping {
        let key_str = match key.as_str() {
            Some(s) => s.to_string(),
            None => continue,
        };

        // Convert value to string representation.
        let value_str = yaml_value_to_string(value);

        attributes.push(Attribute {
            file: file_path.clone(),
            key: key_str,
            value: value_str,
        });
    }

    attributes
}

/// Extract the raw frontmatter block from content.
/// Returns the text between the opening and closing `---` delimiters.
fn parse_frontmatter_block(content: &str) -> Option<String> {
    let trimmed = content.trim_start();

    // Must start with ---
    if !trimmed.starts_with("---") {
        return None;
    }

    // Find the end of the first line (the opening ---)
    let after_open = &trimmed[3..];
    let rest = after_open.trim_start_matches(|c: char| c != '\n');
    let rest = rest.strip_prefix('\n')?;

    // Find the closing ---
    let close_pos = rest.find("\n---")?;
    let fm_content = &rest[..close_pos];

    Some(fm_content.to_string())
}

/// Convert a YAML value to a string for storage.
/// Scalars are stored as-is. Arrays and maps are stored as
/// compact YAML/JSON-like strings.
fn yaml_value_to_string(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::Null => "null".to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Sequence(seq) => {
            // Store arrays as JSON-like: ["a", "b", "c"]
            let items: Vec<String> = seq.iter().map(yaml_value_to_string).collect();
            format!("[{}]", items.join(", "))
        }
        serde_yaml::Value::Mapping(_) => {
            // Store maps as YAML string.
            serde_yaml::to_string(value).unwrap_or_else(|_| "{}".to_string())
        }
        serde_yaml::Value::Tagged(tagged) => yaml_value_to_string(&tagged.value),
    }
}
// @end frontmatter-parser

#[cfg(test)]
mod tests {
    use super::*;

    fn np(s: &str) -> NodePath {
        NodePath::new(s).unwrap()
    }

    #[test]
    fn simple_frontmatter() {
        let content = r#"---
title: "My Post"
date: 2026-03-18
---

Body text here.
"#;
        let attrs = extract_frontmatter(&np("/test.md"), content);
        assert_eq!(attrs.len(), 2);

        let title = attrs.iter().find(|a| a.key == "title").unwrap();
        assert_eq!(title.value, "My Post");

        let date = attrs.iter().find(|a| a.key == "date").unwrap();
        assert_eq!(date.value, "2026-03-18");
    }

    #[test]
    fn frontmatter_with_tags_array() {
        let content = r#"---
title: "Test"
tags: ["rust", "supercollider", "music"]
---

Body.
"#;
        let attrs = extract_frontmatter(&np("/test.md"), content);
        let tags = attrs.iter().find(|a| a.key == "tags").unwrap();
        assert_eq!(tags.value, "[rust, supercollider, music]");
    }

    #[test]
    fn frontmatter_with_numeric_values() {
        let content = r#"---
title: "Book"
author: "Someone"
finished: 0
rating: 5
---
"#;
        let attrs = extract_frontmatter(&np("/test.md"), content);
        assert_eq!(attrs.len(), 4);

        let finished = attrs.iter().find(|a| a.key == "finished").unwrap();
        assert_eq!(finished.value, "0");

        let rating = attrs.iter().find(|a| a.key == "rating").unwrap();
        assert_eq!(rating.value, "5");
    }

    #[test]
    fn no_frontmatter() {
        let content = "Just some text without frontmatter.";
        let attrs = extract_frontmatter(&np("/test.md"), content);
        assert!(attrs.is_empty());
    }

    #[test]
    fn malformed_frontmatter() {
        let content = r#"---
this is not: [valid yaml: {
---

Body.
"#;
        let attrs = extract_frontmatter(&np("/test.md"), content);
        assert!(attrs.is_empty());
    }

    #[test]
    fn frontmatter_bool_value() {
        let content = r#"---
draft: true
public: false
---
"#;
        let attrs = extract_frontmatter(&np("/test.md"), content);

        let draft = attrs.iter().find(|a| a.key == "draft").unwrap();
        assert_eq!(draft.value, "true");

        let public = attrs.iter().find(|a| a.key == "public").unwrap();
        assert_eq!(public.value, "false");
    }

    #[test]
    fn frontmatter_preserves_file_path() {
        let content = r#"---
title: "Test"
---
"#;
        let path = np("/home/murphy/notes/test.md");
        let attrs = extract_frontmatter(&path, content);
        assert_eq!(attrs[0].file, path);
    }

    #[test]
    fn only_first_frontmatter_block() {
        let content = r#"---
title: "First"
---

Some text.

---
title: "Second"
---
"#;
        let attrs = extract_frontmatter(&np("/test.md"), content);
        let title = attrs.iter().find(|a| a.key == "title").unwrap();
        assert_eq!(title.value, "First");
    }

    #[test]
    fn book_entry_example() {
        let content = r#"---
title: "Gödel, Escher, Bach"
author: "Douglas Hofstadter"
finished: 2026-01-15
---

Great book about strange loops.
"#;
        let attrs = extract_frontmatter(&np("/notes/books/geb.md"), content);
        assert_eq!(attrs.len(), 3);

        let title = attrs.iter().find(|a| a.key == "title").unwrap();
        assert_eq!(title.value, "Gödel, Escher, Bach");

        let finished = attrs.iter().find(|a| a.key == "finished").unwrap();
        assert_eq!(finished.value, "2026-01-15");
    }
}
