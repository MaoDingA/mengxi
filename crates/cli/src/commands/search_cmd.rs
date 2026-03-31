use std::collections::HashSet;
use std::path::Path;
use std::process;

use unicode_width::UnicodeWidthStr;

use mengxi_core::db;
use mengxi_core::hybrid_scoring;
use mengxi_core::search;

#[allow(clippy::too_many_arguments)]
pub fn execute(
    image: Option<String>,
    tag: Option<String>,
    limit: Option<u32>,
    project: Option<String>,
    accept: Option<u32>,
    reject: Option<u32>,
    format: String,
    search_mode: Option<String>,
    weights: Option<String>,
    tile_mode: Option<String>,
    tile_range: Option<String>,
) {
    let _ = (tile_mode, tile_range); // reserved for future use
    let is_json = format == "json";

    // F-07: warn when --search-mode/--weights used without --image
    if image.is_none() && (search_mode.is_some() || weights.is_some()) {
        eprintln!("warning: --search-mode and --weights require --image, flags ignored");
    }

    // --image: embedding-based search (optionally combined with --tag)
    if let Some(ref img_path) = image {
        let cfg = crate::config::load_or_create_config().unwrap_or_default();
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
                let options = search::SearchOptions {
                    project: project.clone(),
                    limit: limit_val as usize,
                    use_pyramid: search_mode.as_deref() == Some("pyramid"),
                };

                // Resolve search weights via config cascade when no CLI args
                let config_weights = if search_mode.is_some() || weights.is_some() {
                    None
                } else {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    match crate::config::resolve_search_config(&cwd) {
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

                    match search::hybrid_search(&conn, file_id, &resolved_weights, &options) {
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
                            search::search_by_image_and_tag(
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
                            search::search_by_image(
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
                                            "score": r.score,
                                            "score_breakdown": null,
                                            "human_readable": "",
                                            "match_warnings": []
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
        let cfg = crate::config::load_or_create_config().unwrap_or_default();
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
                let options = search::SearchOptions {
                    project: project.clone(),
                    limit: limit_val as usize,
                    use_pyramid: false,
                };

                match search::search_by_tag(&conn, tag_text, &options) {
                    Ok(results) => {
                        display_search_results(&results, is_json);
                        record_feedback_if_needed(&conn, &results, accept, reject, "tag", is_json);
                    }
                    Err(search::SearchError::NoFingerprints) => {
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
        let cfg = crate::config::load_or_create_config().unwrap_or_default();
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
                let options = search::SearchOptions {
                    project: project.clone(),
                    limit: limit_val as usize,
                    use_pyramid: false,
                };

                match search::search_histograms(&conn, &options) {
                    Ok(results) => {
                        if is_json {
                            let json_results: Vec<serde_json::Value> = results
                                .iter()
                                .map(|r| {
                                    serde_json::json!({
                                        "rank": r.rank,
                                        "project": r.project_name,
                                        "file": r.file_path,
                                        "score": r.score,
                                        "score_breakdown": null,
                                        "human_readable": "",
                                        "match_warnings": []
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
                    Err(search::SearchError::NoFingerprints) => {
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
                    Err(search::SearchError::ProjectNotFound(name)) => {
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

fn display_search_results(results: &[search::SearchResult], is_json: bool) {
    if is_json {
        let json_results: Vec<serde_json::Value> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "rank": r.rank,
                    "project": r.project_name,
                    "file": r.file_path,
                    "score": r.score.max(0.0),
                    "score_breakdown": null,
                    "human_readable": "",
                    "match_warnings": []
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
) -> Result<hybrid_scoring::SignalWeights, String> {
    if let Some(ws) = weights_str {
        // Parse "grading=0.6,clip=0.3,tag=0.1"
        let mut grading = 0.0_f64;
        let mut clip = 0.0_f64;
        let mut tag = 0.0_f64;
        let mut seen_keys: HashSet<&str> = HashSet::new();

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

        Ok(hybrid_scoring::SignalWeights { grading, clip, tag })
    } else if let Some(mode) = search_mode {
        match mode {
            "grading-first" => Ok(hybrid_scoring::SignalWeights::grading_first()),
            "balanced" => Ok(hybrid_scoring::SignalWeights::balanced()),
            "pyramid" => Ok(hybrid_scoring::SignalWeights::grading_first()),
            _ => unreachable!("clap validates --search-mode values"),
        }
    } else {
        Ok(hybrid_scoring::SignalWeights::grading_first())
    }
}

/// Resolve an image path to a file_id in the database.
fn resolve_image_to_file_id(
    conn: &db::DbConnection,
    image_path: &str,
    project: Option<&str>,
) -> Result<i64, String> {
    let filename = Path::new(image_path)
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
                "warning: {} files named '{}', using first match -- specify --project to disambiguate",
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
fn format_breakdown(breakdown: &hybrid_scoring::ScoreBreakdown) -> String {
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
    conn: &db::DbConnection,
    results: &[search::SearchResult],
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
