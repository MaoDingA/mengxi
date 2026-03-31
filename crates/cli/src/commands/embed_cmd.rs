use std::process;

use mengxi_core::db;
use mengxi_core::python_bridge;
use mengxi_core::search;


pub fn execute(project: Option<String>, force: bool, format: String) {
    let is_json = format == "json";
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

    let cfg = crate::config::load_or_create_config().unwrap_or_default();
    let mut bridge = python_bridge::PythonBridge::new(
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
    let skipped = 0usize;
    let mut failed = 0usize;
    let mut failures: Vec<serde_json::Value> = Vec::new();

    for (i, (fp_id, fp_path)) in fps.iter().enumerate() {
        eprintln!("Embedding {} ({}/{})", fp_path, i + 1, total);
        match bridge.generate_embedding(fp_path) {
            Ok(embedding) => {
                let blob = search::serialize_embedding(&embedding);
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
