use std::process;

use mengxi_core::python_bridge::PythonBridge;

pub fn execute(
    video: Option<String>,
    mode: String,
    interval: f64,
    max_frames: Option<usize>,
    diameter: usize,
    output: Option<String>,
    format: String,
    title: Option<String>,
    director: Option<String>,
    colorist: Option<String>,
    team: Option<String>,
    project_type: Option<String>,
    year: Option<String>,
    font: Option<String>,
    watermark: bool,
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

    // --- Metadata resolution (CLI priority + auto-extraction fallback) ---
    let resolved_title = title.unwrap_or_else(|| {
        // Extract from filename stem (e.g., "EP01.mov" -> "EP01")
        video_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_uppercase())
            .unwrap_or_else(|| "FINGERPRINT".into())
    });

    let resolved_director = director.unwrap_or_else(|| "-".into());
    let resolved_colorist = colorist.unwrap_or_else(|| "-".into());
    let resolved_team = team.unwrap_or_else(|| String::new());
    let resolved_project_type = project_type.unwrap_or_else(|| String::new());

    let resolved_year = year.unwrap_or_else(|| {
        // Try to extract year from filename regex, or use file modification time
        let fname = video_path.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        // Regex-like: look for 19xx or 20xx pattern
        if let Some(cap) = regex_find_year(fname) {
            cap
        } else {
            "----".into()
        }
    });

    // 4. Poster mode: show resolved metadata before generating
    if mode == "poster" && !is_json {
        let stem_name = video_path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        eprintln!("┌─────────────────────────────────────────┐");
        eprintln!("│  Poster 元信息预览                        │");
        eprintln!("├─────────────────────────────────────────┤");
        eprintln!("│  标题   : {}{}", resolved_title, if resolved_title == stem_name { " (来自文件名)" } else { "" });
        eprintln!("│  类型   : {}", if resolved_project_type.is_empty() { "(未指定)" } else { &resolved_project_type });
        eprintln!("│  年份   : {}", if resolved_year.is_empty() || resolved_year == "----" { "(未指定)" } else { &resolved_year });
        eprintln!("│  调光指导: {}", if resolved_colorist.is_empty() || resolved_colorist == "-" { "(未指定)" } else { &resolved_colorist });
        eprintln!("│  团队   : {}", if resolved_team.is_empty() { "(未指定)" } else { &resolved_team });
        eprintln!("│  导演   : {}", if resolved_director.is_empty() || resolved_director == "-" { "(未指定)" } else { &resolved_director });
        eprintln!("└─────────────────────────────────────────┘");
        if resolved_title == stem_name {
            eprintln!("提示: 使用 --title 可设置自定义标题（如 \"逐玉 EP01\"）");
        }
        eprintln!();
    }

    // 5. Build fingerprint mode
    let fingerprint_mode = match mode.as_str() {
        "strip" => mengxi_core::movie_fingerprint::FingerprintMode::Strip,
        "cineiris" => mengxi_core::movie_fingerprint::FingerprintMode::CineIris { diameter },
        "both" => mengxi_core::movie_fingerprint::FingerprintMode::Both { diameter },
        "cineprint" => mengxi_core::movie_fingerprint::FingerprintMode::CinePrint { thumbnails: 12 },
        "poster" => mengxi_core::movie_fingerprint::FingerprintMode::Poster {
            title: resolved_title,
            project_type: resolved_project_type,
            colorist: resolved_colorist,
            team: resolved_team,
            director: resolved_director,
            year: resolved_year,
            font_path: font,
            watermark,
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
                if let Some(ref poster_path) = result.poster_path {
                    json_out["poster_path"] = serde_json::json!(poster_path.to_string_lossy());
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
                if let Some(ref poster_path) = result.poster_path {
                    eprintln!("Poster fingerprint: {}", poster_path.display());

                    // Enhance poster with fluorescent color spheres via Python
                    if let Some(ref fractions) = result.color_fractions {
                        const CANVAS_W: u32 = 1200;
                        const CANVAS_H: u32 = 1800;
                        let iris_diameter = ((CANVAS_W as f64 * 0.78).min((CANVAS_H as f64 - 435.0) * 0.88)) as i32 / 2 * 2;
                        let iris_r = iris_diameter / 2;
                        let iris_cx = (CANVAS_W / 2) as i32;
                        let iris_cy = (160 + (CANVAS_H as i32 - 160 - 275) / 2) as i32;

                        let mut bridge = PythonBridge::new(300, 30, String::new());
                        match bridge.enhance_poster(
                            poster_path.to_str().unwrap_or(""),
                            poster_path.to_str().unwrap_or(""),
                            fractions,
                            CANVAS_W, CANVAS_H,
                            iris_cx, iris_cy, iris_r,
                            35, 80,
                        ) {
                            Ok(_) => eprintln!("Poster enhanced with color spheres"),
                            Err(e) => eprintln!("Warning: poster enhancement skipped ({})", e),
                        }
                    }
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

/// Try to find a 4-digit year pattern in a string.
fn regex_find_year(s: &str) -> Option<String> {
    // Simple manual scan for 19xx or 20xx patterns (word-bounded-ish)
    let bytes = s.as_bytes();
    for i in 0..bytes.len().saturating_sub(3) {
        if (i == 0 || !bytes[i - 1].is_ascii_digit())
            && i + 3 < bytes.len()
            && (i + 4 >= bytes.len() || !bytes[i + 4].is_ascii_digit())
        {
            let c0 = bytes[i];
            let c1 = bytes[i + 1];
            let c2 = bytes[i + 2];
            let c3 = bytes[i + 3];
            if c0.is_ascii_digit() && c1.is_ascii_digit() && c2.is_ascii_digit() && c3.is_ascii_digit() {
                let y_str = unsafe { std::str::from_utf8_unchecked(&bytes[i..=i + 3]) };
                let y: i32 = y_str.parse().unwrap_or(0);
                if (1900..=2100).contains(&y) {
                    return Some(y_str.to_string());
                }
            }
        }
    }
    None
}
