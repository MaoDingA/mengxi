// vector_index.rs — HNSW vector index for approximate nearest neighbor search on CLIP embeddings

use instant_distance::{Builder, HnswMap, Point, Search};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Minimum number of embeddings to justify HNSW index usage.
/// Below this threshold, brute-force linear scan is faster.
const MIN_INDEX_SIZE: usize = 1000;

/// HNSW ef_search parameter — controls search quality vs speed trade-off.
const DEFAULT_EF_SEARCH: usize = 500;

/// HNSW ef_construction parameter — controls build quality.
const DEFAULT_EF_CONSTRUCTION: usize = 150;

/// Wrapper for f32 vector implementing the Point trait with cosine distance.
#[derive(Clone, Serialize, Deserialize)]
struct EmbeddingPoint(Vec<f32>);

impl Point for EmbeddingPoint {
    fn distance(&self, other: &Self) -> f32 {
        let a = &self.0;
        let b = &other.0;
        if a.len() != b.len() || a.is_empty() {
            return 2.0; // max cosine distance for incompatible dimensions
        }
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 2.0;
        }
        1.0 - dot / (norm_a * norm_b)
    }
}

/// Errors from vector index operations.
#[derive(Debug, thiserror::Error)]
pub enum VectorIndexError {
    /// Database query failed.
    #[error("VECTOR_INDEX_DB_ERROR -- {0}")]
    DbError(String),
    /// File I/O error (read/write index file).
    #[error("VECTOR_INDEX_IO_ERROR -- {0}")]
    IoError(String),
    /// Serialization/deserialization error.
    #[error("VECTOR_INDEX_SERIALIZE_ERROR -- {0}")]
    SerializeError(String),
}

/// HNSW vector index for approximate nearest neighbor search on CLIP embeddings.
///
/// Wraps `instant_distance::HnswMap` and provides build/search/persist operations.
/// Falls back to linear scan for datasets with fewer than 1000 embeddings.
///
/// The index maps embedding vectors to fingerprint IDs and supports:
/// - Fast ANN search (O(log n) instead of O(n) linear scan)
/// - Binary persistence alongside the SQLite database
/// - Automatic staleness detection and rebuild
pub struct VectorIndex {
    map: HnswMap<EmbeddingPoint, i64>,
    /// Number of fingerprints with embeddings when the index was built.
    fingerprint_count: usize,
}

