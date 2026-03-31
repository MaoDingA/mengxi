use std::process;

use mengxi_core::db;
use mengxi_core::lut_diff;

use super::helpers::seconds_to_datetime;

pub fn execute(lut: Option<String>, format: Option<String>) {
    let is_json = format.as_deref() == Some("json");

    let lut_path = match &lut {
        Some(p) => {
            if p == "~" {
                match dirs::home_dir() {
                    Some(home) => home,
                    None => {
                        eprintln!("Error: LUTDEP_MISSING_ARG -- cannot resolve home directory for '~'");
                        process::exit(1);
                    }
                }
            } else if let Some(stripped) = p.strip_prefix("~/") {
                match dirs::home_dir() {
                    Some(home) => home.join(stripped),
                    None => {
                        eprintln!("Error: LUTDEP_MISSING_ARG -- cannot resolve home directory for '~/...'");
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
                    "error": { "code": "LUTDEP_MISSING_ARG", "message": "--lut <path> is required" }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: LUTDEP_MISSING_ARG -- --lut <path> is required");
            }
            process::exit(1);
        }
    };

    match db::open_db() {
        Ok(conn) => {
            match lut_diff::query_lut_dependency(&conn, &lut_path.to_string_lossy()) {
                Ok(Some(dep)) => {
                    let timestamp = if dep.exported_at > 0 {
                        let secs = dep.exported_at as u64;
                        let (year, month, day, hour, min, sec) = seconds_to_datetime(secs);
                        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, min, sec)
                    } else {
                        "unknown".to_string()
                    };
                    if is_json {
                        let output = serde_json::json!({
                            "status": "ok",
                            "dependency": {
                                "project": dep.project_name,
                                "file": dep.file_path,
                                "format": dep.format,
                                "grid_size": dep.grid_size,
                                "exported_at": timestamp,
                                "lut_path": lut_path.to_string_lossy(),
                            }
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        println!(
                            "+----------+------------------------------+\n\
                             | Field    | Value                        |\n\
                             +----------+------------------------------+\n\
                             | Project  | {:<28} |\n\
                             | Scene    | {:<28} |\n\
                             | Format   | {:<28} |\n\
                             | Grid     | {}x{}x{:<23} |\n\
                             | Exported | {:<28} |\n\
                             | LUT Path | {:<28} |\n\
                             +----------+------------------------------+",
                            dep.project_name,
                            dep.file_path,
                            dep.format,
                            dep.grid_size,
                            dep.grid_size,
                            dep.grid_size,
                            timestamp,
                            lut_path.display(),
                        );
                    }
                }
                Ok(None) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "ok",
                            "dependency": null,
                            "message": "No dependency records found for this LUT"
                        });
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        println!("No dependency records found for this LUT");
                    }
                }
                Err(e) => {
                    if is_json {
                        let output = serde_json::json!({
                            "status": "error",
                            "error": { "code": "LUTDEP_DB_ERROR", "message": e.to_string() }
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
                    "error": { "code": "LUTDEP_DB_ERROR", "message": "Failed to initialize database" }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("Error: LUTDEP_DB_ERROR -- {e}");
            }
            process::exit(1);
        }
    }
}
