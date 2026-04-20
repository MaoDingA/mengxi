// hybrid_search.rs — Hybrid search combining grading, CLIP, and tag signals

use rusqlite::Connection;
use std::collections::HashMap;

use crate::color_science::{bhattacharyya_distance, GradingFeatures};
use crate::hybrid_scoring::{self, HybridSearchResult, SignalWeights};
use crate::python_bridge::PythonBridge;
use crate::vector_index::VectorIndex;

use super::bhattacharyya_search::load_grading_features;
use super::embedding::deserialize_embedding;
use super::tag_search::search_by_tag;
use super::types::{CombinedSearchRow, SearchError, SearchOptions};
use super::histogram_utils::cosine_similarity;

/// Load tags for a batch of fingerprint IDs.
/// Returns a HashMap mapping fingerprint_id -> Vec<tag>.
fn load_tags_batch(
    conn: &Connection,
    fingerprint_ids: &[i64],
) -> Result<HashMap<i64, Vec<String>>, SearchError> {
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
) -> Result<Vec<super::types::SearchResult>, SearchError> {
    use super::types::{TagSearchRow, SearchResult};
    
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

    let rows: Vec<TagSearchRow> = stmt
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

/// Load tags for a batch of fingerprint IDs.
/// Returns a HashMap mapping fingerprint_id -> Vec<tag>.

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
    hybrid_search_impl(conn, query_file_id, weights, options, None)
}

/// Hybrid search with optional HNSW vector index for embedding pre-filtering.
///
/// When a `VectorIndex` is provided and the query has an embedding, uses HNSW
/// to pre-filter candidates by embedding similarity before full hybrid scoring.
/// Falls back to linear scan when no index is available or dataset is small.
pub fn hybrid_search_with_index(
    conn: &Connection,
    query_file_id: i64,
    weights: &SignalWeights,
    options: &SearchOptions,
    vector_index: Option<&VectorIndex>,
) -> Result<Vec<HybridSearchResult>, SearchError> {
    hybrid_search_impl(conn, query_file_id, weights, options, vector_index)
}

