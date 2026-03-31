use std::process;

use mengxi_core::lut_diff;

pub fn execute(lut_a: Option<String>, lut_b: Option<String>, format: Option<String>) {
    let is_json = format.as_deref() == Some("json");

    // Validate required args
    let path_a = match &lut_a {
        Some(p) => {
            if p == "~" {
                match dirs::home_dir() {
                    Some(home) => home,
                    None => {
                        eprintln!("Error: LUTDIFF_MISSING_ARG -- cannot resolve home directory for '~'");
                        process::exit(1);
                    }
                }
            } else if let Some(stripped) = p.strip_prefix("~/") {
                match dirs::home_dir() {
                    Some(home) => home.join(stripped),
                    None => {
                        eprintln!("Error: LUTDIFF_MISSING_ARG -- cannot resolve home directory for '~/...'");
                        process::exit(1);
                    }
                }
            } else {
                std::path::PathBuf::from(p)
            }
        }
        None => {
            if is_json {
                let output = serde_json::json!({
                    "status": "error",
                    "error": { "code": "LUTDIFF_MISSING_ARG", "message": "<lut_a> is required" }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: LUTDIFF_MISSING_ARG -- <lut_a> is required");
            }
            process::exit(1);
        }
    };
    let path_b = match &lut_b {
        Some(p) => {
            if p == "~" {
                match dirs::home_dir() {
                    Some(home) => home,
                    None => {
                        eprintln!("Error: LUTDIFF_MISSING_ARG -- cannot resolve home directory for '~'");
                        process::exit(1);
                    }
                }
            } else if let Some(stripped) = p.strip_prefix("~/") {
                match dirs::home_dir() {
                    Some(home) => home.join(stripped),
                    None => {
                        eprintln!("Error: LUTDIFF_MISSING_ARG -- cannot resolve home directory for '~/...'");
                        process::exit(1);
                    }
                }
            } else {
                std::path::PathBuf::from(p)
            }
        }
        None => {
            if is_json {
                let output = serde_json::json!({
                    "status": "error",
                    "error": { "code": "LUTDIFF_MISSING_ARG", "message": "<lut_b> is required" }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: LUTDIFF_MISSING_ARG -- <lut_b> is required");
            }
            process::exit(1);
        }
    };

    match lut_diff::compare_luts(&path_a, &path_b) {
        Ok(result) => {
            if is_json {
                let channels = serde_json::json!([
                    { "channel": "R", "mean_delta": result.channels[0].mean_delta, "max_delta": result.channels[0].max_delta, "changed_values": result.channels[0].changed_count },
                    { "channel": "G", "mean_delta": result.channels[1].mean_delta, "max_delta": result.channels[1].max_delta, "changed_values": result.channels[1].changed_count },
                    { "channel": "B", "mean_delta": result.channels[2].mean_delta, "max_delta": result.channels[2].max_delta, "changed_values": result.channels[2].changed_count },
                ]);
                let output = serde_json::json!({
                    "status": "ok",
                    "lut_a": path_a.to_string_lossy(),
                    "lut_b": path_b.to_string_lossy(),
                    "total_points": result.total_points,
                    "channels": channels
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                println!(
                    "LUT Diff: {} vs {}\n",
                    path_a.display(),
                    path_b.display()
                );
                println!("{:<12} {:>12} {:>12} {:>14}",
                    "Channel", "Mean Delta", "Max Delta", "Changed");
                println!("{:<12} {:<12} {:<12} {:<14}", "----------", "----------", "----------", "----------");
                for (name, ch) in [("R", &result.channels[0]), ("G", &result.channels[1]), ("B", &result.channels[2])] {
                    println!("{:<12} {:>12.6} {:>12.6} {:>14}",
                        name,
                        ch.mean_delta,
                        ch.max_delta,
                        ch.changed_count,
                    );
                }
                println!("\nTotal points compared: {}", result.total_points);
            }
        }
        Err(e) => {
            if is_json {
                let output = serde_json::json!({
                    "status": "error",
                    "error": { "code": format!("{}", e).split(" -- ").next().unwrap_or("LUTDIFF_ERROR"), "message": format!("{}", e) }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: {}", e);
            }
            process::exit(1);
        }
    }
}
