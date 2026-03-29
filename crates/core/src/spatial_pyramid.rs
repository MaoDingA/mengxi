// spatial_pyramid.rs — Spatial Pyramid Matching for grading feature comparison
//
// Encodes features at multiple spatial resolutions (1x1, 2x2, 4x4) and
// computes a weighted similarity score across all levels using standard SPM
// weighting: 1/4 for level 0 (1x1), 1/4 for level 1 (2x2), 1/2 for level 2 (4x4).
//
// Reference: Lazebnik et al., "Beyond Bags of Features: Spatial Pyramid
// Matching for Recognizing Natural Scene Categories", CVPR 2006.

use crate::color_science::bhattacharyya_distance;
use crate::feature_pipeline::{self, TileFeatures};
use crate::grading_features::GradingFeatures;

// ---------------------------------------------------------------------------
// Pyramid structure
// ---------------------------------------------------------------------------

/// A spatial pyramid of grading features at multiple resolutions.
#[derive(Debug, Clone)]
pub struct SpatialPyramid {
    /// Features at each level: level 0 = 1x1, level 1 = 2x2, level 2 = 4x4.
    /// Each level contains grid_size*grid_size tiles.
    pub levels: Vec<PyramidLevel>,
}

/// One level of the spatial pyramid.
#[derive(Debug, Clone)]
pub struct PyramidLevel {
    /// Grid size at this level (1, 2, or 4).
    pub grid_size: usize,
    /// Per-tile features at this level.
    pub tiles: Vec<TileFeatures>,
}

/// Standard SPM weights: [level0=1/4, level1=1/4, level2=1/2].
pub const SPM_WEIGHTS: [f64; 3] = [0.25, 0.25, 0.50];

// ---------------------------------------------------------------------------
// Pyramid construction
// ---------------------------------------------------------------------------

/// Build a spatial pyramid from Oklab pixel data.
///
/// Extracts features at 3 levels: 1x1 (global), 2x2, and 4x4.
/// Returns the pyramid with all levels populated.
pub fn build_spatial_pyramid(
    oklab_data: &[f64],
    width: usize,
    height: usize,
    color_tag: &str,
    hist_bins: usize,
) -> Result<SpatialPyramid, feature_pipeline::FeaturePipelineError> {
    let grid_sizes = [1, 2, 4];
    let mut levels = Vec::with_capacity(3);

    for &gs in &grid_sizes {
        let tiles = feature_pipeline::extract_tile_features(
            oklab_data, width, height, color_tag, gs, hist_bins,
        )?;
        levels.push(PyramidLevel { grid_size: gs, tiles });
    }

    Ok(SpatialPyramid { levels })
}

/// Serialize a spatial pyramid into a single BLOB.
///
/// Layout:
/// - 4 bytes: number of levels (u32 LE)
/// - Per level:
///   - 4 bytes: grid_size (u32 LE)
///   - 4 bytes: tile_count (u32 LE)
///   - Per tile: row(4), col(4), then GradingFeatures blob
pub fn serialize_pyramid(pyramid: &SpatialPyramid) -> Vec<u8> {
    let mut blob = Vec::new();
    blob.extend_from_slice(&(pyramid.levels.len() as u32).to_le_bytes());

    for level in &pyramid.levels {
        blob.extend_from_slice(&(level.grid_size as u32).to_le_bytes());
        blob.extend_from_slice(&(level.tiles.len() as u32).to_le_bytes());

        for tile in &level.tiles {
            blob.extend_from_slice(&(tile.row as u32).to_le_bytes());
            blob.extend_from_slice(&(tile.col as u32).to_le_bytes());
            let feat_blob = tile.features.to_blob();
            blob.extend_from_slice(&(feat_blob.len() as u32).to_le_bytes());
            blob.extend_from_slice(&feat_blob);
        }
    }

    blob
}

/// Deserialize a spatial pyramid from a BLOB.
pub fn deserialize_pyramid(blob: &[u8], hist_bins: usize) -> Option<SpatialPyramid> {
    if blob.len() < 4 {
        return None;
    }

    let mut offset = 0;
    let num_levels = read_u32_le(blob, &mut offset)? as usize;
    if num_levels > 10 {
        return None; // sanity check
    }

    let mut levels = Vec::with_capacity(num_levels);

    for _ in 0..num_levels {
        let grid_size = read_u32_le(blob, &mut offset)? as usize;
        let tile_count = read_u32_le(blob, &mut offset)? as usize;

        let mut tiles = Vec::with_capacity(tile_count);
        for _ in 0..tile_count {
            let row = read_u32_le(blob, &mut offset)? as usize;
            let col = read_u32_le(blob, &mut offset)? as usize;
            let feat_len = read_u32_le(blob, &mut offset)? as usize;
            if offset + feat_len > blob.len() {
                return None;
            }
            let features = GradingFeatures::from_blob(&blob[offset..offset + feat_len], hist_bins).ok()?;
            offset += feat_len;
            tiles.push(TileFeatures { row, col, features });
        }

        levels.push(PyramidLevel { grid_size, tiles });
    }

    Some(SpatialPyramid { levels })
}

fn read_u32_le(data: &[u8], offset: &mut usize) -> Option<u32> {
    if *offset + 4 > data.len() {
        return None;
    }
    let val = u32::from_le_bytes(data[*offset..*offset + 4].try_into().ok()?);
    *offset += 4;
    Some(val)
}

