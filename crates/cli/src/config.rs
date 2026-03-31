use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;

/// Root configuration structure matching ~/.mengxi/config TOML schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[derive(Default)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub import: ImportConfig,
    #[serde(default)]
    pub export: ExportConfig,
    #[serde(default)]
    pub search: SearchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeneralConfig {
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    #[serde(default = "default_search_limit")]
    pub default_search_limit: u32,
    #[serde(default = "default_export_format")]
    pub default_export_format: String,
    #[serde(default = "default_user")]
    pub user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AiConfig {
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default)]
    pub embedding_endpoint: String,
    #[serde(default = "default_true")]
    pub tag_generation: bool,
    #[serde(default)]
    pub tag_model: String,
    #[serde(default = "default_tag_top_n")]
    pub tag_top_n: u32,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_secs: u64,
    #[serde(default = "default_inference_timeout")]
    pub inference_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImportConfig {
    #[serde(default = "default_true")]
    pub auto_detect_format: bool,
    #[serde(default = "default_keyframe_extraction")]
    pub keyframe_extraction: String,
    /// Grid size for per-tile feature extraction (0 = disabled, N = NxN grid).
    #[serde(default)]
    pub tile_grid_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportConfig {
    #[serde(default = "default_output_dir")]
    pub default_output_dir: String,
}

/// Search configuration section (used in both global and project-level config).
/// Weights must sum to 1.0 and each >= 0.1 (validated at resolve time).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchConfig {
    #[serde(default = "default_grading_weight")]
    pub grading_weight: f64,
    #[serde(default = "default_clip_weight")]
    pub clip_weight: f64,
    #[serde(default = "default_tag_weight")]
    pub tag_weight: f64,
    #[serde(default = "default_search_mode")]
    pub default_mode: String,
}

fn default_grading_weight() -> f64 { 0.6 }
fn default_clip_weight() -> f64 { 0.3 }
fn default_tag_weight() -> f64 { 0.1 }
fn default_search_mode() -> String { "grading-first".to_string() }

fn default_log_level() -> String { "info".to_string() }
fn default_data_dir() -> String { "~/.mengxi/data".to_string() }
fn default_search_limit() -> u32 { 5 }
fn default_export_format() -> String { "cube".to_string() }
fn default_true() -> bool { true }
fn default_idle_timeout() -> u64 { 300 }
fn default_inference_timeout() -> u64 { 30 }
fn default_tag_top_n() -> u32 { 5 }
fn default_keyframe_extraction() -> String { "auto".to_string() }
fn default_output_dir() -> String { "~/lut".to_string() }
fn default_user() -> String { "default".to_string() }


impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            data_dir: default_data_dir(),
            default_search_limit: default_search_limit(),
            default_export_format: default_export_format(),
            user: "default".to_string(),
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            embedding_model: String::new(),
            embedding_endpoint: String::new(),
            tag_generation: default_true(),
            tag_model: String::new(),
            tag_top_n: default_tag_top_n(),
            idle_timeout_secs: default_idle_timeout(),
            inference_timeout_secs: default_inference_timeout(),
        }
    }
}

impl Default for ImportConfig {
    fn default() -> Self {
        Self {
            auto_detect_format: default_true(),
            keyframe_extraction: default_keyframe_extraction(),
            tile_grid_size: 0,
        }
    }
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            default_output_dir: default_output_dir(),
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            grading_weight: default_grading_weight(),
            clip_weight: default_clip_weight(),
            tag_weight: default_tag_weight(),
            default_mode: default_search_mode(),
        }
    }
}

/// Returns the configuration directory path: `~/.mengxi/`
pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".mengxi")
}

/// Returns the configuration file path: `~/.mengxi/config`
pub fn config_path() -> PathBuf {
    config_dir().join("config")
}

/// Load config from disk, or create with defaults if it doesn't exist.
pub fn load_or_create_config() -> Result<Config, Box<dyn std::error::Error>> {
    let path = config_path();
    if path.exists() {
        let content = fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    } else {
        let config = Config::default();
        save_config(&config)?;
        Ok(config)
    }
}

/// Save config to disk, creating the directory if needed.
pub fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let dir = config_dir();
    fs::create_dir_all(&dir)?;
    let content = toml::to_string_pretty(config)?;
    fs::write(config_path(), content)?;
    Ok(())
}

