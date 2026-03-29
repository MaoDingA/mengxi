use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Supported image formats for evaluation datasets.
const SUPPORTED_EXTENSIONS: &[&str] = &["dpx", "exr"];

/// Built-in default vocabulary for grading style tags.
/// Used as fallback when no `vocabulary.toml` is found.
const DEFAULT_VOCABULARY: &[&str] = &[
    // Contrast
    "high_contrast",
    "low_contrast",
    "medium_contrast",
    // Color temperature
    "warm",
    "cool",
    "neutral_temperature",
    // Saturation
    "highly_saturated",
    "desaturated",
    "normal_saturation",
    // Brightness / Exposure
    "overexposed",
    "underexposed",
    "normal_exposure",
    // Tone
    "highlight_rolled",
    "shadow_crushed",
    "soft_tone",
    "harsh_tone",
    // Color cast
    "green_cast",
    "magenta_cast",
    "blue_shift",
    "orange_shift",
    // Style
    "film_emulation",
    "bleach_bypass",
    "cross_process",
    "monochrome",
    "teal_orange",
    "vintage",
    "modern_clean",
];

/// Load vocabulary from a `vocabulary.toml` file in the given directory.
///
/// Format: `tags = ["high_contrast", "warm", ...]`
/// Falls back to built-in defaults if the file is missing or unparseable.
/// Silently deduplicates entries.
fn load_vocabulary(dir: &Path) -> Vec<String> {
    let vocab_path = dir.join("vocabulary.toml");
    if !vocab_path.is_file() {
        return DEFAULT_VOCABULARY.iter().map(|s| s.to_string()).collect();
    }

    match std::fs::read_to_string(&vocab_path) {
        Ok(content) => {
            #[derive(Deserialize)]
            struct VocabFile {
                tags: Vec<String>,
            }
            match toml::from_str::<VocabFile>(&content) {
                Ok(vf) => {
                    let mut seen = std::collections::HashSet::new();
                    vf.tags.into_iter()
                        .filter(|t| {
                            let key = t.trim().to_lowercase();
                            !t.is_empty() && seen.insert(key)
                        })
                        .map(|t| t.trim().to_string())
                        .collect()
                }
                Err(_) => {
                    eprintln!("Warning: failed to parse vocabulary.toml, using defaults");
                    DEFAULT_VOCABULARY.iter().map(|s| s.to_string()).collect()
                }
            }
        }
        Err(_) => {
            eprintln!("Warning: cannot read vocabulary.toml, using defaults");
            DEFAULT_VOCABULARY.iter().map(|s| s.to_string()).collect()
        }
    }
}

/// Valid material types.
const VALID_MATERIAL_TYPES: &[&str] = &["dpx", "exr", "mov"];

/// Valid color spaces.
const VALID_COLOR_SPACES: &[&str] = &["srgb", "acescct", "linear", "rec709", "log"];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntryMetadata {
    material_type: String,
    color_space: String,
    grading_style_tags: Vec<String>,
}

#[derive(Debug, Clone)]
struct ValidationEntry {
    image_path: PathBuf,
    metadata_path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct EntryResult {
    image: String,
    status: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DatasetSummary {
    total: usize,
    valid: usize,
    invalid: usize,
}

/// Discover image files and their expected metadata JSON pairs in a directory.
fn discover_entries(dir: &Path) -> Vec<ValidationEntry> {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return entries;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_lowercase(),
            None => continue,
        };
        if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        let metadata_path = path.with_extension("json");
        entries.push(ValidationEntry {
            image_path: path,
            metadata_path,
        });
    }

    entries.sort_by(|a, b| a.image_path.cmp(&b.image_path));
    entries
}