// ---------------------------------------------------------------------------
// Pyramid comparison
// ---------------------------------------------------------------------------

/// Result of comparing two spatial pyramids.
#[derive(Debug, Clone)]
pub struct PyramidMatchResult {
    /// Weighted overall score.
    pub score: f64,
    /// Per-level scores.
    pub level_scores: Vec<f64>,
}

/// Compare two spatial pyramids using weighted SPM scoring.
///
/// At each level, computes the average Bhattacharyya coefficient between
/// corresponding tiles. The final score is a weighted sum across levels
/// using SPM_WEIGHTS.
pub fn compare_pyramids(
    query: &SpatialPyramid,
    candidate: &SpatialPyramid,
) -> PyramidMatchResult {
    let mut level_scores = Vec::with_capacity(query.levels.len());
    let mut total_score = 0.0;

    for (i, q_level) in query.levels.iter().enumerate() {
        // Find matching level in candidate
        let c_level = candidate.levels.iter().find(|l| l.grid_size == q_level.grid_size);

        let level_score = if let Some(c_level) = c_level {
            // Compare tiles at same positions
            let mut score_sum = 0.0;
            let mut count = 0;

            for qt in &q_level.tiles {
                if let Some(ct) = c_level.tiles.iter().find(|t| t.row == qt.row && t.col == qt.col) {
                    let s = bhattacharyya_distance(&qt.features, &ct.features).unwrap_or(0.0);
                    score_sum += s;
                    count += 1;
                }
            }

            if count > 0 { score_sum / count as f64 } else { 0.0 }
        } else {
            0.0
        };

        level_scores.push(level_score);
        let weight = SPM_WEIGHTS.get(i).copied().unwrap_or(0.0);
        total_score += weight * level_score;
    }

    PyramidMatchResult { score: total_score, level_scores }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_features(l_mean: f64) -> GradingFeatures {
        GradingFeatures {
            hist_l: vec![l_mean; 64],
            hist_a: vec![0.1; 64],
            hist_b: vec![0.2; 64],
            moments: [l_mean, 0.2, 0.1, -0.3, 0.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }

    fn make_pyramid(l_mean: f64) -> SpatialPyramid {
        let mut levels = Vec::new();
        for &gs in &[1, 2, 4] {
            let mut tiles = Vec::new();
            for row in 0..gs {
                for col in 0..gs {
                    tiles.push(TileFeatures { row, col, features: make_features(l_mean) });
                }
            }
            levels.push(PyramidLevel { grid_size: gs, tiles });
        }
        SpatialPyramid { levels }
    }

    #[test]
    fn test_pyramid_identical() {
        let p = make_pyramid(0.5);
        let result = compare_pyramids(&p, &p);
        assert!(result.score > 0.9, "identical pyramids should score high, got {}", result.score);
        assert_eq!(result.level_scores.len(), 3);
        for &s in &result.level_scores {
            assert!(s > 0.9);
        }
    }

    #[test]
    fn test_pyramid_serialization_roundtrip() {
        let original = make_pyramid(0.5);
        let blob = serialize_pyramid(&original);
        let restored = deserialize_pyramid(&blob, 64).unwrap();

        assert_eq!(restored.levels.len(), 3);
        for (orig, rest) in original.levels.iter().zip(restored.levels.iter()) {
            assert_eq!(orig.grid_size, rest.grid_size);
            assert_eq!(orig.tiles.len(), rest.tiles.len());
        }
    }

    #[test]
    fn test_pyramid_deserialize_too_short() {
        assert!(deserialize_pyramid(&[0, 0], 64).is_none());
    }

    #[test]
    fn test_spm_weights() {
        let sum: f64 = SPM_WEIGHTS.iter().sum();
        assert!((sum - 1.0).abs() < 1e-10, "SPM weights must sum to 1.0, got {}", sum);
    }

    #[test]
    fn test_pyramid_different_levels() {
        // Use peaked histograms so bhattacharyya can differentiate them
        let p1 = make_pyramid_peaked(10);
        let p2 = make_pyramid_peaked(50);
        let result = compare_pyramids(&p1, &p2);
        assert!(result.score >= 0.0);
        // Identical peaked histograms should score higher than different ones
        let identical = compare_pyramids(&p1, &p1);
        assert!(identical.score > result.score, "identical should score higher than different: {} vs {}", identical.score, result.score);
    }

    fn make_features_peaked(peak_bin: usize) -> GradingFeatures {
        let mut hist_l = vec![0.0; 64];
        hist_l[peak_bin] = 1.0;
        GradingFeatures {
            hist_l,
            hist_a: vec![1.0 / 64.0; 64],
            hist_b: vec![1.0 / 64.0; 64],
            moments: [0.5, 0.2, 0.1, -0.3, 0.0, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        }
    }

    fn make_pyramid_peaked(peak_bin: usize) -> SpatialPyramid {
        let mut levels = Vec::new();
        for &gs in &[1, 2, 4] {
            let mut tiles = Vec::new();
            for row in 0..gs {
                for col in 0..gs {
                    tiles.push(TileFeatures { row, col, features: make_features_peaked(peak_bin) });
                }
            }
            levels.push(PyramidLevel { grid_size: gs, tiles });
        }
        SpatialPyramid { levels }
    }
}
