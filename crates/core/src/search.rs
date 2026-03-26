// search.rs — Histogram-based and embedding-based search engine

use rusqlite::Connection;

use crate::color_science::{bhattacharyya_distance, GradingFeatures};
use crate::fingerprint::BINS_PER_CHANNEL;
use crate::hybrid_scoring::{self, HybridSearchResult, SignalWeights};
use crate::python_bridge::{AiError, PythonBridge};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from search operations.
#[derive(Debug)]
pub enum SearchError {
    /// No fingerprints exist in the database.
    NoFingerprints,
    /// No results found for the specified project.
    ProjectNotFound(String),
    /// A database error occurred.
    DatabaseError(String),
    /// Invalid format parameter.
    InvalidFormat(String),
    /// AI embedding generation is unavailable.
    AiUnavailable(String),
    /// Error during embedding computation or storage.
    EmbeddingError(String),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::NoFingerprints => {
                write!(f, "SEARCH_NO_FINGERPRINTS -- No indexed projects found")
            }
            SearchError::ProjectNotFound(name) => {
                write!(f, "SEARCH_PROJECT_NOT_FOUND -- No results found for project '{}'", name)
            }
            SearchError::DatabaseError(msg) => {
                write!(f, "SEARCH_DB_ERROR -- {}", msg)
            }
            SearchError::InvalidFormat(msg) => {
                write!(f, "SEARCH_INVALID_FORMAT -- {}", msg)
            }
            SearchError::AiUnavailable(msg) => {
                write!(f, "SEARCH_AI_UNAVAILABLE -- {}", msg)
            }
            SearchError::EmbeddingError(msg) => {
                write!(f, "SEARCH_EMBEDDING_ERROR -- {}", msg)
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

/// Compute summary statistics for a histogram channel.
fn summarize_histogram(hist: &[f64]) -> HistogramSummary {
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

// ---------------------------------------------------------------------------
// Histogram parsing and similarity
// ---------------------------------------------------------------------------

/// Parse a comma-separated f64 string into a Vec of histogram bin values.
/// Expects exactly `BINS_PER_CHANNEL` (64) elements.
/// Rejects NaN and infinity values.
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

    // Reject NaN and infinity values
    for v in &values {
        if !v.is_finite() {
            return Err(SearchError::DatabaseError(format!(
                "histogram contains non-finite value: {}",
                v
            )));
        }
    }

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
// Cosine similarity (embedding-based search)
// ---------------------------------------------------------------------------

/// Compute cosine similarity between two vectors.
/// Returns a value in [-1.0, 1.0] where 1.0 = identical, 0.0 = orthogonal.
pub fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f64 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f64 = a.iter().map(|x| x * x).sum::<f64>().sqrt();
    let norm_b: f64 = b.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

// ---------------------------------------------------------------------------
// Embedding serialization helpers
// ---------------------------------------------------------------------------

/// Serialize a Vec<f64> embedding to raw bytes (stored as f32 BLOB).
pub fn serialize_embedding(embedding: &[f64]) -> Vec<u8> {
    embedding
        .iter()
        .flat_map(|v| (*v as f32).to_le_bytes())
        .collect()
}

/// Deserialize a BLOB back to Vec<f64>.
/// Returns None if the blob length is not a multiple of 4 bytes.
pub fn deserialize_embedding(blob: &[u8]) -> Option<Vec<f64>> {
    if blob.len() % 4 != 0 {
        return None;
    }
    Some(
        blob.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64)
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Image-based search with embeddings
// ---------------------------------------------------------------------------

/// Search by reference image using embedding similarity.
///
/// Creates a PythonBridge internally, generates embeddings as needed,
/// and falls back to histogram-only search if AI is unavailable.
pub fn search_by_image(
    conn: &Connection,
    image_path: &str,
    options: &SearchOptions,
    idle_timeout_secs: u64,
    inference_timeout_secs: u64,
    model_name: &str,
) -> Result<Vec<SearchResult>, SearchError> {
    let mut bridge = PythonBridge::new(idle_timeout_secs, inference_timeout_secs, model_name.to_string());

    // Step 1: Generate embedding for the reference image
    let ref_embedding = match bridge.generate_embedding(image_path) {
        Ok(emb) => emb,
        Err(AiError::SubprocessNotFound(_msg)) => {
            eprintln!("Warning: AI embedding unavailable — falling back to histogram search");
            return search_histograms(conn, options);
        }
        Err(e) => {
            eprintln!("Warning: AI embedding unavailable — falling back to histogram search ({})", e);
            return search_histograms(conn, options);
        }
    };

    // Step 2: Query all fingerprints with their embeddings
    let mut sql = String::from(
        "SELECT p.name, f.filename, f.format,
                fp.embedding, fp.embedding_model
         FROM fingerprints fp
         JOIN files f ON f.id = fp.file_id
         JOIN projects p ON p.id = f.project_id"
    );
    if options.project.is_some() {
        sql.push_str(" WHERE p.name = ?1");
    }

    let mut stmt = conn.prepare(&sql).map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    let rows: Vec<(String, String, String, Option<Vec<u8>>, Option<String>)> =
        match &options.project {
            Some(proj) => stmt
                .query_map([proj], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<Vec<u8>>>(3)?,
                        row.get::<_, Option<String>>(4)?,
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
                        row.get::<_, Option<Vec<u8>>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                })
                .map_err(|e| SearchError::DatabaseError(e.to_string()))?
                .collect::<Result<_, _>>()
                .map_err(|e| SearchError::DatabaseError(e.to_string()))?,
        };

    if rows.is_empty() {
        return if options.project.is_some() {
            Err(SearchError::ProjectNotFound(options.project.clone().unwrap()))
        } else {
            Err(SearchError::NoFingerprints)
        };
    }

    // Step 3: Score each fingerprint
    let mut scored: Vec<(String, String, String, f64)> = Vec::new();

    for (project_name, file_name, file_format, embedding_blob, cached_model) in rows {
        match (embedding_blob, cached_model) {
            // Has cached embedding from the same model
            (Some(blob), Some(ref m)) if m == model_name || model_name.is_empty() => {
                if let Some(cached_emb) = deserialize_embedding(&blob) {
                    // Verify dimension compatibility; skip if mismatched
                    if cached_emb.len() == ref_embedding.len() {
                        let score = cosine_similarity(&ref_embedding, &cached_emb);
                        scored.push((project_name, file_name, file_format, score));
                    } else {
                        eprintln!(
                            "Warning: embedding dimension mismatch for {} / {} ({} vs {}), skipping",
                            project_name, file_name, cached_emb.len(), ref_embedding.len()
                        );
                    }
                } else {
                    eprintln!(
                        "Warning: malformed embedding blob for {} / {}, skipping",
                        project_name, file_name
                    );
                }
            }
            // No embedding cached or different model — skip rather than include with 0.0
            _ => {
                eprintln!(
                    "Warning: no embedding cached for {} / {}, skipping",
                    project_name, file_name
                );
            }
        }
    }

    if scored.is_empty() {
        return Err(SearchError::NoFingerprints);
    }

    // Sort by score descending
    scored.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));

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
// Tag-based search
// ---------------------------------------------------------------------------

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

/// Combined search: filter by tag, then rank by image similarity within filtered set.
/// If image is not provided, falls back to pure tag search.
pub fn search_by_image_and_tag(
    conn: &Connection,
    tag: &str,
    image_path: &str,
    options: &SearchOptions,
    idle_timeout_secs: u64,
    inference_timeout_secs: u64,
    model_name: &str,
) -> Result<Vec<SearchResult>, SearchError> {
    // Step 1: Get fingerprint IDs matching the tag(s)
    let query_tags: Vec<&str> = tag.split_whitespace().collect();
    if query_tags.is_empty() {
        return Err(SearchError::NoFingerprints);
    }

    let placeholders: Vec<String> = (1..=query_tags.len())
        .map(|i| format!("?{}", i))
        .collect();
    let where_tags = placeholders.join(", ");

    let mut sql = format!(
        "SELECT fp.id, p.name, f.filename, f.format,
                fp.embedding, fp.embedding_model
         FROM fingerprints fp
         JOIN files f ON f.id = fp.file_id
         JOIN projects p ON p.id = f.project_id
         JOIN tags t ON t.fingerprint_id = fp.id
         WHERE t.tag IN ({})",
        where_tags
    );

    if options.project.is_some() {
        let proj_idx = query_tags.len() + 1;
        sql.push_str(&format!(" AND p.name = ?{}", proj_idx));
    }

    sql.push_str(" GROUP BY fp.id");

    let mut stmt = conn.prepare(&sql).map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = query_tags
        .iter()
        .map(|t| Box::new(t.to_string()) as Box<dyn rusqlite::types::ToSql>)
        .collect();

    if let Some(ref proj) = options.project {
        params.push(Box::new(proj.clone()));
    }

    let rows: Vec<(i64, String, String, String, Option<Vec<u8>>, Option<String>)> = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    if rows.is_empty() {
        return Err(SearchError::NoFingerprints);
    }

    // Step 2: Generate embedding for reference image
    let mut bridge =
        PythonBridge::new(idle_timeout_secs, inference_timeout_secs, model_name.to_string());

    let ref_embedding = match bridge.generate_embedding(image_path) {
        Ok(emb) => emb,
        Err(e) => {
            eprintln!(
                "Warning: AI embedding unavailable — falling back to tag-only search ({})",
                e
            );
            // Fall back to pure tag search
            return search_by_tag(conn, tag, options);
        }
    };

    // Step 3: Score filtered fingerprints by cosine similarity
    let mut scored: Vec<(String, String, String, f64)> = Vec::new();

    for (_fp_id, project_name, file_name, file_format, embedding_blob, cached_model) in rows {
        match (embedding_blob, cached_model) {
            (Some(blob), Some(ref m)) if m == model_name || model_name.is_empty() => {
                if let Some(cached_emb) = deserialize_embedding(&blob) {
                    if cached_emb.len() == ref_embedding.len() {
                        let score = cosine_similarity(&ref_embedding, &cached_emb);
                        scored.push((project_name, file_name, file_format, score));
                    } else {
                        eprintln!(
                            "Warning: embedding dimension mismatch for {} / {} ({} vs {}), skipping",
                            project_name, file_name, cached_emb.len(), ref_embedding.len()
                        );
                    }
                } else {
                    eprintln!(
                        "Warning: malformed embedding blob for {} / {}, skipping",
                        project_name, file_name
                    );
                }
            }
            _ => {
                // No embedding available — skip rather than including with 0.0 score
                eprintln!(
                    "Warning: no embedding cached for {} / {}, skipping",
                    project_name, file_name
                );
            }
        }
    }

    if scored.is_empty() {
        return Err(SearchError::NoFingerprints);
    }

    scored.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
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
// Bhattacharyya distance search (grading features)
// ---------------------------------------------------------------------------

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
                fp.oklab_hist_l, fp.oklab_hist_a, fp.oklab_hist_b, fp.color_moments
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

    let rows: Vec<(i64, String, String, String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>)> = stmt
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

    for (_file_id, project_name, filename, format, hist_l, hist_a, hist_b, moments) in rows {
        let candidate = match GradingFeatures::from_separate_blobs(&hist_l, &hist_a, &hist_b, &moments) {
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

/// Load grading features for a file from DB, returning GradingFeatures.
pub(crate) fn load_grading_features(
    conn: &Connection,
    file_id: i64,
) -> Result<GradingFeatures, SearchError> {
    let sql = "SELECT oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments
               FROM fingerprints
               WHERE file_id = ?1 AND oklab_hist_l IS NOT NULL
               LIMIT 1";

    let result = conn.query_row(sql, rusqlite::params![file_id], |row| {
        Ok((
            row.get::<_, Option<Vec<u8>>>(0)?,
            row.get::<_, Option<Vec<u8>>>(1)?,
            row.get::<_, Option<Vec<u8>>>(2)?,
            row.get::<_, Option<Vec<u8>>>(3)?,
        ))
    });

    match result {
        Ok((Some(hist_l), Some(hist_a), Some(hist_b), Some(moments))) => {
            GradingFeatures::from_separate_blobs(&hist_l, &hist_a, &hist_b, &moments)
                .map_err(|e| SearchError::DatabaseError(format!("grading feature decode: {}", e)))
        }
        Ok(_) => Err(SearchError::NoFingerprints),
        Err(e) => Err(SearchError::DatabaseError(format!(
            "query grading features for file_id {}: {}",
            file_id, e
        ))),
    }
}

// ---------------------------------------------------------------------------
// Hybrid search (three-signal weighted scoring)
// ---------------------------------------------------------------------------

/// Load tags for a batch of fingerprint IDs.
/// Returns a HashMap mapping fingerprint_id -> Vec<tag>.
fn load_tags_batch(
    conn: &Connection,
    fingerprint_ids: &[i64],
) -> Result<std::collections::HashMap<i64, Vec<String>>, SearchError> {
    use std::collections::HashMap;

    let mut result: HashMap<i64, Vec<String>> = HashMap::new();

    if fingerprint_ids.is_empty() {
        return Ok(result);
    }

    let placeholders: Vec<String> = (1..=fingerprint_ids.len())
        .map(|i| format!("?{}", i))
        .collect();
    let where_clause = placeholders.join(", ");

    let sql = format!(
        "SELECT fingerprint_id, tag FROM tags WHERE fingerprint_id IN ({})",
        where_clause
    );

    let params: Vec<Box<dyn rusqlite::types::ToSql>> = fingerprint_ids
        .iter()
        .map(|id| Box::new(*id) as Box<dyn rusqlite::types::ToSql>)
        .collect();

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| SearchError::DatabaseError(format!("load_tags_batch prepare: {}", e)))?;

    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| SearchError::DatabaseError(format!("load_tags_batch query: {}", e)))?;

    for row in rows {
        let row = row.map_err(|e| SearchError::DatabaseError(format!("load_tags_batch row: {}", e)))?;
        result.entry(row.0).or_default().push(row.1);
    }

    Ok(result)
}

/// Hybrid search using weighted combination of grading, CLIP, and tag signals.
///
/// Loads query and candidate features from DB, computes per-signal similarities,
/// resolves weights with graceful degradation for missing signals, and returns
/// results ranked by hybrid score descending.
pub fn hybrid_search(
    conn: &Connection,
    query_file_id: i64,
    weights: &SignalWeights,
    options: &SearchOptions,
) -> Result<Vec<HybridSearchResult>, SearchError> {
    // Load query grading features
    let query_features = load_grading_features(conn, query_file_id)?;

    // Load query CLIP embedding
    let query_embedding: Option<Vec<f64>> = conn
        .query_row(
            "SELECT embedding FROM fingerprints WHERE file_id = ?1",
            rusqlite::params![query_file_id],
            |row| row.get::<_, Option<Vec<u8>>>(0),
        )
        .ok()
        .and_then(|blob_opt| blob_opt.map(|b| deserialize_embedding(&b)))
        .and_then(|opt| opt);

    // Load query tags
    let query_fp_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM fingerprints WHERE file_id = ?1 LIMIT 1",
            rusqlite::params![query_file_id],
            |row| row.get::<_, i64>(0),
        )
        .ok();
    let query_tags: Vec<String> = match query_fp_id {
        Some(fp_id) => {
            let sql = "SELECT t.tag FROM tags t WHERE t.fingerprint_id = ?1";
            let mut stmt = conn
                .prepare(sql)
                .map_err(|e| SearchError::DatabaseError(e.to_string()))?;
            let rows: Vec<String> = stmt
                .query_map(rusqlite::params![fp_id], |row| row.get::<_, String>(0))
                .map_err(|e| SearchError::DatabaseError(e.to_string()))?
                .collect::<Result<_, _>>()
                .map_err(|e| SearchError::DatabaseError(e.to_string()))?;
            rows
        }
        None => vec![],
    };

    // Load all candidates with grading features
    let mut sql = String::from(
        "SELECT fp.id, fp.file_id, p.name, f.filename, f.format,
                fp.oklab_hist_l, fp.oklab_hist_a, fp.oklab_hist_b, fp.color_moments,
                fp.embedding
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

    let rows: Vec<(i64, i64, String, String, String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Option<Vec<u8>>)> =
        stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, i64>(0)?,   // fp.id
                row.get::<_, i64>(1)?,   // fp.file_id
                row.get::<_, String>(2)?, // p.name
                row.get::<_, String>(3)?, // f.filename
                row.get::<_, String>(4)?, // f.format
                row.get::<_, Vec<u8>>(5)?, // oklab_hist_l
                row.get::<_, Vec<u8>>(6)?, // oklab_hist_a
                row.get::<_, Vec<u8>>(7)?, // oklab_hist_b
                row.get::<_, Vec<u8>>(8)?, // color_moments
                row.get::<_, Option<Vec<u8>>>(9)?, // embedding
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

    // Collect fingerprint IDs and load tags in batch
    let fp_ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
    let tags_map = load_tags_batch(conn, &fp_ids)?;

    // Score each candidate
    let mut scored: Vec<(f64, hybrid_scoring::HybridScore, String, String, String)> = Vec::new();

    for (_fp_id, _file_id, project_name, filename, format, hist_l, hist_a, hist_b, moments, embedding_blob) in rows {
        // Deserialize grading features
        let candidate_features = match GradingFeatures::from_separate_blobs(&hist_l, &hist_a, &hist_b, &moments) {
            Ok(gf) => gf,
            Err(e) => {
                eprintln!("warning: skipping candidate {} ({}): grading feature decode failed: {}", project_name, filename, e);
                continue;
            }
        };

        // Compute grading similarity
        let grading_sim = match bhattacharyya_distance(&query_features, &candidate_features) {
            Ok(score) => Some(score),
            Err(e) => {
                eprintln!("warning: skipping candidate {} ({}): bhattacharyya_distance failed: {}", project_name, filename, e);
                continue;
            }
        };

        // Compute CLIP similarity (if both embeddings available)
        let clip_sim = match (&query_embedding, &embedding_blob) {
            (Some(ref q_emb), Some(ref c_blob)) => {
                if let Some(c_emb) = deserialize_embedding(c_blob) {
                    if q_emb.len() == c_emb.len() {
                        match hybrid_scoring::clip_similarity(q_emb, &c_emb) {
                            Ok(sim) => Some(sim),
                            Err(_) => None, // Skip on dimension mismatch
                        }
                    } else {
                        None // Dimension mismatch, skip CLIP signal
                    }
                } else {
                    None // Malformed blob, skip CLIP signal
                }
            }
            _ => None, // Missing embedding on either side
        };

        // Compute tag similarity
        let candidate_tags = tags_map.get(&_fp_id).cloned().unwrap_or_default();
        let tag_sim = if query_tags.is_empty() || candidate_tags.is_empty() {
            None
        } else {
            Some(hybrid_scoring::tag_similarity(&query_tags, &candidate_tags))
        };

        // Compute hybrid score
        match hybrid_scoring::compute_hybrid_score(grading_sim, clip_sim, tag_sim, weights) {
            Ok(hybrid) => scored.push((hybrid.final_score, hybrid, project_name, filename, format)),
            Err(_) => continue,
        }
    }

    if scored.is_empty() {
        return Err(SearchError::NoFingerprints);
    }

    // Sort by score descending
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(options.limit);

    let results: Vec<HybridSearchResult> = scored
        .into_iter()
        .enumerate()
        .map(|(i, (score, hybrid, project_name, file_path, file_format))| HybridSearchResult {
            rank: i + 1,
            project_name,
            file_path,
            file_format,
            score,
            score_breakdown: hybrid.breakdown,
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
             CREATE TABLE fingerprints (id INTEGER PRIMARY KEY AUTOINCREMENT, file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE, histogram_r TEXT NOT NULL, histogram_g TEXT NOT NULL, histogram_b TEXT NOT NULL, luminance_mean REAL NOT NULL, luminance_stddev REAL NOT NULL, color_space_tag TEXT NOT NULL, embedding BLOB, embedding_model TEXT, oklab_hist_l BLOB, oklab_hist_a BLOB, oklab_hist_b BLOB, color_moments BLOB, feature_status TEXT, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, fingerprint_id INTEGER NOT NULL REFERENCES fingerprints(id) ON DELETE CASCADE, tag TEXT NOT NULL, created_at INTEGER NOT NULL DEFAULT (unixepoch()));
             CREATE UNIQUE INDEX idx_tags_fingerprint_tag ON tags(fingerprint_id, tag);",
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
            SearchError::ProjectNotFound(name) => assert_eq!(name, "test"),
            other => panic!("Expected ProjectNotFound, got: {:?}", other),
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

        let err = SearchError::ProjectNotFound("test_proj".to_string());
        assert!(format!("{}", err).contains("SEARCH_PROJECT_NOT_FOUND"));
    }

    #[test]
    fn test_parse_histogram_rejects_nan() {
        let mut hist_parts: Vec<String> = (0..63).map(|_| "0.1".to_string()).collect();
        hist_parts.push("NaN".to_string());
        let hist = hist_parts.join(",");
        let result = parse_histogram(&hist);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("non-finite"));
    }

    #[test]
    fn test_parse_histogram_rejects_infinity() {
        let mut hist_parts: Vec<String> = (0..63).map(|_| "0.1".to_string()).collect();
        hist_parts.push("inf".to_string());
        let hist = hist_parts.join(",");
        let result = parse_histogram(&hist);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("non-finite"));
    }

    #[test]
    fn test_search_project_not_found() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('other', '/tmp/other')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        )
        .unwrap();

        // Search for a project that exists but has no fingerprints
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: Some("nonexistent".to_string()),
                limit: 5,
            },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::ProjectNotFound(name) => assert_eq!(name, "nonexistent"),
            other => panic!("Expected ProjectNotFound, got: {:?}", other),
        }

        // Global search should still work
        let result = search_histograms(
            &conn,
            &SearchOptions {
                project: None,
                limit: 5,
            },
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }

    // --- Cosine similarity tests ---

    #[test]
    fn test_cosine_similarity_identical() {
        let a: Vec<f64> = vec![0.1, 0.2, 0.3, 0.4];
        let score = cosine_similarity(&a, &a);
        assert!((score - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a: Vec<f64> = vec![1.0, 0.0];
        let b: Vec<f64> = vec![0.0, 1.0];
        let score = cosine_similarity(&a, &b);
        assert!(score.abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a: Vec<f64> = vec![1.0, 0.0];
        let b: Vec<f64> = vec![-1.0, 0.0];
        let score = cosine_similarity(&a, &b);
        assert!((score - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_zero_vector() {
        let a: Vec<f64> = vec![0.0, 0.0, 0.0];
        let b: Vec<f64> = vec![1.0, 2.0, 3.0];
        let score = cosine_similarity(&a, &b);
        assert!(score.abs() < 1e-10);
    }

    #[test]
    fn test_cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched_dims() {
        let a: Vec<f64> = vec![1.0, 2.0];
        let b: Vec<f64> = vec![1.0, 2.0, 3.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_cosine_similarity_positive() {
        let a: Vec<f64> = vec![1.0, 0.0, 0.0];
        let b: Vec<f64> = vec![0.707, 0.707, 0.0];
        let score = cosine_similarity(&a, &b);
        assert!(score > 0.0);
        assert!(score < 1.0);
        assert!((score - 0.70710678).abs() < 1e-5);
    }

    // --- Embedding serialization tests ---

    #[test]
    fn test_embedding_roundtrip() {
        let original: Vec<f64> = vec![0.1, 0.2, 0.3, 0.4, -0.5, 1.0];
        let bytes = serialize_embedding(&original);
        assert_eq!(bytes.len(), original.len() * 4); // f32 = 4 bytes
        let restored = deserialize_embedding(&bytes).unwrap();
        assert_eq!(restored.len(), original.len());
        for (orig, rest) in original.iter().zip(restored.iter()) {
            assert!((*orig - *rest).abs() < 1e-6); // f32 precision
        }
    }

    #[test]
    fn test_embedding_roundtrip_empty() {
        let original: Vec<f64> = vec![];
        let bytes = serialize_embedding(&original);
        assert!(bytes.is_empty());
        let restored = deserialize_embedding(&bytes).unwrap();
        assert!(restored.is_empty());
    }

    #[test]
    fn test_embedding_roundtrip_single() {
        let original: Vec<f64> = vec![42.0];
        let bytes = serialize_embedding(&original);
        assert_eq!(bytes.len(), 4);
        let restored = deserialize_embedding(&bytes).unwrap();
        assert_eq!(restored.len(), 1);
        assert!((restored[0] - 42.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_deserialize_truncated_blob() {
        // 5 bytes — not a multiple of 4
        let blob = vec![0x00, 0x00, 0x80, 0x3f, 0xFF];
        assert!(deserialize_embedding(&blob).is_none());
    }

    #[test]
    fn test_embedding_deserialize_one_byte() {
        let blob = vec![0x42];
        assert!(deserialize_embedding(&blob).is_none());
    }

    // --- SearchError new variants ---

    #[test]
    fn test_search_error_ai_unavailable() {
        let err = SearchError::AiUnavailable("Python not installed".to_string());
        assert_eq!(
            format!("{}", err),
            "SEARCH_AI_UNAVAILABLE -- Python not installed"
        );
    }

    #[test]
    fn test_search_error_embedding_error() {
        let err = SearchError::EmbeddingError("model failed".to_string());
        assert_eq!(
            format!("{}", err),
            "SEARCH_EMBEDDING_ERROR -- model failed"
        );
    }

    // --- Fingerprint info tests ---

    #[test]
    fn test_fingerprint_info_valid() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.02), make_histogram_csv(0.01)],
        ).unwrap();

        let info = fingerprint_info(&conn, "film", "scene.dpx").unwrap();
        assert_eq!(info.project_name, "film");
        assert_eq!(info.file_path, "scene.dpx");
        assert_eq!(info.file_format, "dpx");
        assert_eq!(info.color_space_tag, "acescg");
        assert!((info.luminance_mean - 0.5).abs() < 1e-10);
        assert!((info.luminance_stddev - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_fingerprint_info_not_found() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'scene.dpx', 'dpx')", [])
            .unwrap();

        let result = fingerprint_info(&conn, "film", "nonexistent.dpx");
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::ProjectNotFound(msg) => {
                assert!(msg.contains("nonexistent.dpx"));
            }
            other => panic!("Expected ProjectNotFound, got: {:?}", other),
        }
    }

    #[test]
    fn test_summarize_histogram() {
        // Uniform histogram
        let uniform: Vec<f64> = vec![1.0 / 64.0; 64];
        let summary = summarize_histogram(&uniform);
        assert!((summary.mean_value - 1.0 / 64.0).abs() < 1e-10);

        // Histogram with a dominant bin
        let mut hist = vec![0.0; 64];
        hist[10] = 0.5;
        let summary = summarize_histogram(&hist);
        assert_eq!(summary.dominant_bin_min, 10);
        assert_eq!(summary.dominant_bin_max, 10);
    }

    // --- Tag search tests ---

    #[test]
    fn test_search_by_tag_basic() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's2.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        // Tag s1 with "warm"
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'warm')", [])
            .unwrap();
        // Tag s2 with "warm" and "industrial"
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'warm')", [])
            .unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'industrial')", [])
            .unwrap();

        // Search for single tag "warm" — both match 1 tag, so both score 1.0
        let results = search_by_tag(
            &conn,
            "warm",
            &SearchOptions {
                project: None,
                limit: 10,
            },
        )
        .unwrap();
        assert_eq!(results.len(), 2);

        // Search for multi-tag "warm industrial" — s2 matches 2, s1 matches 1
        let results = search_by_tag(
            &conn,
            "warm industrial",
            &SearchOptions {
                project: None,
                limit: 10,
            },
        )
        .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].file_path, "s2.dpx");
        assert!((results[0].score - 1.0).abs() < 1e-10); // 2/2 = 1.0
        assert_eq!(results[1].file_path, "s1.dpx");
        assert!((results[1].score - 0.5).abs() < 1e-10); // 1/2 = 0.5
    }

    #[test]
    fn test_search_by_tag_no_results() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();

        let result = search_by_tag(
            &conn,
            "nonexistent",
            &SearchOptions {
                project: None,
                limit: 10,
            },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints => {}
            other => panic!("Expected NoFingerprints, got: {:?}", other),
        }
    }

    #[test]
    fn test_search_by_tag_project_scoped() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_b', '/tmp/b')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 's1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (2, 's2.dpx', 'dpx')", [])
            .unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (1, 'warm')", [])
            .unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'warm')", [])
            .unwrap();

        let results = search_by_tag(
            &conn,
            "warm",
            &SearchOptions {
                project: Some("film_a".to_string()),
                limit: 10,
            },
        )
        .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_name, "film_a");
    }

    // --- Hybrid search tests ---

    #[test]
    fn test_hybrid_search_all_signals() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Query fingerprint with grading features and embedding
        let query_emb = serialize_embedding(&vec![0.1, 0.2, 0.3, 0.4]);
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, embedding, embedding_model) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, ?, 'test-model')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m, query_emb],
        ).unwrap();

        // Candidate with grading features, embedding, and tags
        let cand_emb = serialize_embedding(&vec![0.1, 0.2, 0.3, 0.4]);
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, embedding, embedding_model) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?, ?, 'test-model')",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m, cand_emb],
        ).unwrap();
        // Add tags to candidate
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'warm')", [])
            .unwrap();

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 10 },
        ).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rank, 1);
        assert!(results[0].score > 0.0);
        // With identical features, grading_sim ~ 1.0
        assert!(results[0].score_breakdown.grading > 0.9);
        // CLIP: identical embeddings -> cos_sim = 1.0 -> normalized = 1.0
        assert!(results[0].score_breakdown.clip.is_some());
        assert!((results[0].score_breakdown.clip.unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_hybrid_search_missing_clip() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Query without embedding
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Candidate without embedding but with tags
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute("INSERT INTO tags (fingerprint_id, tag) VALUES (2, 'warm')", [])
            .unwrap();

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 10 },
        ).unwrap();

        assert_eq!(results.len(), 1);
        // CLIP should be None (omitted)
        assert_eq!(results[0].score_breakdown.clip, None);
        // Grading should be present (identical features)
        assert!(results[0].score_breakdown.grading > 0.9);
    }

    #[test]
    fn test_hybrid_search_missing_clip_and_tag() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Both without embedding or tags
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 10 },
        ).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].score_breakdown.clip, None);
        assert_eq!(results[0].score_breakdown.tag, None);
        // Only grading, weight = 1.0, identical features → score ~ 1.0
        assert!(results[0].score > 0.9);
    }

    #[test]
    fn test_hybrid_search_ranking() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'near.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'far.dpx', 'dpx')", [])
            .unwrap();

        // Query: uniform features
        let query_gf = GradingFeatures {
            hist_l: vec![50.0; 64],
            hist_a: vec![25.0; 64],
            hist_b: vec![25.0; 64],
            moments: [0.5, 0.1, 0.0, 0.05, 0.0, 0.05],
        };
        let (qhl, qha, qhb, qm) = (query_gf.hist_l_blob(), query_gf.hist_a_blob(), query_gf.hist_b_blob(), query_gf.moments_blob());

        // Near: similar features
        let near_gf = GradingFeatures {
            hist_l: vec![55.0; 64],
            hist_a: vec![27.0; 64],
            hist_b: vec![27.0; 64],
            moments: [0.52, 0.11, 0.01, 0.05, 0.01, 0.05],
        };
        let (nhl, nha, nhb, nm) = (near_gf.hist_l_blob(), near_gf.hist_a_blob(), near_gf.hist_b_blob(), near_gf.moments_blob());

        // Far: very different features
        let far_gf = GradingFeatures {
            hist_l: vec![1000.0; 64],
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.9, 0.01, 0.0, 0.01, 0.0, 0.01],
        };
        let (fhl, fha, fhb, fm) = (far_gf.hist_l_blob(), far_gf.hist_a_blob(), far_gf.hist_b_blob(), far_gf.moments_blob());

        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), qhl, qha, qhb, qm],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), nhl, nha, nhb, nm],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (3, ?, ?, ?, 0.1, 0.3, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), fhl, fha, fhb, fm],
        ).unwrap();

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 10 },
        ).unwrap();

        assert_eq!(results.len(), 2);
        // "near" should rank higher than "far"
        assert!(results[0].score >= results[1].score,
            "Expected near (score={}) >= far (score={})", results[0].score, results[1].score);
    }

    #[test]
    fn test_hybrid_search_limit() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Add 5 identical candidates
        for i in 0..5 {
            conn.execute(
                &format!("INSERT INTO files (project_id, filename, format) VALUES (1, 'c{}.dpx', 'dpx')", i),
                [],
            ).unwrap();
            conn.execute(
                &format!("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES ({}, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)", i + 2),
                rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
            ).unwrap();
        }

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: None, limit: 2 },
        ).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_hybrid_search_project_scoped() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_a', '/tmp/a')", [])
            .unwrap();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film_b', '/tmp/b')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'c1.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (2, 'c2.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (3, ?, ?, ?, 0.3, 0.2, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let weights = SignalWeights {
            grading: 0.6,
            clip: 0.3,
            tag: 0.1,
        };

        // Search within film_a only
        let results = hybrid_search(
            &conn,
            1,
            &weights,
            &SearchOptions { project: Some("film_a".to_string()), limit: 10 },
        ).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_name, "film_a");
    }

    // --- Bhattacharyya search tests ---

    /// Helper to create grading feature BLOBs for test data (uniform histograms).
    fn make_grading_features_blob() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
        let gf = GradingFeatures {
            hist_l: vec![100.0; 64],
            hist_a: vec![50.0; 64],
            hist_b: vec![50.0; 64],
            moments: [0.5, 0.1, 0.0, 0.05, 0.0, 0.05],
        };
        (gf.hist_l_blob(), gf.hist_a_blob(), gf.hist_b_blob(), gf.moments_blob())
    }

    #[test]
    fn test_bhattacharyya_search_identical_features() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'candidate.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();

        // Both query and candidate have identical grading features
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        let results = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 10 },
        ).unwrap();

        assert_eq!(results.len(), 1);
        assert!((results[0].score - 1.0).abs() < 0.01, "Identical features should score near 1.0, got {}", results[0].score);
    }

    #[test]
    fn test_bhattacharyya_search_no_grading_features() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'q.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'c.dpx', 'dpx')", [])
            .unwrap();
        // Insert fingerprints without grading features (oklab_hist_l is NULL)
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg')",
            [make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625)],
        ).unwrap();

        let result = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 10 },
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SearchError::NoFingerprints | SearchError::DatabaseError(_) => {}
            other => panic!("Expected NoFingerprints or DatabaseError, got: {:?}", other),
        }
    }

    #[test]
    fn test_bhattacharyya_search_ranking_order() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'near.dpx', 'dpx')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'far.dpx', 'dpx')", [])
            .unwrap();

        // Query: bright image
        let query_gf = GradingFeatures {
            hist_l: vec![50.0; 64],
            hist_a: vec![25.0; 64],
            hist_b: vec![25.0; 64],
            moments: [0.5, 0.1, 0.0, 0.05, 0.0, 0.05],
        };
        let (qhl, qha, qhb, qm) = (query_gf.hist_l_blob(), query_gf.hist_a_blob(), query_gf.hist_b_blob(), query_gf.moments_blob());

        // Near candidate: similar features (same shape, slightly different counts)
        let near_gf = GradingFeatures {
            hist_l: vec![55.0; 64],
            hist_a: vec![27.0; 64],
            hist_b: vec![27.0; 64],
            moments: [0.52, 0.11, 0.01, 0.05, 0.01, 0.05],
        };
        let (nhl, nha, nhb, nm) = (near_gf.hist_l_blob(), near_gf.hist_a_blob(), near_gf.hist_b_blob(), near_gf.moments_blob());

        // Far candidate: very different features
        let far_gf = GradingFeatures {
            hist_l: vec![1000.0; 64], // Concentrated in one bin
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.9, 0.01, 0.0, 0.01, 0.0, 0.01],
        };
        let (fhl, fha, fhb, fm) = (far_gf.hist_l_blob(), far_gf.hist_a_blob(), far_gf.hist_b_blob(), far_gf.moments_blob());

        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), qhl, qha, qhb, qm],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (2, ?, ?, ?, 0.3, 0.2, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), nhl, nha, nhb, nm],
        ).unwrap();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (3, ?, ?, ?, 0.1, 0.3, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), fhl, fha, fhb, fm],
        ).unwrap();

        let results = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 10 },
        ).unwrap();

        assert_eq!(results.len(), 2);
        // "near" candidate should rank higher than "far"
        assert!(results[0].score >= results[1].score,
            "Expected near (score={}) >= far (score={})", results[0].score, results[1].score);
    }

    #[test]
    fn test_bhattacharyya_search_limit() {
        let conn = setup_test_db();
        conn.execute("INSERT INTO projects (name, path) VALUES ('film', '/tmp/f')", [])
            .unwrap();
        conn.execute("INSERT INTO files (project_id, filename, format) VALUES (1, 'query.dpx', 'dpx')", [])
            .unwrap();

        let (hl, ha, hb, m) = make_grading_features_blob();
        conn.execute(
            "INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES (1, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)",
            rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
        ).unwrap();

        // Add 5 identical candidates
        for i in 0..5 {
            conn.execute(
                &format!("INSERT INTO files (project_id, filename, format) VALUES (1, 'c{}.dpx', 'dpx')", i),
                [],
            ).unwrap();
            conn.execute(
                &format!("INSERT INTO fingerprints (file_id, histogram_r, histogram_g, histogram_b, luminance_mean, luminance_stddev, color_space_tag, oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments) VALUES ({}, ?, ?, ?, 0.5, 0.1, 'acescg', ?, ?, ?, ?)", i + 2),
                rusqlite::params![make_histogram_csv(0.015625), make_histogram_csv(0.015625), make_histogram_csv(0.015625), hl, ha, hb, m],
            ).unwrap();
        }

        let results = bhattacharyya_search(
            &conn,
            1,
            &SearchOptions { project: None, limit: 2 },
        ).unwrap();
        assert_eq!(results.len(), 2);
    }
}
