// tag_search.rs — Tag-based search

use rusqlite::Connection;

use super::types::{SearchError, SearchResult, SearchOptions};

/// Search by tag, returning results ranked by tag match count.
///
/// The `tag` parameter can contain multiple space-separated tags (e.g. "industrial warm").
/// Results are ranked by the number of matching tags (normalized to [0, 1]).
/// When `project` is set in options, results are scoped to that project.
pub fn search_by_tag(
    conn: &Connection,
    tag: &str,
    options: &SearchOptions,
) -> Result<Vec<SearchResult>, SearchError> {
    // Split tag query into individual tags
    let query_tags: Vec<&str> = tag.split_whitespace().collect();
    if query_tags.is_empty() {
        return Err(SearchError::NoFingerprints);
    }

    // Build dynamic SQL with one parameter per tag
    let placeholders: Vec<String> = (1..=query_tags.len())
        .map(|i| format!("?{}", i))
        .collect();
    let where_tags = placeholders.join(", ");

    let mut sql = format!(
        "SELECT p.name, f.filename, f.format, COUNT(DISTINCT t.tag) as match_count
         FROM fingerprints fp
         JOIN files f ON f.id = fp.file_id
         JOIN projects p ON p.id = f.project_id
         JOIN tags t ON t.fingerprint_id = fp.id
         WHERE t.tag IN ({})",
        where_tags
    );

    // Determine parameter index for project filter
    if options.project.is_some() {
        let proj_idx = query_tags.len() + 1;
        sql.push_str(&format!(" AND p.name = ?{}", proj_idx));
    }

    sql.push_str(" GROUP BY fp.id ORDER BY match_count DESC");

    let mut stmt = conn.prepare(&sql).map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = query_tags
        .iter()
        .map(|t| Box::new(t.to_string()) as Box<dyn rusqlite::types::ToSql>)
        .collect();

    if let Some(ref proj) = options.project {
        params.push(Box::new(proj.clone()));
    }

    let rows: Vec<(String, String, String, i64)> = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    if rows.is_empty() {
        return Err(SearchError::NoFingerprints);
    }

    // Normalize scores: divide by max match count
    let max_count = rows.iter().map(|(_, _, _, c)| *c).max().unwrap_or(1) as f64;

    let results: Vec<SearchResult> = rows
        .into_iter()
        .take(options.limit)
        .enumerate()
        .map(|(i, (project_name, file_path, file_format, count))| SearchResult {
            rank: i + 1,
            project_name,
            file_path,
            file_format,
            score: count as f64 / max_count,
        })
        .collect();

    Ok(results)
}