impl VectorIndex {
    /// Build a new index from all embeddings in the database.
    pub fn build_from_db(conn: &Connection) -> Result<Self, VectorIndexError> {
        let mut stmt = conn
            .prepare(
                "SELECT fp.id, fp.embedding FROM fingerprints fp WHERE fp.embedding IS NOT NULL",
            )
            .map_err(|e| VectorIndexError::DbError(e.to_string()))?;

        let rows: Vec<(i64, Vec<u8>)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                ))
            })
            .map_err(|e| VectorIndexError::DbError(e.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| VectorIndexError::DbError(e.to_string()))?;

        let mut points = Vec::with_capacity(rows.len());
        let mut values = Vec::with_capacity(rows.len());

        for (fp_id, blob) in &rows {
            if let Some(embedding) = deserialize_embedding_f32(blob) {
                points.push(EmbeddingPoint(embedding));
                values.push(*fp_id);
            }
        }

        let map = Builder::default()
            .ef_search(DEFAULT_EF_SEARCH)
            .ef_construction(DEFAULT_EF_CONSTRUCTION)
            .build(points, values);

        Ok(Self {
            map,
            fingerprint_count: rows.len(),
        })
    }

    /// Search for k nearest neighbors by embedding similarity.
    ///
    /// Returns `(fingerprint_id, cosine_distance)` pairs, sorted by distance (nearest first).
    /// Cosine distance = 1 - cosine_similarity, range [0, 2] where 0 = identical.
    pub fn search(&self, query_embedding: &[f64], k: usize) -> Vec<(i64, f32)> {
        let query_f32: Vec<f32> = query_embedding.iter().map(|&v| v as f32).collect();
        let query_point = EmbeddingPoint(query_f32);
        let mut search_state = Search::default();
        self.map
            .search(&query_point, &mut search_state)
            .take(k)
            .map(|item| (*item.value, item.distance))
            .collect()
    }

    /// Number of embeddings in the index.
    pub fn len(&self) -> usize {
        self.fingerprint_count
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.fingerprint_count == 0
    }

    /// Whether HNSW should be used (only for datasets >= MIN_INDEX_SIZE).
    pub fn should_use_hnsw(&self) -> bool {
        self.fingerprint_count >= MIN_INDEX_SIZE
    }

    /// Count embeddings currently in the database.
    pub fn count_db_embeddings(conn: &Connection) -> Result<usize, VectorIndexError> {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM fingerprints WHERE embedding IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .map_err(|e| VectorIndexError::DbError(e.to_string()))?;
        Ok(count as usize)
    }

    /// Check if the index is stale (DB has different count than index).
    pub fn is_stale(&self, conn: &Connection) -> Result<bool, VectorIndexError> {
        let db_count = Self::count_db_embeddings(conn)?;
        Ok(db_count != self.fingerprint_count)
    }

    /// Save index to a binary file.
    pub fn save(&self, path: &Path) -> Result<(), VectorIndexError> {
        let bytes = bincode::serialize(&self.map)
            .map_err(|e| VectorIndexError::SerializeError(e.to_string()))?;
        std::fs::write(path, bytes)
            .map_err(|e| VectorIndexError::IoError(e.to_string()))?;
        Ok(())
    }

    /// Load index from a binary file.
    pub fn load(path: &Path) -> Result<Self, VectorIndexError> {
        let bytes = std::fs::read(path)
            .map_err(|e| VectorIndexError::IoError(e.to_string()))?;
        let map: HnswMap<EmbeddingPoint, i64> = bincode::deserialize(&bytes)
            .map_err(|e| VectorIndexError::SerializeError(e.to_string()))?;
        let fingerprint_count = map.values.len();
        Ok(Self {
            map,
            fingerprint_count,
        })
    }

    /// Get or build a fresh index.
    ///
    /// Tries to load from file first. If the file doesn't exist or the index is stale
    /// (DB embedding count changed), rebuilds from DB and saves.
    pub fn get_or_build(conn: &Connection, index_path: &Path) -> Result<Self, VectorIndexError> {
        if index_path.exists() {
            match Self::load(index_path) {
                Ok(index) => {
                    if !index.is_stale(conn)? {
                        return Ok(index);
                    }
                    // Stale — fall through to rebuild
                }
                Err(_) => {
                    // Corrupt or incompatible — fall through to rebuild
                }
            }
        }

        let index = Self::build_from_db(conn)?;
        if let Err(e) = index.save(index_path) {
            eprintln!("Warning: failed to save vector index: {}", e);
        }
        Ok(index)
    }
}

