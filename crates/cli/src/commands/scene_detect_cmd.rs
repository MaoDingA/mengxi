// scene_detect_cmd.rs — Detect scene boundaries in a fingerprint strip
use std::process;

pub fn execute(
    strip_image: Option<String>,
    threshold: f64,
    min_scene_length: usize,
    max_boundaries: usize,
    format: String,
) {
    let is_json = format == "json";

    let strip_path = match strip_image {
        Some(ref p) => std::path::PathBuf::from(p),
        None => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "SCENE_DETECT_MISSING_ARG", "message": "strip image path is required" }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: SCENE_DETECT_MISSING_ARG -- strip image path is required");
            }
            process::exit(1);
        }
    };

    if !strip_path.exists() {
        if is_json {
            let out = serde_json::json!({
                "status": "error",
                "error": { "code": "SCENE_DETECT_FILE_NOT_FOUND", "message": format!("file not found: {}", strip_path.display()) }
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        } else {
            eprintln!("Error: SCENE_DETECT_FILE_NOT_FOUND -- file not found: {}", strip_path.display());
        }
        process::exit(1);
    }

    // Read strip image via mengxi-core helper
    let (width, height, strip) = match mengxi_core::movie_fingerprint::read_strip_png(&strip_path) {
        Ok(data) => data,
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "SCENE_DETECT_IMAGE_ERROR", "message": format!("failed to read strip image: {}", e) }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: SCENE_DETECT_IMAGE_ERROR -- failed to read strip image: {}", e);
            }
            process::exit(1);
        }
    };

    // Detect scene boundaries
    match mengxi_core::scene_boundary::detect_scene_boundaries(
        &strip, width, height, threshold, min_scene_length, max_boundaries,
    ) {
        Ok(boundaries) => {
            if is_json {
                let bounds_json: Vec<serde_json::Value> = boundaries.iter().map(|b| {
                    serde_json::json!({
                        "frame_idx": b.frame_idx,
                        "confidence": format!("{:.4}", b.confidence),
                    })
                }).collect();
                let out = serde_json::json!({
                    "status": "ok",
                    "boundary_count": boundaries.len(),
                    "strip_width": width,
                    "strip_height": height,
                    "boundaries": bounds_json
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Detected {} scene boundaries (threshold: {}, strip: {}x{})",
                    boundaries.len(), threshold, width, height);
                for (i, b) in boundaries.iter().enumerate() {
                    eprintln!("  Boundary {}: frame {} (confidence: {:.4})", i + 1, b.frame_idx, b.confidence);
                }
            }
        }
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "SCENE_DETECT_ERROR", "message": e.to_string() }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: SCENE_DETECT_ERROR -- {}", e);
            }
            process::exit(1);
        }
    }
}
