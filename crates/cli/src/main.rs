mod config;
mod validate;
mod validate_dataset;

use unicode_width::UnicodeWidthStr;

use clap::{Parser, Subcommand};
use std::io::{self, Write, BufRead};
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use mengxi_core::analytics;
use mengxi_core::db;
use mengxi_core::project;

/// Mengxi — CLI-based color pipeline management platform
#[derive(Parser)]
#[command(name = "mengxi", version, about = "Color style search and LUT management for film colorists")]
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
        /// Search mode preset (grading-first, balanced)
        #[arg(long, value_parser = ["grading-first", "balanced"])]
        search_mode: Option<String>,
        /// Override signal weights (e.g., grading=0.6,clip=0.3,tag=0.1)
        #[arg(long)]
        weights: Option<String>,
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
        #[arg(long)]
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
        /// Output as structured JSON
        #[arg(long)]
        json: bool,
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
        /// Output as structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Validate evaluation dataset format compliance
    #[command(name = "validate-dataset")]
    ValidateDataset {
        /// Directory containing evaluation dataset
        dir: String,
        /// Output as structured JSON
        #[arg(long)]
        json: bool,
    },
    /// Browse and query the database
    Db {
        #[command(subcommand)]
        command: DbSubcommand,
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
                analytics::get_last_search_started_at(&conn).ok().flatten().map(|search_ts| {
                    let now_ts = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as i64;
                    let delta = now_ts - search_ts;
                    if delta > 0 { Some(delta) } else { None }
                }).flatten()
            } else {
                None
            };
            ("export".to_string(), serde_json::to_string(&obj).unwrap_or_default(), search_to_export_ms)
        }
        Some(Commands::Info { project, .. }) => {
            let obj = serde_json::json!({ "project": project });
            ("info".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
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
        None => ("help".to_string(), "{}".to_string(), None),
    }
}

/// Record a session to the database (best-effort, never blocks CLI exit).
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
        if let Err(e) = analytics::record_session(
            &conn, session_id, command, args_json,
            started_at, ended_at, duration_ms, exit_code, search_to_export_ms, user,
        ) {
            eprintln!("Warning: Failed to record session: {}", e);
        }
    } else {
        eprintln!("Warning: Failed to open database for session recording");
    }
}

