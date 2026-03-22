mod config;

use unicode_width::UnicodeWidthStr;

use clap::{Parser, Subcommand};
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
        #[arg(long, conflicts_with = "add", conflicts_with = "remove", conflicts_with = "list", conflicts_with = "edit", conflicts_with = "edit_new")]
        generate: bool,
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
}

// ---------------------------------------------------------------------------
// Session tracking helpers
// ---------------------------------------------------------------------------

/// Generate a simple session ID: timestamp + random suffix.
fn generate_session_id() -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let rand_suffix: u32 = (((ts as u64) & 0xFFFF) ^ ((ts as u64 >> 16) & 0xFFFF)) as u32;
    format!("{}_{}", ts, rand_suffix)
}

/// Extract command name, key args as JSON, and optional search-to-export timing.
fn extract_command_info(cli: &Cli) -> (String, String, Option<i64>) {
    match &cli.command {
        Some(Commands::Import { name, .. }) => {
            let obj = serde_json::json!({ "name": name });
            ("import".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::Search { project, tag, .. }) => {
            let mut obj = serde_json::Map::new();
            if let Some(p) = project { obj.insert("project".to_string(), serde_json::json!(p)); }
            if let Some(t) = tag { obj.insert("tag".to_string(), serde_json::json!(t)); }
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
                    now_ts - search_ts
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
        Some(Commands::Tag { project, .. }) => {
            let obj = serde_json::json!({ "project": project });
            ("tag".to_string(), serde_json::to_string(&obj).unwrap_or_default(), None)
        }
        Some(Commands::LutDiff { .. }) => ("lut-diff".to_string(), "{}".to_string(), None),
        Some(Commands::LutDep { .. }) => ("lut-dep".to_string(), "{}".to_string(), None),
        Some(Commands::Stats { .. }) => ("stats".to_string(), "{}".to_string(), None),
        Some(Commands::Config { .. }) => ("config".to_string(), "{}".to_string(), None),
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
) {
    if let Ok(conn) = db::open_db() {
        if let Err(e) = analytics::record_session(
            &conn, session_id, command, args_json,
            started_at, ended_at, duration_ms, exit_code, search_to_export_ms,
        ) {
            eprintln!("Warning: Failed to record session: {}", e);
        }
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
                            eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
        }) => {
            let is_json = format == "json";

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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                                    eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                            eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                                    eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                            eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                    eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                                eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                                eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: INFO_MISSING_ARG -- --project and --file are required");
                    }
                    process::exit(1);
                }
            }
        }
        Some(Commands::Tag { result: _, project, scene: _, add, remove, list, edit, edit_new, generate }) => {
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
                        eprintln!("Error: TAG_MISSING_ARG -- specify --generate, --add, --remove, or --list");
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                                eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
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
                        eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: LUTDEP_DB_ERROR -- {e}");
                    }
                    process::exit(1);
                }
            }
        }
        Some(Commands::Stats { user: _user, period, format }) => {
            // --user is a no-op placeholder (single-user tool)
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

            let total_sessions = analytics::get_session_count(&conn, since_timestamp).unwrap_or(0);
            let avg_duration_ms = analytics::get_average_duration_ms(&conn, since_timestamp).unwrap_or(0);
            let breakdown = analytics::get_command_breakdown(&conn, since_timestamp).unwrap_or_default();
            let recent = analytics::get_sessions(&conn, since_timestamp, 10).unwrap_or_default();

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
                let output = serde_json::json!({
                    "period": period_label,
                    "total_sessions": total_sessions,
                    "average_duration_ms": avg_duration_ms,
                    "command_breakdown": cmd_map,
                    "recent_sessions": recent_json,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!("Usage Statistics ({}):", period_label);
                println!("  Total sessions:    {}", total_sessions);
                println!("  Average duration:  {}", format_duration_ms(avg_duration_ms));
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
                        let (y, m, d, h, min, sec) = seconds_to_datetime(s.started_at as u64);
                        let time_str = format!("{}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, min, sec);
                        let status = if s.exit_code == 0 { "OK" } else { "ERROR" };
                        println!("  {:2}  {:<20} {:<10} {:<10} {}", i + 1, time_str, s.command, format_duration_ms(s.duration_ms), status);
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
        None => {
            // No subcommand — clap displays help automatically
        }
    }

    // Record session (best-effort, non-blocking)
    let ended_at = SystemTime::now();
    let ended_at_unix = ended_at.duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    let duration_ms = ended_at_unix - started_at_unix;

    record_session_best_effort(
        &session_id, &command_name, &args_json,
        started_at_unix, ended_at_unix, duration_ms, 0,
        search_to_export_ms_override,
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
            Some(Commands::Search { image, tag, limit, project, accept, reject, format }) => {
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
}
