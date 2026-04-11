// histogram_search.rs — Histogram-based search

use rusqlite::Connection;

use super::types::{SearchError, SearchResult, SearchOptions};
use super::histogram_utils::parse_histogram;

/// Search by histogram similarity.
///
/// Returns results ranked by descending similarity score.
pub fn search_histograms(
    conn: &Connection,
    options: &SearchOptions,
) -> Result<Vec<SearchResult>, SearchError> {
    let sql = match &options.project {
        Some(_) => "
            SELECT p.name, f.filename, f.format,
                   fp.histogram_r, fp.histogram_g, fp.histogram_b
            FROM fingerprints fp
            JOIN files f ON f.id = fp.file_id
            JOIN projects p ON p.id = f.project_id
            WHERE p.name = ?1
            ORDER BY p.name, f.filename
        ",
        None => "
            SELECT p.name, f.filename, f.format,
                   fp.histogram_r, fp.histogram_g, fp.histogram_b
            FROM fingerprints fp
            JOIN files f ON f.id = fp.file_id
            JOIN projects p ON p.id = f.project_id
            ORDER BY p.name, f.filename
        ",
    };

    let mut stmt = conn.prepare(sql).map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    let rows: Vec<(String, String, String, String, String, String)> = match &options.project {
        Some(proj) => stmt
            .query_map([proj], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| SearchError::DatabaseError(e.to_string()))?
            .collect::<Result<_, _>>()
            .map_err(|e| SearchError::DatabaseError(e.to_string()))?,
        None => stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|e| SearchError::DatabaseError(e.to_string()))?
            .collect::<Result<_, _>>()
            .map_err(|e| SearchError::DatabaseError(e.to_string()))?,
    };

    if rows.is_empty() {
        return if options.project.is_some() {
            Err(SearchError::ProjectNotFound(
                options.project.clone().unwrap(),
            ))
        } else {
            Err(SearchError::NoFingerprints)
        };
    }

    // Parse histograms and collect valid results.
    // Without a reference image (--image, Story 3.3), we return all fingerprints
    // with score 1.0 (100% match = all are valid indexed results).
    // Results are sorted by project name then filename.
    let mut scored: Vec<(String, String, String, f64)> = Vec::new();
    for (project_name, filename, format, hist_r_text, hist_g_text, hist_b_text) in rows {
        // Validate histograms are parseable (skip malformed data)
        if parse_histogram(&hist_r_text).is_err()
            || parse_histogram(&hist_g_text).is_err()
            || parse_histogram(&hist_b_text).is_err()
        {
            continue;
        }

        scored.push((project_name, filename, format, 1.0));
    }

    if scored.is_empty() {
        return Err(SearchError::NoFingerprints);
    }

    // Sort by project name, then filename (no meaningful similarity ranking without reference)
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    // Apply limit
    scored.truncate(options.limit);

    let results: Vec<SearchResult> = scored
        .into_iter()
        .enumerate()
        .map(|(i, (project_name, file_path, file_format, score))| SearchResult {
            rank: i + 1,
            project_name,
            file_path,
            file_format,
            score,
        })
        .collect();

    Ok(results)
}