/// Format milliseconds into human-readable duration string.
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
            let is_json = format == "json";

            let project_path = match project {
                Some(p) => p,
                None => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "IMPORT_MISSING_ARG", "message": "--project <path> is required" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: IMPORT_MISSING_ARG — --project <path> is required");
                    }
                    process::exit(1);
                }
            };
            let project_name = match name {
                Some(n) => n,
                None => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "IMPORT_MISSING_ARG", "message": "--name <string> is required" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: IMPORT_MISSING_ARG — --name <string> is required");
                    }
                    process::exit(1);
                }
            };

            let path = Path::new(&project_path);

            match db::open_db() {
                Ok(conn) => match project::register_project(&conn, &project_name, &path, |current, total, filename| {
                    let percent = if total == 0 { 100 } else { (current * 100) / total };
                    let filled = (percent / 5).min(20);
                    let empty = 20 - filled;
                    eprintln!("[{}{}] {}% ({}/{}) Processing {}...",
                        "█".repeat(filled),
                        "░".repeat(empty),
                        percent,
                        current,
                        total,
                        filename,
                    );
                }) {
                    Ok((proj, breakdown)) => {
                        // Post-import tag generation (if enabled)
                        let mut tag_count: usize = 0;
                        let mut tag_error_count: usize = 0;
                        let cfg = config::load_or_create_config().unwrap_or_default();
                        if cfg.ai.tag_generation {
                            let fp_ids = match mengxi_core::tag::fingerprint_ids_for_project(&conn, &project_name) {
                                Ok(ids) => ids,
                                Err(e) => {
                                    eprintln!("Warning: Could not query fingerprints for tag generation: {}", e);
                                    Vec::new()
                                }
                            };

                            if !fp_ids.is_empty() {
                                let total_fps = fp_ids.len();
                                let mut bridge = mengxi_core::python_bridge::PythonBridge::new(
                                    cfg.ai.idle_timeout_secs,
                                    cfg.ai.inference_timeout_secs,
                                    cfg.ai.tag_model.clone(),
                                );

                                // Early subprocess health check — fail once instead of per-fingerprint
                                match bridge.ping() {
                                    Ok(true) => {},
                                    Ok(false) => {
                                        eprintln!("Warning: AI subprocess not responding, skipping tag generation for {} fingerprints", total_fps);
                                        tag_error_count = total_fps;
                                    }
                                    Err(e) => {
                                        eprintln!("Warning: AI subprocess not available ({})", e);
                                        eprintln!("Warning: Skipping tag generation for {} fingerprints", total_fps);
                                        tag_error_count = total_fps;
                                    }
                                }

                                if tag_error_count == 0 {
                                    let top_n = cfg.ai.tag_top_n.max(1);
                                    // Get personalized tags from calibration (best-effort)
                                    let personalized_tags = mengxi_core::calibration::get_personalized_tags(&conn).unwrap_or_default();
                                    for (i, fp_id) in fp_ids.iter().enumerate() {
                                        eprintln!("Generating tags... {}/{}", i + 1, total_fps);
                                        // Get file path for fingerprint
                                        let fpath_result: Result<(String, String), _> = conn.query_row(
                                            "SELECT p.path, f.filename FROM fingerprints fp
                                             JOIN files f ON f.id = fp.file_id
                                             JOIN projects p ON p.id = f.project_id
                                             WHERE fp.id = ?1",
                                            [*fp_id],
                                            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                                        );
                                        match fpath_result {
                                            Ok((base_path, filename)) => {
                                                let fpath = format!("{}/{}", base_path, filename);
                                                match bridge.generate_tags_with_calibration(&fpath, top_n, &personalized_tags) {
                                                    Ok(tags) => {
                                                        for tag in &tags {
                                                            if let Err(e) = mengxi_core::tag::tag_add(&conn, *fp_id, tag) {
                                                                eprintln!("Warning: Failed to add tag '{}': {}", tag, e);
                                                                tag_error_count += 1;
                                                            } else {
                                                                tag_count += 1;
                                                            }
                                                        }
                                                    }
                                                    Err(e) => {
                                                        eprintln!("Warning: Tag generation failed for fingerprint {}: {}", fp_id, e);
                                                        tag_error_count += 1;
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                eprintln!("Warning: Could not get path for fingerprint {}: {}", fp_id, e);
                                                tag_error_count += 1;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if is_json {
                            let output = serde_json::json!({
                                "status": "ok",
                                "project": {
                                    "id": proj.id,
                                    "name": proj.name,
                                    "path": proj.path,
                                    "dpx_count": proj.dpx_count,
                                    "exr_count": proj.exr_count,
                                    "mov_count": proj.mov_count,
                                    "created_at": proj.created_at,
                                },
                                "summary": {
                                    "dpx_count": proj.dpx_count,
                                    "exr_count": proj.exr_count,
                                    "mov_count": proj.mov_count,
                                    "fingerprint_count": breakdown.fingerprint_count,
                                    "skipped_count": breakdown.skipped_count,
                                    "resumed_count": breakdown.resumed_count,
                                    "tag_count": tag_count,
                                    "variants": breakdown.variants,
                                }
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                        } else {
                            let dpx_detail = if breakdown.variants.iter().any(|v| v.contains("-bit")) || proj.dpx_count == 0 {
                                if proj.dpx_count == 0 {
                                    format!("{} DPX files", proj.dpx_count)
                                } else {
                                    let dpx_variants: Vec<&str> = breakdown.variants.iter()
                                        .filter(|v| v.contains("-bit"))
                                        .map(|s| s.as_str())
                                        .collect();
                                    if dpx_variants.is_empty() {
                                        format!("{} DPX files", proj.dpx_count)
                                    } else {
                                        format!("{} DPX files ({})", proj.dpx_count, dpx_variants.join(", "))
                                    }
                                }
                            } else {
                                format!("{} DPX files", proj.dpx_count)
                            };
                            let exr_detail = {
                                let exr_variants: Vec<&str> = breakdown.variants.iter()
                                    .filter(|v| v.contains("half-float") || v.contains("float") || v.contains("uint"))
                                    .map(|s| s.as_str())
                                    .collect();
                                if exr_variants.is_empty() {
                                    format!("{} EXR files", proj.exr_count)
                                } else {
                                    format!("{} EXR files ({})", proj.exr_count, exr_variants.join(", "))
                                }
                            };
                            let skipped_detail = if breakdown.skipped_count > 0 {
                                format!(" ({} skipped)", breakdown.skipped_count)
                            } else {
                                String::new()
                            };
                            let mov_detail = {
                                let mov_variants: Vec<&str> = breakdown.variants.iter()
                                    .filter(|v| !v.contains("-bit") && !v.contains("half-float") && !v.contains("float") && !v.contains("uint"))
                                    .map(|s| s.as_str())
                                    .collect();
                                if mov_variants.is_empty() {
                                    format!("{} MOV files", proj.mov_count)
                                } else {
                                    format!("{} MOV files ({})", proj.mov_count, mov_variants.join(", "))
                                }
                            };
                            let fp_detail = format!("{} fingerprints extracted", breakdown.fingerprint_count);
                            let tag_detail = if tag_count > 0 {
                                format!("{} AI tags generated", tag_count)
                            } else if tag_error_count > 0 {
                                format!("{} tag errors", tag_error_count)
                            } else {
                                "No AI tags".to_string()
                            };
                            println!(
                                "+----------+------------------------------+\n\
                                 | Field    | Value                        |\n\
                                 +----------+------------------------------+\n\
                                 | Name     | {:<28} |\n\
                                 | Path     | {:<28} |\n\
                                 | DPX      | {:<28}|\n\
                                 | EXR      | {:<28} |\n\
                                 | MOV      | {:<28} |\n\
                                 | Color    | {:<28} |\n\
                                 | Tags     | {:<28} |\n\
                                 +----------+------------------------------+",
                                proj.name,
                                proj.path,
                                dpx_detail,
                                exr_detail,
                                format!("{}{}", mov_detail, skipped_detail),
                                fp_detail,
                                tag_detail,
                            );
                        }
                    }
                    Err(e) => {
                        if is_json {
                            let (code, message) = match &e {
                                project::ImportError::PathNotFound(msg) => ("IMPORT_PATH_NOT_FOUND", msg.clone()),
                                project::ImportError::DuplicateName(msg) => ("IMPORT_DUPLICATE_NAME", msg.clone()),
                                project::ImportError::DbError(_) => ("IMPORT_DB_ERROR", "Database operation failed".to_string()),
                                project::ImportError::CorruptFile { filename, reason } => {
                                    ("IMPORT_CORRUPT_FILE", format!("Failed to decode {}: {}", filename, reason))
                                }
                            };
                            let output = serde_json::json!({
                                "status": "error",
                                "error": {
                                    "code": code,
                                    "message": message,
                                }
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            process::exit(1);
                        } else {
                            match e {
                                project::ImportError::PathNotFound(msg) => {
                                    eprintln!("Error: {msg}");
                                }
                                project::ImportError::DuplicateName(msg) => {
                                    eprintln!("Error: {msg}");
                                }
                                project::ImportError::DbError(msg) => {
                                    eprintln!("Error: IMPORT_DB_ERROR — {msg}");
                                }
                                project::ImportError::CorruptFile { filename, reason } => {
                                    eprintln!("Error: IMPORT_CORRUPT_FILE -- Failed to decode {}: {}", filename, reason);
                                }
                            }
                            process::exit(1);
                        }
                    }
                },
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": {
                                "code": "IMPORT_DB_INIT_FAILED",
                                "message": "Failed to initialize database",
                            }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: IMPORT_DB_INIT_FAILED — {e}");
                    }
                    process::exit(1);
                }
            }
        }
        Some(Commands::Search {
            image,
            tag,
            limit,
            project,
            accept,
            reject,
            format,
            search_mode,
            weights,
        }) => {
            let is_json = format == "json";

            // F-07: warn when --search-mode/--weights used without --image
            if image.is_none() && (search_mode.is_some() || weights.is_some()) {
                eprintln!("warning: --search-mode and --weights require --image, flags ignored");
            }

            // --image: embedding-based search (optionally combined with --tag)
            if let Some(ref img_path) = image {
                let cfg = config::load_or_create_config().unwrap_or_default();
                let limit_val = limit.unwrap_or(cfg.general.default_search_limit);

                // Reject --limit 0
                if limit_val == 0 {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "SEARCH_INVALID_LIMIT", "message": "--limit must be at least 1" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: SEARCH_INVALID_LIMIT -- --limit must be at least 1");
                    }
                    process::exit(1);
                }

                match db::open_db() {
                    Ok(conn) => {
                        let options = mengxi_core::search::SearchOptions {
                            project: project.clone(),
                            limit: limit_val as usize,
                        };

                        // Resolve search weights via config cascade when no CLI args
                        let config_weights = if search_mode.is_some() || weights.is_some() {
                            None
                        } else {
                            let cwd = std::env::current_dir().unwrap_or_default();
                            match config::resolve_search_config(&cwd) {
                                Ok(w) => Some(w),
                                Err(e) => {
                                    if is_json {
                                        let output = serde_json::json!({
                                            "status": "error",
                                            "error": { "code": "CONFIG_VALIDATION_ERROR", "message": e }
                                        });
                                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                                    } else {
                                        eprintln!("Error: {}", e);
                                    }
                                    process::exit(1);
                                }
                            }
                        };

                        let use_hybrid = search_mode.is_some() || weights.is_some() || config_weights.is_some();

                        if use_hybrid {
                            // F-06: warn when --tag is provided but hybrid mode ignores it
                            if tag.is_some() {
                                eprintln!("warning: --tag is ignored in hybrid search mode (use --search-mode or --weights without --tag)");
                            }

                            // Resolve weights (config cascade: CLI args > project config > global config > defaults)
                            let resolved_weights = if search_mode.is_some() || weights.is_some() {
                                resolve_hybrid_weights(search_mode.as_deref(), weights.as_deref())
                            } else {
                                Ok(config_weights.unwrap())
                            };
                            let resolved_weights = match resolved_weights {
                                Ok(w) => w,
                                Err(e) => {
                                    if is_json {
                                        let output = serde_json::json!({
                                            "status": "error",
                                            "error": { "code": "SEARCH_WEIGHT_ERROR", "message": e }
                                        });
                                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                                    } else {
                                        eprintln!("Error: {}", e);
                                    }
                                    process::exit(1);
                                }
                            };

                            // Resolve image path to file_id
                            let file_id = match resolve_image_to_file_id(&conn, img_path, project.as_deref()) {
                                Ok(id) => id,
                                Err(e) => {
                                    if is_json {
                                        let output = serde_json::json!({
                                            "status": "error",
                                            "error": { "code": "SEARCH_IMAGE_ERROR", "message": e }
                                        });
                                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                                    } else {
                                        eprintln!("Error: {}", e);
                                    }
                                    process::exit(1);
                                }
                            };

                            match mengxi_core::search::hybrid_search(&conn, file_id, &resolved_weights, &options) {
                                Ok(results) => {
                                    if is_json {
                                        let json_results: Vec<serde_json::Value> = results
                                            .iter()
                                            .map(|r| {
                                                let mut bd = serde_json::Map::new();
                                                bd.insert("oklab_histogram".to_string(), serde_json::json!(r.score_breakdown.grading));
                                                if let Some(clip) = r.score_breakdown.clip {
                                                    bd.insert("clip_semantic".to_string(), serde_json::json!(clip));
                                                }
                                                if let Some(tag) = r.score_breakdown.tag {
                                                    bd.insert("tag_match".to_string(), serde_json::json!(tag));
                                                }
                                                let mut obj = serde_json::Map::new();
                                                obj.insert("rank".to_string(), serde_json::json!(r.rank));
                                                obj.insert("project".to_string(), serde_json::json!(r.project_name));
                                                obj.insert("file".to_string(), serde_json::json!(r.file_path));
                                                obj.insert("score".to_string(), serde_json::json!(r.score));
                                                obj.insert("score_breakdown".to_string(), serde_json::json!(bd));
                                                obj.insert("human_readable".to_string(), serde_json::json!(r.human_readable));
                                                if !r.match_warnings.is_empty() {
                                                    obj.insert("match_warnings".to_string(), serde_json::json!(r.match_warnings));
                                                }
                                                serde_json::Value::Object(obj)
                                            })
                                            .collect();

                                        let mut output = serde_json::Map::new();
                                        output.insert("status".to_string(), serde_json::json!("ok"));
                                        output.insert("results".to_string(), serde_json::json!(json_results));
                                        if let Some(explanation) = low_result_explanation(results.len()) {
                                            output.insert("low_result_reason".to_string(), serde_json::json!(explanation));
                                        }
                                        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(output)).unwrap());
                                    } else {
                                        if results.is_empty() {
                                            println!("No results found.");
                                            if let Some(explanation) = low_result_explanation(results.len()) {
                                                println!("{}", explanation);
                                            }
                                        } else {
                                            println!(
                                                "+------+------------------+--------------------------+-------+------------------------------------------+"
                                            );
                                            println!(
                                                "| Rank | Project          | File                     | Score | Breakdown                                |"
                                            );
                                            println!(
                                                "+------+------------------+--------------------------+-------+------------------------------------------+"
                                            );
                                            let all_warnings: Vec<&str> = results.iter()
                                                .flat_map(|r| r.match_warnings.iter().map(|s| s.as_str()))
                                                .collect();

                                            for r in &results {
                                                let score_pct = format!("{:.1}%", r.score * 100.0);
                                                let breakdown = format_breakdown(&r.score_breakdown);
                                                println!(
                                                    "| {:<4} | {:<16} | {:<24} | {:<5} | {:<40} |",
                                                    r.rank,
                                                    truncate_str(&r.project_name, 16),
                                                    truncate_str(&r.file_path, 24),
                                                    score_pct,
                                                    truncate_str(&breakdown, 40),
                                                );
                                                if !r.human_readable.is_empty() {
                                                    println!("        {}", r.human_readable);
                                                }
                                            }
                                            println!(
                                                "+------+------------------+--------------------------+-------+------------------------------------------+"
                                            );
                                            for w in &all_warnings {
                                                eprintln!("warning: {}", w);
                                            }
                                            if let Some(explanation) = low_result_explanation(results.len()) {
                                                println!("{}", explanation);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    if is_json {
                                        let output = serde_json::json!({
                                            "status": "error",
                                            "error": { "code": "SEARCH_HYBRID_ERROR", "message": e.to_string() }
                                        });
                                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                                    } else {
                                        eprintln!("Error: {}", e);
                                    }
                                    process::exit(1);
                                }
                            }
                        } else {
                        // Existing search logic (unchanged)
                        let search_result = match &tag {
                            Some(tag_text) => {
                                // Combined --image + --tag search
                                mengxi_core::search::search_by_image_and_tag(
                                    &conn,
                                    tag_text,
                                    img_path,
                                    &options,
                                    cfg.ai.idle_timeout_secs,
                                    cfg.ai.inference_timeout_secs,
                                    &cfg.ai.embedding_model,
                                )
                            }
                            None => {
                                // Image-only search
                                mengxi_core::search::search_by_image(
                                    &conn,
                                    img_path,
                                    &options,
                                    cfg.ai.idle_timeout_secs,
                                    cfg.ai.inference_timeout_secs,
                                    &cfg.ai.embedding_model,
                                )
                            }
                        };

                        match search_result {
                            Ok(results) => {
                                if is_json {
                                    let json_results: Vec<serde_json::Value> = results
                                        .iter()
                                        .map(|r| {
                                            serde_json::json!({
                                                "rank": r.rank,
                                                "project": r.project_name,
                                                "file": r.file_path,
                                                "score": r.score
                                            })
                                        })
                                        .collect();

                                    let output = serde_json::json!({
                                        "status": "ok",
                                        "results": json_results,
                                    });
                                    println!("{}", serde_json::to_string_pretty(&output).unwrap());
                                } else {
                                    // Text table output
                                    if results.is_empty() {
                                        println!("No results found.");
                                    } else {
                                        println!(
                                            "+------+------------------+--------------------------+-------------+"
                                        );
                                        println!(
                                            "| Rank | Project          | File                     | Similarity  |"
                                        );
                                        println!(
                                            "+------+------------------+--------------------------+-------------+"
                                        );
                                        for r in &results {
                                            let display_score = r.score.max(0.0);
                                            let score_pct = format!("{:.1}%", display_score * 100.0);
                                            println!(
                                                "| {:<4} | {:<16} | {:<24} | {:<11} |",
                                                r.rank,
                                                truncate_str(&r.project_name, 16),
                                                truncate_str(&r.file_path, 24),
                                                score_pct
                                            );
                                        }
                                        println!(
                                            "+------+------------------+--------------------------+-------------+"
                                        );
                                    }
                                }
                                // Record accept/reject feedback if requested
                                record_feedback_if_needed(&conn, &results, accept, reject, "image", is_json);
                            }
                            Err(e) => {
                                if is_json {
                                    let output = serde_json::json!({
                                        "status": "error",
                                        "error": { "code": "SEARCH_IMAGE_ERROR", "message": e.to_string() }
                                    });
                                    println!("{}", serde_json::to_string_pretty(&output).unwrap());
                                } else {
                                    eprintln!("Error: {}", e);
                                }
                                process::exit(1);
                            }
                        }
                    }
                    }
                    Err(e) => {
                        if is_json {
                            let output = serde_json::json!({
                                "status": "error",
                                "error": { "code": "SEARCH_DB_ERROR", "message": e.to_string() }
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                        } else {
                            eprintln!("Error: SEARCH_DB_ERROR -- {}", e);
                        }
                        process::exit(1);
                    }
                }
            } else if let Some(ref tag_text) = tag {
                // Tag-only search
                let cfg = config::load_or_create_config().unwrap_or_default();
                let limit_val = limit.unwrap_or(cfg.general.default_search_limit);

                if limit_val == 0 {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "SEARCH_INVALID_LIMIT", "message": "--limit must be at least 1" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: SEARCH_INVALID_LIMIT -- --limit must be at least 1");
                    }
                    process::exit(1);
                }

                match db::open_db() {
                    Ok(conn) => {
                        let options = mengxi_core::search::SearchOptions {
                            project: project.clone(),
                            limit: limit_val as usize,
                        };

                        match mengxi_core::search::search_by_tag(&conn, tag_text, &options) {
                            Ok(results) => {
                                display_search_results(&results, is_json);
                                record_feedback_if_needed(&conn, &results, accept, reject, "tag", is_json);
                            }
                            Err(mengxi_core::search::SearchError::NoFingerprints) => {
                                if is_json {
                                    let output = serde_json::json!({
                                        "status": "ok",
                                        "query": {
                                            "tag": tag_text,
                                            "project": project,
                                            "limit": limit_val
                                        },
                                        "results": [],
                                        "message": "No results found for the specified tag."
                                    });
                                    println!("{}", serde_json::to_string_pretty(&output).unwrap());
                                } else {
                                    println!("No results found for tag '{}'.", tag_text);
                                }
                            }
                            Err(e) => {
                                if is_json {
                                    let output = serde_json::json!({
                                        "status": "error",
                                        "error": { "code": "SEARCH_TAG_ERROR", "message": e.to_string() }
                                    });
                                    println!("{}", serde_json::to_string_pretty(&output).unwrap());
                                } else {
                                    eprintln!("Error: {}", e);
                                }
                                process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        if is_json {
                            let output = serde_json::json!({
                                "status": "error",
                                "error": { "code": "SEARCH_DB_ERROR", "message": e.to_string() }
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                        } else {
                            eprintln!("Error: SEARCH_DB_ERROR -- {}", e);
                        }
                        process::exit(1);
                    }
                }
            } else {
                // Histogram search (no --image, no --tag)
                // Resolve limit from CLI flag or config default
            let cfg = config::load_or_create_config().unwrap_or_default();
            let limit_val = limit.unwrap_or(cfg.general.default_search_limit);

            // Reject --limit 0
            if limit_val == 0 {
                if is_json {
                    let output = serde_json::json!({
                        "status": "error",
                        "error": { "code": "SEARCH_INVALID_LIMIT", "message": "--limit must be at least 1" }
                    });
                    println!("{}", serde_json::to_string_pretty(&output).unwrap());
                } else {
                    eprintln!("Error: SEARCH_INVALID_LIMIT -- --limit must be at least 1");
                }
                process::exit(1);
            }

            // Execute histogram search
            match db::open_db() {
                Ok(conn) => {
                    let options = mengxi_core::search::SearchOptions {
                        project: project.clone(),
                        limit: limit_val as usize,
                    };

                    match mengxi_core::search::search_histograms(&conn, &options) {
                        Ok(results) => {
                            if is_json {
                                let json_results: Vec<serde_json::Value> = results
                                    .iter()
                                    .map(|r| {
                                        serde_json::json!({
                                            "rank": r.rank,
                                            "project": r.project_name,
                                            "file": r.file_path,
                                            "score": r.score
                                        })
                                    })
                                    .collect();

                                let output = serde_json::json!({
                                    "status": "ok",
                                    "query": {
                                        "project": project,
                                        "limit": limit_val
                                    },
                                    "results": json_results
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                // Text table output
                                if results.is_empty() {
                                    println!("No results found.");
                                } else {
                                    // Header
                                    println!(
                                        "+------+------------------+--------------------------+-------------+"
                                    );
                                    println!(
                                        "| Rank | Project          | File                     | Similarity  |"
                                    );
                                    println!(
                                        "+------+------------------+--------------------------+-------------+"
                                    );
                                    for r in &results {
                                        let score_pct = format!("{:.1}%", r.score * 100.0);
                                        println!(
                                            "| {:<4} | {:<16} | {:<24} | {:<11} |",
                                            r.rank,
                                            truncate_str(&r.project_name, 16),
                                            truncate_str(&r.file_path, 24),
                                            score_pct
                                        );
                                    }
                                    println!(
                                        "+------+------------------+--------------------------+-------------+"
                                    );
                                }
                                // Record accept/reject feedback if requested
                                record_feedback_if_needed(&conn, &results, accept, reject, "histogram", is_json);
                            }
                        }
                        Err(mengxi_core::search::SearchError::NoFingerprints) => {
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "ok",
                                    "query": {
                                        "project": project,
                                        "limit": limit_val
                                    },
                                    "results": [],
                                    "message": "No indexed projects found. Run 'mengxi import' first."
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                println!("No indexed projects found. Run 'mengxi import' first.");
                            }
                        }
                        Err(mengxi_core::search::SearchError::ProjectNotFound(name)) => {
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "ok",
                                    "query": {
                                        "project": Some(&name),
                                        "limit": limit_val
                                    },
                                    "results": [],
                                    "message": format!("No fingerprints found for project '{}'.", name)
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                println!("No fingerprints found for project '{}'.", name);
                            }
                        }
                        Err(e) => {
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "error",
                                    "error": { "code": "SEARCH_DB_ERROR", "message": e.to_string() }
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                eprintln!("Error: {}", e);
                            }
                            process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "SEARCH_DB_INIT_FAILED", "message": e.to_string() }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: SEARCH_DB_INIT_FAILED -- {e}");
                    }
                    process::exit(1);
                }
            }
            } // end else (histogram search)
        }
        Some(Commands::Export {
            result,
            format,
            output,
            grid_size,
            force,
        }) => {
            let is_json = std::env::var("MENGXI_JSON").is_ok();

            // Validate required args
            let result_id = match result {
                Some(id) => id as i64,
                None => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "EXPORT_MISSING_ARG", "message": "--result <id> is required" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: EXPORT_MISSING_ARG -- --result <id> is required");
                    }
                    process::exit(1);
                }
            };

            // Resolve format and output path
            let fmt = match &format {
                Some(f) => f.clone(),
                None => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "EXPORT_MISSING_ARG", "message": "--format is required" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: EXPORT_MISSING_ARG -- --format is required");
                    }
                    process::exit(1);
                }
            };

            let out_path = match &output {
                Some(p) => {
                    // Expand ~ to home directory
                    let expanded = if p == "~" {
                        dirs::home_dir().unwrap_or_default()
                    } else if p.starts_with("~/") {
                        let home = dirs::home_dir().unwrap_or_default();
                        home.join(&p[2..])
                    } else {
                        std::path::PathBuf::from(p)
                    };
                    // Add extension if missing
                    if expanded.extension().is_none() {
                        expanded.with_extension(&fmt)
                    } else {
                        expanded
                    }
                }
                None => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "EXPORT_MISSING_ARG", "message": "--output is required" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: EXPORT_MISSING_ARG -- --output is required");
                    }
                    process::exit(1);
                }
            };

            // Determine if interactive or scripted mode
            let interactive = !force && format.is_some() && output.is_some();

            let export_config = mengxi_core::lut_generation::ExportLutConfig {
                project_id: result_id,
                fingerprint_id: None,
                format: fmt.clone(),
                output_path: out_path.clone(),
                grid_size,
                force,
                interactive,
            };

            // Open DB and export
            match db::open_db() {
                Ok(conn) => {
                    match mengxi_core::lut_generation::export_lut(&conn, &export_config) {
                        Ok(result) => {
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "ok",
                                    "path": result.path.to_string_lossy(),
                                    "grid_size": result.grid_size,
                                    "format": result.format
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                println!(
                                    "Exported LUT to {} (grid: {}x{}x{}, format: {})",
                                    result.path.display(),
                                    result.grid_size,
                                    result.grid_size,
                                    result.grid_size,
                                    result.format
                                );
                            }
                        }
                        Err(mengxi_core::lut_generation::LutGenerationError::FileExists(path)) => {
                            // Interactive overwrite prompt
                            if interactive {
                                eprintln!(
                                    "File {} already exists. Overwrite? [y/N]",
                                    path.display()
                                );
                                let mut response = String::new();
                                std::io::stdin()
                                    .read_line(&mut response)
                                    .unwrap_or_default();
                                if response.trim().to_lowercase() == "y" {
                                    match mengxi_core::lut_generation::export_lut_force(
                                        &conn,
                                        export_config,
                                    ) {
                                        Ok(result) => {
                                            if is_json {
                                                let output = serde_json::json!({
                                                    "status": "ok",
                                                    "path": result.path.to_string_lossy(),
                                                    "grid_size": result.grid_size,
                                                    "format": result.format
                                                });
                                                println!(
                                                    "{}",
                                                    serde_json::to_string_pretty(&output).unwrap()
                                                );
                                            } else {
                                                println!(
                                                    "Exported LUT to {} (grid: {}x{}x{}, format: {})",
                                                    result.path.display(),
                                                    result.grid_size,
                                                    result.grid_size,
                                                    result.grid_size,
                                                    result.format
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            if is_json {
                                                let output = serde_json::json!({
                                                    "status": "error",
                                                    "error": { "code": "LUT_EXPORT_ERROR", "message": e.to_string() }
                                                });
                                                eprintln!(
                                                    "{}",
                                                    serde_json::to_string_pretty(&output).unwrap()
                                                );
                                            } else {
                                                eprintln!("Error: {e}");
                                            }
                                            process::exit(1);
                                        }
                                    }
                                } else {
                                    if is_json {
                                        let output = serde_json::json!({
                                            "status": "error",
                                            "error": { "code": "EXPORT_CANCELLED", "message": "Export cancelled by user" }
                                        });
                                        eprintln!(
                                            "{}",
                                            serde_json::to_string_pretty(&output).unwrap()
                                        );
                                    } else {
                                        eprintln!("Export cancelled.");
                                    }
                                }
                            } else {
                                if is_json {
                                    let output = serde_json::json!({
                                        "status": "error",
                                        "error": { "code": "EXPORT_FILE_EXISTS", "message": format!("File {} already exists. Use --force to overwrite.", path.display()) }
                                    });
                                    eprintln!(
                                        "{}",
                                        serde_json::to_string_pretty(&output).unwrap()
                                    );
                                } else {
                                    eprintln!(
                                        "Error: EXPORT_FILE_EXISTS -- {}. Use --force to overwrite.",
                                        path.display()
                                    );
                                }
                                process::exit(1);
                            }
                        }
                        Err(e) => {
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "error",
                                    "error": { "code": "LUT_EXPORT_ERROR", "message": e.to_string() }
                                });
                                eprintln!(
                                    "{}",
                                    serde_json::to_string_pretty(&output).unwrap()
                                );
                            } else {
                                eprintln!("Error: {e}");
                            }
                            process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "EXPORT_DB_INIT_FAILED", "message": "Failed to initialize database" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: EXPORT_DB_INIT_FAILED -- {e}");
                    }
                    process::exit(1);
                }
            }
        }
        Some(Commands::Info { project, file, format }) => {
            let is_json = format == "json";

            match (project.as_deref(), file.as_deref()) {
                (Some(proj), Some(fp)) => {
                    match db::open_db() {
                        Ok(conn) => {
                            match mengxi_core::search::fingerprint_info_with_tags(&conn, proj, fp) {
                                Ok(info) => {
                                    if is_json {
                                        let output = serde_json::json!({
                                            "status": "ok",
                                            "fingerprint": {
                                                "project": info.project_name,
                                                "file": info.file_path,
                                                "format": info.file_format,
                                                "color_space": info.color_space_tag,
                                                "luminance": {
                                                    "mean": info.luminance_mean,
                                                    "stddev": info.luminance_stddev,
                                                },
                                                "histogram": {
                                                    "r": {
                                                        "mean": info.histogram_r_summary.mean_value,
                                                        "dominant_bin": info.histogram_r_summary.dominant_bin_min,
                                                    },
                                                    "g": {
                                                        "mean": info.histogram_g_summary.mean_value,
                                                        "dominant_bin": info.histogram_g_summary.dominant_bin_min,
                                                    },
                                                    "b": {
                                                        "mean": info.histogram_b_summary.mean_value,
                                                        "dominant_bin": info.histogram_b_summary.dominant_bin_min,
                                                    },
                                                },
                                                "tags": info.tags,
                                            }
                                        });
                                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                                    } else {
                                        let tags_str = if info.tags.is_empty() {
                                            "(none)".to_string()
                                        } else {
                                            info.tags.join(", ")
                                        };
                                        println!(
                                            "+---------------+------------------------------+\n\
                                             | Field         | Value                        |\n\
                                             +---------------+------------------------------+\n\
                                             | Project       | {:<28} |\n\
                                             | File          | {:<28} |\n\
                                             | Format        | {:<28} |\n\
                                             | Color Space   | {:<28} |\n\
                                             | Luminance     | {:.4} +/- {:.4}                |\n\
                                             | Hist R (mean) | {:.6}                     |\n\
                                             | Hist G (mean) | {:.6}                     |\n\
                                             | Hist B (mean) | {:.6}                     |\n\
                                             | Dominant R    | bin {}                      |\n\
                                             | Dominant G    | bin {}                      |\n\
                                             | Dominant B    | bin {}                      |\n\
                                             | Tags          | {:<28} |\n\
                                             +---------------+------------------------------+",
                                            truncate_str(&info.project_name, 28),
                                            truncate_str(&info.file_path, 28),
                                            truncate_str(&info.file_format, 28),
                                            truncate_str(&info.color_space_tag, 28),
                                            info.luminance_mean,
                                            info.luminance_stddev,
                                            info.histogram_r_summary.mean_value,
                                            info.histogram_g_summary.mean_value,
                                            info.histogram_b_summary.mean_value,
                                            info.histogram_r_summary.dominant_bin_min,
                                            info.histogram_g_summary.dominant_bin_min,
                                            info.histogram_b_summary.dominant_bin_min,
                                            truncate_str(&tags_str, 28),
                                        );
                                    }
                                }
                                Err(e) => {
                                    if is_json {
                                        let output = serde_json::json!({
                                            "status": "error",
                                            "error": { "code": "INFO_NOT_FOUND", "message": e.to_string() }
                                        });
                                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                                    } else {
                                        eprintln!("Error: {}", e);
                                    }
                                    process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "error",
                                    "error": { "code": "INFO_DB_ERROR", "message": e.to_string() }
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                eprintln!("Error: INFO_DB_ERROR -- {}", e);
                            }
                            process::exit(1);
                        }
                    }
                }
                _ => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "INFO_MISSING_ARG", "message": "--project and --file are required" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: INFO_MISSING_ARG -- --project and --file are required");
                    }
                    process::exit(1);
                }
            }
        }
        Some(Commands::Tag { result: _, project, scene: _, add, remove, list, edit, edit_new, generate, ask }) => {
            let proj_name = match project {
                Some(ref p) => p.clone(),
                None => {
                    eprintln!("Error: TAG_MISSING_ARG -- --project is required");
                    process::exit(1);
                }
            };

            match db::open_db() {
                Ok(conn) => {
                    if generate {
                        // Generate AI tags for all fingerprints in project
                        let cfg = config::load_or_create_config().unwrap_or_default();
                        if !cfg.ai.tag_generation {
                            eprintln!("Error: TAG_GENERATION_DISABLED -- tag generation is disabled in config. Set ai.tag_generation = true to enable.");
                            process::exit(1);
                        }

                        // Check project exists before querying fingerprints
                        let project_exists: bool = conn.query_row(
                            "SELECT COUNT(*) FROM projects WHERE name = ?1",
                            [&proj_name],
                            |row| row.get::<_, i64>(0),
                        ).unwrap_or(0) > 0;

                        if !project_exists {
                            eprintln!("Error: PROJECT_NOT_FOUND -- project '{}' not found", proj_name);
                            process::exit(1);
                        }

                        let fingerprint_ids = match mengxi_core::tag::fingerprint_ids_for_project(&conn, &proj_name) {
                            Ok(ids) => ids,
                            Err(e) => {
                                eprintln!("Error: {}", e);
                                process::exit(1);
                            }
                        };

                        if fingerprint_ids.is_empty() {
                            println!("No fingerprints found for project '{}'.", proj_name);
                            return;
                        }

                        // Get file paths for each fingerprint
                        let mut fingerprint_paths: Vec<(i64, String)> = Vec::new();
                        for fp_id in &fingerprint_ids {
                            let path_result: Result<(String, String), _> = conn.query_row(
                                "SELECT p.path, f.filename FROM fingerprints fp
                                 JOIN files f ON f.id = fp.file_id
                                 JOIN projects p ON p.id = f.project_id
                                 WHERE fp.id = ?1",
                                [*fp_id],
                                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                            );
                            match path_result {
                                Ok((base_path, filename)) => {
                                    fingerprint_paths.push((*fp_id, format!("{}/{}", base_path, filename)));
                                }
                                Err(e) => {
                                    eprintln!("Warning: Could not get path for fingerprint {}: {}", fp_id, e);
                                    continue;
                                }
                            }
                        }

                        let total = fingerprint_paths.len();
                        let mut tag_count = 0;
                        let mut error_count = 0;

                        let mut bridge = mengxi_core::python_bridge::PythonBridge::new(
                            cfg.ai.idle_timeout_secs,
                            cfg.ai.inference_timeout_secs,
                            cfg.ai.tag_model.clone(),
                        );

                        // Early subprocess health check
                        match bridge.ping() {
                            Ok(true) => {},
                            Ok(false) => {
                                eprintln!("Error: AI subprocess not responding. Is Python installed and the mengxi_ai module available?");
                                process::exit(1);
                            }
                            Err(e) => {
                                eprintln!("Error: AI subprocess not available ({})", e);
                                process::exit(1);
                            }
                        }

                        let top_n = cfg.ai.tag_top_n.max(1);
                        // Get personalized tags from calibration (best-effort)
                        let personalized_tags = mengxi_core::calibration::get_personalized_tags(&conn).unwrap_or_default();
                        for (i, (fp_id, fpath)) in fingerprint_paths.iter().enumerate() {
                            eprintln!("Generating tags... {}/{}", i + 1, total);
                            match bridge.generate_tags_with_calibration(fpath, top_n, &personalized_tags) {
                                Ok(tags) => {
                                    for tag in &tags {
                                        if let Err(e) = mengxi_core::tag::tag_add(&conn, *fp_id, tag) {
                                            eprintln!("Warning: Failed to add tag '{}' to fingerprint {}: {}", tag, fp_id, e);
                                            error_count += 1;
                                        } else {
                                            tag_count += 1;
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Warning: Tag generation failed for fingerprint {}: {}", fp_id, e);
                                    error_count += 1;
                                }
                            }
                        }

                        if error_count > 0 {
                            println!("Generated {} tags for {} fingerprints in project '{}' ({} errors).", tag_count, total - error_count, proj_name, error_count);
                        } else {
                            println!("Generated {} tags for {} fingerprints in project '{}'.", tag_count, total, proj_name);
                        }
                    } else if ask {
                        // Interactive tag input: prompt for tags on each fingerprint
                        let fingerprint_ids = match mengxi_core::tag::fingerprint_ids_for_project(&conn, &proj_name) {
                            Ok(ids) => ids,
                            Err(e) => {
                                eprintln!("Error: {}", e);
                                process::exit(1);
                            }
                        };

                        if fingerprint_ids.is_empty() {
                            println!("No fingerprints found for project '{}'.", proj_name);
                            return;
                        }

                        // Get file paths for display
                        let mut fingerprint_paths: Vec<(i64, String)> = Vec::new();
                        for fp_id in &fingerprint_ids {
                            let path_result: Result<(String, String), _> = conn.query_row(
                                "SELECT p.path, f.filename FROM fingerprints fp
                                 JOIN files f ON f.id = fp.file_id
                                 JOIN projects p ON p.id = f.project_id
                                 WHERE fp.id = ?1",
                                [*fp_id],
                                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                            );
                            match path_result {
                                Ok((base_path, filename)) => {
                                    fingerprint_paths.push((*fp_id, format!("{}/{}", base_path, filename)));
                                }
                                Err(_) => continue,
                            }
                        }

                        let total = fingerprint_paths.len();
                        let mut total_tags = 0;
                        let stdin = io::stdin();

                        eprintln!("Project '{}': {} fingerprints to tag.", proj_name, total);
                        eprintln!("Enter comma-separated tags, or press Enter to skip. Type 'q' to quit.\n");

                        for (i, (fp_id, fpath)) in fingerprint_paths.iter().enumerate() {
                            // Show existing tags for this fingerprint
                            let existing_tags = mengxi_core::tag::tag_list_for_fingerprint_with_source(&conn, *fp_id)
                                .unwrap_or_default();
                            let existing_display: Vec<String> = existing_tags.iter()
                                .map(|(t, s)| format!("{} ({})", t, s))
                                .collect();
                            let existing_str = if existing_display.is_empty() {
                                String::new()
                            } else {
                                format!("  [existing: {}]", existing_display.join(", "))
                            };

                            eprintln!("[{}/{}] {}{}", i + 1, total, fpath, existing_str);
                            eprint!("  Tags: ");
                            let _ = io::stderr().flush();

                            let mut input = String::new();
                            match stdin.lock().read_line(&mut input) {
                                Ok(0) | Err(_) => break, // EOF or error → stop
                                Ok(_) => {
                                    let trimmed = input.trim();
                                    if trimmed == "q" || trimmed == "quit" {
                                        eprintln!("\nStopped. Tagged {}/{} fingerprints.", i, total);
                                        break;
                                    }
                                    if trimmed.is_empty() {
                                        continue; // skip this fingerprint
                                    }
                                    // Parse comma-separated tags
                                    let new_tags: Vec<&str> = trimmed
                                        .split(',')
                                        .map(|t| t.trim())
                                        .filter(|t| !t.is_empty())
                                        .collect();
                                    let mut added = 0;
                                    for tag in &new_tags {
                                        match mengxi_core::tag::tag_add_with_source(&conn, *fp_id, tag, "manual") {
                                            Ok(()) => added += 1,
                                            Err(_) => {} // skip duplicates
                                        }
                                    }
                                    total_tags += added;
                                    if added > 0 {
                                        eprintln!("  → added: {}", new_tags.join(", "));
                                    }
                                }
                            }
                        }
                        println!("Added {} manual tag(s) across project '{}'.", total_tags, proj_name);
                    } else if list {
                        // List tags for project with source indicator
                        match mengxi_core::tag::tag_list_for_project_with_source(&conn, &proj_name) {
                            Ok(tags) => {
                                if tags.is_empty() {
                                    println!("No tags for project '{}'.", proj_name);
                                } else {
                                    println!("Tags for project '{}':", proj_name);
                                    for (i, (tag, source)) in tags.iter().enumerate() {
                                        println!("  {}. {} ({})", i + 1, tag, source);
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error: {}", e);
                                process::exit(1);
                            }
                        }
                    } else if let Some(ref tag_text) = add {
                        // Add tag to project's fingerprints (source = "manual")
                        if tag_text.trim().is_empty() {
                            eprintln!("Error: TAG_MISSING_ARG -- tag must not be empty or whitespace-only");
                            process::exit(1);
                        }
                        match mengxi_core::tag::tag_add_to_project_with_source(&conn, &proj_name, tag_text, "manual") {
                            Ok(count) => {
                                // Record calibration (best-effort, once per operation)
                                let fp_ids = mengxi_core::tag::fingerprint_ids_for_project(&conn, &proj_name).unwrap_or_default();
                                if let Some(&fp_id) = fp_ids.first() {
                                    let added_json = serde_json::to_string(&[tag_text]).unwrap_or_else(|_| "[]".to_string());
                                    if let Err(e) = mengxi_core::calibration::record_calibration(&conn, &proj_name, fp_id, "[]", &added_json, "[]") {
                                        eprintln!("Warning: Failed to record calibration: {}", e);
                                    }
                                }
                                println!("Added tag '{}' to {} fingerprint(s) in project '{}'.", tag_text, count, proj_name);
                            }
                            Err(e) => {
                                eprintln!("Error: {}", e);
                                process::exit(1);
                            }
                        }
                    } else if let (Some(ref old_tag), Some(ref new_tag)) = (edit, edit_new) {
                        // Rename tag in project
                        match mengxi_core::tag::tag_rename_in_project(&conn, &proj_name, old_tag, new_tag) {
                            Ok(count) => {
                                // Record calibration (best-effort, once per operation)
                                let fp_ids = mengxi_core::tag::fingerprint_ids_for_project(&conn, &proj_name).unwrap_or_default();
                                if let Some(&fp_id) = fp_ids.first() {
                                    let renamed_json = serde_json::to_string(&[serde_json::json!({"old": old_tag, "new": new_tag})]).unwrap_or_else(|_| "[]".to_string());
                                    if let Err(e) = mengxi_core::calibration::record_calibration(&conn, &proj_name, fp_id, "[]", "[]", &renamed_json) {
                                        eprintln!("Warning: Failed to record calibration: {}", e);
                                    }
                                }
                                println!("Renamed tag '{}' to '{}' for {} fingerprint(s) in project '{}'.", old_tag, new_tag, count, proj_name);
                            }
                            Err(e) => {
                                match &e {
                                    mengxi_core::tag::TagError::NotFound(_) => {
                                        eprintln!("Error: TAG_NOT_FOUND -- tag '{}' not found in project '{}'", old_tag, proj_name);
                                    }
                                    mengxi_core::tag::TagError::DuplicateTag(_) => {
                                        eprintln!("Error: TAG_DUPLICATE -- tag '{}' already exists in project '{}'", new_tag, proj_name);
                                    }
                                    _ => {
                                        eprintln!("Error: {}", e);
                                    }
                                }
                                process::exit(1);
                            }
                        }
                    } else if let Some(ref tag_text) = remove {
                        // Remove tag from project's fingerprints
                        if tag_text.trim().is_empty() {
                            eprintln!("Error: TAG_MISSING_ARG -- tag must not be empty or whitespace-only");
                            process::exit(1);
                        }
                        // Check if tag is AI-sourced BEFORE removal (tag won't exist after delete)
                        let tags_with_source = mengxi_core::tag::tag_list_for_project_with_source(&conn, &proj_name).unwrap_or_default();
                        let ai_removed = tags_with_source.iter()
                            .any(|(t, s)| t == tag_text && s == "ai");

                        match mengxi_core::tag::tag_remove_from_project(&conn, &proj_name, tag_text) {
                            Ok(count) => {
                                if count > 0 {
                                    // Record calibration if tag was AI-sourced (best-effort, once per operation)
                                    if ai_removed {
                                        let fp_ids = mengxi_core::tag::fingerprint_ids_for_project(&conn, &proj_name).unwrap_or_default();
                                        if let Some(&fp_id) = fp_ids.first() {
                                            let removed_json = serde_json::to_string(&[tag_text]).unwrap_or_else(|_| "[]".to_string());
                                            if let Err(e) = mengxi_core::calibration::record_calibration(&conn, &proj_name, fp_id, &removed_json, "[]", "[]") {
                                                eprintln!("Warning: Failed to record calibration: {}", e);
                                            }
                                        }
                                    }
                                    println!("Removed tag '{}' from {} fingerprint(s) in project '{}'.", tag_text, count, proj_name);
                                } else {
                                    println!("Tag '{}' not found in project '{}'.", tag_text, proj_name);
                                }
                            }
                            Err(e) => {
                                eprintln!("Error: {}", e);
                                process::exit(1);
                            }
                        }
                    } else {
                        eprintln!("Error: TAG_MISSING_ARG -- specify --generate, --ask, --add, --remove, or --list");
                        process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Error: TAG_DB_ERROR -- {}", e);
                    process::exit(1);
                }
            }
        }
        Some(Commands::LutDiff { lut_a, lut_b, format }) => {
            let is_json = format.as_deref() == Some("json");

            // Validate required args
            let path_a = match &lut_a {
                Some(p) => {
                    if p == "~" {
                        match dirs::home_dir() {
                            Some(home) => home,
                            None => {
                                eprintln!("Error: LUTDIFF_MISSING_ARG -- cannot resolve home directory for '~'");
                                process::exit(1);
                            }
                        }
                    } else if p.starts_with("~/") {
                        match dirs::home_dir() {
                            Some(home) => home.join(&p[2..]),
                            None => {
                                eprintln!("Error: LUTDIFF_MISSING_ARG -- cannot resolve home directory for '~/...'");
                                process::exit(1);
                            }
                        }
                    } else {
                        std::path::PathBuf::from(p)
                    }
                }
                None => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "LUTDIFF_MISSING_ARG", "message": "<lut_a> is required" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: LUTDIFF_MISSING_ARG -- <lut_a> is required");
                    }
                    process::exit(1);
                }
            };
            let path_b = match &lut_b {
                Some(p) => {
                    if p == "~" {
                        match dirs::home_dir() {
                            Some(home) => home,
                            None => {
                                eprintln!("Error: LUTDIFF_MISSING_ARG -- cannot resolve home directory for '~'");
                                process::exit(1);
                            }
                        }
                    } else if p.starts_with("~/") {
                        match dirs::home_dir() {
                            Some(home) => home.join(&p[2..]),
                            None => {
                                eprintln!("Error: LUTDIFF_MISSING_ARG -- cannot resolve home directory for '~/...'");
                                process::exit(1);
                            }
                        }
                    } else {
                        std::path::PathBuf::from(p)
                    }
                }
                None => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "LUTDIFF_MISSING_ARG", "message": "<lut_b> is required" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: LUTDIFF_MISSING_ARG -- <lut_b> is required");
                    }
                    process::exit(1);
                }
            };

            match mengxi_core::lut_diff::compare_luts(&path_a, &path_b) {
                Ok(result) => {
                    if is_json {
                        let channels = serde_json::json!([
                            { "channel": "R", "mean_delta": result.channels[0].mean_delta, "max_delta": result.channels[0].max_delta, "changed_values": result.channels[0].changed_count },
                            { "channel": "G", "mean_delta": result.channels[1].mean_delta, "max_delta": result.channels[1].max_delta, "changed_values": result.channels[1].changed_count },
                            { "channel": "B", "mean_delta": result.channels[2].mean_delta, "max_delta": result.channels[2].max_delta, "changed_values": result.channels[2].changed_count },
                        ]);
                        let output = serde_json::json!({
                            "status": "ok",
                            "lut_a": path_a.to_string_lossy(),
                            "lut_b": path_b.to_string_lossy(),
                            "total_points": result.total_points,
                            "channels": channels
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        println!(
                            "LUT Diff: {} vs {}\n",
                            path_a.display(),
                            path_b.display()
                        );
                        println!("{:<12} {:>12} {:>12} {:>14}",
                            "Channel", "Mean Delta", "Max Delta", "Changed");
                        println!("{:<12} {:<12} {:<12} {:<14}", "----------", "----------", "----------", "----------");
                        for (name, ch) in [("R", &result.channels[0]), ("G", &result.channels[1]), ("B", &result.channels[2])] {
                            println!("{:<12} {:>12.6} {:>12.6} {:>14}",
                                name,
                                ch.mean_delta,
                                ch.max_delta,
                                ch.changed_count,
                            );
                        }
                        println!("\nTotal points compared: {}", result.total_points);
                    }
                }
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": format!("{}", e).split(" -- ").next().unwrap_or("LUTDIFF_ERROR"), "message": format!("{}", e) }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: {}", e);
                    }
                    process::exit(1);
                }
            }
        }
        Some(Commands::LutDep { lut, format }) => {
            let is_json = format.as_deref() == Some("json");

            let lut_path = match &lut {
                Some(p) => {
                    if p == "~" {
                        match dirs::home_dir() {
                            Some(home) => home,
                            None => {
                                eprintln!("Error: LUTDEP_MISSING_ARG -- cannot resolve home directory for '~'");
                                process::exit(1);
                            }
                        }
                    } else if p.starts_with("~/") {
                        match dirs::home_dir() {
                            Some(home) => home.join(&p[2..]),
                            None => {
                                eprintln!("Error: LUTDEP_MISSING_ARG -- cannot resolve home directory for '~/...'");
                                process::exit(1);
                            }
                        }
                    } else {
                        std::path::PathBuf::from(p)
                    }
                }
                None => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "LUTDEP_MISSING_ARG", "message": "--lut <path> is required" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: LUTDEP_MISSING_ARG -- --lut <path> is required");
                    }
                    process::exit(1);
                }
            };

            match db::open_db() {
                Ok(conn) => {
                    match mengxi_core::lut_diff::query_lut_dependency(&conn, &lut_path.to_string_lossy()) {
                        Ok(Some(dep)) => {
                            let timestamp = if dep.exported_at > 0 {
                                let secs = dep.exported_at as u64;
                                let (year, month, day, hour, min, sec) = seconds_to_datetime(secs);
                                format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, min, sec)
                            } else {
                                "unknown".to_string()
                            };
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "ok",
                                    "dependency": {
                                        "project": dep.project_name,
                                        "file": dep.file_path,
                                        "format": dep.format,
                                        "grid_size": dep.grid_size,
                                        "exported_at": timestamp,
                                        "lut_path": lut_path.to_string_lossy(),
                                    }
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                println!(
                                    "+----------+------------------------------+\n\
                                     | Field    | Value                        |\n\
                                     +----------+------------------------------+\n\
                                     | Project  | {:<28} |\n\
                                     | Scene    | {:<28} |\n\
                                     | Format   | {:<28} |\n\
                                     | Grid     | {}x{}x{:<23} |\n\
                                     | Exported | {:<28} |\n\
                                     | LUT Path | {:<28} |\n\
                                     +----------+------------------------------+",
                                    dep.project_name,
                                    dep.file_path,
                                    dep.format,
                                    dep.grid_size,
                                    dep.grid_size,
                                    dep.grid_size,
                                    timestamp,
                                    lut_path.display(),
                                );
                            }
                        }
                        Ok(None) => {
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "ok",
                                    "dependency": null,
                                    "message": "No dependency records found for this LUT"
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                println!("No dependency records found for this LUT");
                            }
                        }
                        Err(e) => {
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "error",
                                    "error": { "code": "LUTDEP_DB_ERROR", "message": e.to_string() }
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                eprintln!("Error: {}", e);
                            }
                            process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "LUTDEP_DB_ERROR", "message": "Failed to initialize database" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: LUTDEP_DB_ERROR -- {e}");
                    }
                    process::exit(1);
                }
            }
        }
        Some(Commands::Stats { user, period, format }) => {
            let is_json = format.as_deref() == Some("json");

            let conn = match db::open_db() {
                Ok(c) => c,
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "STATS_DB_ERROR", "message": format!("Failed to open database: {}", e) }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: STATS_DB_ERROR — Failed to open database: {}", e);
                    }
                    process::exit(1);
                }
            };

            // Parse period into since_timestamp
            let since_timestamp: Option<i64> = match period.as_deref() {
                Some("1day") => Some(started_at_unix - 86_400_000),
                Some("1week") => Some(started_at_unix - 604_800_000),
                Some("2weeks") => Some(started_at_unix - 1_209_600_000),
                Some("1month") => Some(started_at_unix - 2_592_000_000),
                Some(invalid) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "STATS_INVALID_PERIOD", "message": format!("Invalid period: '{}'. Use: 1day, 1week, 2weeks, 1month", invalid) }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: STATS_INVALID_PERIOD — Invalid period: '{}'. Use: 1day, 1week, 2weeks, 1month", invalid);
                    }
                    process::exit(1);
                }
                None => None,
            };

            let period_label = match period.as_deref() {
                Some(p) => p.to_string(),
                None => "all time".to_string(),
            };

            // Resolve user filter: CLI --user flag takes priority, fallback to config
            let effective_user = user.as_deref()
                .filter(|u| !u.is_empty())
                .map(|u| u.to_string())
                .or_else(|| {
                    config::load_or_create_config().ok().map(|c| c.general.user)
                });

            // Query session stats — user-scoped or global
            let (total_sessions, avg_duration_ms, total_searches, breakdown, recent) =
                if let Some(ref u) = effective_user {
                    let user_stats = analytics::get_user_stats(&conn, u, since_timestamp).unwrap_or_else(|_| analytics::UserStats {
                        user: u.clone(), session_count: 0, avg_duration_ms: 0, search_count: 0, last_session_at: None,
                    });
                    let breakdown = analytics::get_command_breakdown_for_user(&conn, u, since_timestamp).unwrap_or_default();
                    let recent = analytics::get_sessions_for_user(&conn, u, since_timestamp, 10).unwrap_or_default();
                    (user_stats.session_count, user_stats.avg_duration_ms, user_stats.search_count, breakdown, recent)
                } else {
                    let count = analytics::get_session_count(&conn, since_timestamp).unwrap_or(0);
                    let avg = analytics::get_average_duration_ms(&conn, since_timestamp).unwrap_or(0);
                    let bd = analytics::get_command_breakdown(&conn, since_timestamp).unwrap_or_default();
                    let rec = analytics::get_sessions(&conn, since_timestamp, 10).unwrap_or_default();
                    // Extract total searches from command breakdown
                    let searches = bd.iter().find(|(cmd, _)| cmd == "search").map(|(_, c)| *c).unwrap_or(0);
                    (count, avg, searches, bd, rec)
                };

            // New metrics: hit rate, calibration, vocabulary (best-effort)
            // search_feedback and calibration_activities store timestamps in seconds,
            // but since_timestamp is in milliseconds — convert before passing.
            let since_seconds = since_timestamp.map(|ts| ts / 1000);
            let hit_rate = analytics::get_search_hit_rate(&conn, since_seconds).unwrap_or_else(|_| analytics::HitRateMetrics {
                accepted: 0, rejected: 0, total: 0, rate: 0.0,
            });
            let calibration = analytics::get_calibration_metrics(&conn, since_seconds).unwrap_or_else(|_| analytics::CalibrationMetrics {
                total_corrections: 0, project_breakdown: Vec::new(), latest_correction_at: None,
            });
            let trend = analytics::get_calibration_trend(&conn).unwrap_or_default();
            let vocab = analytics::get_vocabulary_metrics(&conn).unwrap_or_else(|_| analytics::VocabularyMetrics {
                total_unique_tags: 0, new_tags_last_week: 0, top_tags: Vec::new(),
            });

            // Per-user breakdown (only when --user is NOT specified)
            let per_user = if effective_user.is_none() {
                analytics::get_per_user_breakdown(&conn, since_timestamp).unwrap_or_default()
            } else {
                Vec::new()
            };

            if is_json {
                let mut cmd_map = serde_json::Map::new();
                for (cmd, count) in &breakdown {
                    cmd_map.insert(cmd.clone(), serde_json::json!(*count));
                }
                let recent_json: Vec<serde_json::Value> = recent.iter().map(|s| {
                    let mut obj = serde_json::Map::new();
                    obj.insert("session_id".to_string(), serde_json::json!(&s.session_id));
                    obj.insert("command".to_string(), serde_json::json!(&s.command));
                    obj.insert("started_at".to_string(), serde_json::json!(s.started_at));
                    obj.insert("duration_ms".to_string(), serde_json::json!(s.duration_ms));
                    obj.insert("exit_code".to_string(), serde_json::json!(s.exit_code));
                    if let Some(ste) = s.search_to_export_ms {
                        obj.insert("search_to_export_ms".to_string(), serde_json::json!(ste));
                    }
                    serde_json::Value::Object(obj)
                }).collect();
                // Build calibration project_breakdown as ordered map
                let mut cal_breakdown = serde_json::Map::new();
                for (k, v) in &calibration.project_breakdown {
                    cal_breakdown.insert(k.clone(), serde_json::json!(*v));
                }
                // Build trend as JSON array
                let trend_json: Vec<serde_json::Value> = trend.iter().map(|tp| {
                    serde_json::json!({
                        "week_start": tp.week_start,
                        "rate": tp.rate,
                    })
                }).collect();

                let mut output_map = serde_json::Map::new();
                output_map.insert("period".to_string(), serde_json::json!(period_label));
                output_map.insert("total_sessions".to_string(), serde_json::json!(total_sessions));
                output_map.insert("average_duration_ms".to_string(), serde_json::json!(avg_duration_ms));
                output_map.insert("total_searches".to_string(), serde_json::json!(total_searches));
                output_map.insert("command_breakdown".to_string(), serde_json::json!(cmd_map));
                output_map.insert("recent_sessions".to_string(), serde_json::json!(recent_json));
                output_map.insert("search_hit_rate".to_string(), serde_json::json!({
                    "accepted": hit_rate.accepted,
                    "rejected": hit_rate.rejected,
                    "total": hit_rate.total,
                    "rate": hit_rate.rate,
                }));
                output_map.insert("calibration".to_string(), serde_json::json!({
                    "total_corrections": calibration.total_corrections,
                    "project_breakdown": cal_breakdown,
                    "latest_correction_at": calibration.latest_correction_at,
                }));
                output_map.insert("trend".to_string(), serde_json::json!(trend_json));
                output_map.insert("vocabulary".to_string(), serde_json::json!({
                    "total_unique_tags": vocab.total_unique_tags,
                    "new_tags_last_week": vocab.new_tags_last_week,
                    "top_tags": vocab.top_tags.iter().map(|(tag, count)| serde_json::json!({"tag": tag, "count": count})).collect::<Vec<_>>(),
                }));

                // User-scoped output
                if let Some(ref u) = effective_user {
                    output_map.insert("user".to_string(), serde_json::json!(u));
                }
                // Per-user breakdown
                if !per_user.is_empty() {
                    let users_json: Vec<serde_json::Value> = per_user.iter().map(|us| {
                        serde_json::json!({
                            "user": us.user,
                            "session_count": us.session_count,
                            "avg_duration_ms": us.avg_duration_ms,
                            "search_count": us.search_count,
                        })
                    }).collect();
                    output_map.insert("users".to_string(), serde_json::json!(users_json));
                }

                println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(output_map)).unwrap());
            } else {
                println!("Usage Statistics ({}):", period_label);
                println!("  Total sessions:    {}", total_sessions);
                println!("  Average duration:  {}", format_duration_ms(avg_duration_ms));
                println!("  Total searches:     {}", total_searches);
                if !breakdown.is_empty() {
                    println!("  Command breakdown:");
                    for (cmd, count) in &breakdown {
                        println!("    {:12} {}", cmd, count);
                    }
                }
                if !recent.is_empty() {
                    println!("\nRecent sessions:");
                    println!("  {:2}  {:<20} {:<10} {:<10} {}", "#", "Time", "Command", "Duration", "Status");
                    for (i, s) in recent.iter().enumerate() {
                        let (y, m, d, h, min, sec) = seconds_to_datetime((s.started_at / 1000) as u64);
                        let time_str = format!("{}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, min, sec);
                        let status = if s.exit_code == 0 { "OK" } else { "ERROR" };
                        println!("  {:2}  {:<20} {:<10} {:<10} {}", i + 1, time_str, s.command, format_duration_ms(s.duration_ms), status);
                    }
                }

                // Search quality metrics
                if hit_rate.total > 0 {
                    println!("\nSearch Quality:");
                    println!("  Acceptance rate:  {:.1}% ({} accepted, {} rejected)",
                        hit_rate.rate * 100.0, hit_rate.accepted, hit_rate.rejected);
                }

                // Calibration metrics
                if calibration.total_corrections > 0 {
                    println!("\nCalibration:");
                    println!("  Total corrections: {}", calibration.total_corrections);
                    if let Some(latest) = calibration.latest_correction_at {
                        let (y, m, d, h, min, sec) = seconds_to_datetime(latest as u64);
                        println!("  Latest at:         {}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, min, sec);
                    }
                    if !calibration.project_breakdown.is_empty() {
                        println!("  Top projects:");
                        for (proj, count) in &calibration.project_breakdown {
                            println!("    {:<20} {}", proj, count);
                        }
                    }
                }

                // Trend metrics
                if trend.len() >= 2 {
                    let direction = if trend.last().unwrap().rate > trend.first().unwrap().rate { "improving" } else { "declining" };
                    println!("\nTrend ({} weeks, {}):", trend.len(), direction);
                    for tp in &trend {
                        let (y, m, d, _, _, _) = seconds_to_datetime(tp.week_start as u64);
                        println!("  {}-{:02}-{:02}  {:.1}%", y, m, d, tp.rate * 100.0);
                    }
                }

                // Vocabulary metrics
                if vocab.total_unique_tags > 0 {
                    println!("\nVocabulary:");
                    println!("  Unique tags:       {}", vocab.total_unique_tags);
                    if vocab.new_tags_last_week > 0 {
                        println!("  New this week:     {}", vocab.new_tags_last_week);
                    }
                    if !vocab.top_tags.is_empty() {
                        println!("  Top tags:");
                        for (tag, count) in &vocab.top_tags {
                            println!("    {:<20} {}", tag, count);
                        }
                    }
                }

                // Per-user breakdown (only when multiple users exist and no --user filter)
                if per_user.len() > 1 && effective_user.is_none() {
                    println!("\nUser Breakdown:");
                    println!("  {:<20} | {:>8} | {:>13} | {:>8}", "User", "Sessions", "Avg Duration", "Searches");
                    println!("  {}-+-{}-+-{}-+-{}-", "--------------------", "--------", "-------------", "--------");
                    for us in &per_user {
                        println!("  {:<20} | {:>8} | {:>13} | {:>8}",
                            us.user, us.session_count, format_duration_ms(us.avg_duration_ms), us.search_count);
                    }
                }
            }
        }
        Some(Commands::Config { show: true, edit: false }) => {
            match config::load_or_create_config() {
                Ok(cfg) => println!("{cfg}"),
                Err(e) => {
                    eprintln!("Error: CONFIG_LOAD_FAILED — {e}");
                    process::exit(1);
                }
            }
        }
        Some(Commands::Config { show: false, edit: true }) => {
            eprintln!("Error: 'config --edit' is not yet implemented");
            process::exit(1);
        }
        Some(Commands::Config { show: false, edit: false })
        | Some(Commands::Config { show: true, edit: true }) => {
            eprintln!("Error: Specify --show or --edit");
            process::exit(1);
        }
        Some(Commands::Validate { format, full }) => {
            let is_json = format == "json";
            let exit_code = validate::run_validate(is_json, full);
            process::exit(exit_code);
        }
        Some(Commands::Reextract { project, file, json: is_json }) => {
            if project.is_none() && file.is_none() {
                eprintln!("Error: specify --project <name> or provide a FILE path");
                process::exit(1);
            }
            let conn = match db::open_db() {
                Ok(c) => c,
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "REEXTRACT_DB_ERROR", "message": e.to_string() }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: DB_OPEN_FAILED — {}", e);
                    }
                    process::exit(1);
                }
            };
            let fps = if let Some(ref proj) = project {
                match mengxi_core::fingerprint::list_fingerprints_by_project(&conn, proj) {
                    Ok(fps) if fps.is_empty() => {
                        if is_json {
                            let output = serde_json::json!({
                                "status": "error",
                                "error": { "code": "REEXTRACT_NOT_FOUND", "message": format!("no fingerprints found for project: {}", proj) }
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                        } else {
                            eprintln!("Error: no fingerprints found for project: {}", proj);
                        }
                        process::exit(1);
                    }
                    Ok(fps) => fps,
                    Err(e) => {
                        if is_json {
                            let output = serde_json::json!({
                                "status": "error",
                                "error": { "code": "REEXTRACT_DB_ERROR", "message": e.to_string() }
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                        } else {
                            eprintln!("Error: {}", e);
                        }
                        process::exit(1);
                    }
                }
            } else {
                // File mode: look up fingerprints for the given file path
                let file_path = file.as_ref().unwrap();
                match mengxi_core::fingerprint::list_fingerprints_by_file(&conn, file_path) {
                    Ok(fps) if fps.is_empty() => {
                        if is_json {
                            let output = serde_json::json!({
                                "status": "error",
                                "error": { "code": "REEXTRACT_NOT_FOUND", "message": format!("no fingerprint found for file: {}", file_path) }
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                        } else {
                            eprintln!("Error: no fingerprint found for file: {}", file_path);
                        }
                        process::exit(1);
                    }
                    Ok(fps) => fps,
                    Err(e) => {
                        if is_json {
                            let output = serde_json::json!({
                                "status": "error",
                                "error": { "code": "REEXTRACT_DB_ERROR", "message": e.to_string() }
                            });
                            println!("{}", serde_json::to_string_pretty(&output).unwrap());
                        } else {
                            eprintln!("Error: {}", e);
                        }
                        process::exit(1);
                    }
                }
            };
            let mut reextracted = 0usize;
            let mut skipped = 0usize;
            let mut failed = 0usize;
            let mut failures: Vec<serde_json::Value> = Vec::new();
            for (fp_id, fp_path) in &fps {
                eprintln!("re-extracting {} ({}/{})", fp_path, reextracted + skipped + failed + 1, fps.len());
                match mengxi_core::fingerprint::reextract_grading_features(&conn, *fp_id) {
                    Ok(mengxi_core::fingerprint::ReextractResult::Reextracted) => reextracted += 1,
                    Ok(mengxi_core::fingerprint::ReextractResult::Skipped(reason)) => {
                        skipped += 1;
                        eprintln!("  skipped: {}", reason);
                    }
                    Ok(mengxi_core::fingerprint::ReextractResult::Error(reason)) => {
                        failed += 1;
                        eprintln!("  error: {}", reason);
                        failures.push(serde_json::json!({
                            "file": fp_path,
                            "reason": reason,
                        }));
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("  error: {}", e);
                        failures.push(serde_json::json!({
                            "file": fp_path,
                            "reason": e.to_string(),
                        }));
                    }
                }
            }
            if is_json {
                let mut output = serde_json::Map::new();
                output.insert("status".to_string(), serde_json::json!("ok"));
                output.insert("total".to_string(), serde_json::json!(fps.len()));
                output.insert("reextracted".to_string(), serde_json::json!(reextracted));
                output.insert("skipped".to_string(), serde_json::json!(skipped));
                output.insert("failed".to_string(), serde_json::json!(failed));
                if !failures.is_empty() {
                    output.insert("failures".to_string(), serde_json::json!(failures));
                }
                println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(output)).unwrap());
            } else {
                println!("Re-extraction complete: {} reextracted, {} skipped, {} failed ({} total)",
                    reextracted, skipped, failed, fps.len());
            }
            if failed > 0 {
                process::exit(1);
            }
        }
        Some(Commands::Embed { project, force, json: is_json }) => {
            let conn = match db::open_db() {
                Ok(c) => c,
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "EMBED_DB_ERROR", "message": e.to_string() }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: DB_OPEN_FAILED — {}", e);
                    }
                    process::exit(1);
                }
            };

            // Build query for fingerprints needing embeddings
            let mut sql = String::from(
                "SELECT fp.id, p.path || '/' || f.filename
                 FROM fingerprints fp
                 JOIN files f ON fp.file_id = f.id
                 JOIN projects p ON f.project_id = p.id"
            );
            if !force {
                sql.push_str(" WHERE fp.embedding IS NULL");
            }
            if let Some(ref _proj) = project {
                if sql.contains("WHERE") {
                    sql.push_str(" AND p.name = ?1");
                } else {
                    sql.push_str(" WHERE p.name = ?1");
                }
            }

            let fps: Vec<(i64, String)> = {
                let mut stmt = conn.prepare(&sql).unwrap();
                let rows: Result<Vec<_>, _> = match &project {
                    Some(proj) => stmt.query_map(rusqlite::params![proj], |row| {
                        Ok((row.get::<_, i64>(0).unwrap(), row.get::<_, String>(1).unwrap()))
                    }).unwrap().collect(),
                    None => stmt.query_map([], |row| {
                        Ok((row.get::<_, i64>(0).unwrap(), row.get::<_, String>(1).unwrap()))
                    }).unwrap().collect(),
                };
                rows.unwrap_or_default()
            };

            if fps.is_empty() {
                if is_json {
                    let output = serde_json::json!({
                        "status": "ok",
                        "generated": 0,
                        "skipped": 0,
                        "failed": 0,
                        "message": "no fingerprints to embed"
                    });
                    println!("{}", serde_json::to_string_pretty(&output).unwrap());
                } else {
                    println!("No fingerprints to embed.");
                }
                process::exit(0);
            }

            let total = fps.len();
            eprintln!("Generating embeddings for {} fingerprints...", total);

            let cfg = config::load_or_create_config().unwrap_or_default();
            let mut bridge = mengxi_core::python_bridge::PythonBridge::new(
                cfg.ai.idle_timeout_secs,
                cfg.ai.inference_timeout_secs,
                cfg.ai.embedding_model.clone(),
            );

            // Health check
            match bridge.ping() {
                Ok(true) => {},
                Ok(false) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "EMBED_AI_UNAVAILABLE", "message": "AI subprocess not responding. Is Python installed and mengxi_ai module available?" }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: AI subprocess not responding. Is Python installed and mengxi_ai module available?");
                    }
                    process::exit(1);
                }
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "EMBED_AI_UNAVAILABLE", "message": format!("AI subprocess error: {}", e) }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: AI subprocess not available ({})", e);
                    }
                    process::exit(1);
                }
            }

            let mut generated = 0usize;
            let mut skipped = 0usize;
            let mut failed = 0usize;
            let mut failures: Vec<serde_json::Value> = Vec::new();

            for (i, (fp_id, fp_path)) in fps.iter().enumerate() {
                eprintln!("Embedding {} ({}/{})", fp_path, i + 1, total);
                match bridge.generate_embedding(fp_path) {
                    Ok(embedding) => {
                        let blob = mengxi_core::search::serialize_embedding(&embedding);
                        match conn.execute(
                            "UPDATE fingerprints SET embedding = ?1, embedding_model = ?2 WHERE id = ?3",
                            rusqlite::params![blob, &cfg.ai.embedding_model, fp_id],
                        ) {
                            Ok(_) => generated += 1,
                            Err(e) => {
                                failed += 1;
                                eprintln!("  error: DB write failed: {}", e);
                                failures.push(serde_json::json!({
                                    "file": fp_path,
                                    "reason": format!("DB write failed: {}", e),
                                }));
                            }
                        }
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("  error: {}", e);
                        failures.push(serde_json::json!({
                            "file": fp_path,
                            "reason": e.to_string(),
                        }));
                    }
                }
            }

            if is_json {
                let mut output = serde_json::Map::new();
                output.insert("status".to_string(), serde_json::json!("ok"));
                output.insert("total".to_string(), serde_json::json!(total));
                output.insert("generated".to_string(), serde_json::json!(generated));
                output.insert("skipped".to_string(), serde_json::json!(skipped));
                output.insert("failed".to_string(), serde_json::json!(failed));
                if !failures.is_empty() {
                    output.insert("failures".to_string(), serde_json::json!(failures));
                }
                println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(output)).unwrap());
            } else {
                println!("Embedding complete: {} generated, {} skipped, {} failed ({} total)",
                    generated, skipped, failed, total);
            }
            if failed > 0 {
                process::exit(1);
            }
        }
        Some(Commands::ValidateDataset { dir, json: is_json }) => {
            let exit_code = validate_dataset::run_validate_dataset(&dir, is_json);
            process::exit(exit_code);
        }
        Some(Commands::Db { command }) => {
            let conn = match db::open_db() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error: DB_OPEN_FAILED — {e}");
                    process::exit(1);
                }
            };
            match command {
                DbSubcommand::Projects { format } => {
                    let is_json = format == "json";
                    let projects = db::db_list_projects(&conn).unwrap_or_default();
                    if is_json {
                        let arr: Vec<serde_json::Value> = projects.iter().map(|p| {
                            serde_json::json!({
                                "id": p.id,
                                "name": p.name,
                                "path": p.path,
                                "dpx_count": p.dpx_count,
                                "exr_count": p.exr_count,
                                "mov_count": p.mov_count,
                                "file_count": p.file_count,
                                "fingerprint_count": p.fingerprint_count,
                                "created_at": p.created_at,
                            })
                        }).collect();
                        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": "ok", "projects": arr })).unwrap());
                    } else if projects.is_empty() {
                        println!("No projects found.");
                    } else {
                        println!("{:<4} {:<20} {:<30} {:>6} {:>6} {:>6} {:>6} {:>6}",
                            "ID", "Name", "Path", "DPX", "EXR", "MOV", "Files", "FPs");
                        for p in &projects {
                            println!("{:<4} {:<20} {:<30} {:>6} {:>6} {:>6} {:>6} {:>6}",
                                p.id, truncate_str(&p.name, 20), truncate_str(&p.path, 30),
                                p.dpx_count, p.exr_count, p.mov_count, p.file_count, p.fingerprint_count);
                        }
                    }
                }
                DbSubcommand::Files { project, format } => {
                    let is_json = format == "json";
                    let files = db::db_list_files(&conn, &project).unwrap_or_default();
                    if is_json {
                        let arr: Vec<serde_json::Value> = files.iter().map(|f| {
                            serde_json::json!({
                                "id": f.id,
                                "filename": f.filename,
                                "format": f.format,
                                "fingerprint_count": f.fingerprint_count,
                                "created_at": f.created_at,
                            })
                        }).collect();
                        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": "ok", "files": arr })).unwrap());
                    } else if files.is_empty() {
                        println!("No files found in project '{}'.", project);
                    } else {
                        println!("{:<4} {:<30} {:<8} {:>6}", "ID", "Filename", "Format", "FPs");
                        for f in &files {
                            println!("{:<4} {:<30} {:<8} {:>6}",
                                f.id, truncate_str(&f.filename, 30), f.format, f.fingerprint_count);
                        }
                    }
                }
                DbSubcommand::Tags { project, format } => {
                    let is_json = format == "json";
                    let tags = db::db_list_tags(&conn, project.as_deref()).unwrap_or_default();
                    if is_json {
                        let arr: Vec<serde_json::Value> = tags.iter().map(|t| {
                            serde_json::json!({
                                "id": t.id,
                                "tag": t.tag,
                                "source": t.source,
                                "project": t.project_name,
                                "filename": t.filename,
                                "created_at": t.created_at,
                            })
                        }).collect();
                        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": "ok", "tags": arr })).unwrap());
                    } else if tags.is_empty() {
                        println!("No tags found.");
                    } else {
                        println!("{:<4} {:<20} {:<8} {:<16} {:<24}", "ID", "Tag", "Source", "Project", "File");
                        for t in &tags {
                            println!("{:<4} {:<20} {:<8} {:<16} {:<24}",
                                t.id, truncate_str(&t.tag, 20), t.source,
                                truncate_str(&t.project_name, 16), truncate_str(&t.filename, 24));
                        }
                    }
                }
                DbSubcommand::Luts { format } => {
                    let is_json = format == "json";
                    let luts = db::db_list_luts(&conn).unwrap_or_default();
                    if is_json {
                        let arr: Vec<serde_json::Value> = luts.iter().map(|l| {
                            serde_json::json!({
                                "id": l.id,
                                "title": l.title,
                                "format": l.format,
                                "grid_size": l.grid_size,
                                "output_path": l.output_path,
                                "project": l.project_name,
                                "created_at": l.created_at,
                            })
                        }).collect();
                        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": "ok", "luts": arr })).unwrap());
                    } else if luts.is_empty() {
                        println!("No LUTs found.");
                    } else {
                        println!("{:<4} {:<20} {:<8} {:>6} {:<30} {:<16}", "ID", "Title", "Format", "Grid", "Output", "Project");
                        for l in &luts {
                            println!("{:<4} {:<20} {:<8} {:>6} {:<30} {:<16}",
                                l.id,
                                truncate_str(&l.title.as_deref().unwrap_or("-"), 20),
                                l.format, l.grid_size,
                                truncate_str(&l.output_path, 30),
                                truncate_str(&l.project_name, 16));
                        }
                    }
                }
                DbSubcommand::Sql { query } => {
                    match db::db_run_query(&conn, &query) {
                        Ok((cols, rows)) => {
                            if cols.is_empty() {
                                println!("Query returned no columns.");
                            } else {
                                // Compute column widths
                                let mut widths: Vec<usize> = cols.iter().map(|c| c.len()).collect();
                                for row in &rows {
                                    for (i, val) in row.iter().enumerate() {
                                        if i < widths.len() {
                                            widths[i] = widths[i].max(val.len());
                                        }
                                    }
                                }
                                // Print header
                                let header: String = cols.iter().zip(widths.iter())
                                    .map(|(c, w)| format!(" {:w$} ", truncate_str(c, *w), w = w))
                                    .collect::<Vec<_>>()
                                    .join("|");
                                let separator: String = widths.iter()
                                    .map(|w| format!("{}-{}", "-", "-".repeat(*w)))
                                    .collect::<Vec<_>>()
                                    .join("+");
                                println!("+{}+", separator);
                                println!("|{}|", header);
                                println!("+{}+", separator);
                                for row in &rows {
                                    let line: String = row.iter().zip(widths.iter())
                                        .map(|(v, w)| format!(" {:w$} ", truncate_str(v, *w), w = w))
                                        .collect::<Vec<_>>()
                                        .join("|");
                                    println!("|{}|", line);
                                }
                                println!("+{}+", separator);
                                println!("{} row(s)", rows.len());
                            }
                        }
                        Err(e) => {
                            eprintln!("Error: {e}");
                            process::exit(1);
                        }
                    }
                }
            }
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
        &session_id, &command_name, &args_json,
        started_at_unix, ended_at_unix, duration_ms, 0,
        search_to_export_ms_override, &user_name,
    );
}

