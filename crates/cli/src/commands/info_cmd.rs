use std::process;

use mengxi_core::db;

use super::helpers::truncate_str;

pub fn execute(project: Option<String>, file: Option<String>, format: String) {
    let is_json = format == "json";

    match (project.as_deref(), file.as_deref()) {
        (Some(proj), Some(fp)) => {
            match db::open_db() {
                Ok(conn) => {
                    match mengxi_core::search::fingerprint_info_with_tags(&conn, proj, fp) {
                        Ok(info) => {
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "ok",
                                    "fingerprint": {
                                        "project": info.project_name,
                                        "file": info.file_path,
                                        "format": info.file_format,
                                        "color_space": info.color_space_tag,
                                        "luminance": {
                                            "mean": info.luminance_mean,
                                            "stddev": info.luminance_stddev,
                                        },
                                        "histogram": {
                                            "r": {
                                                "mean": info.histogram_r_summary.mean_value,
                                                "dominant_bin": info.histogram_r_summary.dominant_bin_min,
                                            },
                                            "g": {
                                                "mean": info.histogram_g_summary.mean_value,
                                                "dominant_bin": info.histogram_g_summary.dominant_bin_min,
                                            },
                                            "b": {
                                                "mean": info.histogram_b_summary.mean_value,
                                                "dominant_bin": info.histogram_b_summary.dominant_bin_min,
                                            },
                                        },
                                        "tags": info.tags,
                                    }
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                let tags_str = if info.tags.is_empty() {
                                    "(none)".to_string()
                                } else {
                                    info.tags.join(", ")
                                };
                                println!(
                                    "+---------------+------------------------------+\n\
                                     | Field         | Value                        |\n\
                                     +---------------+------------------------------+\n\
                                     | Project       | {:<28} |\n\
                                     | File          | {:<28} |\n\
                                     | Format        | {:<28} |\n\
                                     | Color Space   | {:<28} |\n\
                                     | Luminance     | {:.4} +/- {:.4}                |\n\
                                     | Hist R (mean) | {:.6}                     |\n\
                                     | Hist G (mean) | {:.6}                     |\n\
                                     | Hist B (mean) | {:.6}                     |\n\
                                     | Dominant R    | bin {}                      |\n\
                                     | Dominant G    | bin {}                      |\n\
                                     | Dominant B    | bin {}                      |\n\
                                     | Tags          | {:<28} |\n\
                                     +---------------+------------------------------+",
                                    truncate_str(&info.project_name, 28),
                                    truncate_str(&info.file_path, 28),
                                    truncate_str(&info.file_format, 28),
                                    truncate_str(&info.color_space_tag, 28),
                                    info.luminance_mean,
                                    info.luminance_stddev,
                                    info.histogram_r_summary.mean_value,
                                    info.histogram_g_summary.mean_value,
                                    info.histogram_b_summary.mean_value,
                                    info.histogram_r_summary.dominant_bin_min,
                                    info.histogram_g_summary.dominant_bin_min,
                                    info.histogram_b_summary.dominant_bin_min,
                                    truncate_str(&tags_str, 28),
                                );
                            }
                        }
                        Err(e) => {
                            if is_json {
                                let output = serde_json::json!({
                                    "status": "error",
                                    "error": { "code": "INFO_NOT_FOUND", "message": e.to_string() }
                                });
                                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                            } else {
                                eprintln!("Error: {}", e);
                            }
                            process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "INFO_DB_ERROR", "message": e.to_string() }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        eprintln!("Error: INFO_DB_ERROR -- {}", e);
                    }
                    process::exit(1);
                }
            }
        }
        _ => {
            if is_json {
                let output = serde_json::json!({
                    "status": "error",
                    "error": { "code": "INFO_MISSING_ARG", "message": "--project and --file are required" }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: INFO_MISSING_ARG -- --project and --file are required");
            }
            process::exit(1);
        }
    }
}
