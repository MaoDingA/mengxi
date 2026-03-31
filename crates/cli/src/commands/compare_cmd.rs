use std::process;

use mengxi_core::db;

pub fn execute(id_a: i64, id_b: i64, format: String) {
    let is_json = format == "json";

    match db::open_db() {
        Ok(conn) => {
            match mengxi_core::comparison::compare_fingerprints(&conn, id_a, id_b) {
                Ok(result) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "ok",
                            "fingerprint_a": {
                                "id": result.id_a,
                                "project": result.project_a,
                                "file": result.file_a,
                                "color_space": result.color_space_a,
                            },
                            "fingerprint_b": {
                                "id": result.id_b,
                                "project": result.project_b,
                                "file": result.file_b,
                                "color_space": result.color_space_b,
                            },
                            "color_space_match": result.color_space_match,
                            "histogram_deltas": {
                                "L": {
                                    "mean_abs_diff": result.hist_l_delta.mean_abs_diff,
                                    "max_abs_diff": result.hist_l_delta.max_abs_diff,
                                    "max_diff_bin": result.hist_l_delta.max_diff_bin,
                                    "l1_norm": result.hist_l_delta.l1_norm,
                                },
                                "a": {
                                    "mean_abs_diff": result.hist_a_delta.mean_abs_diff,
                                    "max_abs_diff": result.hist_a_delta.max_abs_diff,
                                    "max_diff_bin": result.hist_a_delta.max_diff_bin,
                                    "l1_norm": result.hist_a_delta.l1_norm,
                                },
                                "b": {
                                    "mean_abs_diff": result.hist_b_delta.mean_abs_diff,
                                    "max_abs_diff": result.hist_b_delta.max_abs_diff,
                                    "max_diff_bin": result.hist_b_delta.max_diff_bin,
                                    "l1_norm": result.hist_b_delta.l1_norm,
                                },
                            },
                            "luminance_delta": {
                                "mean_delta": result.luminance_delta.mean_delta,
                                "stddev_delta": result.luminance_delta.stddev_delta,
                            },
                            "overall_distance": result.overall_distance,
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        if !result.color_space_match {
                            eprintln!("warning: color spaces differ ({} vs {})", result.color_space_a, result.color_space_b);
                        }
                        println!("Comparison: #{} vs #{}", result.id_a, result.id_b);
                        println!("  A: {} ({})", result.file_a, result.project_a);
                        println!("  B: {} ({})", result.file_b, result.project_b);
                        println!();
                        println!("Histogram Deltas:");
                        println!("  {:<4} {:>14} {:>14} {:>10} {:>10}", "Ch", "MeanAbsDiff", "MaxAbsDiff", "MaxBin", "L1 Norm");
                        println!("  {:<4} {:>14} {:>14} {:>10} {:>10}", "---", "----------", "----------", "------", "-------");
                        for (name, delta) in [("L", &result.hist_l_delta), ("a", &result.hist_a_delta), ("b", &result.hist_b_delta)] {
                            println!("  {:<4} {:>14.6} {:>14.6} {:>10} {:>10.4}",
                                name, delta.mean_abs_diff, delta.max_abs_diff, delta.max_diff_bin, delta.l1_norm);
                        }
                        println!();
                        println!("Luminance Delta:");
                        println!("  Mean:  {:+.6}", result.luminance_delta.mean_delta);
                        println!("  StdDev: {:+.6}", result.luminance_delta.stddev_delta);
                        println!();
                        println!("Overall Distance: {:.6}", result.overall_distance);
                    }
                }
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "COMPARE_ERROR", "message": e.to_string() }
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
                    "error": { "code": "COMPARE_DB_ERROR", "message": e.to_string() }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: COMPARE_DB_ERROR -- {}", e);
            }
            process::exit(1);
        }
    }
}