/// Format config as an aligned text table for CLI display.
pub fn format_config_table(config: &Config) -> String {
    let mut table = String::new();
    let section_width = 8;
    let key_width = 28;
    let value_width = 30;

    let separator = format!(
        "+-{:-<section_width$}-+-{:-<key_width$}-+-{:-<value_width$}-+\n",
        "", "", ""
    );
    let header = format!(
        "| {:<section_width$} | {:<key_width$} | {:<value_width$} |\n",
        "Section", "Key", "Value"
    );

    table.push_str(&separator);
    table.push_str(&header);
    table.push_str(&separator);

    let general_entries: Vec<(&str, String)> = vec![
        ("log_level", config.general.log_level.clone()),
        ("data_dir", config.general.data_dir.clone()),
        ("default_search_limit", config.general.default_search_limit.to_string()),
        ("default_export_format", config.general.default_export_format.clone()),
        ("user", config.general.user.clone()),
    ];

    for (key, value) in &general_entries {
        table.push_str(&format!(
            "| {:<section_width$} | {:<key_width$} | {:<value_width$} |\n",
            "general", key, value
        ));
    }

    let ai_entries: Vec<(&str, String)> = vec![
        ("embedding_model", config.ai.embedding_model.clone()),
        ("embedding_endpoint", config.ai.embedding_endpoint.clone()),
        ("tag_generation", config.ai.tag_generation.to_string()),
        ("tag_model", config.ai.tag_model.clone()),
        ("tag_top_n", config.ai.tag_top_n.to_string()),
    ];

    for (key, value) in &ai_entries {
        table.push_str(&format!(
            "| {:<section_width$} | {:<key_width$} | {:<value_width$} |\n",
            "ai", key, value
        ));
    }

    let import_entries: Vec<(&str, String)> = vec![
        ("auto_detect_format", config.import.auto_detect_format.to_string()),
        ("keyframe_extraction", config.import.keyframe_extraction.clone()),
    ];

    for (key, value) in &import_entries {
        table.push_str(&format!(
            "| {:<section_width$} | {:<key_width$} | {:<value_width$} |\n",
            "import", key, value
        ));
    }

    let export_entries: Vec<(&str, String)> = vec![
        ("default_output_dir", config.export.default_output_dir.clone()),
    ];

    for (key, value) in &export_entries {
        table.push_str(&format!(
            "| {:<section_width$} | {:<key_width$} | {:<value_width$} |\n",
            "export", key, value
        ));
    }

    let search_entries: Vec<(&str, String)> = vec![
        ("grading_weight", config.search.grading_weight.to_string()),
        ("clip_weight", config.search.clip_weight.to_string()),
        ("tag_weight", config.search.tag_weight.to_string()),
        ("default_mode", config.search.default_mode.clone()),
    ];

    for (key, value) in &search_entries {
        table.push_str(&format!(
            "| {:<section_width$} | {:<key_width$} | {:<value_width$} |\n",
            "search", key, value
        ));
    }

    table.push_str(&separator);
    table
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format_config_table(self))
    }
}

// ---------------------------------------------------------------------------
// Project-level config cascade (ADR-v2-4)
// ---------------------------------------------------------------------------

/// Search upward from `cwd` for `.mengxi/config`.
/// Returns the path to the first config found, or None if none exists up to filesystem root.
pub fn find_project_config(cwd: &std::path::Path) -> Option<PathBuf> {
    let mut dir = cwd;
    loop {
        let config_path = dir.join(".mengxi").join("config");
        if config_path.is_file() {
            return Some(config_path);
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent,
            _ => return None,
        }
    }
}

/// Load `[search]` section from a project config file.
/// The file may have `[search]` header or be top-level values.
pub fn load_project_config(path: &std::path::Path) -> Result<SearchConfig, String> {
    let content = fs::read_to_string(path).map_err(|e| {
        format!("CONFIG_VALIDATION_ERROR -- cannot read '{}': {}", path.display(), e)
    })?;

    // Parse with [search] section support
    #[derive(Deserialize)]
    struct ProjectConfigFile {
        #[serde(default)]
        search: SearchConfig,
    }

    let file: ProjectConfigFile = toml::from_str(&content).map_err(|e| {
        format!("CONFIG_VALIDATION_ERROR -- invalid TOML in '{}': {}", path.display(), e)
    })?;
    Ok(file.search)
}

