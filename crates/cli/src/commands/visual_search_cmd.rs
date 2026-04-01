// visual_search_cmd.rs — Visual search for similar movies by color DNA
use std::process;

pub fn execute(
    query: Option<String>,
    library: Option<String>,
    limit: usize,
    format: String,
) {
    let is_json = format == "json";

    let query_path = match query {
        Some(ref p) => std::path::PathBuf::from(p),
        None => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "VISUAL_SEARCH_MISSING_QUERY", "message": "query strip image path is required" }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: VISUAL_SEARCH_MISSING_QUERY -- query strip image path is required");
            }
            process::exit(1);
        }
    };

    let library_path = match library {
        Some(ref p) => std::path::PathBuf::from(p),
        None => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "VISUAL_SEARCH_MISSING_LIBRARY", "message": "library directory path is required (--library)" }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: VISUAL_SEARCH_MISSING_LIBRARY -- library directory path is required (--library)");
            }
            process::exit(1);
        }
    };

    if !query_path.exists() {
        eprintln!("Error: VISUAL_SEARCH_FILE_NOT_FOUND -- query strip not found: {}", query_path.display());
        process::exit(1);
    }

    match mengxi_core::movie_search::visual_search(&query_path, &library_path, limit) {
        Ok(results) => {
            if is_json {
                let results_json: Vec<serde_json::Value> = results.iter().map(|r| {
                    serde_json::json!({
                        "name": r.name,
                        "path": r.path.to_string_lossy(),
                        "overall_similarity": format!("{:.4}", r.overall_similarity),
                        "hue_similarity": format!("{:.4}", r.hue_similarity),
                        "contrast_diff": format!("{:.4}", r.contrast_diff),
                        "warmth_diff": format!("{:.4}", r.warmth_diff),
                    })
                }).collect();
                let out = serde_json::json!({
                    "status": "ok",
                    "result_count": results_json.len(),
                    "results": results_json
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Visual search results ({} matches):", results.len());
                for (i, r) in results.iter().enumerate() {
                    eprintln!(
                        "  {}. {} (similarity: {:.4}, hue: {:.4}, contrast_diff: {:.4})",
                        i + 1, r.name, r.overall_similarity, r.hue_similarity, r.contrast_diff
                    );
                }
            }
        }
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "VISUAL_SEARCH_ERROR", "message": e.to_string() }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: VISUAL_SEARCH_ERROR -- {}", e);
            }
            process::exit(1);
        }
    }
}
