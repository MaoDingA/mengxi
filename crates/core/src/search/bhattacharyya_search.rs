// bhattacharyya_search.rs — Bhattacharyya distance search using grading features

use rusqlite::Connection;

use crate::color_science::{bhattacharyya_distance, GradingFeatures};

use super::types::{FeatureSearchRow, SearchError, SearchResult, SearchOptions};

/// Load grading features for a file from DB, returning GradingFeatures.
pub(crate) fn load_grading_features(
    conn: &Connection,
    file_id: i64,
) -> Result<GradingFeatures, SearchError> {
    let sql = "SELECT oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, COALESCE(hist_bins, 64)
               FROM fingerprints
               WHERE file_id = ?1 AND oklab_hist_l IS NOT NULL
               LIMIT 1";

    let result = conn.query_row(sql, rusqlite::params![file_id], |row| {
        Ok((
            row.get::<_, Option<Vec<u8>>>(0)?,
            row.get::<_, Option<Vec<u8>>>(1)?,
            row.get::<_, Option<Vec<u8>>>(2)?,
            row.get::<_, Option<Vec<u8>>>(3)?,
            row.get::<_, i32>(4)?,
        ))
    });

    match result {
        Ok((Some(hist_l), Some(hist_a), Some(hist_b), Some(moments), hist_bins_i32)) => {
            let hist_bins = hist_bins_i32 as usize;
            GradingFeatures::from_separate_blobs(&hist_l, &hist_a, &hist_b, &moments, hist_bins)
                .map_err(|e| SearchError::DatabaseError(format!("grading feature decode: {}", e)))
        }
        Ok(_) => Err(SearchError::NoFingerprints),
        Err(e) => Err(SearchError::DatabaseError(format!(
            "query grading features for file_id {}: {}",
            file_id, e
        ))),
    }
}

/// Search by grading feature similarity using Bhattacharyya distance.
///
/// Loads query grading features from DB, then computes Bhattacharyya similarity
/// against all candidates with grading features. Results are sorted by
/// similarity descending (1.0 = identical).
pub fn bhattacharyya_search(
    conn: &Connection,
    query_file_id: i64,
    options: &SearchOptions,
) -> Result<Vec<SearchResult>, SearchError> {
    // Load query grading features
    let query_features = load_grading_features(conn, query_file_id)?;

    // Load all candidates with grading features
    let mut sql = String::from(
        "SELECT fp.file_id, p.name, f.filename, f.format,
                fp.oklab_hist_l, fp.oklab_hist_a, fp.oklab_hist_b, fp.color_moments,
                COALESCE(fp.hist_bins, 64)
         FROM fingerprints fp
         JOIN files f ON f.id = fp.file_id
         JOIN projects p ON p.id = f.project_id
         WHERE fp.oklab_hist_l IS NOT NULL AND fp.oklab_hist_a IS NOT NULL
               AND fp.oklab_hist_b IS NOT NULL AND fp.color_moments IS NOT NULL
               AND fp.file_id != ?1"
    );

    if options.project.is_some() {
        sql.push_str(" AND p.name = ?2");
    }

    let mut stmt = conn.prepare(&sql).map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(query_file_id)];
    if let Some(ref proj) = options.project {
        params.push(Box::new(proj.clone()));
    }

    let rows: Vec<FeatureSearchRow> = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, Vec<u8>>(6)?,
                row.get::<_, Vec<u8>>(7)?,
                row.get::<_, i32>(8)?,
            ))
        })
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    if rows.is_empty() {
        if options.project.is_some() {
            return Err(SearchError::ProjectNotFound(options.project.clone().unwrap()));
        }
        return Err(SearchError::NoFingerprints);
    }

    // Score each candidate
    let mut scored: Vec<(f64, String, String, String)> = Vec::new();

    for (_file_id, project_name, filename, format, hist_l, hist_a, hist_b, moments, hist_bins_i32) in rows {
        let hist_bins = hist_bins_i32 as usize;
        let candidate = match GradingFeatures::from_separate_blobs(&hist_l, &hist_a, &hist_b, &moments, hist_bins) {
            Ok(gf) => gf,
            Err(e) => {
                eprintln!("warning: skipping candidate {} ({}): grading feature decode failed: {}", project_name, filename, e);
                continue;
            }
        };

        match bhattacharyya_distance(&query_features, &candidate) {
            Ok(score) => scored.push((score, project_name, filename, format)),
            Err(e) => {
                eprintln!("warning: skipping candidate {} ({}): bhattacharyya_distance failed: {}", project_name, filename, e);
                continue;
            }
        }
    }

    if scored.is_empty() {
        return Err(SearchError::NoFingerprints);
    }

    // Sort by score descending
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(options.limit);

    let results: Vec<SearchResult> = scored
        .into_iter()
        .enumerate()
        .map(|(i, (score, project_name, file_path, file_format))| SearchResult {
            rank: i + 1,
            project_name,
            file_path,
            file_format,
            score,
        })
        .collect();

    Ok(results)
}