/// Resolve search weights from config cascade (no CLI args).
/// Priority: project config > global config > built-in defaults.
/// When CLI args are present, the caller should use resolve_hybrid_weights instead.
pub fn resolve_search_config(
    cwd: &std::path::Path,
) -> Result<mengxi_core::hybrid_scoring::SignalWeights, String> {
    // Priority 1: Project config
    if let Some(project_path) = find_project_config(cwd) {
        let project_config = load_project_config(&project_path)?;
        // Use default_mode if set, otherwise use the weight values directly
        let weights = match project_config.default_mode.as_str() {
            "balanced" => mengxi_core::hybrid_scoring::SignalWeights::balanced(),
            "grading-first" => search_config_to_weights(&project_config)?,
            other => {
                eprintln!("warning: unknown default_mode '{}', using weight values directly", other);
                search_config_to_weights(&project_config)?
            }
        };
        weights.validate().map_err(|e| {
            format!("CONFIG_VALIDATION_ERROR -- {} (in '{}')", e, project_path.display())
        })?;
        return Ok(weights);
    }

    // Priority 2: Global config
    if let Ok(global_config) = load_or_create_config() {
        let weights = match global_config.search.default_mode.as_str() {
            "balanced" => mengxi_core::hybrid_scoring::SignalWeights::balanced(),
            "grading-first" => search_config_to_weights(&global_config.search)?,
            other => {
                eprintln!("warning: unknown default_mode '{}', using weight values directly", other);
                search_config_to_weights(&global_config.search)?
            }
        };
        weights.validate().map_err(|e| {
            format!("CONFIG_VALIDATION_ERROR -- {} (in global config)", e)
        })?;
        return Ok(weights);
    }

    // Priority 3: Built-in defaults
    Ok(mengxi_core::hybrid_scoring::SignalWeights::grading_first())
}

