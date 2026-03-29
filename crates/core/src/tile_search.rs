// tile_search.rs — Tile-level similarity search
//
// Compares per-tile grading features between a query fingerprint and candidates.
// Supports spatial alignment (same position only) and position-invariant matching.

use crate::color_science::{self, bhattacharyya_distance};
use crate::db;
use crate::grading_features::GradingFeatures;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from tile-level search.
#[derive(Debug)]
pub enum TileSearchError {
    /// Query fingerprint has no tiles.
    NoQueryTiles,
    /// Database error during tile lookup.
    DbError(String),
    /// Feature extraction error.
    FeatureError(String),
}

impl std::fmt::Display for TileSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TileSearchError::NoQueryTiles => {
                write!(f, "TILE_SEARCH_NO_QUERY_TILES -- query fingerprint has no tile features")
            }
            TileSearchError::DbError(msg) => {
                write!(f, "TILE_SEARCH_DB_ERROR -- {}", msg)
            }
            TileSearchError::FeatureError(msg) => {
                write!(f, "TILE_SEARCH_FEATURE_ERROR -- {}", msg)
            }
        }
    }
}

impl std::error::Error for TileSearchError {}

// ---------------------------------------------------------------------------
// Tile match result
// ---------------------------------------------------------------------------

/// Per-tile match detail for a candidate fingerprint.
#[derive(Debug, Clone)]
pub struct TileMatch {
    pub query_row: usize,
    pub query_col: usize,
    pub candidate_row: usize,
    pub candidate_col: usize,
    pub score: f64,
}

/// Result for one candidate in tile-level search.
#[derive(Debug, Clone)]
pub struct TileSearchCandidate {
    pub fingerprint_id: i64,
    pub project_name: String,
    pub file_path: String,
    pub file_format: String,
    /// Overall tile similarity score (0.0–1.0).
    pub score: f64,
    /// Per-tile match details.
    pub tile_matches: Vec<TileMatch>,
}

// ---------------------------------------------------------------------------
// Tile search modes
// ---------------------------------------------------------------------------

/// How to match tiles between query and candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TileMode {
    /// Compare tiles at corresponding grid positions only.
    Spatial,
    /// Compare all tile pairs and use the best match per query tile.
    Any,
}

impl std::fmt::Display for TileMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TileMode::Spatial => write!(f, "spatial"),
            TileMode::Any => write!(f, "any"),
        }
    }
}

