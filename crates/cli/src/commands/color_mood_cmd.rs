// color_mood_cmd.rs — Compute color mood timeline from fingerprint strip
use std::process;

pub fn execute(
    strip_image: Option<String>,
    boundaries: Option<String>,
    format: String,
) {
    let is_json = format == "json";

    let strip_path = match strip_image {
        Some(ref p) => std::path::PathBuf::from(p),
        None => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "COLOR_MOOD_MISSING_ARG", "message": "strip image path is required" }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: COLOR_MOOD_MISSING_ARG -- strip image path is required");
            }
            process::exit(1);
        }
    };

    if !strip_path.exists() {
        if is_json {
            let out = serde_json::json!({
                "status": "error",
                "error": { "code": "COLOR_MOOD_FILE_NOT_FOUND", "message": format!("file not found: {}", strip_path.display()) }
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        } else {
            eprintln!("Error: COLOR_MOOD_FILE_NOT_FOUND -- file not found: {}", strip_path.display());
        }
        process::exit(1);
    }

    // Parse optional boundaries (comma-separated frame indices)
    let boundary_frames: Vec<usize> = match boundaries {
        Some(ref s) => s.split(',').filter_map(|v| v.trim().parse().ok()).collect(),
        None => vec![],
    };

    // Read strip image via mengxi-core helper
    let (width, height, strip) = match mengxi_core::movie_fingerprint::read_strip_png(&strip_path) {
        Ok(data) => data,
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "COLOR_MOOD_IMAGE_ERROR", "message": format!("failed to read strip image: {}", e) }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: COLOR_MOOD_IMAGE_ERROR -- failed to read strip image: {}", e);
            }
            process::exit(1);
        }
    };

    // Compute mood timeline
    match mengxi_core::color_mood::compute_mood_timeline(&strip, width, height, &boundary_frames) {
        Ok(segments) => {
            if is_json {
                let segs_json: Vec<serde_json::Value> = segments.iter().map(|s| {
                    serde_json::json!({
                        "start_frame": s.start_frame,
                        "end_frame": s.end_frame,
                        "mood": s.mood.description_en(),
                        "mood_zh": s.mood.description_zh(),
                    })
                }).collect();
                let out = serde_json::json!({
                    "status": "ok",
                    "segment_count": segments.len(),
                    "strip_width": width,
                    "strip_height": height,
                    "segments": segs_json
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Color mood timeline ({} segments, strip: {}x{}):", segments.len(), width, height);
                for s in &segments {
                    eprintln!("  Frames {}-{}: {} ({})",
                        s.start_frame, s.end_frame,
                        s.mood.description_zh(),
                        s.mood.description_en());
                }
            }
        }
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "COLOR_MOOD_ERROR", "message": e.to_string() }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: COLOR_MOOD_ERROR -- {}", e);
            }
            process::exit(1);
        }
    }
}
