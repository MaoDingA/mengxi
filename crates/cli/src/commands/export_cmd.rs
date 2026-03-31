use std::process;

use mengxi_core::db;

pub fn execute(result: Option<u32>, format: Option<String>, output: Option<String>, grid_size: u32, force: bool, output_format: String) {
    let is_json = output_format == "json";

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
            } else if let Some(stripped) = p.strip_prefix("~/") {
                let home = dirs::home_dir().unwrap_or_default();
                home.join(stripped)
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
                                        println!(
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
                                println!(
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
                            println!(
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
                        println!(
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
