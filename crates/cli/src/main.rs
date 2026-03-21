mod config;

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
        #[arg(long, default_value_t = 5)]
        limit: u32,
        /// Scope search to a specific project
        #[arg(long)]
        project: Option<String>,
        /// Output format (text, json)
        #[arg(long, default_value = "text")]
        output_format: String,
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
        #[arg(long)]
        format: Option<String>,
    },
    /// Track LUT dependencies
    #[command(name = "lut-dep")]
    LutDep {
        /// LUT file path
        #[arg(long)]
        lut: Option<String>,
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
        Some(Commands::Search { .. }) => {
            eprintln!("Error: 'search' command is not yet implemented");
            process::exit(1);
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
        Some(Commands::LutDiff { .. }) => {
            eprintln!("Error: 'lut-diff' command is not yet implemented");
            process::exit(1);
        }
        Some(Commands::LutDep { .. }) => {
            eprintln!("Error: 'lut-dep' command is not yet implemented");
            process::exit(1);
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
            Some(Commands::Search { image, tag, limit, project, .. }) => {
                assert_eq!(image.as_deref(), Some("/ref/mood.jpg"));
                assert_eq!(tag.as_deref(), Some("industrial"));
                assert_eq!(limit, 10);
                assert_eq!(project.as_deref(), Some("my_film"));
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
}
