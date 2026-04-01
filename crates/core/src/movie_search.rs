// movie_search.rs — Visual style search using color DNA comparison
//
// Scans a library of fingerprint strips, extracts color DNA from each,
// and ranks them by similarity to a query strip.

use std::path::{Path, PathBuf};

use crate::color_dna;
use crate::movie_fingerprint;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from movie search operations.
#[derive(Debug, thiserror::Error)]
pub enum MovieSearchError {
    /// I/O error.
    #[error("MOVIE_SEARCH_IO_ERROR -- {0}")]
    IoError(#[from] std::io::Error),
    /// Invalid input parameters.
    #[error("MOVIE_SEARCH_INVALID_INPUT -- {0}")]
    InvalidInput(String),
    /// Color DNA extraction failed.
    #[error("MOVIE_SEARCH_DNA_ERROR -- {0}")]
    DnaError(String),
    /// No valid strips found.
    #[error("MOVIE_SEARCH_NO_RESULTS -- {0}")]
    NoResults(String),
}

type Result<T> = std::result::Result<T, MovieSearchError>;

// ---------------------------------------------------------------------------
// Search result types
// ---------------------------------------------------------------------------

/// A single search result entry.
#[derive(Debug, Clone)]
pub struct MovieSearchResult {
    /// Path to the matched strip image.
    pub path: PathBuf,
    /// File name (without extension).
    pub name: String,
    /// Overall similarity score [0.0, 1.0].
    pub overall_similarity: f64,
    /// Hue similarity score [0.0, 1.0].
    pub hue_similarity: f64,
    /// Contrast difference.
    pub contrast_diff: f64,
    /// Warmth difference.
    pub warmth_diff: f64,
}

// ---------------------------------------------------------------------------
// Search functions
// ---------------------------------------------------------------------------

/// Search a directory of fingerprint strips for visually similar movies.
///
/// # Arguments
/// * `query_strip` — Path to the query fingerprint strip PNG.
/// * `library_dir` — Directory containing candidate strip PNG files.
/// * `limit` — Maximum number of results to return.
///
/// # Returns
/// Ranked list of search results, sorted by overall similarity (descending).
pub fn visual_search(
    query_strip: &Path,
    library_dir: &Path,
    limit: usize,
) -> Result<Vec<MovieSearchResult>> {
    if !query_strip.exists() {
        return Err(MovieSearchError::InvalidInput(format!(
            "query strip not found: {}", query_strip.display()
        )));
    }
    if !library_dir.is_dir() {
        return Err(MovieSearchError::InvalidInput(format!(
            "library directory not found: {}", library_dir.display()
        )));
    }

    // Extract query DNA
    let (qw, qh, qdata) = movie_fingerprint::read_strip_png(query_strip)
        .map_err(|e| MovieSearchError::DnaError(format!("failed to read query strip: {}", e)))?;
    let query_dna = color_dna::extract_color_dna(&qdata, qw, qh)
        .map_err(|e| MovieSearchError::DnaError(format!("failed to extract query DNA: {}", e)))?;

    // Scan library directory for PNG files
    let entries = std::fs::read_dir(library_dir)
        .map_err(MovieSearchError::IoError)?;

    let mut results: Vec<MovieSearchResult> = Vec::new();

    for entry in entries {
        let entry = entry.map_err(MovieSearchError::IoError)?;
        let path = entry.path();

        // Skip non-PNG files and the query itself
        if path.extension().and_then(|e| e.to_str()) != Some("png") {
            continue;
        }
        if path == query_strip {
            continue;
        }

        let (w, h, data) = match movie_fingerprint::read_strip_png(&path) {
            Ok(d) => d,
            Err(_) => continue, // skip unreadable files
        };

        let candidate_dna = match color_dna::extract_color_dna(&data, w, h) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let comp = match color_dna::compare_color_dna(&query_dna, &candidate_dna) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let name = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        results.push(MovieSearchResult {
            path,
            name,
            overall_similarity: comp.overall_similarity,
            hue_similarity: comp.hue_similarity,
            contrast_diff: comp.contrast_diff,
            warmth_diff: comp.warmth_diff,
        });
    }

    // Sort by overall similarity descending
    results.sort_by(|a, b| {
        b.overall_similarity.partial_cmp(&a.overall_similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results.truncate(limit);

    if results.is_empty() {
        return Err(MovieSearchError::NoResults(
            "no valid strip images found in library directory".to_string(),
        ));
    }

    Ok(results)
}

/// Compare a query strip against a list of specific strip paths.
///
/// Convenience function when the candidate list is known in advance.
pub fn visual_search_paths(
    query_strip: &Path,
    candidate_paths: &[PathBuf],
    limit: usize,
) -> Result<Vec<MovieSearchResult>> {
    if !query_strip.exists() {
        return Err(MovieSearchError::InvalidInput(format!(
            "query strip not found: {}", query_strip.display()
        )));
    }

    let (qw, qh, qdata) = movie_fingerprint::read_strip_png(query_strip)
        .map_err(|e| MovieSearchError::DnaError(format!("failed to read query strip: {}", e)))?;
    let query_dna = color_dna::extract_color_dna(&qdata, qw, qh)
        .map_err(|e| MovieSearchError::DnaError(format!("failed to extract query DNA: {}", e)))?;

    let mut results: Vec<MovieSearchResult> = Vec::new();

    for path in candidate_paths {
        if !path.exists() || path == query_strip {
            continue;
        }

        let (w, h, data) = match movie_fingerprint::read_strip_png(path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let candidate_dna = match color_dna::extract_color_dna(&data, w, h) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let comp = match color_dna::compare_color_dna(&query_dna, &candidate_dna) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let name = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        results.push(MovieSearchResult {
            path: path.clone(),
            name,
            overall_similarity: comp.overall_similarity,
            hue_similarity: comp.hue_similarity,
            contrast_diff: comp.contrast_diff,
            warmth_diff: comp.warmth_diff,
        });
    }

    results.sort_by(|a, b| {
        b.overall_similarity.partial_cmp(&a.overall_similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results.truncate(limit);
    Ok(results)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_strip(dir: &Path, name: &str, r: f64, g: f64, b: f64, width: usize, height: usize) -> PathBuf {
        let path = dir.join(name);
        let data: Vec<f64> = (0..width * height).flat_map(|_| [r, g, b]).collect();
        crate::movie_fingerprint::save_fingerprint_png(&data, width, height, &path).unwrap();
        path
    }

    #[test]
    fn test_visual_search_nonexistent_query() {
        let result = visual_search(Path::new("/nonexistent.png"), Path::new("/tmp"), 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("query strip"));
    }

    #[test]
    fn test_visual_search_not_a_directory() {
        let dir = TempDir::new().unwrap();
        let query = create_test_strip(dir.path(), "query.png", 0.5, 0.5, 0.5, 10, 10);
        let result = visual_search(&query, &query, 10); // query path is not a dir
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("library directory"));
    }

    #[test]
    fn test_visual_search_empty_directory() {
        let dir = TempDir::new().unwrap();
        let query = create_test_strip(dir.path(), "query.png", 0.5, 0.5, 0.5, 10, 10);
        let lib_dir = TempDir::new().unwrap();
        let result = visual_search(&query, lib_dir.path(), 10);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no valid"));
    }

    #[test]
    fn test_visual_search_ranks_similar_higher() {
        let dir = TempDir::new().unwrap();
        let query = create_test_strip(dir.path(), "query.png", 0.5, 0.5, 0.5, 10, 10);

        let lib_dir = dir.path().join("lib");
        std::fs::create_dir_all(&lib_dir).unwrap();

        // Similar strip (same gray)
        create_test_strip(&lib_dir, "similar.png", 0.5, 0.5, 0.5, 10, 10);
        // Different strip (pure red)
        create_test_strip(&lib_dir, "different.png", 1.0, 0.0, 0.0, 10, 10);

        let results = visual_search(&query, &lib_dir, 10).unwrap();
        assert_eq!(results.len(), 2);
        // Similar should rank higher (greater similarity)
        assert!(results[0].overall_similarity > results[1].overall_similarity,
            "similar strip should rank higher: {} vs {}",
            results[0].overall_similarity, results[1].overall_similarity);
        assert_eq!(results[0].name, "similar");
    }

    #[test]
    fn test_visual_search_limit() {
        let dir = TempDir::new().unwrap();
        let query = create_test_strip(dir.path(), "query.png", 0.5, 0.5, 0.5, 10, 10);

        let lib_dir = dir.path().join("lib");
        std::fs::create_dir_all(&lib_dir).unwrap();

        for i in 0..5 {
            create_test_strip(&lib_dir, &format!("strip{}.png", i), 0.5, 0.5, 0.5, 10, 10);
        }

        let results = visual_search(&query, &lib_dir, 3).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_visual_search_paths() {
        let dir = TempDir::new().unwrap();
        let query = create_test_strip(dir.path(), "query.png", 0.5, 0.5, 0.5, 10, 10);
        let candidate = create_test_strip(dir.path(), "candidate.png", 0.6, 0.4, 0.5, 10, 10);

        let results = visual_search_paths(&query, &[candidate.clone()], 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "candidate");
    }

    #[test]
    fn test_error_display() {
        let err = MovieSearchError::InvalidInput("bad params".to_string());
        assert!(err.to_string().contains("MOVIE_SEARCH_INVALID_INPUT"));

        let err = MovieSearchError::NoResults("empty".to_string());
        assert!(err.to_string().contains("MOVIE_SEARCH_NO_RESULTS"));
    }
}
