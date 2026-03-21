mod config;

use unicode_width::UnicodeWidthStr;

use clap::{Parser, Subcommand};
use std::path::Path;
use std::process;

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
        /// Search result ID
        #[arg(long)]
        result: Option<u32>,
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
        #[arg(long, conflicts_with = "remove", conflicts_with = "list", conflicts_with = "edit")]
        add: Option<String>,
        /// Remove a tag
        #[arg(long, conflicts_with = "add", conflicts_with = "list", conflicts_with = "edit")]
        remove: Option<String>,
        /// List all tags
        #[arg(long, conflicts_with = "add", conflicts_with = "remove", conflicts_with = "edit")]
        list: bool,
        /// Edit (rename) a tag
        #[arg(long)]
        edit: Option<String>,
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

fn main() {
    let cli = Cli::parse();

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
                                 +----------+------------------------------+",
                                proj.name,
                                proj.path,
                                dpx_detail,
                                exr_detail,
                                format!("{}{}", mov_detail, skipped_detail),
                                fp_detail,
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
            format,
        }) => {
            let is_json = format == "json";

            // --image not yet implemented (Story 3.3)
            if image.is_some() {
                if is_json {
                    let output = serde_json::json!({
                        "status": "error",
                        "error": { "code": "SEARCH_IMAGE_NOT_AVAILABLE", "message": "Image-based search requires AI embedding (Story 3.3)" }
                    });
                    eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
                } else {
                    eprintln!("Error: SEARCH_IMAGE_NOT_AVAILABLE -- Image-based search requires AI embedding (Story 3.3)");
                }
                process::exit(1);
            }

            // --tag not yet implemented (Story 3.4)
            if tag.is_some() {
                if is_json {
                    let output = serde_json::json!({
                        "status": "error",
                        "error": { "code": "SEARCH_TAG_NOT_AVAILABLE", "message": "Tag search not yet implemented (Story 3.4)" }
                    });
                    eprintln!("{}", serde_json::to_string_pretty(&output).unwrap());
                } else {
                    eprintln!("Error: SEARCH_TAG_NOT_AVAILABLE -- Tag search not yet implemented (Story 3.4)");
                }
                process::exit(1);
            }

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
        Some(Commands::Info { .. }) => {
            eprintln!("Error: 'info' command is not yet implemented");
            process::exit(1);
        }
        Some(Commands::Tag { .. }) => {
            eprintln!("Error: 'tag' command is not yet implemented");
            process::exit(1);
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
        Some(Commands::Stats { .. }) => {
            eprintln!("Error: 'stats' command is not yet implemented");
            process::exit(1);
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
            Some(Commands::Search { image, tag, limit, project, format }) => {
                assert_eq!(image.as_deref(), Some("/ref/mood.jpg"));
                assert_eq!(tag.as_deref(), Some("industrial"));
                assert_eq!(limit, Some(10));
                assert_eq!(project.as_deref(), Some("my_film"));
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
            Some(Commands::Tag { add, project, .. }) => {
                assert_eq!(add.as_deref(), Some("industrial warm"));
                assert_eq!(project.as_deref(), Some("my_film"));
            }
            _ => panic!("Expected Tag command"),
        }
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
