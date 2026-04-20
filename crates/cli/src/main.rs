mod commands;
mod config;
mod project_ops;
mod tui;
mod validate;
mod validate_dataset;


use clap::{Parser, Subcommand};
use std::time::{SystemTime, UNIX_EPOCH};

use mengxi_core::analytics;
use mengxi_core::db;

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
        #[arg(long, value_parser = ["strip", "cineiris", "both", "cineprint"], default_value = "strip")]
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
        /// Watermark image path for CinePrint mode
        #[arg(long)]
        watermark: Option<String>,
        /// Watermark position for CinePrint mode (left, center, right)
        #[arg(long, value_parser = ["left", "center", "right"], default_value = "right")]
        wm_position: String,
        /// Show EP episode label in CinePrint mode
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        show_ep: bool,
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
        Some(Commands::FingerprintGen { video, mode, interval, max_frames, diameter, output, format, watermark, wm_position, show_ep, .. }) => {
            commands::fingerprint_cmd::execute(video, mode, interval, max_frames, diameter, output, format, watermark, wm_position, show_ep);
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

#[cfg(test)]
mod test_utils;