/// Display search results in text table or JSON format.
fn display_search_results(results: &[mengxi_core::search::SearchResult], is_json: bool) {
    if is_json {
        let json_results: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "rank": r.rank,
                    "project": r.project_name,
                    "file": r.file_path,
                    "score": r.score.max(0.0)
                })
            })
            .collect();
        let output = serde_json::json!({
            "status": "ok",
            "results": json_results,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else if results.is_empty() {
        println!("No results found.");
    } else {
        println!(
            "+------+------------------+--------------------------+-------------+"
        );
        println!(
            "| Rank | Project          | File                     | Similarity  |"
        );
        println!(
            "+------+------------------+--------------------------+-------------+"
        );
        for r in results {
            let display_score = r.score.max(0.0);
            let score_pct = format!("{:.1}%", display_score * 100.0);
            println!(
                "| {:<4} | {:<16} | {:<24} | {:<11} |",
                r.rank,
                truncate_str(&r.project_name, 16),
                truncate_str(&r.file_path, 24),
                score_pct
            );
        }
        println!(
            "+------+------------------+--------------------------+-------------+"
        );
    }
}

/// Resolve hybrid search weights from --search-mode and --weights flags.
/// When both are provided, --weights takes priority (FR15).
/// Per-query --weights allows weight=0.0 (warning to stderr).
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
            _ => unreachable!("clap validates --search-mode values"),
        }
    } else {
        Ok(mengxi_core::hybrid_scoring::SignalWeights::grading_first())
    }
}