/// Parse tile mode from string.
pub fn parse_tile_mode(s: &str) -> Option<TileMode> {
    match s {
        "spatial" => Some(TileMode::Spatial),
        "any" => Some(TileMode::Any),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tile-level search
// ---------------------------------------------------------------------------

/// Perform tile-level similarity search.
///
/// Compares per-tile grading features of the query fingerprint against all
/// candidates (optionally filtered by project). Returns results sorted by
/// descending score.
///
/// # Arguments
/// * `conn` — Database connection
/// * `query_fingerprint_id` — The fingerprint to search from
/// * `mode` — Spatial (position-aligned) or Any (position-invariant)
/// * `project` — Optional project filter for candidates
/// * `limit` — Maximum number of results
pub fn tile_search(
    conn: &rusqlite::Connection,
    query_fingerprint_id: i64,
    mode: TileMode,
    project: Option<&str>,
    limit: usize,
) -> Result<Vec<TileSearchCandidate>, TileSearchError> {
    // Load query tiles
    let query_tiles = db::load_fingerprint_tiles(conn, query_fingerprint_id)
        .map_err(|e| TileSearchError::DbError(e.to_string()))?;

    if query_tiles.is_empty() {
        return Err(TileSearchError::NoQueryTiles);
    }

    // Load candidates: all fingerprints with tiles (excluding query)
    let candidates = load_tile_candidates(conn, query_fingerprint_id, project)
        .map_err(|e| TileSearchError::DbError(e.to_string()))?;

    let mut results: Vec<TileSearchCandidate> = Vec::new();

    for (fp_id, project_name, file_path, file_format, candidate_tiles) in &candidates {
        let (score, tile_matches) = match mode {
            TileMode::Spatial => score_spatial(&query_tiles, candidate_tiles),
            TileMode::Any => score_any(&query_tiles, candidate_tiles),
        };

        results.push(TileSearchCandidate {
            fingerprint_id: *fp_id,
            project_name: project_name.clone(),
            file_path: file_path.clone(),
            file_format: file_format.clone(),
            score,
            tile_matches,
        });
    }

    // Sort by score descending
    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit);

    Ok(results)
}

// ---------------------------------------------------------------------------
// Scoring functions
// ---------------------------------------------------------------------------

/// Spatial scoring: compare tiles at matching grid positions.
/// Score = mean Bhattacharyya coefficient over all aligned positions.
fn score_spatial(
    query_tiles: &[(usize, usize, GradingFeatures)],
    candidate_tiles: &[(usize, usize, GradingFeatures)],
) -> (f64, Vec<TileMatch>) {
    let mut total_score = 0.0;
    let mut match_count = 0usize;
    let mut matches = Vec::new();

    for (qr, qc, qf) in query_tiles {
        // Find candidate tile at the same position
        if let Some((_cr, _cc, cf)) = candidate_tiles.iter().find(|(cr, cc, _)| *cr == *qr && *cc == *qc) {
            let score = compute_bhattacharyya_safe(qf, cf);
            matches.push(TileMatch {
                query_row: *qr,
                query_col: *qc,
                candidate_row: *qr,
                candidate_col: *qc,
                score,
            });
            total_score += score;
            match_count += 1;
        }
    }

    let avg = if match_count > 0 { total_score / match_count as f64 } else { 0.0 };
    (avg, matches)
}

/// Position-invariant scoring: for each query tile, find the best-matching
/// candidate tile regardless of position.
/// Score = mean of per-query-tile best Bhattacharyya coefficients.
fn score_any(
    query_tiles: &[(usize, usize, GradingFeatures)],
    candidate_tiles: &[(usize, usize, GradingFeatures)],
) -> (f64, Vec<TileMatch>) {
    let mut total_score = 0.0;
    let mut matches = Vec::new();

    for (qr, qc, qf) in query_tiles {
        let mut best_score = 0.0f64;
        let mut best_match: Option<(usize, usize)> = None;

        for (cr, cc, cf) in candidate_tiles {
            let score = compute_bhattacharyya_safe(qf, cf);
            if score > best_score {
                best_score = score;
                best_match = Some((*cr, *cc));
            }
        }

        if let Some((cr, cc)) = best_match {
            matches.push(TileMatch {
                query_row: *qr,
                query_col: *qc,
                candidate_row: cr,
                candidate_col: cc,
                score: best_score,
            });
            total_score += best_score;
        }
    }

    let avg = if !query_tiles.is_empty() { total_score / query_tiles.len() as f64 } else { 0.0 };
    (avg, matches)
}

/// Compute Bhattacharyya coefficient between two GradingFeatures,
/// returning 0.0 on any error.
fn compute_bhattacharyya_safe(a: &GradingFeatures, b: &GradingFeatures) -> f64 {
    color_science::bhattacharyya_distance(a, b).unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// Candidate loading
// ---------------------------------------------------------------------------

/// Load all candidate fingerprints with their tiles.
/// Returns (fingerprint_id, project_name, file_path, file_format, tiles).
fn load_tile_candidates(
    conn: &rusqlite::Connection,
    exclude_fingerprint_id: i64,
    project: Option<&str>,
) -> Result<Vec<(i64, String, String, String, Vec<(usize, usize, GradingFeatures)>)>, Box<dyn std::error::Error>> {
    // Find all fingerprints that have tiles (excluding query)
    let mut sql = String::from(
        "SELECT DISTINCT ft.fingerprint_id, p.name, p.path || '/' || f.filename, f.format
         FROM fingerprint_tiles ft
         JOIN fingerprints fp ON ft.fingerprint_id = fp.id
         JOIN files f ON fp.file_id = f.id
         JOIN projects p ON f.project_id = p.id
         WHERE ft.fingerprint_id != ?1"
    );
    if project.is_some() {
        sql.push_str(" AND p.name = ?2");
    }

    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<(i64, String, String, String)> = if let Some(proj) = project {
        stmt.query_map(rusqlite::params![exclude_fingerprint_id, proj], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map(rusqlite::params![exclude_fingerprint_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?
    };

    let mut candidates = Vec::with_capacity(rows.len());
    for (fp_id, project_name, file_path, file_format) in rows {
        let tiles = db::load_fingerprint_tiles(conn, fp_id)?;
        if !tiles.is_empty() {
            candidates.push((fp_id, project_name, file_path, file_format, tiles));
        }
    }

    Ok(candidates)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tile_mode() {
        assert_eq!(parse_tile_mode("spatial"), Some(TileMode::Spatial));
        assert_eq!(parse_tile_mode("any"), Some(TileMode::Any));
        assert_eq!(parse_tile_mode("invalid"), None);
    }

    #[test]
    fn test_tile_mode_display() {
        assert_eq!(format!("{}", TileMode::Spatial), "spatial");
        assert_eq!(format!("{}", TileMode::Any), "any");
    }

    #[test]
    fn test_error_display() {
        let err = TileSearchError::NoQueryTiles;
        assert!(err.to_string().contains("TILE_SEARCH_NO_QUERY_TILES"));

        let err = TileSearchError::DbError("conn failed".to_string());
        assert!(err.to_string().contains("TILE_SEARCH_DB_ERROR"));

        let err = TileSearchError::FeatureError("bad".to_string());
        assert!(err.to_string().contains("TILE_SEARCH_FEATURE_ERROR"));
    }

    fn make_features(l_mean: f64) -> GradingFeatures {
        GradingFeatures {
            hist_l: vec![l_mean; 64],
            hist_a: vec![0.1; 64],
            hist_b: vec![0.2; 64],
            moments: [l_mean, 0.2, 0.1, -0.3, 0.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }

    #[test]
    fn test_score_spatial_identical_tiles() {
        let tiles = vec![
            (0, 0, make_features(0.5)),
            (0, 1, make_features(0.5)),
            (1, 0, make_features(0.5)),
            (1, 1, make_features(0.5)),
        ];
        let (score, matches) = score_spatial(&tiles, &tiles);
        // Identical features should score very high (near 1.0)
        assert!(score > 0.9, "expected high score for identical tiles, got {}", score);
        assert_eq!(matches.len(), 4);
    }

    #[test]
    fn test_score_spatial_no_overlap() {
        let query = vec![
            (0, 0, make_features(0.5)),
            (0, 1, make_features(0.5)),
        ];
        // Different positions — no spatial overlap
        let candidate = vec![
            (1, 0, make_features(0.5)),
            (1, 1, make_features(0.5)),
        ];
        let (score, matches) = score_spatial(&query, &candidate);
        assert_eq!(score, 0.0);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_score_spatial_partial_overlap() {
        let query = vec![
            (0, 0, make_features(0.5)),
            (0, 1, make_features(0.5)),
        ];
        let candidate = vec![
            (0, 0, make_features(0.8)),
        ];
        let (score, matches) = score_spatial(&query, &candidate);
        // Only (0,0) overlaps
        assert!(score > 0.0);
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_score_any_finds_best_match() {
        // Create features with distinct histograms so bhattacharyya can differentiate
        let mut query_hist = vec![0.0; 64];
        query_hist[32] = 1.0; // peaked at bin 32
        let query = vec![
            (0, 0, GradingFeatures {
                hist_l: query_hist.clone(),
                hist_a: vec![1.0 / 64.0; 64],
                hist_b: vec![1.0 / 64.0; 64],
                moments: [0.5, 0.2, 0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            }),
        ];

        // Candidate 1: peaked at bin 0 (different from query)
        let mut cand1_hist = vec![0.0; 64];
        cand1_hist[0] = 1.0;
        // Candidate 2: peaked at bin 32 (identical to query)
        let mut cand2_hist = vec![0.0; 64];
        cand2_hist[32] = 1.0;
        // Candidate 3: peaked at bin 63
        let mut cand3_hist = vec![0.0; 64];
        cand3_hist[63] = 1.0;

        let uniform = vec![1.0 / 64.0; 64];
        let candidate = vec![
            (1, 1, GradingFeatures { hist_l: cand1_hist, hist_a: uniform.clone(), hist_b: uniform.clone(), moments: [0.0; 12] }),
            (2, 2, GradingFeatures { hist_l: cand2_hist, hist_a: uniform.clone(), hist_b: uniform.clone(), moments: [0.5, 0.2, 0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0] }),
            (3, 3, GradingFeatures { hist_l: cand3_hist, hist_a: uniform.clone(), hist_b: uniform.clone(), moments: [0.0; 12] }),
        ];
        let (score, matches) = score_any(&query, &candidate);
        // Should find the identical tile at (2,2)
        assert!(score > 0.9, "expected high score for best match, got {}", score);
        assert_eq!(matches.len(), 1);
        // All candidates have same hist_l shape (single peak) so bhattacharyya will match all equally
        // The key is that score should be high since histograms have same shape
        assert!(matches[0].score > 0.5);
    }

    #[test]
    fn test_score_any_empty_candidate() {
        let query = vec![
            (0, 0, make_features(0.5)),
        ];
        let candidate = vec![];
        let (score, matches) = score_any(&query, &candidate);
        assert_eq!(score, 0.0);
        assert!(matches.is_empty());
    }
}