fn hybrid_search_impl(
    conn: &Connection,
    query_file_id: i64,
    weights: &SignalWeights,
    options: &SearchOptions,
    vector_index: Option<&VectorIndex>,
) -> Result<Vec<HybridSearchResult>, SearchError> {
    // Load query grading features
    let query_features = load_grading_features(conn, query_file_id)?;

    // Load query color_space_tag
    let query_cs_tag: String = match conn.query_row(
        "SELECT color_space_tag FROM fingerprints WHERE file_id = ?1 LIMIT 1",
        rusqlite::params![query_file_id],
        |row| row.get::<_, String>(0),
    ) {
        Ok(tag) => tag,
        Err(_) => {
            eprintln!("warning: could not load color_space_tag for query file, using 'unknown'");
            "unknown".to_string()
        }
    };

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

    // Load query pyramid tiles (if pyramid mode enabled)
    let query_pyramid = if options.use_pyramid {
        query_fp_id.and_then(|fp_id| {
            let tiles = crate::db::load_fingerprint_tiles(conn, fp_id).ok()?;
            if tiles.is_empty() { return None; }
            Some(crate::spatial_pyramid::build_spatial_pyramid_from_tiles(&tiles))
        })
    } else {
        None
    };

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

    // Decide whether to use HNSW pre-filtering
    let hnsw_candidate_ids: Option<Vec<i64>> = match (&query_embedding, vector_index) {
        (Some(ref q_emb), Some(idx)) if idx.should_use_hnsw() => {
            let pre_filter_k = std::cmp::max(options.limit * 5, 200);
            let results = idx.search(q_emb, pre_filter_k);
            if results.is_empty() {
                None
            } else {
                Some(results.into_iter().map(|(id, _)| id).collect())
            }
        }
        _ => None,
    };

    // Build candidate SQL query
    let mut sql = String::from(
        "SELECT fp.id, fp.file_id, p.name, f.filename, f.format,
                fp.oklab_hist_l, fp.oklab_hist_a, fp.oklab_hist_b, fp.color_moments,
                COALESCE(fp.hist_bins, 64),
                fp.embedding, fp.color_space_tag, fp.feature_status
         FROM fingerprints fp
         JOIN files f ON f.id = fp.file_id
         JOIN projects p ON p.id = f.project_id
         WHERE fp.oklab_hist_l IS NOT NULL AND fp.oklab_hist_a IS NOT NULL
               AND fp.oklab_hist_b IS NOT NULL AND fp.color_moments IS NOT NULL
               AND fp.file_id != ?1"
    );

    if let Some(ref ids) = hnsw_candidate_ids {
        let placeholders: Vec<String> = (0..ids.len()).map(|_| "?".to_string()).collect();
        sql.push_str(&format!(" AND fp.id IN ({})", placeholders.join(", ")));
    }

    if options.project.is_some() {
        sql.push_str(" AND p.name = ?2");
    }

    let mut stmt = conn.prepare(&sql).map_err(|e| SearchError::DatabaseError(e.to_string()))?;

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(query_file_id)];
    if let Some(ref ids) = hnsw_candidate_ids {
        for id in ids {
            params.push(Box::new(*id));
        }
    }
    if let Some(ref proj) = options.project {
        params.push(Box::new(proj.clone()));
    }

    let rows: Vec<CombinedSearchRow> =
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
                row.get::<_, i32>(9)?,   // hist_bins
                row.get::<_, Option<Vec<u8>>>(10)?, // embedding
                row.get::<_, String>(11)?, // color_space_tag
                row.get::<_, Option<String>>(12)?, // feature_status
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

    // First pass: collect stale fingerprint IDs for batch re-extraction
    let stale_fp_ids: Vec<(i64, String)> = rows.iter()
        .filter_map(|(fp_id, file_id, _project_name, _filename, _format, _hl, _ha, _hb, _moments, _bins, _emb, _cs_tag, feature_status)| {
            let is_stale = feature_status.is_none() || feature_status.as_deref() == Some("stale");
            if is_stale {
                let path = format!("file_id_{}", file_id);
                Some((*fp_id, path))
            } else {
                None
            }
        })
        .collect();

    // Batch recompute stale features — moved to CLI layer (project_ops.rs) in Phase 2a
    // to eliminate Core's dependency on Format crate for pixel I/O.
    // CLI should call project_ops::batch_reextract_grading_features() before search if needed.
    let recomputed_count = 0usize;

    // Second pass: reload updated BLOBs and score candidates
    let mut scored: Vec<(f64, hybrid_scoring::HybridScore, String, String, String, String, String)> = Vec::new();

    for (_fp_id, _file_id, project_name, filename, format, hist_l, hist_a, hist_b, moments, hist_bins_i32, embedding_blob, candidate_cs_tag, _feature_status) in rows {
        let hist_bins = hist_bins_i32 as usize;

        // Reload BLOBs if this fingerprint was among the recomputed ones
        let (hist_l, hist_a, hist_b, moments, hist_bins) = if recomputed_count > 0 && stale_fp_ids.iter().any(|(id, _)| *id == _fp_id) {
            match conn.query_row(
                "SELECT oklab_hist_l, oklab_hist_a, oklab_hist_b, color_moments, COALESCE(hist_bins, 64) FROM fingerprints WHERE id = ?1",
                rusqlite::params![_fp_id],
                |row| Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, i32>(4)?,
                )),
            ) {
                Ok((new_hl, new_ha, new_hb, new_m, new_bins)) => {
                    eprintln!("debug: using recomputed features for {} ({})", project_name, filename);
                    (new_hl, new_ha, new_hb, new_m, new_bins as usize)
                }
                Err(e) => {
                    eprintln!("warning: failed to reload recomputed features for {} ({}): {}, using original", project_name, filename, e);
                    (hist_l, hist_a, hist_b, moments, hist_bins)
                }
            }
        } else {
            (hist_l, hist_a, hist_b, moments, hist_bins)
        };

        // Deserialize grading features
        let candidate_features = match GradingFeatures::from_separate_blobs(&hist_l, &hist_a, &hist_b, &moments, hist_bins) {
            Ok(gf) => gf,
            Err(e) => {
                eprintln!("warning: skipping candidate {} ({}): grading feature decode failed: {}", project_name, filename, e);
                continue;
            }
        };

        // Compute grading similarity
        let grading_sim = if options.use_pyramid {
            if let Some(query_pyr) = query_pyramid.as_ref() {
                // Pyramid mode: use spatial pyramid comparison
                let candidate_tiles = crate::db::load_fingerprint_tiles(conn, _fp_id).unwrap_or_default();
                if candidate_tiles.is_empty() {
                    // Fallback to flat grading when candidate has no tiles
                    match bhattacharyya_distance(&query_features, &candidate_features) {
                        Ok(score) => Some(score),
                        Err(_) => continue,
                    }
                } else {
                    let candidate_pyr = crate::spatial_pyramid::build_spatial_pyramid_from_tiles(&candidate_tiles);
                    let result = crate::spatial_pyramid::compare_pyramids(query_pyr, &candidate_pyr);
                    Some(result.score)
                }
            } else {
                // Pyramid requested but no pyramid available — flat fallback
                match bhattacharyya_distance(&query_features, &candidate_features) {
                    Ok(score) => Some(score),
                    Err(_) => continue,
                }
            }
        } else {
            // Standard flat Bhattacharyya
            match bhattacharyya_distance(&query_features, &candidate_features) {
                Ok(score) => Some(score),
                Err(e) => {
                    eprintln!("warning: skipping candidate {} ({}): bhattacharyya_distance failed: {}", project_name, filename, e);
                    continue;
                }
            }
        };

        // Compute CLIP similarity (if both embeddings available)
        let clip_sim = match (&query_embedding, &embedding_blob) {
            (Some(ref q_emb), Some(ref c_blob)) => {
                if let Some(c_emb) = deserialize_embedding(c_blob) {
                    if q_emb.len() == c_emb.len() {
                        hybrid_scoring::clip_similarity(q_emb, &c_emb).ok()
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
        match hybrid_scoring::compute_hybrid_score(grading_sim, clip_sim, tag_sim, weights, &query_cs_tag, &candidate_cs_tag) {
            Ok(hybrid) => {
                let hr = crate::feature_translation::translate_features(&candidate_features);
                scored.push((hybrid.final_score, hybrid, project_name, filename, format, candidate_cs_tag, hr));
            }
            Err(_) => continue,
        }
    }

    if scored.is_empty() {
        return Ok(vec![]);
    }

    // Sort by score descending
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(options.limit);

    let results: Vec<HybridSearchResult> = scored
        .into_iter()
        .enumerate()
        .map(|(i, (score, hybrid, project_name, file_path, file_format, _cs_tag, human_readable))| HybridSearchResult {
            rank: i + 1,
            project_name,
            file_path,
            file_format,
            score,
            score_breakdown: hybrid.breakdown,
            match_warnings: hybrid.warnings,
            human_readable,
        })
        .collect();

    Ok(results)
}

#[cfg(test)]
#[path = "hybrid_search_tests.rs"]
mod tests;
