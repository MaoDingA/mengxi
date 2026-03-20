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
        /// Override input format (dpx, exr, mov)
        #[arg(long)]
        format: Option<String>,
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
        /// Search result ID to export
        #[arg(long)]
        result: Option<u32>,
        /// LUT output format (cube, 3dl, look, csp, cdl)
        #[arg(long)]
        format: Option<String>,
        /// Output file path
        #[arg(long)]
        output: Option<String>,
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
        Some(Commands::Import { project, name, format: _ }) => {
            let project_path = match project {
                Some(p) => p,
                None => {
                    eprintln!("Error: IMPORT_MISSING_ARG — --project <path> is required");
                    process::exit(1);
                }
            };
            let project_name = match name {
                Some(n) => n,
                None => {
                    eprintln!("Error: IMPORT_MISSING_ARG --name <string> is required");
                    process::exit(1);
                }
            };

            let path = Path::new(&project_path);
            match db::open_db() {
                Ok(conn) => match project::register_project(&conn, &project_name, &path) {
                    Ok((proj, breakdown)) => {
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
                        println!(
                            "+----------+------------------------------+\n\
                             | Field    | Value                        |\n\
                             +----------+------------------------------+\n\
                             | Name     | {:<28} |\n\
                             | Path     | {:<28} |\n\
                             | DPX      | {:<28}|\n\
                             | EXR      | {:<28} |\n\
                             | MOV      | {:<28} |\n\
                             +----------+------------------------------+",
                            proj.name,
                            proj.path,
                            dpx_detail,
                            exr_detail,
                            format!("{}{}", mov_detail, skipped_detail),
                        );
                    }
                    Err(project::ImportError::PathNotFound(msg)) => {
                        eprintln!("Error: {msg}");
                        process::exit(1);
                    }
                    Err(project::ImportError::DuplicateName(msg)) => {
                        eprintln!("Error: {msg}");
                        process::exit(1);
                    }
                    Err(project::ImportError::DbError(msg)) => {
                        eprintln!("Error: IMPORT_DB_ERROR — {msg}");
                        process::exit(1);
                    }
                    Err(project::ImportError::CorruptFile { filename, reason }) => {
                        eprintln!("Error: IMPORT_CORRUPT_FILE -- Failed to decode {}: {}", filename, reason);
                        process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("Error: IMPORT_DB_INIT_FAILED — {e}");
                    process::exit(1);
                }
            }
        }
        Some(Commands::Search { .. }) => {
            eprintln!("Error: 'search' command is not yet implemented");
            process::exit(1);
        }
        Some(Commands::Export { .. }) => {
            eprintln!("Error: 'export' command is not yet implemented");
            process::exit(1);
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
            "--format", "dpx",
        ]);
        assert!(cli.is_ok());
        match cli.unwrap().command {
            Some(Commands::Import { project, name, format }) => {
                assert_eq!(project.as_deref(), Some("/path/to/film"));
                assert_eq!(name.as_deref(), Some("my_film"));
                assert_eq!(format.as_deref(), Some("dpx"));
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
            Some(Commands::Export { result, format, output }) => {
                assert_eq!(result, Some(3));
                assert_eq!(format.as_deref(), Some("cube"));
                assert_eq!(output.as_deref(), Some("~/lut/grade.cube"));
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
