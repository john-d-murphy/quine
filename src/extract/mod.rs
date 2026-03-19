pub mod config;
pub mod engine;
pub mod frontmatter;
pub mod links;
pub mod regions;

use std::fs;

use crate::errors::{LifeError, LifeWarning};
use crate::types::*;

use config::{builtin_extractors, find_extractor, ExtractorDef};

/// Result of extraction across multiple files.
pub struct ExtractionResult {
    pub extracted: Vec<Extracted>,
    pub warnings: Vec<LifeWarning>,
}

/// Run extraction on a list of files. Returns extracted data and warnings.
/// Returns Err for hard errors (nested regions, duplicate region names).
///
/// For each file:
/// - Find a matching extractor based on file extension.
/// - If no match, emit a NoExtractor warning (file is a leaf).
/// - If matched, read file contents and run the extraction engine.
pub fn run(files: &[WalkedFile]) -> Result<ExtractionResult, LifeError> {
    let extractors = builtin_extractors();
    extract_files(files, &extractors)
}

fn extract_files(
    files: &[WalkedFile],
    extractors: &[ExtractorDef],
) -> Result<ExtractionResult, LifeError> {
    let mut result = ExtractionResult {
        extracted: Vec::new(),
        warnings: Vec::new(),
    };

    for file in files {
        let path_str = file.path.as_str();

        let def = match find_extractor(path_str, extractors) {
            Some(d) => d,
            None => {
                result.warnings.push(LifeWarning::NoExtractor {
                    file: file.path.clone(),
                });
                continue;
            }
        };

        // Read file contents. Binary or unreadable files are silently skipped.
        let content = match fs::read_to_string(file.path.as_path()) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Run extraction — may error on region violations.
        let extracted = engine::extract(&file.path, &content, def)?;
        result.extracted.push(extracted);
    }

    Ok(result)
}
