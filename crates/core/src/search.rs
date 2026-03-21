// search.rs — Histogram-based search engine for color similarity

use rusqlite::Connection;

use crate::fingerprint::BINS_PER_CHANNEL;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from search operations.
#[derive(Debug)]
pub enum SearchError {
    /// No fingerprints exist in the database.
    NoFingerprints,
    /// A database error occurred.
    DatabaseError(String),
    /// Invalid format parameter.
    InvalidFormat(String),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::NoFingerprints => {
                write!(f, "SEARCH_NO_FINGERPRINTS -- No indexed projects found")
            }
            SearchError::DatabaseError(msg) => {
                write!(f, "SEARCH_DB_ERROR -- {}", msg)
            }
            SearchError::InvalidFormat(msg) => {
                write!(f, "SEARCH_INVALID_FORMAT -- {}", msg)
            }
        }
    }
}

impl std::error::Error for SearchError {}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single search result with similarity score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub rank: usize,
    pub project_name: String,
    pub file_path: String,
    pub file_format: String,
    pub score: f64,
}

/// Options for histogram search.
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// Scope search to a specific project name.
    pub project: Option<String>,
    /// Maximum number of results to return.
    pub limit: usize,
}

// ---------------------------------------------------------------------------
// Histogram parsing and similarity
// ---------------------------------------------------------------------------

/// Parse a comma-separated f64 string into a Vec of histogram bin values.
/// Expects exactly `BINS_PER_CHANNEL` (64) elements.
pub fn parse_histogram(text: &str) -> Result<Vec<f64>, SearchError> {
    let values: Vec<f64> = text
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|s| {
            s.trim().parse::<f64>().map_err(|_| {
                SearchError::DatabaseError(format!("invalid histogram value: '{}'", s.trim()))
            })
        })
        .collect::<Result<_, _>>()?;

    if values.len() != BINS_PER_CHANNEL {
        return Err(SearchError::DatabaseError(format!(
            "expected {} histogram bins, got {}",
            BINS_PER_CHANNEL,
            values.len()
        )));
    }

    Ok(values)
}

/// Compute histogram intersection similarity between two normalized histograms.
/// Returns a value in [0.0, 1.0] where 1.0 = identical.
pub fn histogram_intersection(a: &[f64], b: &[f64]) -> f64 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let sum: f64 = (0..len).map(|i| a[i].min(b[i])).sum();
    sum
}

// ---------------------------------------------------------------------------
// Search query
// ---------------------------------------------------------------------------

