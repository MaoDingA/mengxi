// color_transfer_cmd.rs — Generate color transfer LUT from two fingerprint strips
use std::process;

pub fn execute(
    source: Option<String>,
    target: Option<String>,
    grid_size: usize,
    output: Option<String>,
    format: String,
) {
    let is_json = format == "json";

    let src_path = match source {
        Some(ref p) => std::path::PathBuf::from(p),
        None => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "COLOR_TRANSFER_MISSING_SOURCE", "message": "source strip image path is required" }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: COLOR_TRANSFER_MISSING_SOURCE -- source strip image path is required");
            }
            process::exit(1);
        }
    };

    let tgt_path = match target {
        Some(ref p) => std::path::PathBuf::from(p),
        None => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "COLOR_TRANSFER_MISSING_TARGET", "message": "target strip image path is required" }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: COLOR_TRANSFER_MISSING_TARGET -- target strip image path is required");
            }
            process::exit(1);
        }
    };

    for (label, path) in [("source", &src_path), ("target", &tgt_path)] {
        if !path.exists() {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "COLOR_TRANSFER_FILE_NOT_FOUND", "message": format!("{} file not found: {}", label, path.display()) }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: COLOR_TRANSFER_FILE_NOT_FOUND -- {} file not found: {}", label, path.display());
            }
            process::exit(1);
        }
    }

    // Read images via mengxi-core helpers
    let (src_w, src_h, src_data) = match mengxi_core::movie_fingerprint::read_strip_png(&src_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error: failed to read source image: {}", e);
            process::exit(1);
        }
    };

    let (tgt_w, tgt_h, tgt_data) = match mengxi_core::movie_fingerprint::read_strip_png(&tgt_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error: failed to read target image: {}", e);
            process::exit(1);
        }
    };

    if !is_json {
        eprintln!("Generating color transfer LUT (grid: {})...", grid_size);
    }

    match mengxi_core::color_transfer::generate_color_transfer_lut(
        &src_data, src_w, src_h, &tgt_data, tgt_w, tgt_h, grid_size,
    ) {
        Ok(lut) => {
            let out_path = match &output {
                Some(p) => std::path::PathBuf::from(p),
                None => std::path::PathBuf::from("color_transfer.cube"),
            };

            if let Err(e) = lut.write_cube_file(&out_path) {
                if is_json {
                    let out = serde_json::json!({
                        "status": "error",
                        "error": { "code": "COLOR_TRANSFER_WRITE_ERROR", "message": format!("failed to write .cube file: {}", e) }
                    });
                    println!("{}", serde_json::to_string_pretty(&out).unwrap());
                } else {
                    eprintln!("Error: COLOR_TRANSFER_WRITE_ERROR -- failed to write .cube file: {}", e);
                }
                process::exit(1);
            }

            if is_json {
                let out = serde_json::json!({
                    "status": "ok",
                    "grid_size": lut.grid_size,
                    "output_path": out_path.to_string_lossy(),
                    "source_dimensions": { "width": src_w, "height": src_h },
                    "target_dimensions": { "width": tgt_w, "height": tgt_h }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Color transfer LUT written to {}", out_path.display());
                eprintln!("  Grid size: {} ({} entries)", lut.grid_size, lut.grid_size.pow(3));
                eprintln!("  Source: {}x{}, Target: {}x{}", src_w, src_h, tgt_w, tgt_h);
            }
        }
        Err(e) => {
            if is_json {
                let out = serde_json::json!({
                    "status": "error",
                    "error": { "code": "COLOR_TRANSFER_ERROR", "message": e.to_string() }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                eprintln!("Error: COLOR_TRANSFER_ERROR -- {}", e);
            }
            process::exit(1);
        }
    }
}
