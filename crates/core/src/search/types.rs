// types.rs — Core types for search module

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from search operations.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    /// No fingerprints exist in the database.
    #[error("SEARCH_NO_FINGERPRINTS -- No indexed projects found")]
    NoFingerprints,
    /// No results found for the specified project.
    #[error("SEARCH_PROJECT_NOT_FOUND -- No results found for project '{0}'")]
    ProjectNotFound(String),
    /// A database error occurred.
    #[error("SEARCH_DB_ERROR -- {0}")]
    DatabaseError(String),
    /// Invalid format parameter.
    #[error("SEARCH_INVALID_FORMAT -- {0}")]
    InvalidFormat(String),
    /// AI embedding generation is unavailable.
    #[error("SEARCH_AI_UNAVAILABLE -- {0}")]
    AiUnavailable(String),
    /// Error during embedding computation or storage.
    #[error("SEARCH_EMBEDDING_ERROR -- {0}")]
    EmbeddingError(String),
}

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
    /// Use spatial pyramid matching for grading score instead of flat Bhattacharyya.
    pub use_pyramid: bool,
}

/// Detailed fingerprint information for display.
#[derive(Debug, Clone)]
pub struct FingerprintInfo {
    pub project_name: String,
    pub file_path: String,
    pub file_format: String,
    pub color_space_tag: String,
    pub luminance_mean: f64,
    pub luminance_stddev: f64,
    pub histogram_r_summary: HistogramSummary,
    pub histogram_g_summary: HistogramSummary,
    pub histogram_b_summary: HistogramSummary,
    pub tags: Vec<String>,
}

/// Summary statistics for a single histogram channel.
#[derive(Debug, Clone)]
pub struct HistogramSummary {
    pub mean_value: f64,
    pub dominant_bin_min: usize,
    pub dominant_bin_max: usize,
}

// Type aliases for database query results
pub(crate) type FingerprintSearchRowWithId = (String, String, String, i64, Option<Vec<u8>>, Option<String>);
pub(crate) type TagSearchRow = (i64, String, String, String, Option<Vec<u8>>, Option<String>);
pub(crate) type FeatureSearchRow = (i64, String, String, String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, i32);
pub(crate) type CombinedSearchRow = (
    i64,
    i64,
    String,
    String,
    String,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    Vec<u8>,
    i32,
    Option<Vec<u8>>,
    String,
    Option<String>,
);

/// Compute summary statistics for a histogram channel.
pub(crate) fn summarize_histogram(hist: &[f64]) -> HistogramSummary {
    if hist.is_empty() {
        return HistogramSummary {
            mean_value: 0.0,
            dominant_bin_min: 0,
            dominant_bin_max: 0,
        };
    }
    let mean_value = hist.iter().sum::<f64>() / hist.len() as f64;
    let (dominant_idx, _) = hist
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((0, &0.0));
    HistogramSummary {
        mean_value,
        dominant_bin_min: dominant_idx,
        dominant_bin_max: dominant_idx,
    }
}