/// Deserialize f32 little-endian BLOB to Vec<f32>.
fn deserialize_embedding_f32(blob: &[u8]) -> Option<Vec<f32>> {
    if !blob.len().is_multiple_of(4) || blob.is_empty() {
        return None;
    }
    Some(
        blob.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_point_distance_identical() {
        let a = EmbeddingPoint(vec![1.0, 0.0, 0.0]);
        assert!(a.distance(&a).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_point_distance_orthogonal() {
        let a = EmbeddingPoint(vec![1.0, 0.0, 0.0]);
        let b = EmbeddingPoint(vec![0.0, 1.0, 0.0]);
        assert!((a.distance(&b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_point_distance_opposite() {
        let a = EmbeddingPoint(vec![1.0, 0.0, 0.0]);
        let b = EmbeddingPoint(vec![-1.0, 0.0, 0.0]);
        assert!((a.distance(&b) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_point_distance_different_lengths() {
        let a = EmbeddingPoint(vec![1.0, 0.0]);
        let b = EmbeddingPoint(vec![1.0, 0.0, 0.0]);
        assert_eq!(a.distance(&b), 2.0);
    }

    #[test]
    fn test_embedding_point_distance_empty() {
        let a = EmbeddingPoint(vec![]);
        let b = EmbeddingPoint(vec![]);
        assert_eq!(a.distance(&b), 2.0);
    }

    #[test]
    fn test_deserialize_embedding_f32_valid() {
        let blob: Vec<u8> = vec![
            0x00, 0x00, 0x80, 0x3f, // 1.0 f32 LE
            0x00, 0x00, 0x00, 0x00, // 0.0 f32 LE
            0x00, 0x00, 0x00, 0x40, // 2.0 f32 LE
        ];
        let result = deserialize_embedding_f32(&blob).unwrap();
        assert_eq!(result.len(), 3);
        assert!((result[0] - 1.0).abs() < 1e-6);
        assert!(result[1].abs() < 1e-6);
        assert!((result[2] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_deserialize_embedding_f32_invalid_length() {
        let blob = vec![0x00, 0x00, 0x80]; // 3 bytes
        assert!(deserialize_embedding_f32(&blob).is_none());
    }

    #[test]
    fn test_deserialize_embedding_f32_empty() {
        assert!(deserialize_embedding_f32(&[]).is_none());
    }

    fn build_test_index(n: usize) -> VectorIndex {
        let points: Vec<EmbeddingPoint> = (0..n)
            .map(|i| EmbeddingPoint(vec![i as f32, 0.0, 0.0]))
            .collect();
        let values: Vec<i64> = (100..100 + n as i64).collect();
        let map = Builder::default()
            .seed(42)
            .ef_search(n.min(DEFAULT_EF_SEARCH))
            .ef_construction(n.min(DEFAULT_EF_CONSTRUCTION))
            .build(points, values);
        VectorIndex {
            map,
            fingerprint_count: n,
        }
    }

    #[test]
    fn test_vector_index_build_and_search() {
        let index = build_test_index(50);
        // Query near point 5 (fingerprint_id = 105)
        let query = vec![5.1_f64, 0.0, 0.0];
        let results = index.search(&query, 3);
        assert!(!results.is_empty());
        // HNSW is approximate; nearest should be within a few positions of 105
        let closest_id = results[0].0;
        assert!(
            (closest_id - 105).unsigned_abs() <= 2,
            "expected close to 105, got {}",
            closest_id
        );
    }

    #[test]
    fn test_vector_index_search_returns_sorted_by_distance() {
        let index = build_test_index(20);
        let query = vec![5.0_f64, 0.0, 0.0];
        let results = index.search(&query, 5);
        // Results should be sorted by distance (ascending)
        for i in 1..results.len() {
            assert!(results[i - 1].1 <= results[i].1);
        }
    }

    #[test]
    fn test_vector_index_should_use_hnsw_small() {
        let index = build_test_index(10);
        assert!(!index.should_use_hnsw()); // < 1000
    }

    #[test]
    fn test_vector_index_should_use_hnsw_large() {
        let n = 1200;
        let points: Vec<EmbeddingPoint> = (0..n)
            .map(|i| EmbeddingPoint(vec![i as f32 / n as f32, 0.0, 0.0]))
            .collect();
        let values: Vec<i64> = (0..n as i64).collect();
        let map = Builder::default().build(points, values);
        let index = VectorIndex {
            map,
            fingerprint_count: n,
        };
        assert!(index.should_use_hnsw()); // >= 1000
    }

    #[test]
    fn test_vector_index_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test_index.bin");

        let original = build_test_index(20);
        let query = vec![5.1_f64, 0.0, 0.0];
        let orig_results = original.search(&query, 3);

        original.save(&path).unwrap();
        assert!(path.exists());

        let loaded = VectorIndex::load(&path).unwrap();
        assert_eq!(loaded.fingerprint_count, 20);
        assert_eq!(loaded.len(), 20);

        let load_results = loaded.search(&query, 3);
        assert_eq!(orig_results, load_results);
    }

    #[test]
    fn test_vector_index_load_nonexistent_file() {
        let result = VectorIndex::load(Path::new("/nonexistent/index.bin"));
        match result {
            Err(e) => assert!(e.to_string().contains("VECTOR_INDEX_IO_ERROR")),
            Ok(_) => panic!("Expected error for nonexistent file"),
        }
    }

    #[test]
    fn test_vector_index_get_or_build_creates_new() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("new_index.bin");

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE fingerprints (id INTEGER PRIMARY KEY, embedding BLOB);",
        )
        .unwrap();

        // Insert test embeddings
        let e1: Vec<u8> = [1.0_f32, 0.0, 0.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let e2: Vec<u8> = [0.0_f32, 1.0, 0.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        conn.execute("INSERT INTO fingerprints (id, embedding) VALUES (1, ?1)", [&e1])
            .unwrap();
        conn.execute("INSERT INTO fingerprints (id, embedding) VALUES (2, ?1)", [&e2])
            .unwrap();

        assert!(!index_path.exists());

        let index = VectorIndex::get_or_build(&conn, &index_path).unwrap();
        assert_eq!(index.len(), 2);
        assert!(index_path.exists()); // Should have been saved
    }

    #[test]
    fn test_vector_index_get_or_build_reuses_valid() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("existing_index.bin");

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE fingerprints (id INTEGER PRIMARY KEY, embedding BLOB);",
        )
        .unwrap();

        let e1: Vec<u8> = [1.0_f32, 0.0, 0.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        conn.execute("INSERT INTO fingerprints (id, embedding) VALUES (1, ?1)", [&e1])
            .unwrap();

        // Build and save
        let index1 = VectorIndex::build_from_db(&conn).unwrap();
        index1.save(&index_path).unwrap();

        // get_or_build should reuse the saved index
        let index2 = VectorIndex::get_or_build(&conn, &index_path).unwrap();
        assert_eq!(index2.len(), 1);
    }

    #[test]
    fn test_vector_index_get_or_build_rebuilds_when_stale() {
        let dir = tempfile::tempdir().unwrap();
        let index_path = dir.path().join("stale_index.bin");

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE fingerprints (id INTEGER PRIMARY KEY, embedding BLOB);",
        )
        .unwrap();

        // Build with 1 embedding
        let e1: Vec<u8> = [1.0_f32, 0.0, 0.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        conn.execute("INSERT INTO fingerprints (id, embedding) VALUES (1, ?1)", [&e1])
            .unwrap();
        let index1 = VectorIndex::build_from_db(&conn).unwrap();
        index1.save(&index_path).unwrap();

        // Add another embedding (index is now stale)
        let e2: Vec<u8> = [0.0_f32, 1.0, 0.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        conn.execute("INSERT INTO fingerprints (id, embedding) VALUES (2, ?1)", [&e2])
            .unwrap();

        let index2 = VectorIndex::get_or_build(&conn, &index_path).unwrap();
        assert_eq!(index2.len(), 2); // Rebuilt with 2 embeddings
    }

    #[test]
    fn test_vector_index_empty_db() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE fingerprints (id INTEGER PRIMARY KEY, embedding BLOB);",
        )
        .unwrap();

        let index = VectorIndex::build_from_db(&conn).unwrap();
        assert!(index.is_empty());
        assert!(!index.should_use_hnsw());
    }

    #[test]
    fn test_vector_index_error_display() {
        let err = VectorIndexError::DbError("test".to_string());
        assert!(err.to_string().contains("VECTOR_INDEX_DB_ERROR"));

        let err = VectorIndexError::IoError("test".to_string());
        assert!(err.to_string().contains("VECTOR_INDEX_IO_ERROR"));

        let err = VectorIndexError::SerializeError("test".to_string());
        assert!(err.to_string().contains("VECTOR_INDEX_SERIALIZE_ERROR"));
    }

    #[test]
    fn test_count_db_embeddings() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE fingerprints (id INTEGER PRIMARY KEY, embedding BLOB);",
        )
        .unwrap();

        assert_eq!(VectorIndex::count_db_embeddings(&conn).unwrap(), 0);

        let e: Vec<u8> = [1.0_f32, 0.0, 0.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        conn.execute("INSERT INTO fingerprints (id, embedding) VALUES (1, ?1)", [&e])
            .unwrap();
        assert_eq!(VectorIndex::count_db_embeddings(&conn).unwrap(), 1);
    }

    #[test]
    fn test_is_stale() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE fingerprints (id INTEGER PRIMARY KEY, embedding BLOB);",
        )
        .unwrap();

        let e: Vec<u8> = [1.0_f32, 0.0, 0.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        conn.execute("INSERT INTO fingerprints (id, embedding) VALUES (1, ?1)", [&e])
            .unwrap();

        let index = VectorIndex::build_from_db(&conn).unwrap();
        assert!(!index.is_stale(&conn).unwrap()); // Freshly built, not stale

        // Add another embedding
        let e2: Vec<u8> = [0.0_f32, 1.0, 0.0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        conn.execute("INSERT INTO fingerprints (id, embedding) VALUES (2, ?1)", [&e2])
            .unwrap();
        assert!(index.is_stale(&conn).unwrap()); // Now stale (1 in index, 2 in DB)
    }
}