/// Convert SearchConfig weights to SignalWeights.
/// Returns error if any weight is NaN or Infinity.
fn search_config_to_weights(sc: &SearchConfig) -> Result<mengxi_core::hybrid_scoring::SignalWeights, String> {
    if !sc.grading_weight.is_finite() || !sc.clip_weight.is_finite() || !sc.tag_weight.is_finite() {
        return Err("CONFIG_VALIDATION_ERROR -- weights must be finite numbers (no NaN or Infinity)".to_string());
    }
    Ok(mengxi_core::hybrid_scoring::SignalWeights {
        grading: sc.grading_weight,
        clip: sc.clip_weight,
        tag: sc.tag_weight,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_creation() {
        let config = Config::default();
        assert_eq!(config.general.log_level, "info");
        assert_eq!(config.general.data_dir, "~/.mengxi/data");
        assert_eq!(config.general.default_search_limit, 5);
        assert_eq!(config.general.default_export_format, "cube");
        assert_eq!(config.general.user, "default");
        assert!(config.ai.tag_generation);
        assert!(config.ai.tag_model.is_empty());
        assert_eq!(config.ai.tag_top_n, 5);
        assert!(config.import.auto_detect_format);
        assert_eq!(config.import.keyframe_extraction, "auto");
        assert_eq!(config.export.default_output_dir, "~/lut");
        assert!(config.ai.embedding_model.is_empty());
        assert!(config.ai.embedding_endpoint.is_empty());
    }

    #[test]
    fn test_config_roundtrip_toml() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config, parsed);
    }

    #[test]
    fn test_config_custom_values() {
        let config = Config {
            general: GeneralConfig {
                log_level: "debug".to_string(),
                data_dir: "/custom/data".to_string(),
                default_search_limit: 10,
                default_export_format: "3dl".to_string(),
                user: "chen_liang".to_string(),
            },
            ai: AiConfig {
                embedding_model: "model.onnx".to_string(),
                embedding_endpoint: "http://localhost:8080".to_string(),
                tag_generation: false,
                tag_model: "clip_vit_b32.onnx".to_string(),
                tag_top_n: 10,
                idle_timeout_secs: 600,
                inference_timeout_secs: 60,
            },
            import: ImportConfig {
                auto_detect_format: false,
                keyframe_extraction: "manual".to_string(),
                tile_grid_size: 0,
            },
            export: ExportConfig {
                default_output_dir: "/custom/lut".to_string(),
            },
            search: SearchConfig::default(),
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.general.log_level, "debug");
        assert_eq!(parsed.general.default_search_limit, 10);
        assert_eq!(parsed.general.user, "chen_liang");
        assert_eq!(parsed.ai.embedding_model, "model.onnx");
        assert!(!parsed.ai.tag_generation);
        assert!(!parsed.import.auto_detect_format);
    }

    #[test]
    fn test_config_display_formatting() {
        let config = Config::default();
        let table = format_config_table(&config);
        // Sections are displayed as column values in the aligned table
        assert!(table.contains("general"));
        assert!(table.contains("log_level"));
        assert!(table.contains("info"));
        assert!(table.contains("ai"));
        assert!(table.contains("tag_generation"));
        assert!(table.contains("import"));
        assert!(table.contains("export"));
        // Verify table has separator lines
        assert!(table.starts_with('+'));
    }

    #[test]
    fn test_load_or_create_config_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let test_path = dir.path().join(".mengxi").join("config");
        assert!(!test_path.exists());

        // We can't easily test load_or_create_config because it uses
        // a hardcoded home dir path, so test the save/load logic instead
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        fs::create_dir_all(dir.path().join(".mengxi")).unwrap();
        fs::write(&test_path, &toml_str).unwrap();
        assert!(test_path.exists());

        let loaded_str = fs::read_to_string(&test_path).unwrap();
        let loaded: Config = toml::from_str(&loaded_str).unwrap();
        assert_eq!(config, loaded);
    }

    // --- Project config cascade tests (Story 5.1) ---

    #[test]
    fn test_find_project_config_at_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let mengxi_dir = dir.path().join(".mengxi");
        fs::create_dir_all(&mengxi_dir).unwrap();
        fs::write(mengxi_dir.join("config"), "[search]\ngrading_weight = 0.5\n").unwrap();

        let result = find_project_config(dir.path());
        assert!(result.is_some());
        assert_eq!(result.unwrap(), dir.path().join(".mengxi").join("config"));
    }

    #[test]
    fn test_find_project_config_from_subdirectory() {
        let dir = tempfile::tempdir().unwrap();
        let mengxi_dir = dir.path().join(".mengxi");
        fs::create_dir_all(&mengxi_dir).unwrap();
        fs::write(mengxi_dir.join("config"), "[search]\n").unwrap();

        let subdir = dir.path().join("a").join("b").join("c");
        fs::create_dir_all(&subdir).unwrap();

        let result = find_project_config(&subdir);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), dir.path().join(".mengxi").join("config"));
    }

    #[test]
    fn test_find_project_config_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let result = find_project_config(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_load_project_config_valid() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        fs::write(&config_path, "[search]\ngrading_weight = 0.5\nclip_weight = 0.3\ntag_weight = 0.2\ndefault_mode = \"balanced\"\n").unwrap();

        let result = load_project_config(&config_path).unwrap();
        assert_eq!(result.grading_weight, 0.5);
        assert_eq!(result.clip_weight, 0.3);
        assert_eq!(result.tag_weight, 0.2);
        assert_eq!(result.default_mode, "balanced");
    }

    #[test]
    fn test_load_project_config_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        // Empty [search] section — all fields use defaults
        fs::write(&config_path, "[search]\n").unwrap();

        let result = load_project_config(&config_path).unwrap();
        assert_eq!(result.grading_weight, 0.6);
        assert_eq!(result.clip_weight, 0.3);
        assert_eq!(result.tag_weight, 0.1);
        assert_eq!(result.default_mode, "grading-first");
    }

    #[test]
    fn test_load_project_config_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        fs::write(&config_path, "this is not toml {{{").unwrap();

        let result = load_project_config(&config_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("CONFIG_VALIDATION_ERROR"));
    }

    #[test]
    fn test_resolve_search_config_project_overrides_global() {
        let dir = tempfile::tempdir().unwrap();
        // Create project config with balanced weights
        let mengxi_dir = dir.path().join(".mengxi");
        fs::create_dir_all(&mengxi_dir).unwrap();
        fs::write(mengxi_dir.join("config"), "[search]\ngrading_weight = 0.4\nclip_weight = 0.4\ntag_weight = 0.2\n").unwrap();

        // No CLI args, project config should be used
        let result = resolve_search_config(dir.path()).unwrap();
        assert!((result.grading - 0.4).abs() < 1e-10);
        assert!((result.clip - 0.4).abs() < 1e-10);
        assert!((result.tag - 0.2).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_search_config_no_config_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        // No .mengxi/config anywhere
        let result = resolve_search_config(dir.path()).unwrap();
        assert!((result.grading - 0.6).abs() < 1e-10);
        assert!((result.clip - 0.3).abs() < 1e-10);
        assert!((result.tag - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_search_config_weight_sum_validation() {
        let dir = tempfile::tempdir().unwrap();
        let mengxi_dir = dir.path().join(".mengxi");
        fs::create_dir_all(&mengxi_dir).unwrap();
        // Weights sum to 0.8, not 1.0
        fs::write(mengxi_dir.join("config"), "[search]\ngrading_weight = 0.5\nclip_weight = 0.2\ntag_weight = 0.1\n").unwrap();

        let result = resolve_search_config(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("CONFIG_VALIDATION_ERROR"));
        assert!(err.contains("sum to 1.0"));
    }

    #[test]
    fn test_resolve_search_config_weight_minimum_validation() {
        let dir = tempfile::tempdir().unwrap();
        let mengxi_dir = dir.path().join(".mengxi");
        fs::create_dir_all(&mengxi_dir).unwrap();
        // Weight 0.05 is below minimum 0.1
        fs::write(mengxi_dir.join("config"), "[search]\ngrading_weight = 0.05\nclip_weight = 0.5\ntag_weight = 0.45\n").unwrap();

        let result = resolve_search_config(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("CONFIG_VALIDATION_ERROR"));
        assert!(err.contains(">= 0.1"));
    }

    #[test]
    fn test_resolve_search_config_nan_weight_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mengxi_dir = dir.path().join(".mengxi");
        fs::create_dir_all(&mengxi_dir).unwrap();
        // NaN weight should be rejected
        fs::write(mengxi_dir.join("config"), "[search]\ngrading_weight = 0.5\nclip_weight = 0.3\ntag_weight = nan\n").unwrap();

        let result = resolve_search_config(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("CONFIG_VALIDATION_ERROR"));
        assert!(err.contains("finite"));
    }

    #[test]
    fn test_resolve_search_config_default_mode_balanced() {
        let dir = tempfile::tempdir().unwrap();
        let mengxi_dir = dir.path().join(".mengxi");
        fs::create_dir_all(&mengxi_dir).unwrap();
        fs::write(mengxi_dir.join("config"), "[search]\ndefault_mode = \"balanced\"\n").unwrap();

        let result = resolve_search_config(dir.path()).unwrap();
        assert!((result.grading - 0.4).abs() < 1e-10);
        assert!((result.clip - 0.4).abs() < 1e-10);
        assert!((result.tag - 0.2).abs() < 1e-10);
    }

    #[test]
    fn test_search_config_default_values() {
        let config = SearchConfig::default();
        assert!((config.grading_weight - 0.6).abs() < 1e-10);
        assert!((config.clip_weight - 0.3).abs() < 1e-10);
        assert!((config.tag_weight - 0.1).abs() < 1e-10);
        assert_eq!(config.default_mode, "grading-first");
    }

    #[test]
    fn test_config_without_search_section_backward_compat() {
        // Config without [search] section — serde default fills in
        let toml_str = "[general]\nlog_level = \"debug\"\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.general.log_level, "debug");
        assert!((config.search.grading_weight - 0.6).abs() < 1e-10);
        assert_eq!(config.search.default_mode, "grading-first");
    }

    #[test]
    fn test_find_project_config_stops_at_filesystem_root() {
        // Use a temp dir — no .mengxi/config anywhere above it
        let dir = tempfile::tempdir().unwrap();
        let result = find_project_config(dir.path());
        assert!(result.is_none());
    }
}
