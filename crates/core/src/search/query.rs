// query.rs — Single fingerprint query functions

use rusqlite::Connection;

use super::types::{FingerprintInfo, SearchError};
use super::histogram_utils::{parse_histogram};
use super::types::summarize_histogram;

/// Retrieve detailed fingerprint info by project name and file path.
pub fn fingerprint_info(
    conn: &Connection,
    project_name: &str,
    file_path: &str,
) -> Result<FingerprintInfo, SearchError> {
    let sql = "SELECT p.name, f.filename, f.format,
                      fp.luminance_mean, fp.luminance_stddev, fp.color_space_tag,
                      fp.histogram_r, fp.histogram_g, fp.histogram_b
               FROM fingerprints fp
               JOIN files f ON f.id = fp.file_id
               JOIN projects p ON p.id = f.project_id
               WHERE p.name = ?1 AND f.filename = ?2
               LIMIT 1";

    let result = conn
        .query_row(sql, rusqlite::params![project_name, file_path], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(|e| {
            if e.to_string().contains("Query returned no rows") {
                SearchError::ProjectNotFound(format!(
                    "No fingerprint found for {} / {}",
                    project_name, file_path
                ))
            } else {
                SearchError::DatabaseError(e.to_string())
            }
        })?;

    let hist_r = parse_histogram(&result.6).unwrap_or_default();
    let hist_g = parse_histogram(&result.7).unwrap_or_default();
    let hist_b = parse_histogram(&result.8).unwrap_or_default();

    Ok(FingerprintInfo {
        project_name: result.0,
        file_path: result.1,
        file_format: result.2,
        luminance_mean: result.3,
        luminance_stddev: result.4,
        color_space_tag: result.5,
        histogram_r_summary: summarize_histogram(&hist_r),
        histogram_g_summary: summarize_histogram(&hist_g),
        histogram_b_summary: summarize_histogram(&hist_b),
        tags: vec![],
    })
}

/// Retrieve detailed fingerprint info with tags.
pub fn fingerprint_info_with_tags(
    conn: &Connection,
    project_name: &str,
    file_path: &str,
) -> Result<FingerprintInfo, SearchError> {
    let mut info = fingerprint_info(conn, project_name, file_path)?;

    // Query tags for this fingerprint
    let sql = "SELECT t.tag FROM tags t
               JOIN fingerprints fp ON fp.id = t.fingerprint_id
               JOIN files f ON f.id = fp.file_id
               JOIN projects p ON p.id = f.project_id
               WHERE p.name = ?1 AND f.filename = ?2
               ORDER BY t.tag";

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    let tags: Vec<String> = stmt
        .query_map(rusqlite::params![project_name, file_path], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    info.tags = tags;
    Ok(info)
}
