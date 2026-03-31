use std::io::{self, BufRead, Write};
use std::process;

use mengxi_core::db;

#[allow(clippy::too_many_arguments)]
pub fn execute(
#[allow(clippy::too_many_arguments)]
    _result: Option<u32>,
    project: Option<String>,
    _scene: Option<String>,
    add: Option<String>,
    remove: Option<String>,
    list: bool,
    edit: Option<String>,
    edit_new: Option<String>,
    generate: bool,
    ask: bool,
) {
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
                let cfg = crate::config::load_or_create_config().unwrap_or_default();
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
                                if let Ok(()) = mengxi_core::tag::tag_add_with_source(&conn, *fp_id, tag, "manual") {
                                    added += 1
                                } // skip duplicates
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
