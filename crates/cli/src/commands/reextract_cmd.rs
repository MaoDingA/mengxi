use std::process;

use mengxi_core::db;
use mengxi_core::fingerprint;


pub fn execute(project: Option<String>, file: Option<String>, format: String) {
    let is_json = format == "json";
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
        match fingerprint::list_fingerprints_by_project(&conn, proj) {
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
        match fingerprint::list_fingerprints_by_file(&conn, file_path) {
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
    let reextracted;
    let skipped;
    let failed;
    let mut failures: Vec<serde_json::Value> = Vec::new();

    let reextract_cfg = crate::config::load_or_create_config().unwrap_or_default();
    match fingerprint::batch_reextract_grading_features(
        &conn,
        &fps,
        reextract_cfg.import.tile_grid_size,
        |i, total, path| {
            eprintln!("re-extracting {} ({}/{})", path, i + 1, total);
        },
    ) {
        Ok(batch_result) => {
            reextracted = batch_result.reextracted;
            skipped = batch_result.skipped;
            failed = batch_result.failed;
            for (fp_path, reason) in &batch_result.failures {
                failures.push(serde_json::json!({
                    "file": fp_path,
                    "reason": reason,
                }));
            }
        }
        Err(e) => {
            if is_json {
                let output = serde_json::json!({
                    "status": "error",
                    "error": { "code": "REEXTRACT_TX_ERROR", "message": e.to_string() }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: REEXTRACT_TX_ERROR — {}", e);
            }
            process::exit(1);
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
