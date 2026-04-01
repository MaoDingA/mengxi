// movie_compare_cmd.rs — Compare two fingerprint strips by color DNA
use std::process;

pub fn execute(
    strip_a: Option<String>,
    strip_b: Option<String>,
    format: String,
) {
    let is_json = format == "json";

    let path_a = match strip_a {
        Some(ref p) => std::path::PathBuf::from(p),
        None => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "MOVIE_COMPARE_MISSING_A", "message": "first strip image path is required" }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: MOVIE_COMPARE_MISSING_A -- first strip image path is required");
            }
            process::exit(1);
        }
    };

    let path_b = match strip_b {
        Some(ref p) => std::path::PathBuf::from(p),
        None => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "MOVIE_COMPARE_MISSING_B", "message": "second strip image path is required" }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: MOVIE_COMPARE_MISSING_B -- second strip image path is required");
            }
            process::exit(1);
        }
    };

    for (label, path) in [("first", &path_a), ("second", &path_b)] {
        if !path.exists() {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "MOVIE_COMPARE_FILE_NOT_FOUND", "message": format!("{} strip not found: {}", label, path.display()) }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: MOVIE_COMPARE_FILE_NOT_FOUND -- {} strip not found: {}", label, path.display());
            }
            process::exit(1);
        }
    }

    // Read images via mengxi-core helpers
    let (wa, ha, data_a) = match mengxi_core::movie_fingerprint::read_strip_png(&path_a) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error: failed to read first image: {}", e);
            process::exit(1);
        }
    };

    let (wb, hb, data_b) = match mengxi_core::movie_fingerprint::read_strip_png(&path_b) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error: failed to read second image: {}", e);
            process::exit(1);
        }
    };

    // Extract color DNA
    let dna_a = match mengxi_core::color_dna::extract_color_dna(&data_a, wa, ha) {
        Ok(dna) => dna,
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "MOVIE_COMPARE_EXTRACT_A_ERROR", "message": format!("failed to extract DNA from first strip: {}", e) }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: MOVIE_COMPARE_EXTRACT_A_ERROR -- {}", e);
            }
            process::exit(1);
        }
    };

    let dna_b = match mengxi_core::color_dna::extract_color_dna(&data_b, wb, hb) {
        Ok(dna) => dna,
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "MOVIE_COMPARE_EXTRACT_B_ERROR", "message": format!("failed to extract DNA from second strip: {}", e) }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: MOVIE_COMPARE_EXTRACT_B_ERROR -- {}", e);
            }
            process::exit(1);
        }
    };

    // Compare
    match mengxi_core::color_dna::compare_color_dna(&dna_a, &dna_b) {
        Ok(comp) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "ok",
                    "strip_a": { "width": wa, "height": ha },
                    "strip_b": { "width": wb, "height": hb },
                    "comparison": {
                        "overall_similarity": format!("{:.4}", comp.overall_similarity),
                        "hue_similarity": format!("{:.4}", comp.hue_similarity),
                        "contrast_diff": format!("{:.4}", comp.contrast_diff),
                        "warmth_diff": format!("{:.4}", comp.warmth_diff),
                    }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Color DNA Comparison:");
                eprintln!("  Overall similarity: {:.4}", comp.overall_similarity);
                eprintln!("  Hue similarity:    {:.4}", comp.hue_similarity);
                eprintln!("  Contrast diff:    {:.4}", comp.contrast_diff);
                eprintln!("  Warmth diff:       {:.4}", comp.warmth_diff);
                eprintln!();
                eprintln!("  Strip A ({}x{}): L={:.3}, sat={:.3}", wa, ha, dna_a.avg_l, dna_a.saturation);
                eprintln!("  Strip B ({}x{}): L={:.3}, sat={:.3}", wb, hb, dna_b.avg_l, dna_b.saturation);
            }
        }
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "MOVIE_COMPARE_ERROR", "message": e.to_string() }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: MOVIE_COMPARE_ERROR -- {}", e);
            }
            process::exit(1);
        }
    }
}
