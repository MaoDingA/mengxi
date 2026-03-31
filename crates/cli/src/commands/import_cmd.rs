use std::path::Path;
use std::process;

use mengxi_core::db;
use mengxi_core::project;


pub fn execute(project: Option<String>, name: Option<String>, format: String) {
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

    let cfg = crate::config::load_or_create_config().unwrap_or_default();
    let tile_grid_size = cfg.import.tile_grid_size;

    match db::open_db() {
        Ok(conn) => match project::register_project(&conn, &project_name, path, tile_grid_size, |current, total, filename| {
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
                let cfg = crate::config::load_or_create_config().unwrap_or_default();
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
