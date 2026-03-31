use std::process;

use mengxi_core::db;

use crate::DbSubcommand;
use super::helpers::truncate_str;

pub fn execute(command: DbSubcommand) {
    let conn = match db::open_db() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: DB_OPEN_FAILED — {e}");
            process::exit(1);
        }
    };
    match command {
        DbSubcommand::Projects { format } => {
            let is_json = format == "json";
            let projects = db::db_list_projects(&conn).unwrap_or_default();
            if is_json {
                let arr: Vec<serde_json::Value> = projects.iter().map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "name": p.name,
                        "path": p.path,
                        "dpx_count": p.dpx_count,
                        "exr_count": p.exr_count,
                        "mov_count": p.mov_count,
                        "file_count": p.file_count,
                        "fingerprint_count": p.fingerprint_count,
                        "created_at": p.created_at,
                    })
                }).collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": "ok", "projects": arr })).unwrap());
            } else if projects.is_empty() {
                println!("No projects found.");
            } else {
                println!("{:<4} {:<20} {:<30} {:>6} {:>6} {:>6} {:>6} {:>6}",
                    "ID", "Name", "Path", "DPX", "EXR", "MOV", "Files", "FPs");
                for p in &projects {
                    println!("{:<4} {:<20} {:<30} {:>6} {:>6} {:>6} {:>6} {:>6}",
                        p.id, truncate_str(&p.name, 20), truncate_str(&p.path, 30),
                        p.dpx_count, p.exr_count, p.mov_count, p.file_count, p.fingerprint_count);
                }
            }
        }
        DbSubcommand::Files { project, format } => {
            let is_json = format == "json";
            let files = db::db_list_files(&conn, &project).unwrap_or_default();
            if is_json {
                let arr: Vec<serde_json::Value> = files.iter().map(|f| {
                    serde_json::json!({
                        "id": f.id,
                        "filename": f.filename,
                        "format": f.format,
                        "fingerprint_count": f.fingerprint_count,
                        "created_at": f.created_at,
                    })
                }).collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": "ok", "files": arr })).unwrap());
            } else if files.is_empty() {
                println!("No files found in project '{}'.", project);
            } else {
                println!("{:<4} {:<30} {:<8} {:>6}", "ID", "Filename", "Format", "FPs");
                for f in &files {
                    println!("{:<4} {:<30} {:<8} {:>6}",
                        f.id, truncate_str(&f.filename, 30), f.format, f.fingerprint_count);
                }
            }
        }
        DbSubcommand::Tags { project, format } => {
            let is_json = format == "json";
            let tags = db::db_list_tags(&conn, project.as_deref()).unwrap_or_default();
            if is_json {
                let arr: Vec<serde_json::Value> = tags.iter().map(|t| {
                    serde_json::json!({
                        "id": t.id,
                        "tag": t.tag,
                        "source": t.source,
                        "project": t.project_name,
                        "filename": t.filename,
                        "created_at": t.created_at,
                    })
                }).collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": "ok", "tags": arr })).unwrap());
            } else if tags.is_empty() {
                println!("No tags found.");
            } else {
                println!("{:<4} {:<20} {:<8} {:<16} {:<24}", "ID", "Tag", "Source", "Project", "File");
                for t in &tags {
                    println!("{:<4} {:<20} {:<8} {:<16} {:<24}",
                        t.id, truncate_str(&t.tag, 20), t.source,
                        truncate_str(&t.project_name, 16), truncate_str(&t.filename, 24));
                }
            }
        }
        DbSubcommand::Luts { format } => {
            let is_json = format == "json";
            let luts = db::db_list_luts(&conn).unwrap_or_default();
            if is_json {
                let arr: Vec<serde_json::Value> = luts.iter().map(|l| {
                    serde_json::json!({
                        "id": l.id,
                        "title": l.title,
                        "format": l.format,
                        "grid_size": l.grid_size,
                        "output_path": l.output_path,
                        "project": l.project_name,
                        "created_at": l.created_at,
                    })
                }).collect();
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "status": "ok", "luts": arr })).unwrap());
            } else if luts.is_empty() {
                println!("No LUTs found.");
            } else {
                println!("{:<4} {:<20} {:<8} {:>6} {:<30} {:<16}", "ID", "Title", "Format", "Grid", "Output", "Project");
                for l in &luts {
                    println!("{:<4} {:<20} {:<8} {:>6} {:<30} {:<16}",
                        l.id,
                        truncate_str(l.title.as_deref().unwrap_or("-"), 20),
                        l.format, l.grid_size,
                        truncate_str(&l.output_path, 30),
                        truncate_str(&l.project_name, 16));
                }
            }
        }
        DbSubcommand::Sql { query } => {
            match db::db_run_query(&conn, &query) {
                Ok((cols, rows)) => {
                    if cols.is_empty() {
                        println!("Query returned no columns.");
                    } else {
                        // Compute column widths
                        let mut widths: Vec<usize> = cols.iter().map(|c| c.len()).collect();
                        for row in &rows {
                            for (i, val) in row.iter().enumerate() {
                                if i < widths.len() {
                                    widths[i] = widths[i].max(val.len());
                                }
                            }
                        }
                        // Print header
                        let header: String = cols.iter().zip(widths.iter())
                            .map(|(c, w)| format!(" {:w$} ", truncate_str(c, *w), w = w))
                            .collect::<Vec<_>>()
                            .join("|");
                        let separator: String = widths.iter()
                            .map(|w| format!("{}-{}", "-", "-".repeat(*w)))
                            .collect::<Vec<_>>()
                            .join("+");
                        println!("+{}+", separator);
                        println!("|{}|", header);
                        println!("+{}+", separator);
                        for row in &rows {
                            let line: String = row.iter().zip(widths.iter())
                                .map(|(v, w)| format!(" {:w$} ", truncate_str(v, *w), w = w))
                                .collect::<Vec<_>>()
                                .join("|");
                            println!("|{}|", line);
                        }
                        println!("+{}+", separator);
                        println!("{} row(s)", rows.len());
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            }
        }
    }
}