/// Validate a single entry (image + metadata pair).
fn validate_entry(entry: &ValidationEntry, vocabulary: &[String]) -> EntryResult {
    let image_str = entry.image_path.display().to_string();
    let mut errors = Vec::new();

    // Check metadata JSON exists
    if !entry.metadata_path.is_file() {
        errors.push(format!(
            "missing metadata JSON: {}",
            entry.metadata_path.display()
        ));
        return EntryResult {
            image: image_str,
            status: "invalid".to_string(),
            errors,
        };
    }

    // Parse metadata JSON
    let content = match std::fs::read_to_string(&entry.metadata_path) {
        Ok(c) => c,
        Err(e) => {
            errors.push(format!("cannot read metadata JSON: {}", e));
            return EntryResult {
                image: image_str,
                status: "invalid".to_string(),
                errors,
            };
        }
    };

    let metadata: EntryMetadata = match serde_json::from_str(&content) {
        Ok(m) => m,
        Err(e) => {
            errors.push(format!("invalid metadata JSON: {}", e));
            return EntryResult {
                image: image_str,
                status: "invalid".to_string(),
                errors,
            };
        }
    };

    // Validate material_type
    if !VALID_MATERIAL_TYPES.contains(&metadata.material_type.as_str()) {
        errors.push(format!(
            "unknown material_type: {} (expected one of: {})",
            metadata.material_type,
            VALID_MATERIAL_TYPES.join(", ")
        ));
    }

    // Validate color_space
    if !VALID_COLOR_SPACES.contains(&metadata.color_space.as_str()) {
        errors.push(format!(
            "unknown color_space: {} (expected one of: {})",
            metadata.color_space,
            VALID_COLOR_SPACES.join(", ")
        ));
    }

    // Validate grading_style_tags uses controlled vocabulary
    if metadata.grading_style_tags.is_empty() {
        errors.push("grading_style_tags is empty".to_string());
    }
    for tag in &metadata.grading_style_tags {
        if !vocabulary.contains(&tag) {
            errors.push(format!(
                "unknown grading_style_tag: '{}' (not in controlled vocabulary)",
                tag
            ));
        }
    }

    let status = if errors.is_empty() {
        "valid".to_string()
    } else {
        "invalid".to_string()
    };

    EntryResult {
        image: image_str,
        status,
        errors,
    }
}

