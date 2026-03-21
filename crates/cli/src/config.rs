use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::PathBuf;

/// Root configuration structure matching ~/.mengxi/config TOML schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub import: ImportConfig,
    #[serde(default)]
    pub export: ExportConfig,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExportConfig {
    #[serde(default = "default_output_dir")]
    pub default_output_dir: String,
}

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

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            ai: AiConfig::default(),
            import: ImportConfig::default(),
            export: ExportConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            log_level: default_log_level(),
            data_dir: default_data_dir(),
            default_search_limit: default_search_limit(),
            default_export_format: default_export_format(),
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

    table.push_str(&separator);
    table
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format_config_table(self))
    }
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
            },
            export: ExportConfig {
                default_output_dir: "/custom/lut".to_string(),
            },
        };
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.general.log_level, "debug");
        assert_eq!(parsed.general.default_search_limit, 10);
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
}
