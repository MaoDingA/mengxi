// image_search.rs — Image-based search using embeddings

use rusqlite::Connection;

use crate::python_bridge::{AiError, PythonBridge};
use crate::vector_index::VectorIndex;

use super::types::{FingerprintSearchRowWithId, SearchError, SearchResult, SearchOptions};
use super::embedding::deserialize_embedding;
use super::histogram_utils::cosine_similarity;
use super::histogram_search::search_histograms;

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

    // Step 2: Try to use HNSW for pre-filtering
    let db_dir = crate::db::db_dir();
    let index_path = db_dir.join("vector_index.bin");

    let candidate_fp_ids: Option<Vec<i64>> = match VectorIndex::get_or_build(conn, &index_path) {
        Ok(idx) if idx.should_use_hnsw() => {
            // Use HNSW to get candidate IDs (limit * 10 for recall)
            let pre_filter_k = std::cmp::max(options.limit * 10, 500);
            let results = idx.search(&ref_embedding, pre_filter_k);
            if results.is_empty() {
                None
            } else {
                Some(results.into_iter().map(|(id, _)| id).collect())
            }
        }
        Ok(_) => {
            // Index exists but too small — fall back to full scan
            None
        }
        Err(e) => {
            eprintln!("Warning: HNSW index unavailable ({}), falling back to full scan", e);
            None
        }
    };

    // Step 3: Query fingerprints (either candidates or all)
    let mut sql = String::from(
        "SELECT p.name, f.filename, f.format,
                fp.id, fp.embedding, fp.embedding_model
         FROM fingerprints fp
         JOIN files f ON f.id = fp.file_id
         JOIN projects p ON p.id = f.project_id
         WHERE fp.embedding IS NOT NULL"
    );

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(ref ids) = candidate_fp_ids {
        let placeholders: Vec<String> = (0..ids.len()).map(|_| "?".to_string()).collect();
        sql.push_str(&format!(" AND fp.id IN ({})", placeholders.join(", ")));
        for id in ids {
            params.push(Box::new(*id));
        }
    }

    if options.project.is_some() {
        sql.push_str(" AND p.name = ?");
        params.push(Box::new(options.project.clone().unwrap()));
    }

    let mut stmt = conn.prepare(&sql).map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    let rows: Vec<FingerprintSearchRowWithId> =
        stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    if rows.is_empty() {
        return if options.project.is_some() {
            Err(SearchError::ProjectNotFound(options.project.clone().unwrap()))
        } else {
            Err(SearchError::NoFingerprints)
        };
    }

    // Step 4: Score each fingerprint
    let mut scored: Vec<(String, String, String, f64)> = Vec::new();

    for (project_name, file_name, file_format, _fp_id, embedding_blob, cached_model) in rows {
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