/// Resolve an image path to a file_id in the database.
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
fn low_result_explanation(count: usize) -> Option<String> {
    match count {
        0 => Some("无匹配结果 -- 候选集中无高相似度调色风格".to_string()),
        1 => Some("仅找到 1 个匹配 -- 候选集可能不足或参考图风格较特殊".to_string()),
        2 => Some("仅找到 2 个匹配 -- 候选集可能不足或参考图风格较特殊".to_string()),
        _ => None,
    }
}

/// Record accept/reject feedback for a search result.
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
        let cli = Cli::try_parse_from(["mengxi"]);
        // No subcommand should return None (clap shows help)
        assert!(cli.is_ok());
        assert!(cli.unwrap().command.is_none());
    }

    #[test]
    fn test_import_command_parsing() {
        let cli = Cli::try_parse_from([
            "mengxi",
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
            "mengxi",
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
            "mengxi",
            "export",
            "--result", "3",
            "--format", "cube",
            "--output", "~/lut/grade.cube",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Export { result, format, output, grid_size, force }) => {
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
        let cli = Cli::try_parse_from(["mengxi", "config", "--show"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Config { show: true, edit: false }) => {}
            _ => panic!("Expected Config with --show"),
        }
    }

    #[test]
    fn test_lut_diff_command_parsing() {
        let cli = Cli::try_parse_from([
            "mengxi",
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
            "mengxi",
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
            "mengxi",
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
            "mengxi",
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
            "mengxi",
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
            "mengxi",
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
            "mengxi",
            "tag",
            "--project", "my_film",
            "--edit", "old_tag",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_tag_edit_new_requires_edit() {
        let cli = Cli::try_parse_from([
            "mengxi",
            "tag",
            "--project", "my_film",
            "--edit-new", "new_tag",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_tag_edit_conflicts_with_add() {
        let cli = Cli::try_parse_from([
            "mengxi",
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
            "mengxi",
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
            "mengxi",
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
            "mengxi",
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
            "mengxi",
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
            "mengxi",
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
            "mengxi",
            "lut-diff",
            "a.cube", "b.cube",
            "--format", "xml",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_lut_diff_missing_args_parsing() {
        // lut-diff without positional args should still parse (args are Option<String>)
        let cli = Cli::try_parse_from(["mengxi", "lut-diff"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_lut_dep_missing_arg_parsing() {
        // lut-dep without --lut should still parse (arg is Option<String>)
        let cli = Cli::try_parse_from(["mengxi", "lut-dep"]);
        assert!(cli.is_ok());
    }

    #[test]
    fn test_search_format_field_renamed() {
        // Verify --format flag (not --output-format) works on search command
        let cli = Cli::try_parse_from([
            "mengxi",
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
            "mengxi", "search", "--image", "/ref.jpg", "--accept", "2",
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
            "mengxi", "search", "--tag", "warm", "--reject", "1",
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
            "mengxi", "search", "--accept", "1", "--reject", "2",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_info_command_with_project_file() {
        let cli = Cli::try_parse_from([
            "mengxi", "info", "--project", "film", "--file", "scene.dpx",
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
            "mengxi",
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
        let cli = Cli::try_parse_from(["mengxi", "db", "projects"]);
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
        let cli = Cli::try_parse_from(["mengxi", "db", "projects", "--format", "json"]);
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
        let cli = Cli::try_parse_from(["mengxi", "db", "files", "--project", "film_a"]);
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
        let cli = Cli::try_parse_from(["mengxi", "db", "tags", "--project", "film_a"]);
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
        let cli = Cli::try_parse_from(["mengxi", "db", "tags"]);
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
        let cli = Cli::try_parse_from(["mengxi", "db", "luts"]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Db { command: DbSubcommand::Luts { .. } }) => {}
            _ => panic!("Expected Db Luts command"),
        }
    }

    #[test]
    fn test_db_sql_parsing() {
        let cli = Cli::try_parse_from(["mengxi", "db", "sql", "SELECT * FROM projects"]);
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
        let cli = Cli::try_parse_from(["mengxi", "db", "projects"]);
        let (name, args, _) = extract_command_info(&cli.unwrap());
        assert_eq!(name, "db");
        assert_eq!(args, "{}");
    }

    // ── Hybrid search tests (Story 3.3) ──

    #[test]
    fn test_search_mode_grading_first() {
        let cli = Cli::try_parse_from([
            "mengxi", "search", "--image", "ref.tif", "--search-mode", "grading-first",
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
            "mengxi", "search", "--image", "ref.tif", "--search-mode", "balanced",
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
            "mengxi", "search", "--image", "ref.tif", "--search-mode", "invalid",
        ]);
        assert!(cli.is_err());
    }

    #[test]
    fn test_weights_parsing() {
        let cli = Cli::try_parse_from([
            "mengxi", "search", "--image", "ref.tif",
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
            "mengxi", "search", "--image", "ref.tif",
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
            "mengxi", "search", "--image", "ref.tif",
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
            "mengxi", "search", "--image", "ref.tif", "--limit", "5",
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
}