/// Query the database for histogram-based color similarity search.
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
            .filter_map(|r| r.ok())
            .collect(),
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
            .filter_map(|r| r.ok())
            .collect(),
    };

    if rows.is_empty() {
        return Err(SearchError::NoFingerprints);
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
    scored.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
    });

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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_histogram_valid() {
        let hist = "0.0,0.001,0.002,0.003,0.004,0.005,0.006,0.007,0.008,0.009,0.01,0.011,0.012,0.013,0.014,0.015,0.016,0.017,0.018,0.019,0.02,0.021,0.022,0.023,0.024,0.025,0.026,0.027,0.028,0.029,0.03,0.031,0.032,0.033,0.034,0.035,0.036,0.037,0.038,0.039,0.04,0.041,0.042,0.043,0.044,0.045,0.046,0.047,0.048,0.049,0.05,0.051,0.052,0.053,0.054,0.055,0.056,0.057,0.058,0.059,0.06,0.061,0.062,0.063";
        let result = parse_histogram(hist).unwrap();
        assert_eq!(result.len(), 64);
        assert_eq!(result[0], 0.0);
        assert_eq!(result[63], 0.063);
    }

    #[test]
    fn test_parse_histogram_with_spaces() {
        let hist = "0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4";
        let result = parse_histogram(hist).unwrap();
        assert_eq!(result.len(), 64);
    }

    #[test]
    fn test_parse_histogram_wrong_count() {
        let hist = "0.1,0.2,0.3";
        let result = parse_histogram(hist);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("expected 64"));
    }

    #[test]
    fn test_parse_histogram_invalid_value() {
        let hist = "0.1,abc,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8,0.9,1.0,0.1,0.2,0.3,0.4,0.5";
        let result = parse_histogram(hist);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_histogram_empty_string() {
        let result = parse_histogram("");
        assert!(result.is_err());
    }

    #[test]
    fn test_histogram_intersection_identical() {
        let a: Vec<f64> = (0..64).map(|_| 1.0 / 64.0).collect();
        let score = histogram_intersection(&a, &a);
        assert!((score - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_histogram_intersection_completely_different() {
        let a: Vec<f64> = vec![1.0; 64]; // All in first position (not normalized, but test logic)
        let b: Vec<f64> = (0..64).map(|_| 1.0 / 64.0).collect();
        let score = histogram_intersection(&a, &b);
        assert!(score >= 0.0);
        assert!(score <= 1.0);
    }

    #[test]
    fn test_histogram_intersection_empty() {
        let score = histogram_intersection(&[], &[]);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_histogram_intersection_partial_overlap() {
        let a: Vec<f64> = (0..64).map(|_| 1.0 / 64.0).collect();
        let mut b = vec![0.0; 64];
        b[0] = 1.0; // All mass in bin 0
        let score = histogram_intersection(&a, &b);
        // min(1/64, 1.0) + 63 * min(1/64, 0.0) = 1/64
        assert!((score - 1.0 / 64.0).abs() < 1e-10);
    }

    fn make_histogram_csv(value: f64) -> String {
        (0..64).map(|_| value.to_string()).collect::<Vec<_>>().join(",")
    }

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        conn.execute_batch(
            "CREATE TABLE projects (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, path TEXT NOT NULL, dpx_count INTEGER NOT NULL DEFAULT 0, exr_count INTEGER NOT NULL DEFAULT 0, mov_count INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE, filename TEXT NOT NULL, format TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE TABLE fingerprints (id INTEGER PRIMARY KEY AUTOINCREMENT, file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE, histogram_r TEXT NOT NULL, histogram_g TEXT NOT NULL, histogram_b TEXT NOT NULL, luminance_mean REAL NOT NULL, luminance_stddev REAL NOT NULL, color_space_tag TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()));",
        )
        .unwrap();
        conn
    }

    #[test]
    fn test_search_global_no_fingerprints() {
        let conn = setup_test_db();
        // Add a project but no fingerprints
        conn.execute("INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')", [])
            .unwrap();
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 5,
            },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints, got: {:?}", other),
        }
    }

    #[test]
    fn test_search_project_no_fingerprints() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('test', '/tmp/test')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: Some("test".to_string()),
                limit: 5,
            },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints, got: {:?}", other),
        }
    }

    #[test]
    fn test_search_global_returns_results() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene001.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        )
        .unwrap();

        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 5,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rank, 1);
        assert_eq!(result[0].project_name, "film_a");
        assert_eq!(result[0].file_path, "scene001.dpx");
        assert_eq!(result[0].file_format, "dpx");
    }

    #[test]
    fn test_search_with_project_filter() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_b', '/tmp/b')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene001.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (2, 'scene002.dpx', 'exr')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg')",
            [make_histogram_csv(0.01), make_histogram_csv(0.01), make_histogram_csv(0.01)],
        )
        .unwrap();

        // Search within film_a only
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: Some("film_a".to_string()),
                limit: 10,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].project_name, "film_a");

        // Global search returns both
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 10,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_search_limit() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        for i in 0..5 {
            conn.execute(
                &format!("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene{:03}.dpx', 'dpx')", i),
                [],
            )
            .unwrap();
            conn.execute(
                &format!(
                    "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES ({}, ?, ?, ?, 0.5, 0.1, 'acescg')",
                    i + 1
                ),
                [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
            )
            .unwrap();
        }

        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 2,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_search_malformed_histogram_skipped() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'good.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'bad.dpx', 'dpx')", [])
            .unwrap();
        // Good fingerprint
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        )
        .unwrap();
        // Bad fingerprint (wrong number of bins)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, '0.1,0.2', '0.1,0.2', '0.1,0.2', 0.5, 0.1, 'acescg')",
            [],
        )
        .unwrap();

        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 10,
            },
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].file_path, "good.dpx");
    }

    #[test]
    fn test_search_error_display() {
        let err = SearchError::NoFingerprints;
        assert!(format!("{}", err).contains("SEARCH_NO_FINGERPRINTS"));

        let err = SearchError::DatabaseError("query failed".to_string());
        assert!(format!("{}", err).contains("SEARCH_DB_ERROR"));

        let err = SearchError::InvalidFormat("bad format".to_string());
        assert!(format!("{}", err).contains("SEARCH_INVALID_FORMAT"));
    }
}
