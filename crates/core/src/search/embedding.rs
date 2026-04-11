// embedding.rs — Embedding serialization helpers

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
    if !blob.len().is_multiple_of(4) {
        return None;
    }
    Some(
        blob.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_embedding() {
        let original = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let serialized = serialize_embedding(&original);
        let deserialized = deserialize_embedding(&serialized).unwrap();
        assert_eq!(deserialized.len(), original.len());
        for (a, b) in original.iter().zip(deserialized.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn test_deserialize_invalid_length() {
        let invalid_blob = vec![0u8, 1, 2]; // Not multiple of 4
        assert!(deserialize_embedding(&invalid_blob).is_none());
    }
}