pub fn run_validate_dataset(dir: &str, is_json: bool) -> i32 {
    let dir_path = Path::new(dir);
    if !dir_path.is_dir() {
        if is_json {
            let output = serde_json::json!({
                "status": "error",
                "error": { "code": "DATASET_DIR_NOT_FOUND", "message": format!("directory not found: {}", dir) }
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        } else {
            eprintln!("Error: directory not found: {}", dir);
        }
        return 1;
    }

    let entries = discover_entries(dir_path);
    if entries.is_empty() {
        if is_json {
            let output = serde_json::json!({
                "status": "error",
                "error": { "code": "DATASET_EMPTY", "message": format!("no supported image files found in: {}", dir) }
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        } else {
            eprintln!("Error: no supported image files found in: {}", dir);
        }
        return 1;
    }

    let vocabulary = load_vocabulary(dir_path);
    let results: Vec<EntryResult> = entries.iter().map(|e| validate_entry(e, &vocabulary)).collect();
    let valid_count = results.iter().filter(|r| r.status == "valid").count();
    let invalid_count = results.len() - valid_count;

    let summary = DatasetSummary {
        total: results.len(),
        valid: valid_count,
        invalid: invalid_count,
    };

    if is_json {
        let output = serde_json::json!({
            "status": if invalid_count == 0 { "ok" } else { "error" },
            "summary": &summary,
            "results": &results,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("Evaluation Dataset Validation");
        println!("=============================");
        println!();
        for r in &results {
            let icon = if r.status == "valid" { "✓" } else { "✗" };
            println!("{} {}", icon, r.image);
            for err in &r.errors {
                println!("    {}", err);
            }
        }
        println!();
        println!(
            "Summary: {} total, {} valid, {} invalid",
            summary.total, summary.valid, summary.invalid
        );
    }

    if invalid_count > 0 { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_discover_entries_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let entries = discover_entries(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_discover_entries_finds_dpx() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("frame.dpx"), "fake").unwrap();
        let entries = discover_entries(dir.path());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].image_path.ends_with("frame.dpx"));
        assert!(entries[0].metadata_path.ends_with("frame.json"));
    }

    #[test]
    fn test_discover_entries_finds_exr() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("shot.exr"), "fake").unwrap();
        let entries = discover_entries(dir.path());
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_discover_entries_ignores_other_formats() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("photo.jpg"), "fake").unwrap();
        fs::write(dir.path().join("notes.txt"), "fake").unwrap();
        let entries = discover_entries(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_validate_entry_missing_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("frame.dpx");
        fs::write(&image_path, "fake").unwrap();
        let entry = ValidationEntry {
            image_path: image_path.clone(),
            metadata_path: dir.path().join("frame.json"),
        };
        let default_vocab: Vec<String> = DEFAULT_VOCABULARY.iter().map(|s| s.to_string()).collect();
        let result = validate_entry(&entry, &default_vocab);
        assert_eq!(result.status, "invalid");
        assert!(result.errors[0].contains("missing metadata JSON"));
    }

    #[test]
    fn test_validate_entry_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("frame.dpx");
        fs::write(&image_path, "fake").unwrap();
        fs::write(dir.path().join("frame.json"), "{bad json").unwrap();
        let entry = ValidationEntry {
            image_path,
            metadata_path: dir.path().join("frame.json"),
        };
        let default_vocab: Vec<String> = DEFAULT_VOCABULARY.iter().map(|s| s.to_string()).collect();
        let result = validate_entry(&entry, &default_vocab);
        assert_eq!(result.status, "invalid");
        assert!(result.errors.iter().any(|e| e.contains("invalid metadata JSON")));
    }

    #[test]
    fn test_validate_entry_valid() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("frame.dpx");
        fs::write(&image_path, "fake").unwrap();
        let meta = EntryMetadata {
            material_type: "dpx".to_string(),
            color_space: "acescct".to_string(),
            grading_style_tags: vec!["high_contrast".to_string(), "warm".to_string()],
        };
        fs::write(dir.path().join("frame.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();
        let entry = ValidationEntry {
            image_path,
            metadata_path: dir.path().join("frame.json"),
        };
        let default_vocab: Vec<String> = DEFAULT_VOCABULARY.iter().map(|s| s.to_string()).collect();
        let result = validate_entry(&entry, &default_vocab);
        assert_eq!(result.status, "valid");
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_entry_unknown_tag() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("frame.exr");
        fs::write(&image_path, "fake").unwrap();
        let meta = EntryMetadata {
            material_type: "exr".to_string(),
            color_space: "linear".to_string(),
            grading_style_tags: vec!["high_contrast".to_string(), "my_custom_tag".to_string()],
        };
        fs::write(dir.path().join("frame.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();
        let entry = ValidationEntry {
            image_path,
            metadata_path: dir.path().join("frame.json"),
        };
        let default_vocab: Vec<String> = DEFAULT_VOCABULARY.iter().map(|s| s.to_string()).collect();
        let result = validate_entry(&entry, &default_vocab);
        assert_eq!(result.status, "invalid");
        assert!(result.errors.iter().any(|e| e.contains("unknown grading_style_tag") && e.contains("my_custom_tag")));
    }

    #[test]
    fn test_validate_entry_empty_tags() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("frame.dpx");
        fs::write(&image_path, "fake").unwrap();
        let meta = EntryMetadata {
            material_type: "dpx".to_string(),
            color_space: "srgb".to_string(),
            grading_style_tags: vec![],
        };
        fs::write(dir.path().join("frame.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();
        let entry = ValidationEntry {
            image_path,
            metadata_path: dir.path().join("frame.json"),
        };
        let default_vocab: Vec<String> = DEFAULT_VOCABULARY.iter().map(|s| s.to_string()).collect();
        let result = validate_entry(&entry, &default_vocab);
        assert_eq!(result.status, "invalid");
        assert!(result.errors.iter().any(|e| e.contains("grading_style_tags is empty")));
    }

    #[test]
    fn test_validate_entry_unknown_material_type() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("frame.dpx");
        fs::write(&image_path, "fake").unwrap();
        let meta = EntryMetadata {
            material_type: "tiff".to_string(),
            color_space: "srgb".to_string(),
            grading_style_tags: vec!["warm".to_string()],
        };
        fs::write(dir.path().join("frame.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();
        let entry = ValidationEntry {
            image_path,
            metadata_path: dir.path().join("frame.json"),
        };
        let default_vocab: Vec<String> = DEFAULT_VOCABULARY.iter().map(|s| s.to_string()).collect();
        let result = validate_entry(&entry, &default_vocab);
        assert_eq!(result.status, "invalid");
        assert!(result.errors.iter().any(|e| e.contains("unknown material_type")));
    }

    #[test]
    fn test_default_vocabulary_contains_expected_tags() {
        assert!(DEFAULT_VOCABULARY.contains(&"high_contrast"));
        assert!(DEFAULT_VOCABULARY.contains(&"warm"));
        assert!(DEFAULT_VOCABULARY.contains(&"desaturated"));
        assert!(DEFAULT_VOCABULARY.contains(&"film_emulation"));
        assert!(DEFAULT_VOCABULARY.contains(&"teal_orange"));
    }

    #[test]
    fn test_load_vocabulary_no_file_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let vocab = load_vocabulary(dir.path());
        assert!(vocab.contains(&"high_contrast".to_string()));
        assert!(vocab.contains(&"warm".to_string()));
        assert!(vocab.contains(&"desaturated".to_string()));
    }

    #[test]
    fn test_load_vocabulary_custom_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("vocabulary.toml"),
            r#"tags = ["custom_tag_a", "custom_tag_b", "warm"]"#,
        ).unwrap();
        let vocab = load_vocabulary(dir.path());
        assert_eq!(vocab.len(), 3); // deduplicated
        assert!(vocab.contains(&"custom_tag_a".to_string()));
        assert!(vocab.contains(&"custom_tag_b".to_string()));
        assert!(vocab.contains(&"warm".to_string()));
        assert!(!vocab.contains(&"high_contrast".to_string())); // not in custom vocab
    }

    #[test]
    fn test_load_vocabulary_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("vocabulary.toml"),
            r#"tags = ["warm", "warm", "cool", "cool"]"#,
        ).unwrap();
        let vocab = load_vocabulary(dir.path());
        assert_eq!(vocab.len(), 2);
    }

    #[test]
    fn test_load_vocabulary_invalid_toml_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("vocabulary.toml"), "not valid toml {{{").unwrap();
        let vocab = load_vocabulary(dir.path());
        // Falls back to defaults
        assert!(vocab.contains(&"high_contrast".to_string()));
    }

    #[test]
    fn test_validate_entry_with_custom_vocabulary() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("frame.dpx");
        fs::write(&image_path, "fake").unwrap();
        let meta = EntryMetadata {
            material_type: "dpx".to_string(),
            color_space: "srgb".to_string(),
            grading_style_tags: vec!["custom_tag_a".to_string()],
        };
        fs::write(dir.path().join("frame.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();
        let entry = ValidationEntry {
            image_path,
            metadata_path: dir.path().join("frame.json"),
        };

        // With custom vocabulary containing the tag → valid
        let custom_vocab = vec!["custom_tag_a".to_string(), "custom_tag_b".to_string()];
        let result = validate_entry(&entry, &custom_vocab);
        assert_eq!(result.status, "valid");

        // With default vocabulary NOT containing the tag → invalid
        let default_vocab: Vec<String> = DEFAULT_VOCABULARY.iter().map(|s| s.to_string()).collect();
        let result2 = validate_entry(&entry, &default_vocab);
        assert_eq!(result2.status, "invalid");
        assert!(result2.errors.iter().any(|e| e.contains("unknown grading_style_tag")));
    }

    #[test]
    fn test_run_validate_dataset_nonexistent_dir() {
        let code = run_validate_dataset("/nonexistent/path", false);
        assert_eq!(code, 1);
    }

    #[test]
    fn test_run_validate_dataset_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let code = run_validate_dataset(dir.path().to_str().unwrap(), false);
        assert_eq!(code, 1);
    }

    #[test]
    fn test_run_validate_dataset_json_output() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("frame.dpx");
        fs::write(&image_path, "fake").unwrap();
        let meta = EntryMetadata {
            material_type: "dpx".to_string(),
            color_space: "acescct".to_string(),
            grading_style_tags: vec!["warm".to_string()],
        };
        fs::write(dir.path().join("frame.json"), serde_json::to_string_pretty(&meta).unwrap()).unwrap();

        // Capture stdout
        let code = run_validate_dataset(dir.path().to_str().unwrap(), true);
        assert_eq!(code, 0);
    }
}
