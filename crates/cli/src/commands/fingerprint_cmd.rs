use std::process;

pub fn execute(
    video: Option<String>,
    mode: String,
    interval: f64,
    max_frames: Option<usize>,
    diameter: usize,
    output: Option<String>,
    format: String,
    watermark: Option<String>,
    wm_position: String,
    show_ep: bool,
) {
    let is_json = format == "json";

    // 1. Validate video path
    let video_path = match video {
        Some(ref p) => std::path::PathBuf::from(p),
        None => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "FINGERPRINT_MISSING_ARG", "message": "video path is required" }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: FINGERPRINT_MISSING_ARG -- video path is required");
            }
            process::exit(1);
        }
    };

    if !video_path.exists() {
        if is_json {
            let out = serde_json::json!({
                "status": "error",
                "error": { "code": "FINGERPRINT_FILE_NOT_FOUND", "message": format!("video file not found: {}", video_path.display()) }
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        } else {
            eprintln!(
                "Error: FINGERPRINT_FILE_NOT_FOUND -- video file not found: {}",
                video_path.display()
            );
        }
        process::exit(1);
    }

    // Validate interval
    if interval <= 0.0 {
        if is_json {
            let out = serde_json::json!({
                "status": "error",
                "error": { "code": "FINGERPRINT_INVALID_INTERVAL", "message": format!("interval must be > 0, got {}", interval) }
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        } else {
            eprintln!(
                "Error: FINGERPRINT_INVALID_INTERVAL -- interval must be > 0, got {}",
                interval
            );
        }
        process::exit(1);
    }

    // Validate diameter
    if diameter > 4096 {
        if is_json {
            let out = serde_json::json!({
                "status": "error",
                "error": { "code": "FINGERPRINT_INVALID_DIAMETER", "message": format!("diameter {} exceeds maximum 4096 (would allocate ~{:.1} GB)", diameter, (diameter * diameter * 3 * 8) as f64 / 1e9) }
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        } else {
            eprintln!(
                "Error: FINGERPRINT_INVALID_DIAMETER -- diameter {} exceeds maximum 4096",
                diameter
            );
        }
        process::exit(1);
    }

    // 2. Resolve output directory
    let output_dir = match &output {
        Some(p) => {
            let expanded = if p == "~" {
                dirs::home_dir().unwrap_or_default()
            } else if let Some(stripped) = p.strip_prefix("~/") {
                let home = dirs::home_dir().unwrap_or_default();
                home.join(stripped)
            } else {
                std::path::PathBuf::from(p)
            };
            expanded
        }
        None => std::path::PathBuf::from("."),
    };

    // Create output directory if needed
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        if is_json {
            let out = serde_json::json!({
                "status": "error",
                "error": { "code": "FINGERPRINT_DIR_ERROR", "message": format!("failed to create output directory: {}", e) }
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        } else {
            eprintln!("Error: FINGERPRINT_DIR_ERROR -- failed to create output directory: {}", e);
        }
        process::exit(1);
    }

    // 3. Extract frames as PPM using the format crate (single ffmpeg invocation)
    let frame_dir = output_dir.join(".fingerprint_frames");

    if !is_json {
        eprintln!("Extracting frames from {} (interval: {}s)...", video_path.display(), interval);
    }

    let frame_paths = match mengxi_format::keyframe::extract_frames_ppm(
        &video_path,
        &frame_dir,
        interval,
        max_frames,
    ) {
        Ok(paths) => paths,
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "FINGERPRINT_FRAME_EXTRACT_ERROR", "message": e }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: FINGERPRINT_FRAME_EXTRACT_ERROR -- {}", e);
            }
            process::exit(1);
        }
    };

    if frame_paths.is_empty() {
        if is_json {
            let out = serde_json::json!({
                "status": "error",
                "error": { "code": "FINGERPRINT_NO_FRAMES", "message": "no frames extracted from video" }
            });
            println!("{}", serde_json::to_string_pretty(&out).unwrap());
        } else {
            eprintln!("Error: FINGERPRINT_NO_FRAMES -- no frames extracted from video");
        }
        process::exit(1);
    }

    if !is_json {
        eprintln!("Extracted {} frames", frame_paths.len());
    }

    // 5. Build fingerprint mode
    let fingerprint_mode = match mode.as_str() {
        "strip" => mengxi_core::movie_fingerprint::FingerprintMode::Strip,
        "cineiris" => mengxi_core::movie_fingerprint::FingerprintMode::CineIris { diameter },
        "both" => mengxi_core::movie_fingerprint::FingerprintMode::Both { diameter },
        "cineprint" => mengxi_core::movie_fingerprint::FingerprintMode::CinePrint {
            thumbnails: 11,
            watermark_path: watermark,
            watermark_position: wm_position.clone(),
            show_ep_label: show_ep,
        },
        _ => unreachable!("clap validates mode values"),
    };

    if !is_json {
        eprintln!("Generating fingerprint (mode: {})...", mode);
    }

    // 5. Generate fingerprint
    let video_stem = video_path.file_stem()
        .and_then(|s| s.to_str());
    let gen_result = mengxi_core::movie_fingerprint::generate_fingerprint(
        &frame_paths,
        &output_dir,
        &fingerprint_mode,
        video_stem,
    );

    // 6. Clean up temp frame directory (always runs, even after error)
    if let Err(e) = std::fs::remove_dir_all(&frame_dir) {
        eprintln!("Warning: failed to clean up temp frames: {}", e);
    }

    // Handle result after cleanup
    match gen_result {
        Ok(result) => {
            if is_json {
                let mut json_out = serde_json::json!({
                    "status": "ok",
                    "frame_count": result.frame_count,
                });
                if let Some(ref strip_path) = result.strip_path {
                    json_out["strip_path"] = serde_json::json!(strip_path.to_string_lossy());
                }
                if let Some(ref cineiris_path) = result.cineiris_path {
                    json_out["cineiris_path"] = serde_json::json!(cineiris_path.to_string_lossy());
                }
                if let Some(ref cineprint_path) = result.cineprint_path {
                    json_out["cineprint_path"] = serde_json::json!(cineprint_path.to_string_lossy());
                }
                println!("{}", serde_json::to_string_pretty(&json_out).unwrap());
            } else {
                if let Some(ref strip_path) = result.strip_path {
                    eprintln!("Strip fingerprint: {}", strip_path.display());
                }
                if let Some(ref cineiris_path) = result.cineiris_path {
                    eprintln!("CineIris fingerprint: {}", cineiris_path.display());
                }
                if let Some(ref cineprint_path) = result.cineprint_path {
                    eprintln!("CinePrint fingerprint: {}", cineprint_path.display());
                }
                eprintln!("Frames processed: {}", result.frame_count);
            }
        }
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "FINGERPRINT_GENERATION_ERROR", "message": e.to_string() }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: FINGERPRINT_GENERATION_ERROR -- {}", e);
            }
            process::exit(1);
        }
    }
}
