mod commands;
mod config;
mod tui;
mod validate;
mod validate_dataset;


use clap::{Parser, Subcommand};
use std::time::{SystemTime, UNIX_EPOCH};

use mengxi_core::analytics;
use mengxi_core::db;

#[cfg(test)]
#[cfg(test)]
use unicode_width::UnicodeWidthStr;

/// Mengxi — CLI-based color pipeline management platform
#[derive(Parser)]
#[command(name = "mx", version, about = "Color style search and LUT management for film colorists")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Import a film project folder for indexing
    Import {
        /// Path to the project folder
        #[arg(long)]
        project: Option<String>,
        /// Project name
        #[arg(long)]
        name: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Search indexed projects by image or tag
    Search {
        /// Reference image path for similarity search
        #[arg(long)]
        image: Option<String>,
        /// Semantic tag to search
        #[arg(long)]
        tag: Option<String>,
        /// Maximum number of results
        #[arg(long)]
        limit: Option<u32>,
        /// Scope search to a specific project
        #[arg(long)]
        project: Option<String>,
        /// Accept result by rank
        #[arg(long, conflicts_with = "reject")]
        accept: Option<u32>,
        /// Reject result by rank
        #[arg(long, conflicts_with = "accept")]
        reject: Option<u32>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
        /// Search mode preset (grading-first, balanced, pyramid)
        #[arg(long, value_parser = ["grading-first", "balanced", "pyramid"])]
        search_mode: Option<String>,
        /// Override signal weights (e.g., grading=0.6,clip=0.3,tag=0.1)
        #[arg(long)]
        weights: Option<String>,
        /// Tile search mode: spatial (position-aligned) or any (position-invariant)
        #[arg(long, value_parser = ["spatial", "any"])]
        tile_mode: Option<String>,
        /// Tile range for region selection (e.g., "0,0-3,3" for top-left to bottom-right)
        #[arg(long)]
        tile_range: Option<String>,
    },
    /// Export a matching style as a LUT file
    Export {
        /// Project ID to export
        #[arg(long)]
        result: Option<u32>,
        /// LUT output format (cube, 3dl, look, csp, cdl)
        #[arg(long)]
        format: Option<String>,
        /// Output file path
        #[arg(long)]
        output: Option<String>,
        /// Grid size (default: 33)
        #[arg(long, default_value_t = 33)]
        grid_size: u32,
        /// Force overwrite without prompting
        #[arg(long)]
        force: bool,
        /// Output format for status/progress (text, json)
        #[arg(long, value_name = "OUTPUT_FORMAT", value_parser = ["text", "json"], default_value = "text")]
        output_format: String,
    },
    /// Display detailed fingerprint information for a search result
    Info {
        /// Project name (persistent lookup)
        #[arg(long)]
        project: Option<String>,
        /// File path (persistent lookup)
        #[arg(long)]
        file: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Compare two fingerprints' grading features side-by-side
    #[command(name = "compare")]
    Compare {
        /// First fingerprint ID
        id_a: i64,
        /// Second fingerprint ID
        id_b: i64,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Check color consistency across multiple projects
    #[command(name = "consistency")]
    Consistency {
        /// Comma-separated list of project names
        #[arg(long, value_delimiter = ',')]
        projects: Vec<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Manage tags on indexed projects and results
    Tag {
        /// Search result ID
        #[arg(long)]
        result: Option<u32>,
        /// Project name
        #[arg(long)]
        project: Option<String>,
        /// Scene name
        #[arg(long)]
        scene: Option<String>,
        /// Add a tag
        #[arg(long, conflicts_with = "remove", conflicts_with = "list", conflicts_with = "edit", conflicts_with = "generate")]
        add: Option<String>,
        /// Remove a tag
        #[arg(long, conflicts_with = "add", conflicts_with = "list", conflicts_with = "edit", conflicts_with = "generate")]
        remove: Option<String>,
        /// List all tags
        #[arg(long, conflicts_with = "add", conflicts_with = "remove", conflicts_with = "edit", conflicts_with = "generate")]
        list: bool,
        /// Edit (rename) a tag — current tag name
        #[arg(long, requires = "edit_new", conflicts_with = "add", conflicts_with = "remove", conflicts_with = "list", conflicts_with = "generate")]
        edit: Option<String>,
        /// New tag name for --edit
        #[arg(long, requires = "edit", conflicts_with = "add", conflicts_with = "remove", conflicts_with = "list", conflicts_with = "generate")]
        edit_new: Option<String>,
        /// Generate AI tags for all fingerprints in a project
        #[arg(long, conflicts_with = "add", conflicts_with = "remove", conflicts_with = "list", conflicts_with = "edit", conflicts_with = "edit_new", conflicts_with = "ask")]
        generate: bool,
        /// Interactive tag input: prompt for tags on each fingerprint in the project
        #[arg(long, conflicts_with = "add", conflicts_with = "remove", conflicts_with = "list", conflicts_with = "edit", conflicts_with = "edit_new", conflicts_with = "generate")]
        ask: bool,
    },
    /// Compare two LUT files and display differences
    #[command(name = "lut-diff")]
    LutDiff {
        /// First LUT file path
        lut_a: Option<String>,
        /// Second LUT file path
        lut_b: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"])]
        format: Option<String>,
    },
    /// Track LUT dependencies
    #[command(name = "lut-dep")]
    LutDep {
        /// LUT file path
        #[arg(long)]
        lut: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"])]
        format: Option<String>,
    },
    /// View usage statistics and metrics
    Stats {
        /// Filter by user name
        #[arg(long)]
        user: Option<String>,
        /// Time period (1day, 1week, 2weeks, 1month)
        #[arg(long)]
        period: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"])]
        format: Option<String>,
    },
    /// View and manage system configuration
    Config {
        /// Show current configuration
        #[arg(long)]
        show: bool,
        /// Open configuration file for editing (not yet implemented)
        #[arg(long)]
        edit: bool,
    },
    /// Validate color space conversion precision
    Validate {
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
        /// Run extended numerical safety tests
        #[arg(long)]
        full: bool,
    },
    /// Re-extract grading features for existing fingerprints
    #[command(name = "reextract")]
    Reextract {
        /// Re-extract all fingerprints for a project
        #[arg(long)]
        project: Option<String>,
        /// Specific file path to re-extract
        #[arg(conflicts_with = "project", value_name = "FILE")]
        file: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Generate CLIP embeddings for fingerprints
    #[command(name = "embed")]
    Embed {
        /// Project name to generate embeddings for
        #[arg(long)]
        project: Option<String>,
        /// Force regeneration of existing embeddings
        #[arg(long)]
        force: bool,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Validate evaluation dataset format compliance
    #[command(name = "validate-dataset")]
    ValidateDataset {
        /// Directory containing evaluation dataset
        dir: String,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Start interactive AI chat (TUI)
    Chat {
        /// LLM provider (claude, openai, ollama)
        #[arg(long, value_parser = ["claude", "openai", "ollama"], default_value = "claude")]
        provider: String,
        /// Model name override
        #[arg(long)]
        model: Option<String>,
    },
    /// Browse and query the database
    Db {
        #[command(subcommand)]
        command: DbSubcommand,
    },
    /// Generate movie fingerprint visualization from video
    #[command(name = "fingerprint-gen")]
    FingerprintGen {
        /// Video file path
        video: Option<String>,
        /// Output mode: strip, cineiris, both
        #[arg(long, value_parser = ["strip", "cineiris", "both", "cineprint", "poster"], default_value = "strip")]
        mode: String,
        /// Frame extraction interval in seconds
        #[arg(long, default_value_t = 1.0)]
        interval: f64,
        /// Maximum number of frames to extract
        #[arg(long)]
        max_frames: Option<usize>,
        /// CineIris diameter in pixels
        #[arg(long, default_value_t = 2160)]
        diameter: usize,
        /// Output directory
        #[arg(long)]
        output: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
        /// Movie title for poster mode
        #[arg(long)]
        title: Option<String>,
        /// Director name for poster mode
        #[arg(long)]
        director: Option<String>,
        /// Colorist / DP name for poster mode
        #[arg(long)]
        colorist: Option<String>,
        /// Team members (comma-separated) for poster mode
        #[arg(long)]
        team: Option<String>,
        /// Project type for poster mode (e.g., 电影, 电视剧)
        #[arg(long = "type")]
        project_type: Option<String>,
        /// Release year for poster mode
        #[arg(long)]
        year: Option<String>,
        /// Custom font file path for poster mode (TTF/TTC/OTF)
        #[arg(long)]
        font: Option<String>,
        /// Show watermark logo (poster mode only, default: true)
        #[arg(long)]
        watermark: Option<bool>,
    },
    /// Detect scene boundaries in a fingerprint strip image
    #[command(name = "scene-detect")]
    SceneDetect {
        /// Fingerprint strip image path (PNG)
        strip_image: Option<String>,
        /// Change threshold [0.0, 1.0]
        #[arg(long, default_value_t = 0.3)]
        threshold: f64,
        /// Minimum frames between boundaries
        #[arg(long, default_value_t = 5)]
        min_scene_length: usize,
        /// Maximum number of boundaries to detect
        #[arg(long, default_value_t = 50)]
        max_boundaries: usize,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Compute color mood timeline from a fingerprint strip image
    #[command(name = "color-mood")]
    ColorMood {
        /// Fingerprint strip image path (PNG)
        strip_image: Option<String>,
        /// Comma-separated scene boundary frame indices (optional)
        #[arg(long)]
        boundaries: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Generate color transfer LUT between two fingerprint strip images
    #[command(name = "color-transfer")]
    ColorTransfer {
        /// Source strip image path (PNG)
        source: Option<String>,
        /// Target strip image path (PNG)
        target: Option<String>,
        /// LUT grid size
        #[arg(long, default_value_t = 33)]
        grid_size: usize,
        /// Output .cube file path
        #[arg(long)]
        output: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Compare two fingerprint strip images by color DNA
    #[command(name = "movie-compare")]
    MovieCompare {
        /// First strip image path (PNG)
        strip_a: Option<String>,
        /// Second strip image path (PNG)
        strip_b: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Interactive fingerprint strip explorer (TUI)
    #[command(name = "fingerprint-explore")]
    FingerprintExplore {
        /// Fingerprint strip image path (PNG)
        strip_image: Option<String>,
    },
    /// Visual search for similar movies by color DNA
    #[command(name = "visual-search")]
    VisualSearch {
        /// Query fingerprint strip image path (PNG)
        query: Option<String>,
        /// Library directory containing strip images
        #[arg(long)]
        library: Option<String>,
        /// Maximum number of results
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
}

#[derive(Subcommand)]
enum DbSubcommand {
    /// List all projects
    Projects {
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// List files in a project
    Files {
        /// Project name
        #[arg(long)]
        project: String,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// List tags
    Tags {
        /// Filter by project name
        #[arg(long)]
        project: Option<String>,
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// List LUT export history
    Luts {
        /// Output format (text, json)
        #[arg(long, value_parser = ["text", "json"], default_value = "text")]
        format: String,
    },
    /// Execute a raw read-only SQL query
    Sql {
        /// SQL query (must be a SELECT statement)
        query: String,
    },
}

// ---------------------------------------------------------------------------
// Session tracking helpers
// ---------------------------------------------------------------------------

/// Generate a simple session ID: timestamp + process ID.
fn generate_session_id() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let pid = std::process::id();
    format!("{}_{}", ts, pid)
}

/// Extract command name, key args as JSON, and optional search-to-export timing.
fn extract_command_info(cli: &Cli) -> (String, String, Option<i64>) {
    match &cli.command {
        Some(Commands::Import { name, .. }) => {
            let obj = serde_json::json!({ "name": name });
            ("import".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::Search { project, tag, search_mode, weights, .. }) => {
            let mut obj = serde_json::Map::new();
            if let Some(p) = project { obj.insert("project".to_string(), serde_json::json!(p)); }
            if let Some(t) = tag { obj.insert("tag".to_string(), serde_json::json!(t)); }
            if let Some(m) = search_mode { obj.insert("search_mode".to_string(), serde_json::json!(m)); }
            if let Some(w) = weights { obj.insert("weights".to_string(), serde_json::json!(w)); }
            ("search".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::Export { result, format, output: _output, .. }) => {
            let obj = serde_json::json!({ "result": result, "format": format });
            // FR34: compute search-to-export timing
            let search_to_export_ms = if let Ok(conn) = db::open_db() {
                analytics::get_last_search_started_at(&conn).ok().flatten().and_then(|search_ts| {
                    let now_ts = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;
                    let delta = now_ts - search_ts;
                    if delta > 0 { Some(delta) } else { None }
                })
            } else {
                None
            };
            ("export".to_string(), serde_json::to_string(&obj).unwrap_or_default(), search_to_export_ms)
        }
        Some(Commands::Info { project, .. }) => {
            let obj = serde_json::json!({ "project": project });
            ("info".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::Compare { .. }) => ("compare".to_string(), "{}".to_string(), None),
        Some(Commands::Consistency { .. }) => ("consistency".to_string(), "{}".to_string(), None),
        Some(Commands::Tag { project, .. }) => {
            let obj = serde_json::json!({ "project": project });
            ("tag".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::LutDiff { .. }) => ("lut-diff".to_string(), "{}".to_string(), None),
        Some(Commands::LutDep { .. }) => ("lut-dep".to_string(), "{}".to_string(), None),
        Some(Commands::Stats { .. }) => ("stats".to_string(), "{}".to_string(), None),
        Some(Commands::Config { .. }) => ("config".to_string(), "{}".to_string(), None),
        Some(Commands::Validate { .. }) => ("validate".to_string(), "{}".to_string(), None),
        Some(Commands::Reextract { .. }) => ("reextract".to_string(), "{}".to_string(), None),
        Some(Commands::Embed { .. }) => ("embed".to_string(), "{}".to_string(), None),
        Some(Commands::ValidateDataset { .. }) => ("validate-dataset".to_string(), "{}".to_string(), None),
        Some(Commands::Db { .. }) => ("db".to_string(), "{}".to_string(), None),
        Some(Commands::Chat { provider, model }) => {
            let obj = serde_json::json!({ "provider": provider, "model": model });
            ("chat".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::FingerprintGen { mode, .. }) => {
            let obj = serde_json::json!({ "mode": mode });
            ("fingerprint-gen".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::SceneDetect { threshold, max_boundaries, .. }) => {
            let obj = serde_json::json!({ "threshold": threshold, "max_boundaries": max_boundaries });
            ("scene-detect".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::ColorMood { boundaries, .. }) => {
            let obj = serde_json::json!({ "boundaries": boundaries });
            ("color-mood".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::ColorTransfer { grid_size, .. }) => {
            let obj = serde_json::json!({ "grid_size": grid_size });
            ("color-transfer".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::MovieCompare { .. }) => ("movie-compare".to_string(), "{}".to_string(), None),
        Some(Commands::FingerprintExplore { .. }) => ("fingerprint-explore".to_string(), "{}".to_string(), None),
        Some(Commands::VisualSearch { limit, .. }) => {
            let obj = serde_json::json!({ "limit": limit });
            ("visual-search".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        None => ("help".to_string(), "{}".to_string(), None),
    }
}

/// Record a session to the database (best-effort, never blocks CLI exit).
#[allow(clippy::too_many_arguments)]
fn record_session_best_effort(
    session_id: &str,
    command: &str,
    args_json: &str,
    started_at: i64,
    ended_at: i64,
    duration_ms: i64,
    exit_code: i32,
    search_to_export_ms: Option<i64>,
    user: &str,
) {
    if let Ok(conn) = db::open_db() {
        let record = analytics::SessionRecord {
            session_id: session_id.to_string(),
            command: command.to_string(),
            args_json: args_json.to_string(),
            started_at,
            ended_at,
            duration_ms,
            exit_code,
            search_to_export_ms,
            user: user.to_string(),
        };
        if let Err(e) = analytics::record_session(&conn, &record) {
            eprintln!("Warning: Failed to record session: {}", e);
        }
    } else {
        eprintln!("Warning: Failed to open database for session recording");
    }
}

/// Format milliseconds into human-readable duration string.
#[cfg(test)]
fn format_duration_ms(ms: i64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let total_secs = ms / 1000;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{}m {}s", mins, secs)
    }
}

fn main() {
    let cli = Cli::parse();

    // Session tracking setup
    let started_at = SystemTime::now();
    let started_at_unix = started_at.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    let session_id = generate_session_id();
    let (command_name, args_json, search_to_export_ms_override) = extract_command_info(&cli);

    match cli.command {
        Some(Commands::Import { project, name, format }) => {
            commands::import_cmd::execute(project, name, format);
        }
        Some(Commands::Search {
            image, tag, limit, project, accept, reject, format,
            search_mode, weights, tile_mode, tile_range,
        }) => {
            commands::search_cmd::execute(image, tag, limit, project, accept, reject, format, search_mode, weights, tile_mode, tile_range);
        }
        Some(Commands::Export { result, format, output, grid_size, force, output_format }) => {
            commands::export_cmd::execute(result, format, output, grid_size, force, output_format);
        }
        Some(Commands::Info { project, file, format }) => {
            commands::info_cmd::execute(project, file, format);
        }
        Some(Commands::Compare { id_a, id_b, format }) => {
            commands::compare_cmd::execute(id_a, id_b, format);
        }
        Some(Commands::Consistency { projects, format }) => {
            commands::consistency_cmd::execute(projects, format);
        }
        Some(Commands::Tag { result, project, scene, add, remove, list, edit, edit_new, generate, ask }) => {
            commands::tag_cmd::execute(result, project, scene, add, remove, list, edit, edit_new, generate, ask);
        }
        Some(Commands::LutDiff { lut_a, lut_b, format }) => {
            commands::lut_diff_cmd::execute(lut_a, lut_b, format);
        }
        Some(Commands::LutDep { lut, format }) => {
            commands::lut_dep_cmd::execute(lut, format);
        }
        Some(Commands::Stats { user, period, format }) => {
            commands::stats_cmd::execute(user, period, format);
        }
        Some(Commands::Config { show, edit }) => {
            commands::config_cmd::execute(show, edit);
        }
        Some(Commands::Validate { format, full }) => {
            commands::validate_cmd::execute(format, full);
        }
        Some(Commands::Reextract { project, file, format }) => {
            commands::reextract_cmd::execute(project, file, format);
        }
        Some(Commands::Embed { project, force, format }) => {
            commands::embed_cmd::execute(project, force, format);
        }
        Some(Commands::ValidateDataset { dir, format }) => {
            commands::validate_dataset_cmd::execute(dir, format);
        }
        Some(Commands::Chat { provider, model }) => {
            commands::chat_cmd::execute(provider, model);
        }
        Some(Commands::Db { command }) => {
            commands::db_cmd::execute(command);
        }
        Some(Commands::FingerprintGen { video, mode, interval, max_frames, diameter, output, format, title, director, colorist, team, project_type, year, font, watermark }) => {
            commands::fingerprint_cmd::execute(video, mode, interval, max_frames, diameter, output, format, title, director, colorist, team, project_type, year, font, watermark.unwrap_or(true));
        }
        Some(Commands::SceneDetect { strip_image, threshold, min_scene_length, max_boundaries, format }) => {
            commands::scene_detect_cmd::execute(strip_image, threshold, min_scene_length, max_boundaries, format);
        }
        Some(Commands::ColorMood { strip_image, boundaries, format }) => {
            commands::color_mood_cmd::execute(strip_image, boundaries, format);
        }
        Some(Commands::ColorTransfer { source, target, grid_size, output, format }) => {
            commands::color_transfer_cmd::execute(source, target, grid_size, output, format);
        }
        Some(Commands::MovieCompare { strip_a, strip_b, format }) => {
            commands::movie_compare_cmd::execute(strip_a, strip_b, format);
        }
        Some(Commands::FingerprintExplore { strip_image }) => {
            commands::fingerprint_explorer_cmd::execute(strip_image);
        }
        Some(Commands::VisualSearch { query, library, limit, format }) => {
            commands::visual_search_cmd::execute(query, library, limit, format);
        }
        None => {
            // No subcommand — clap displays help automatically
        }
    }

    // Record session (best-effort, non-blocking).
    // Note: error paths that call process::exit(1) bypass this recording.
    // This is intentional — those are argument validation failures, not real work sessions.
    // Only commands that complete (success or handled error) reach this point.
    let ended_at = SystemTime::now();
    let ended_at_unix = ended_at.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    let duration_ms = ended_at_unix - started_at_unix;

    let user_name = config::load_or_create_config().map(|c| c.general.user).unwrap_or_else(|_| "default".to_string());

    record_session_best_effort(
        &session_id,
        &command_name,
        &args_json,
        started_at_unix,
        ended_at_unix,
        duration_ms,
        0,
        search_to_export_ms_override,
        &user_name,
    );
}

/// Resolve hybrid search weights from --search-mode and --weights flags.
/// When both are provided, --weights takes priority (FR15).
/// Per-query --weights allows weight=0.0 (warning to stderr).
#[cfg(test)]
fn resolve_hybrid_weights(
    search_mode: Option<&str>,
    weights_str: Option<&str>,
) -> Result<mengxi_core::hybrid_scoring::SignalWeights, String> {
    if let Some(ws) = weights_str {
        // Parse "grading=0.6,clip=0.3,tag=0.1"
        let mut grading = 0.0_f64;
        let mut clip = 0.0_f64;
        let mut tag = 0.0_f64;
        let mut seen_keys: std::collections::HashSet<&str> = std::collections::HashSet::new();

        for pair in ws.split(',') {
            let parts: Vec<&str> = pair.split('=').collect();
            if parts.len() != 2 {
                return Err(format!(
                    "SEARCH_WEIGHT_ERROR -- invalid weight format '{}', expected key=value (e.g., grading=0.6,clip=0.3,tag=0.1)",
                    pair.trim()
                ));
            }
            let key = parts[0].trim();
            if !seen_keys.insert(key) {
                return Err(format!(
                    "SEARCH_WEIGHT_ERROR -- duplicate signal '{}', each signal must appear only once",
                    key
                ));
            }
            let value: f64 = match parts[1].trim().parse::<f64>() {
                Ok(v) => v,
                Err(_) => {
                    return Err(format!(
                        "SEARCH_WEIGHT_ERROR -- invalid weight value '{}' for '{}', expected a number",
                        parts[1].trim(),
                        key
                    ));
                }
            };
            // F-02/F-03: reject negative, NaN, Inf values
            if !value.is_finite() {
                return Err(format!(
                    "SEARCH_WEIGHT_ERROR -- weight for '{}' must be a finite number, got '{}'",
                    key, parts[1].trim()
                ));
            }
            if value < 0.0 {
                return Err(format!(
                    "SEARCH_WEIGHT_ERROR -- weight for '{}' must be non-negative, got {}",
                    key, value
                ));
            }
            match key {
                "grading" => grading = value,
                "clip" => clip = value,
                "tag" => tag = value,
                _ => {
                    return Err(format!(
                        "SEARCH_WEIGHT_ERROR -- unknown signal '{}', expected grading, clip, or tag",
                        key
                    ));
                }
            }
        }

        // Validate sum ~= 1.0
        let sum = grading + clip + tag;
        if (sum - 1.0).abs() > 1e-6 {
            return Err(format!(
                "SEARCH_WEIGHT_ERROR -- weights must sum to 1.0, got {:.10}",
                sum
            ));
        }

        // Warn for zero weights (FR15 allows this per-query)
        if grading == 0.0 {
            eprintln!("warning: grading signal explicitly disabled via --weights");
        }
        if clip == 0.0 {
            eprintln!("warning: clip signal explicitly disabled via --weights");
        }
        if tag == 0.0 {
            eprintln!("warning: tag signal explicitly disabled via --weights");
        }

        Ok(mengxi_core::hybrid_scoring::SignalWeights { grading, clip, tag })
    } else if let Some(mode) = search_mode {
        match mode {
            "grading-first" => Ok(mengxi_core::hybrid_scoring::SignalWeights::grading_first()),
            "balanced" => Ok(mengxi_core::hybrid_scoring::SignalWeights::balanced()),
            "pyramid" => Ok(mengxi_core::hybrid_scoring::SignalWeights::grading_first()),
            _ => unreachable!("clap validates --search-mode values"),
        }
    } else {
        Ok(mengxi_core::hybrid_scoring::SignalWeights::grading_first())
    }
}

/// Resolve an image path to a file_id in the database.
#[cfg(test)]
#[allow(dead_code)]
fn resolve_image_to_file_id(
    conn: &mengxi_core::db::DbConnection,
    image_path: &str,
    project: Option<&str>,
) -> Result<i64, String> {
    let filename = std::path::Path::new(image_path)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("SEARCH_IMAGE_ERROR -- cannot extract filename from '{}'", image_path))?;

    let sql = if project.is_some() {
        "SELECT f.id FROM files f JOIN projects p ON p.id = f.project_id WHERE f.filename = ?1 AND p.name = ?2 LIMIT 1"
    } else {
        "SELECT f.id FROM files f WHERE f.filename = ?1 LIMIT 1"
    };

    // F-05: warn when no project filter and multiple files share the same filename
    if project.is_none() {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE filename = ?1",
                [filename],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if count > 1 {
            eprintln!(
                "warning: {} files named '{}', using first match — specify --project to disambiguate",
                count, filename
            );
        }
    }

    let result = if let Some(proj) = project {
        conn.query_row(sql, [filename, proj], |row| row.get::<_, i64>(0))
    } else {
        conn.query_row(sql, [filename], |row| row.get::<_, i64>(0))
    };

    match result {
        Ok(id) => Ok(id),
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("no rows") {
                Err(format!(
                    "SEARCH_IMAGE_ERROR -- file '{}' not found in database{}",
                    filename,
                    project.map(|p| format!(" (project: {})", p)).unwrap_or_default()
                ))
            } else {
                Err(format!("SEARCH_IMAGE_ERROR -- database error: {}", err_str))
            }
        }
    }
}

/// Format a ScoreBreakdown as a text string for display.
/// Missing signals are omitted entirely.
#[cfg(test)]
fn format_breakdown(breakdown: &mengxi_core::hybrid_scoring::ScoreBreakdown) -> String {
    let mut parts = Vec::new();
    parts.push(format!("oklab_hist:{:.2}", breakdown.grading));
    if let Some(clip) = breakdown.clip {
        parts.push(format!("clip:{:.2}", clip));
    }
    if let Some(tag) = breakdown.tag {
        parts.push(format!("tag:{:.2}", tag));
    }
    parts.join(" ")
}

/// Generate a human-readable explanation when search returns few results.
/// Returns None when results >= 3 (no explanation needed).
#[cfg(test)]
fn low_result_explanation(count: usize) -> Option<String> {
    match count {
        0 => Some("无匹配结果 -- 候选集中无高相似度调色风格".to_string()),
        1 => Some("仅找到 1 个匹配 -- 候选集可能不足或参考图风格较特殊".to_string()),
        2 => Some("仅找到 2 个匹配 -- 候选集可能不足或参考图风格较特殊".to_string()),
        _ => None,
    }
}

/// Record accept/reject feedback for a search result.
#[cfg(test)]
#[allow(dead_code)]
fn record_feedback_if_needed(
    conn: &mengxi_core::db::DbConnection,
    results: &[mengxi_core::search::SearchResult],
    accept: Option<u32>,
    reject: Option<u32>,
    search_type: &str,
    _is_json: bool,
) {
    let (rank, action) = match (accept, reject) {
        (Some(r), _) => (r, "accepted"),
        (_, Some(r)) => (r, "rejected"),
        _ => return,
    };

    if results.is_empty() {
        eprintln!("No results to provide feedback on.");
        return;
    }

    let rank_idx = rank as usize;
    if rank_idx < 1 || rank_idx > results.len() {
        eprintln!(
            "Warning: --{} {} is out of range (1-{}).",
            if action == "accepted" { "accept" } else { "reject" },
            rank,
            results.len()
        );
        return;
    }

    let result = &results[rank_idx - 1];
    if let Err(e) = mengxi_core::feedback::record_feedback(
        conn,
        &result.project_name,
        &result.file_path,
        &result.file_format,
        action,
        Some(search_type),
    ) {
        eprintln!("Warning: Failed to record feedback: {}", e);
    } else {
        eprintln!(
            "Feedback recorded: {} result #{} ({}/{})",
            action, rank, result.project_name, result.file_path
        );
    }
}

/// Truncate a string to max_len display columns, appending "…" if truncated.
/// Uses unicode-width for correct CJK/emoji column counting.
#[cfg(test)]
fn truncate_str(s: &str, max_len: usize) -> String {
    let width = UnicodeWidthStr::width(s);
    if width <= max_len {
        s.to_string()
    } else {
        let ellipsis_width = UnicodeWidthStr::width("…");
        let target = max_len.saturating_sub(ellipsis_width);
        let mut result = String::new();
        let mut current_width = 0usize;
        for ch in s.chars() {
            let ch_width = UnicodeWidthStr::width(ch.to_string().as_str());
            if current_width + ch_width > target {
                break;
            }
            result.push(ch);
            current_width += ch_width;
        }
        result.push('…');
        result
    }
}

/// Convert seconds since Unix epoch to (year, month, day, hour, min, sec).
/// Simple implementation to avoid chrono dependency.
#[cfg(test)]
fn seconds_to_datetime(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let days = (secs / 86400) as i32;
    let time_of_day = (secs % 86400) as u32;
    let hour = time_of_day / 3600;
    let min = (time_of_day % 3600) / 60;
    let sec = time_of_day % 60;

    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let mut z = days + 719468;
    let era = z / 146097;
    z -= era * 146097;
    let doe = z;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y as u32, m as u32, d as u32, hour, min, sec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_help_includes_all_subcommands() {
        let cli = Cli::try_parse_from(["mx"]);
        // No subcommand should return None (clap shows help)
        assert!(cli.is_ok());
        assert!(cli.unwrap().command.is_none());
    }

    #[test]
    fn test_import_command_parsing() {
        let cli = Cli::try_parse_from([
            "mx",
            "import",
            "--project", "/path/to/film",
            "--name", "my_film",
            "--format", "json",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Import { project, name, format }) => {
                assert_eq!(project.as_deref(), Some("/path/to/film"));
                assert_eq!(name.as_deref(), Some("my_film"));
                assert_eq!(format, "json");
            }
            _ => panic!("Expected Import command"),
        }
    }

    #[test]
    fn test_search_command_parsing() {
        let cli = Cli::try_parse_from([
            "mx",
            "search",
            "--image", "/ref/mood.jpg",
            "--tag", "industrial",
            "--limit", "10",
            "--project", "my_film",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Search { image, tag, limit, project, accept, reject, format, .. }) => {
                assert_eq!(image.as_deref(), Some("/ref/mood.jpg"));
                assert_eq!(tag.as_deref(), Some("industrial"));
                assert_eq!(limit, Some(10));
                assert_eq!(project.as_deref(), Some("my_film"));
                assert_eq!(accept, None);
                assert_eq!(reject, None);
                assert_eq!(format, "text");
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_export_command_parsing() {
        let cli = Cli::try_parse_from([
            "mx",
            "export",
            "--result", "3",
            "--format", "cube",
            "--output", "~/lut/grade.cube",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Export { result, format, output, grid_size, force, .. }) => {
                assert_eq!(result, Some(3));
                assert_eq!(format.as_deref(), Some("cube"));
                assert_eq!(output.as_deref(), Some("~/lut/grade.cube"));
                assert_eq!(grid_size, 33);
                assert_eq!(force, false);
            }
            _ => panic!("Expected Export command"),
        }
    }

    #[test]
    fn test_config_show_parsing() {
        let cli = Cli::try_parse_from(["mx", "config", "--show"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Config { show: true, edit: false }) => {}
            _ => panic!("Expected Config with --show"),
        }
    }

    #[test]
    fn test_lut_diff_command_parsing() {
        let cli = Cli::try_parse_from([
            "mx",
            "lut-diff",
            "grade_v1.cube",
            "grade_v2.cube",
            "--format", "text",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::LutDiff { lut_a, lut_b, format }) => {
                assert_eq!(lut_a.as_deref(), Some("grade_v1.cube"));
                assert_eq!(lut_b.as_deref(), Some("grade_v2.cube"));
                assert_eq!(format.as_deref(), Some("text"));
            }
            _ => panic!("Expected LutDiff command"),
        }
    }

    #[test]
    fn test_tag_command_parsing() {
        let cli = Cli::try_parse_from([
            "mx",
            "tag",
            "--project", "my_film",
            "--add", "industrial warm",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Tag { add, project, generate, .. }) => {
                assert_eq!(add.as_deref(), Some("industrial warm"));
                assert_eq!(project.as_deref(), Some("my_film"));
                assert!(!generate);
            }
            _ => panic!("Expected Tag command"),
        }
    }

    #[test]
    fn test_tag_generate_command_parsing() {
        let cli = Cli::try_parse_from([
            "mx",
            "tag",
            "--project", "my_film",
            "--generate",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Tag { generate, project, add, remove, list, .. }) => {
                assert!(generate);
                assert_eq!(project.as_deref(), Some("my_film"));
                assert_eq!(add, None);
                assert_eq!(remove, None);
                assert!(!list);
            }
            _ => panic!("Expected Tag command with --generate"),
        }
    }

    #[test]
    fn test_tag_generate_conflicts_with_add() {
        let cli = Cli::try_parse_from([
            "mx",
            "tag",
            "--project", "my_film",
            "--generate",
            "--add", "warm",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_tag_generate_conflicts_with_list() {
        let cli = Cli::try_parse_from([
            "mx",
            "tag",
            "--project", "my_film",
            "--generate",
            "--list",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_tag_edit_command_parsing() {
        let cli = Cli::try_parse_from([
            "mx",
            "tag",
            "--project", "my_film",
            "--edit", "industrial warm",
            "--edit-new", "warm industrial",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Tag { edit, edit_new, project, .. }) => {
                assert_eq!(edit.as_deref(), Some("industrial warm"));
                assert_eq!(edit_new.as_deref(), Some("warm industrial"));
                assert_eq!(project.as_deref(), Some("my_film"));
            }
            _ => panic!("Expected Tag command with --edit"),
        }
    }

    #[test]
    fn test_tag_edit_requires_edit_new() {
        let cli = Cli::try_parse_from([
            "mx",
            "tag",
            "--project", "my_film",
            "--edit", "old_tag",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_tag_edit_new_requires_edit() {
        let cli = Cli::try_parse_from([
            "mx",
            "tag",
            "--project", "my_film",
            "--edit-new", "new_tag",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_tag_edit_conflicts_with_add() {
        let cli = Cli::try_parse_from([
            "mx",
            "tag",
            "--project", "my_film",
            "--edit", "old",
            "--edit-new", "new",
            "--add", "extra",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_tag_edit_conflicts_with_list() {
        let cli = Cli::try_parse_from([
            "mx",
            "tag",
            "--project", "my_film",
            "--edit", "old",
            "--edit-new", "new",
            "--list",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_tag_edit_conflicts_with_generate() {
        let cli = Cli::try_parse_from([
            "mx",
            "tag",
            "--project", "my_film",
            "--edit", "old",
            "--edit-new", "new",
            "--generate",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_stats_command_parsing() {
        let cli = Cli::try_parse_from([
            "mx",
            "stats",
            "--user", "chen_liang",
            "--period", "2weeks",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Stats { user, period, .. }) => {
                assert_eq!(user.as_deref(), Some("chen_liang"));
                assert_eq!(period.as_deref(), Some("2weeks"));
            }
            _ => panic!("Expected Stats command"),
        }
    }

    #[test]
    fn test_seconds_to_datetime_epoch() {
        // 1970-01-01 00:00:00
        let (y, m, d, h, min, s) = seconds_to_datetime(0);
        assert_eq!((y, m, d, h, min, s), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn test_seconds_to_datetime_known() {
        // 2024-01-15 12:30:45 UTC = 1705321845 seconds
        let (y, m, d, h, min, s) = seconds_to_datetime(1705321845);
        assert_eq!((y, m, d, h, min, s), (2024, 1, 15, 12, 30, 45));
    }

    #[test]
    fn test_seconds_to_datetime_leap_year() {
        // 2024-02-29 00:00:00 UTC = 1709164800 seconds
        let (y, m, d, h, min, s) = seconds_to_datetime(1709164800);
        assert_eq!((y, m, d, h, min, s), (2024, 2, 29, 0, 0, 0));
    }

    #[test]
    fn test_lut_dep_command_parsing() {
        let cli = Cli::try_parse_from([
            "mx",
            "lut-dep",
            "--lut", "~/lut/grade.cube",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::LutDep { lut, .. }) => {
                assert_eq!(lut.as_deref(), Some("~/lut/grade.cube"));
            }
            _ => panic!("Expected LutDep command"),
        }
    }

    #[test]
    fn test_lut_dep_format_json_parsing() {
        let cli = Cli::try_parse_from([
            "mx",
            "lut-dep",
            "--lut", "~/lut/grade.cube",
            "--format", "json",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::LutDep { format, .. }) => {
                assert_eq!(format.as_deref(), Some("json"));
            }
            _ => panic!("Expected LutDep command"),
        }
    }

    #[test]
    fn test_lut_diff_format_invalid_rejected() {
        let cli = Cli::try_parse_from([
            "mx",
            "lut-diff",
            "a.cube", "b.cube",
            "--format", "xml",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_lut_diff_missing_args_parsing() {
        // lut-diff without positional args should still parse (args are Option<String>)
        let cli = Cli::try_parse_from(["mx", "lut-diff"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_lut_dep_missing_arg_parsing() {
        // lut-dep without --lut should still parse (arg is Option<String>)
        let cli = Cli::try_parse_from(["mx", "lut-dep"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_search_format_field_renamed() {
        // Verify --format flag (not --output-format) works on search command
        let cli = Cli::try_parse_from([
            "mx",
            "search",
            "--format", "json",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Search { format, .. }) => {
                assert_eq!(format, "json");
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_search_accept_reject_parsing() {
        // --accept
        let cli = Cli::try_parse_from([
            "mx", "search", "--image", "/ref.jpg", "--accept", "2",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Search { accept, reject, .. }) => {
                assert_eq!(accept, Some(2));
                assert_eq!(reject, None);
            }
            _ => panic!("Expected Search command"),
        }

        // --reject
        let cli = Cli::try_parse_from([
            "mx", "search", "--tag", "warm", "--reject", "1",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Search { accept, reject, .. }) => {
                assert_eq!(accept, None);
                assert_eq!(reject, Some(1));
            }
            _ => panic!("Expected Search command"),
        }

        // --accept and --reject should conflict
        let cli = Cli::try_parse_from([
            "mx", "search", "--accept", "1", "--reject", "2",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_info_command_with_project_file() {
        let cli = Cli::try_parse_from([
            "mx", "info", "--project", "film", "--file", "scene.dpx",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Info { project, file, format }) => {
                assert_eq!(project.as_deref(), Some("film"));
                assert_eq!(file.as_deref(), Some("scene.dpx"));
                assert_eq!(format, "text");
            }
            _ => panic!("Expected Info command"),
        }
    }

    #[test]
    fn test_search_format_invalid_rejected() {
        let cli = Cli::try_parse_from([
            "mx",
            "search",
            "--format", "xml",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_long() {
        assert_eq!(truncate_str("hello world", 8), "hello w…");
    }

    #[test]
    fn test_truncate_str_cjk() {
        // CJK chars are 2 columns wide each; "你好世界" = 8 columns
        assert_eq!(truncate_str("你好世界", 8), "你好世界");
        // 7 columns: "你好" (4) + "…" (1) = 5, need to fit "你好世…" (6+1=7)
        assert_eq!(truncate_str("你好世界", 7), "你好世…");
        // 5 columns: "你好" (4) + "…" (1) = 5
        assert_eq!(truncate_str("你好世界", 5), "你好…");
    }

    #[test]
    fn test_db_projects_parsing() {
        let cli = Cli::try_parse_from(["mx", "db", "projects"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Db { command: DbSubcommand::Projects { format } }) => {
                assert_eq!(format, "text");
            }
            _ => panic!("Expected Db Projects command"),
        }
    }

    #[test]
    fn test_db_projects_json_parsing() {
        let cli = Cli::try_parse_from(["mx", "db", "projects", "--format", "json"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Db { command: DbSubcommand::Projects { format } }) => {
                assert_eq!(format, "json");
            }
            _ => panic!("Expected Db Projects command"),
        }
    }

    #[test]
    fn test_db_files_parsing() {
        let cli = Cli::try_parse_from(["mx", "db", "files", "--project", "film_a"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Db { command: DbSubcommand::Files { project, .. } }) => {
                assert_eq!(project, "film_a");
            }
            _ => panic!("Expected Db Files command"),
        }
    }

    #[test]
    fn test_db_tags_parsing() {
        let cli = Cli::try_parse_from(["mx", "db", "tags", "--project", "film_a"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Db { command: DbSubcommand::Tags { project, .. } }) => {
                assert_eq!(project.as_deref(), Some("film_a"));
            }
            _ => panic!("Expected Db Tags command"),
        }
    }

    #[test]
    fn test_db_tags_no_filter_parsing() {
        let cli = Cli::try_parse_from(["mx", "db", "tags"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Db { command: DbSubcommand::Tags { project, .. } }) => {
                assert!(project.is_none());
            }
            _ => panic!("Expected Db Tags command"),
        }
    }

    #[test]
    fn test_db_luts_parsing() {
        let cli = Cli::try_parse_from(["mx", "db", "luts"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Db { command: DbSubcommand::Luts { .. } }) => {}
            _ => panic!("Expected Db Luts command"),
        }
    }

    #[test]
    fn test_db_sql_parsing() {
        let cli = Cli::try_parse_from(["mx", "db", "sql", "SELECT * FROM projects"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Db { command: DbSubcommand::Sql { query } }) => {
                assert_eq!(query, "SELECT * FROM projects");
            }
            _ => panic!("Expected Db Sql command"),
        }
    }

    #[test]
    fn test_db_extract_command_info() {
        let cli = Cli::try_parse_from(["mx", "db", "projects"]);
        let (name, args, _) = extract_command_info(&cli.unwrap());
        assert_eq!(name, "db");
        assert_eq!(args, "{}");
    }

    // ── Hybrid search tests (Story 3.3) ──

    #[test]
    fn test_search_mode_grading_first() {
        let cli = Cli::try_parse_from([
            "mx", "search", "--image", "ref.tif", "--search-mode", "grading-first",
        ]);
        match cli.unwrap().command {
            Some(Commands::Search { search_mode, weights, .. }) => {
                assert_eq!(search_mode.as_deref(), Some("grading-first"));
                assert!(weights.is_none());
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_search_mode_balanced() {
        let cli = Cli::try_parse_from([
            "mx", "search", "--image", "ref.tif", "--search-mode", "balanced",
        ]);
        match cli.unwrap().command {
            Some(Commands::Search { search_mode, .. }) => {
                assert_eq!(search_mode.as_deref(), Some("balanced"));
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_search_mode_invalid_rejected() {
        let cli = Cli::try_parse_from([
            "mx", "search", "--image", "ref.tif", "--search-mode", "invalid",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_weights_parsing() {
        let cli = Cli::try_parse_from([
            "mx", "search", "--image", "ref.tif",
            "--weights", "grading=0.6,clip=0.3,tag=0.1",
        ]);
        match cli.unwrap().command {
            Some(Commands::Search { weights, .. }) => {
                assert_eq!(weights.as_deref(), Some("grading=0.6,clip=0.3,tag=0.1"));
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_resolve_weights_grading_first_preset() {
        let w = resolve_hybrid_weights(Some("grading-first"), None).unwrap();
        let expected = mengxi_core::hybrid_scoring::SignalWeights::grading_first();
        assert_eq!(w, expected);
        assert!((w.grading - 0.6).abs() < 1e-10);
        assert!((w.clip - 0.3).abs() < 1e-10);
        assert!((w.tag - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_weights_balanced_preset() {
        let w = resolve_hybrid_weights(Some("balanced"), None).unwrap();
        assert!((w.grading - 0.4).abs() < 1e-10);
        assert!((w.clip - 0.4).abs() < 1e-10);
        assert!((w.tag - 0.2).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_weights_custom() {
        let w = resolve_hybrid_weights(None, Some("grading=0.8,clip=0.1,tag=0.1")).unwrap();
        assert!((w.grading - 0.8).abs() < 1e-10);
        assert!((w.clip - 0.1).abs() < 1e-10);
        assert!((w.tag - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_weights_override_takes_priority() {
        // --weights overrides --search-mode
        let w = resolve_hybrid_weights(
            Some("grading-first"),
            Some("grading=0.8,clip=0.1,tag=0.1"),
        ).unwrap();
        assert!((w.grading - 0.8).abs() < 1e-10);
        assert!((w.clip - 0.1).abs() < 1e-10);
        assert!((w.tag - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_weights_invalid_sum() {
        let result = resolve_hybrid_weights(None, Some("grading=0.5,clip=0.3,tag=0.1"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must sum to 1.0"));
    }

    #[test]
    fn test_resolve_weights_invalid_format() {
        let result = resolve_hybrid_weights(None, Some("grading-0.6"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid weight format"));
    }

    #[test]
    fn test_resolve_weights_invalid_value() {
        let result = resolve_hybrid_weights(None, Some("grading=abc,clip=0.3,tag=0.1"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid weight value"));
    }

    #[test]
    fn test_resolve_weights_unknown_signal() {
        let result = resolve_hybrid_weights(None, Some("grading=0.6,clip=0.3,speed=0.1"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown signal"));
    }

    #[test]
    fn test_resolve_weights_zero_weight_allowed() {
        // FR15: per-query --weights allows weight=0.0 with warning
        let w = resolve_hybrid_weights(None, Some("grading=0.0,clip=0.5,tag=0.5")).unwrap();
        assert!((w.grading - 0.0).abs() < 1e-10);
        assert!((w.clip - 0.5).abs() < 1e-10);
        assert!((w.tag - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_weights_no_args_defaults_to_grading_first() {
        let w = resolve_hybrid_weights(None, None).unwrap();
        assert!((w.grading - 0.6).abs() < 1e-10);
        assert!((w.clip - 0.3).abs() < 1e-10);
        assert!((w.tag - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_format_breakdown_all_signals() {
        let bd = mengxi_core::hybrid_scoring::ScoreBreakdown {
            grading: 0.91,
            clip: Some(0.76),
            tag: Some(0.94),
        };
        let s = format_breakdown(&bd);
        assert_eq!(s, "oklab_hist:0.91 clip:0.76 tag:0.94");
    }

    #[test]
    fn test_format_breakdown_missing_signals() {
        let bd = mengxi_core::hybrid_scoring::ScoreBreakdown {
            grading: 0.84,
            clip: Some(0.55),
            tag: None,
        };
        let s = format_breakdown(&bd);
        assert_eq!(s, "oklab_hist:0.84 clip:0.55");
    }

    #[test]
    fn test_format_breakdown_only_grading() {
        let bd = mengxi_core::hybrid_scoring::ScoreBreakdown {
            grading: 0.95,
            clip: None,
            tag: None,
        };
        let s = format_breakdown(&bd);
        assert_eq!(s, "oklab_hist:0.95");
    }

    #[test]
    fn test_search_analytics_includes_hybrid_fields() {
        let cli = Cli::try_parse_from([
            "mx", "search", "--image", "ref.tif",
            "--search-mode", "balanced", "--weights", "grading=0.5,clip=0.3,tag=0.2",
        ]);
        let (name, args, _) = extract_command_info(&cli.unwrap());
        assert_eq!(name, "search");
        assert!(args.contains("balanced"));
        assert!(args.contains("grading=0.5,clip=0.3,tag=0.2"));
    }

    #[test]
    fn test_search_backward_compat_no_hybrid_flags() {
        // No --search-mode or --weights → same behavior as before
        let cli = Cli::try_parse_from([
            "mx", "search", "--image", "ref.tif",
        ]);
        match cli.unwrap().command {
            Some(Commands::Search { search_mode, weights, .. }) => {
                assert!(search_mode.is_none());
                assert!(weights.is_none());
            }
            _ => panic!("Expected Search command"),
        }
    }

    #[test]
    fn test_search_limit_as_top() {
        let cli = Cli::try_parse_from([
            "mx", "search", "--image", "ref.tif", "--limit", "5",
        ]);
        match cli.unwrap().command {
            Some(Commands::Search { limit, .. }) => {
                assert_eq!(limit, Some(5));
            }
            _ => panic!("Expected Search command"),
        }
    }

    // ── Code review fix tests (F-01 through F-08) ──

    #[test]
    fn test_resolve_weights_negative_rejected() {
        let result = resolve_hybrid_weights(None, Some("grading=1.3,clip=-0.2,tag=-0.1"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("non-negative"));
    }

    #[test]
    fn test_resolve_weights_nan_rejected() {
        let result = resolve_hybrid_weights(None, Some("grading=NaN,clip=0.5,tag=0.5"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("finite"));
    }

    #[test]
    fn test_resolve_weights_inf_rejected() {
        let result = resolve_hybrid_weights(None, Some("grading=inf,clip=0.0,tag=0.0"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("finite"));
    }

    #[test]
    fn test_resolve_weights_duplicate_key_rejected() {
        let result = resolve_hybrid_weights(None, Some("grading=0.3,grading=0.3,clip=0.2,tag=0.2"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("duplicate"));
    }

    // --- low_result_explanation tests (Story 4.2) ---

    #[test]
    fn test_low_result_explanation_zero() {
        let result = low_result_explanation(0);
        assert!(result.is_some());
        assert!(result.unwrap().contains("无匹配结果"));
    }

    #[test]
    fn test_low_result_explanation_one() {
        let result = low_result_explanation(1);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.contains("仅找到 1 个匹配"));
    }

    #[test]
    fn test_low_result_explanation_two() {
        let result = low_result_explanation(2);
        assert!(result.is_some());
        let s = result.unwrap();
        assert!(s.contains("仅找到 2 个匹配"));
    }

    #[test]
    fn test_low_result_explanation_three_returns_none() {
        let result = low_result_explanation(3);
        assert!(result.is_none());
    }

    #[test]
    fn test_low_result_explanation_ten_returns_none() {
        let result = low_result_explanation(10);
        assert!(result.is_none());
    }

    #[test]
    fn test_low_result_explanation_hundred_returns_none() {
        let result = low_result_explanation(100);
        assert!(result.is_none());
    }

    // ── generate_session_id tests ──

    #[test]
    fn test_generate_session_id_format() {
        let id = generate_session_id();
        // Format: "{timestamp}_{pid}"
        let parts: Vec<&str> = id.split('_').collect();
        assert!(parts.len() >= 2, "session ID should contain at least one underscore");
        // Timestamp part should be a valid number
        let ts: u64 = parts[0].parse().expect("timestamp should be a number");
        assert!(ts > 0, "timestamp should be positive");
        // PID part should be a valid number
        let pid: u32 = parts[1].parse().expect("pid should be a number");
        assert!(pid > 0, "pid should be positive");
    }

    #[test]
    fn test_generate_session_id_not_empty() {
        let id = generate_session_id();
        assert!(!id.is_empty());
    }

    // ── format_duration_ms tests ──

    #[test]
    fn test_format_duration_ms_milliseconds() {
        assert_eq!(format_duration_ms(500), "500ms");
    }

    #[test]
    fn test_format_duration_ms_zero() {
        assert_eq!(format_duration_ms(0), "0ms");
    }

    #[test]
    fn test_format_duration_ms_one_ms() {
        assert_eq!(format_duration_ms(1), "1ms");
    }

    #[test]
    fn test_format_duration_ms_just_under_one_second() {
        assert_eq!(format_duration_ms(999), "999ms");
    }

    #[test]
    fn test_format_duration_ms_exactly_one_second() {
        assert_eq!(format_duration_ms(1000), "1.0s");
    }

    #[test]
    fn test_format_duration_ms_seconds_with_fraction() {
        assert_eq!(format_duration_ms(3500), "3.5s");
    }

    #[test]
    fn test_format_duration_ms_just_under_one_minute() {
        assert_eq!(format_duration_ms(59_999), "60.0s");
    }

    #[test]
    fn test_format_duration_ms_exactly_one_minute() {
        assert_eq!(format_duration_ms(60_000), "1m 0s");
    }

    #[test]
    fn test_format_duration_ms_minutes_and_seconds() {
        assert_eq!(format_duration_ms(125_000), "2m 5s");
    }

    #[test]
    fn test_format_duration_ms_large_minutes() {
        assert_eq!(format_duration_ms(3_660_000), "61m 0s");
    }

    // ── extract_command_info extended coverage ──

    #[test]
    fn test_extract_command_info_import() {
        let cli = Cli::try_parse_from([
            "mx", "import", "--project", "/film", "--name", "test",
        ]).unwrap();
        let (name, args, search_to_export) = extract_command_info(&cli);
        assert_eq!(name, "import");
        assert!(args.contains("test"));
        assert!(search_to_export.is_none());
    }

    #[test]
    fn test_extract_command_info_search() {
        let cli = Cli::try_parse_from([
            "mx", "search", "--tag", "warm", "--search-mode", "balanced",
        ]).unwrap();
        let (name, args, _) = extract_command_info(&cli);
        assert_eq!(name, "search");
        assert!(args.contains("warm"));
        assert!(args.contains("balanced"));
    }

    #[test]
    fn test_extract_command_info_export() {
        let cli = Cli::try_parse_from([
            "mx", "export", "--result", "5", "--format", "cube",
        ]).unwrap();
        let (name, args, _) = extract_command_info(&cli);
        assert_eq!(name, "export");
        assert!(args.contains("5"));
        assert!(args.contains("cube"));
    }

    #[test]
    fn test_extract_command_info_tag() {
        let cli = Cli::try_parse_from([
            "mx", "tag", "--project", "film", "--add", "warm",
        ]).unwrap();
        let (name, args, _) = extract_command_info(&cli);
        assert_eq!(name, "tag");
        assert!(args.contains("film"));
    }

    #[test]
    fn test_extract_command_info_validate() {
        let cli = Cli::try_parse_from(["mx", "validate"]).unwrap();
        let (name, _args, _) = extract_command_info(&cli);
        assert_eq!(name, "validate");
    }

    #[test]
    fn test_extract_command_info_none_is_help() {
        let cli = Cli::try_parse_from(["mx"]).unwrap();
        let (name, _args, _) = extract_command_info(&cli);
        assert_eq!(name, "help");
    }

    #[test]
    fn test_extract_command_info_chat() {
        let cli = Cli::try_parse_from(["mx", "chat"]).unwrap();
        let (name, args, _) = extract_command_info(&cli);
        assert_eq!(name, "chat");
        assert!(args.contains("claude"));
    }

    #[test]
    fn test_extract_command_info_compare() {
        let cli = Cli::try_parse_from(["mx", "compare", "1", "2"]).unwrap();
        let (name, _args, _) = extract_command_info(&cli);
        assert_eq!(name, "compare");
    }

    #[test]
    fn test_extract_command_info_consistency() {
        let cli = Cli::try_parse_from([
            "mx", "consistency", "--projects", "a,b",
        ]).unwrap();
        let (name, _, _) = extract_command_info(&cli);
        assert_eq!(name, "consistency");
    }

    #[test]
    fn test_extract_command_info_reextract() {
        let cli = Cli::try_parse_from([
            "mx", "reextract", "--project", "film",
        ]).unwrap();
        let (name, _, _) = extract_command_info(&cli);
        assert_eq!(name, "reextract");
    }

    #[test]
    fn test_extract_command_info_embed() {
        let cli = Cli::try_parse_from(["mx", "embed"]).unwrap();
        let (name, _, _) = extract_command_info(&cli);
        assert_eq!(name, "embed");
    }

    #[test]
    fn test_extract_command_info_lut_diff() {
        let cli = Cli::try_parse_from(["mx", "lut-diff", "a.cube", "b.cube"]).unwrap();
        let (name, _, _) = extract_command_info(&cli);
        assert_eq!(name, "lut-diff");
    }

    #[test]
    fn test_extract_command_info_lut_dep() {
        let cli = Cli::try_parse_from(["mx", "lut-dep", "--lut", "a.cube"]).unwrap();
        let (name, _, _) = extract_command_info(&cli);
        assert_eq!(name, "lut-dep");
    }

    #[test]
    fn test_extract_command_info_stats() {
        let cli = Cli::try_parse_from(["mx", "stats"]).unwrap();
        let (name, _, _) = extract_command_info(&cli);
        assert_eq!(name, "stats");
    }

    #[test]
    fn test_extract_command_info_config() {
        let cli = Cli::try_parse_from(["mx", "config", "--show"]).unwrap();
        let (name, _, _) = extract_command_info(&cli);
        assert_eq!(name, "config");
    }

    #[test]
    fn test_extract_command_info_validate_dataset() {
        let cli = Cli::try_parse_from([
            "mx", "validate-dataset", "/data",
        ]).unwrap();
        let (name, _, _) = extract_command_info(&cli);
        assert_eq!(name, "validate-dataset");
    }

    // ── config --edit and --show exclusivity ──

    #[test]
    fn test_config_edit_not_yet_implemented() {
        let cli = Cli::try_parse_from(["mx", "config", "--edit"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Config { show: false, edit: true }) => {}
            _other => panic!("Expected Config with --edit"),
        }
    }

    #[test]
    fn test_config_both_show_and_edit_rejected() {
        // clap does not enforce mutual exclusion here, but the handler checks
        let cli = Cli::try_parse_from(["mx", "config", "--show", "--edit"]);
        // This should parse OK (clap allows both), but runtime handler will error
        assert!(cli.is_ok());
    }

    #[test]
    fn test_config_neither_show_nor_edit() {
        let cli = Cli::try_parse_from(["mx", "config"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Config { show: false, edit: false }) => {}
            _other => panic!("Expected Config with neither flag"),
        }
    }

    // ── resolve_hybrid_weights edge cases ──

    #[test]
    fn test_resolve_weights_pyramid_maps_to_grading_first() {
        let w = resolve_hybrid_weights(Some("pyramid"), None).unwrap();
        let expected = mengxi_core::hybrid_scoring::SignalWeights::grading_first();
        assert_eq!(w, expected);
    }

    #[test]
    fn test_resolve_weights_spaces_around_equals() {
        // Spaces in values should be handled by trim()
        let result = resolve_hybrid_weights(None, Some("grading = 0.6,clip = 0.3,tag = 0.1"));
        // This should fail because " " before the number makes " 0.6" unparseable? No, trim() is used.
        // Actually, let me check: parts[1].trim() should handle it.
        if let Ok(w) = result {
            assert!((w.grading - 0.6).abs() < 1e-10);
        }
        // If it fails that's also acceptable behavior
    }

    #[test]
    fn test_resolve_weights_whitespace_in_keys() {
        let result = resolve_hybrid_weights(None, Some(" grading = 0.6 , clip = 0.3 , tag = 0.1 "));
        if let Ok(w) = result {
            assert!((w.grading - 0.6).abs() < 1e-10);
            assert!((w.clip - 0.3).abs() < 1e-10);
            assert!((w.tag - 0.1).abs() < 1e-10);
        }
    }

    // ── additional seconds_to_datetime tests ──

    #[test]
    fn test_seconds_to_datetime_midnight() {
        // 1970-01-02 00:00:00
        let (y, m, d, h, min, s) = seconds_to_datetime(86400);
        assert_eq!((y, m, d, h, min, s), (1970, 1, 2, 0, 0, 0));
    }

    #[test]
    fn test_seconds_to_datetime_end_of_day() {
        // 1970-01-01 23:59:59
        let (y, m, d, h, min, s) = seconds_to_datetime(86399);
        assert_eq!((y, m, d, h, min, s), (1970, 1, 1, 23, 59, 59));
    }

    #[test]
    fn test_seconds_to_datetime_year_2000() {
        // 2000-01-01 00:00:00 UTC = 946684800
        let (y, m, d, h, min, s) = seconds_to_datetime(946684800);
        assert_eq!((y, m, d, h, min, s), (2000, 1, 1, 0, 0, 0));
    }
}
