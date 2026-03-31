use std::process;

use mengxi_core::db;
use unicode_width::UnicodeWidthStr;

fn truncate_str(s: &str, max_len: usize) -> String {
    let width = UnicodeWidthStr::width(s);
    if width <= max_len {
        s.to_string()
    } else {
        let ellipsis_width = UnicodeWidthStr::width("…");
        let target = max_len.saturating_sub(ellipsis_width);
        let mut result = String::new();
        let mut current_width = 0usize;
        for ch in s.chars() {
            let ch_width = UnicodeWidthStr::width(ch.to_string().as_str());
            if current_width + ch_width > target {
                break;
            }
            result.push(ch);
            current_width += ch_width;
        }
        result.push('…');
        result
    }
}

pub fn execute(projects: Vec<String>, format: String) {
    let is_json = format == "json";

    if projects.len() < 2 {
        if is_json {
            let output = serde_json::json!({
                "status": "error",
                "error": { "code": "CONSISTENCY_MIN_PROJECTS", "message": "at least 2 projects required for consistency check" }
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        } else {
            eprintln!("Error: CONSISTENCY_MIN_PROJECTS -- at least 2 projects required");
        }
        process::exit(1);
    }

    match db::open_db() {
        Ok(conn) => {
            match mengxi_core::consistency::generate_consistency_report(&conn, &projects) {
                Ok(report) => {
                    if is_json {
                        let mut output = serde_json::Map::new();
                        output.insert("status".to_string(), serde_json::json!("ok"));

                        let summaries: Vec<serde_json::Value> = report.project_summaries.iter().map(|s| {
                            serde_json::json!({
                                "name": s.name,
                                "fingerprint_count": s.fingerprint_count,
                                "l_centroid": s.l_centroid,
                                "a_centroid": s.a_centroid,
                                "b_centroid": s.b_centroid,
                            })
                        }).collect();
                        output.insert("project_summaries".to_string(), serde_json::json!(summaries));

                        let pairs: Vec<serde_json::Value> = report.pair_distances.iter().map(|d| {
                            serde_json::json!({
                                "project_a": d.project_a,
                                "project_b": d.project_b,
                                "histogram_distance": d.histogram_distance,
                                "luminance_diff": d.luminance_diff,
                            })
                        }).collect();
                        output.insert("pair_distances".to_string(), serde_json::json!(pairs));

                        if !report.outliers.is_empty() {
                            let outlier_json: Vec<serde_json::Value> = report.outliers.iter().map(|o| {
                                serde_json::json!({
                                    "id": o.id,
                                    "project": o.project,
                                    "file": o.file,
                                    "distance_from_mean": o.distance_from_mean,
                                })
                            }).collect();
                            output.insert("outliers".to_string(), serde_json::json!(outlier_json));
                        }

                        output.insert("overall_consistency".to_string(), serde_json::json!(report.overall_consistency));
                        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Object(output)).unwrap());
                    } else {
                        println!("Cross-Project Consistency Report");
                        println!("{}", "=".repeat(40));
                        println!();

                        println!("Project Summaries:");
                        println!("  {:<20} {:>8} {:>12} {:>12} {:>12}", "Name", "FPs", "L Centroid", "a Centroid", "b Centroid");
                        println!("  {}", "-".repeat(66));
                        for s in &report.project_summaries {
                            println!("  {:<20} {:>8} {:>12.4} {:>12.4} {:>12.4}",
                                truncate_str(&s.name, 20), s.fingerprint_count,
                                s.l_centroid, s.a_centroid, s.b_centroid);
                        }
                        println!();

                        if !report.pair_distances.is_empty() {
                            println!("Pairwise Distances:");
                            println!("  {:<20} {:<20} {:>14} {:>14}", "Project A", "Project B", "Hist Dist", "Lum Diff");
                            println!("  {}", "-".repeat(70));
                            for d in &report.pair_distances {
                                println!("  {:<20} {:<20} {:>14.6} {:>14.6}",
                                    truncate_str(&d.project_a, 20),
                                    truncate_str(&d.project_b, 20),
                                    d.histogram_distance,
                                    d.luminance_diff);
                            }
                            println!();
                        }

                        if !report.outliers.is_empty() {
                            println!("Outliers (distance > 2x project mean):");
                            println!("  {:<6} {:<20} {:<30} {:>14}", "ID", "Project", "File", "Distance");
                            println!("  {}", "-".repeat(72));
                            for o in &report.outliers {
                                println!("  {:<6} {:<20} {:<30} {:>14.6}",
                                    o.id,
                                    truncate_str(&o.project, 20),
                                    truncate_str(&o.file, 30),
                                    o.distance_from_mean);
                            }
                            println!();
                        }

                        println!("Overall Consistency: {:.6}", report.overall_consistency);
                    }
                }
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "CONSISTENCY_ERROR", "message": e.to_string() }
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
                    "error": { "code": "CONSISTENCY_DB_ERROR", "message": e.to_string() }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: CONSISTENCY_DB_ERROR -- {}", e);
            }
            process::exit(1);
        }
    }
}
