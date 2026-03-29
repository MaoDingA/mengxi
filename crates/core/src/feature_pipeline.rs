// feature_pipeline.rs — Shared feature extraction pipeline
//
// Consolidates the duplicated pixel → grading feature logic used by both
// the import path (project.rs) and the re-extraction path (fingerprint.rs).
//
// Pipeline: pixel_data → downsample → RGB→Oklab → FFI feature extraction → GradingFeatures

use crate::color_science::{self, GradingFeatures};
use crate::downsample;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from the feature extraction pipeline.
#[derive(Debug)]
pub enum FeaturePipelineError {
    /// Downsampling failed.
    DownsampleError(String),
    /// RGB → Oklab conversion failed.
    OklabError(String),
    /// FFI grading feature extraction failed.
    ExtractionError(String),
}

impl std::fmt::Display for FeaturePipelineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FeaturePipelineError::DownsampleError(msg) => {
                write!(f, "PIPELINE_DOWNSAMPLE_ERROR -- {}", msg)
            }
            FeaturePipelineError::OklabError(msg) => {
                write!(f, "PIPELINE_OKLAB_ERROR -- {}", msg)
            }
            FeaturePipelineError::ExtractionError(msg) => {
                write!(f, "PIPELINE_EXTRACTION_ERROR -- {}", msg)
            }
        }
    }
}

impl std::error::Error for FeaturePipelineError {}

// ---------------------------------------------------------------------------
// Color space helpers
// ---------------------------------------------------------------------------

/// Map DPX transfer characteristic string to a color space tag for fingerprint extraction.
pub fn map_transfer_string_to_color_tag(transfer: &str) -> String {
    match transfer {
        "printing_density" | "logarithmic" => "log".to_string(),
        "bt709" | "bt601_bg" | "bt601_m" | "smpte_274m"
        | "unspecified_video" | "ntsc_composite" | "pal_composite" => "video".to_string(),
        _ => "linear".to_string(),
    }
}

/// Determine color space tag from file format and optional transfer characteristic.
///
/// DPX files use the transfer string; EXR and MOV are always linear.
pub fn determine_color_tag(format: &str, transfer: Option<&str>) -> String {
    if format == "dpx" {
        transfer
            .map_or("linear".to_string(), |t| {
                map_transfer_string_to_color_tag(t)
            })
    } else {
        "linear".to_string()
    }
}

/// Convert color space tag string to integer for FFI.
pub fn color_tag_to_int(color_tag: &str) -> i32 {
    match color_tag {
        "log" => 1,
        "video" => 2,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Shared pipeline
// ---------------------------------------------------------------------------

/// Extract grading features from raw RGB pixel data.
///
/// Performs the full pipeline: downsample → RGB→Oklab → FFI feature extraction.
/// This is the shared path used by both import (project.rs) and re-extraction (fingerprint.rs).
///
/// # Arguments
/// * `pixel_data` — Interleaved RGB values normalized to [0.0, 1.0]
/// * `width` — Image width in pixels (Some for downsampling, None to skip)
/// * `height` — Image height in pixels (Some for downsampling, None to skip)
/// * `color_tag` — Color space tag: "linear", "log", or "video"
///
/// # Returns
/// `Ok(GradingFeatures)` on success, or an error if any pipeline step fails.
pub fn extract_features_from_pixels(
    pixel_data: &[f64],
    width: Option<usize>,
    height: Option<usize>,
    color_tag: &str,
) -> Result<GradingFeatures, FeaturePipelineError> {
    // Step 1: Downsample
    let downsampled = match (width, height) {
        (Some(w), Some(h)) => {
            match downsample::downsample_rgb(pixel_data, w, h, downsample::MAX_DIMENSION) {
                Ok((data, _, _)) => data,
                Err(e) => return Err(FeaturePipelineError::DownsampleError(e.to_string())),
            }
        }
        _ => pixel_data.to_vec(),
    };

    // Step 2: RGB → Oklab
    let oklab_data = match color_science::rgb_to_oklab_batch(&downsampled, color_tag) {
        Ok(data) => data,
        Err(e) => return Err(FeaturePipelineError::OklabError(e.to_string())),
    };

    // Step 3: Extract grading features via FFI
    let color_space_tag_int = color_tag_to_int(color_tag);
    match color_science::extract_grading_features(
        &oklab_data,
        color_space_tag_int,
        GradingFeatures::HIST_BINS,
    ) {
        Ok(features) => Ok(features),
        Err(e) => Err(FeaturePipelineError::ExtractionError(e.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Tiled feature extraction
// ---------------------------------------------------------------------------

/// Per-tile grading features with grid position.
#[derive(Debug, Clone)]
pub struct TileFeatures {
    pub row: usize,
    pub col: usize,
    pub features: GradingFeatures,
}

/// Divide downsampled Oklab data into a grid and extract per-tile features.
///
/// The pixel data must be interleaved Oklab [L0,a0,b0, L1,a1,b1, ...] with
/// the given `width` and `height`. The grid divides the image into `grid_size`
/// rows and `grid_size` columns (e.g., grid_size=4 → 4x4 = 16 tiles).
///
/// Each tile's pixels are passed through the same FFI feature extraction pipeline.
pub fn extract_tile_features(
    oklab_data: &[f64],
    width: usize,
    height: usize,
    color_tag: &str,
    grid_size: usize,
    hist_bins: usize,
) -> Result<Vec<TileFeatures>, FeaturePipelineError> {
    if grid_size == 0 {
        return Ok(Vec::new());
    }
    if oklab_data.len() % 3 != 0 {
        return Err(FeaturePipelineError::ExtractionError(
            "oklab data length not divisible by 3".to_string(),
        ));
    }
    if width == 0 || height == 0 {
        return Ok(Vec::new());
    }

    let tile_w = (width + grid_size - 1) / grid_size;
    let tile_h = (height + grid_size - 1) / grid_size;
    let color_space_tag_int = color_tag_to_int(color_tag);

    let mut tiles = Vec::with_capacity(grid_size * grid_size);

    for row in 0..grid_size {
        for col in 0..grid_size {
            let y_start = row * tile_h;
            let y_end = ((row + 1) * tile_h).min(height);
            let x_start = col * tile_w;
            let x_end = ((col + 1) * tile_w).min(width);

            if y_start >= y_end || x_start >= x_end {
                continue;
            }

            // Extract tile pixels
            let tile_pixel_count = (y_end - y_start) * (x_end - x_start);
            let mut tile_oklab = Vec::with_capacity(tile_pixel_count * 3);
            for y in y_start..y_end {
                for x in x_start..x_end {
                    let idx = (y * width + x) * 3;
                    tile_oklab.push(oklab_data[idx]);
                    tile_oklab.push(oklab_data[idx + 1]);
                    tile_oklab.push(oklab_data[idx + 2]);
                }
            }

            // Extract features for this tile
            let features = match color_science::extract_grading_features(
                &tile_oklab,
                color_space_tag_int,
                hist_bins,
            ) {
                Ok(f) => f,
                Err(e) => {
                    return Err(FeaturePipelineError::ExtractionError(format!(
                        "tile ({},{}) extraction failed: {}", row, col, e
                    )));
                }
            };

            tiles.push(TileFeatures {
                row,
                col,
                features,
            });
        }
    }

    Ok(tiles)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_transfer_string_to_color_tag() {
        assert_eq!(map_transfer_string_to_color_tag("printing_density"), "log");
        assert_eq!(map_transfer_string_to_color_tag("logarithmic"), "log");
        assert_eq!(map_transfer_string_to_color_tag("bt709"), "video");
        assert_eq!(map_transfer_string_to_color_tag("bt601_bg"), "video");
        assert_eq!(map_transfer_string_to_color_tag("smpte_274m"), "video");
        assert_eq!(map_transfer_string_to_color_tag("linear"), "linear");
        assert_eq!(map_transfer_string_to_color_tag("user_defined"), "linear");
    }

    #[test]
    fn test_determine_color_tag_dpx() {
        assert_eq!(determine_color_tag("dpx", Some("printing_density")), "log");
        assert_eq!(determine_color_tag("dpx", Some("bt709")), "video");
        assert_eq!(determine_color_tag("dpx", None), "linear");
        assert_eq!(determine_color_tag("dpx", Some("unknown")), "linear");
    }

    #[test]
    fn test_determine_color_tag_exr() {
        assert_eq!(determine_color_tag("exr", None), "linear");
        assert_eq!(determine_color_tag("exr", Some("anything")), "linear");
    }

    #[test]
    fn test_determine_color_tag_mov() {
        assert_eq!(determine_color_tag("mov", None), "linear");
    }

    #[test]
    fn test_color_tag_to_int() {
        assert_eq!(color_tag_to_int("linear"), 0);
        assert_eq!(color_tag_to_int("log"), 1);
        assert_eq!(color_tag_to_int("video"), 2);
        assert_eq!(color_tag_to_int("unknown"), 0);
    }

    #[test]
    fn test_error_display() {
        let err = FeaturePipelineError::DownsampleError("too small".to_string());
        assert!(err.to_string().contains("PIPELINE_DOWNSAMPLE_ERROR"));

        let err = FeaturePipelineError::OklabError("bad data".to_string());
        assert!(err.to_string().contains("PIPELINE_OKLAB_ERROR"));

        let err = FeaturePipelineError::ExtractionError("ffi failed".to_string());
        assert!(err.to_string().contains("PIPELINE_EXTRACTION_ERROR"));
    }

    // --- Tile extraction tests (Story 1.1) ---

    #[test]
    fn test_extract_tile_features_zero_grid() {
        let data = vec![0.5; 12]; // 4 pixels * 3 channels
        let result = extract_tile_features(&data, 2, 2, "linear", 0, 64);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_extract_tile_features_invalid_oklab_length() {
        let data = vec![0.5; 7]; // Not divisible by 3
        let result = extract_tile_features(&data, 2, 2, "linear", 2, 64);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not divisible by 3"));
    }

    #[test]
    fn test_extract_tile_features_zero_dimensions() {
        let data = vec![0.5; 12];
        let result = extract_tile_features(&data, 0, 2, "linear", 2, 64);
        assert!(result.unwrap().is_empty());

        let result = extract_tile_features(&data, 2, 0, "linear", 2, 64);
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_extract_tile_features_2x2_grid() {
        // 4x4 image = 16 pixels, each 3 Oklab channels
        let mut data = Vec::with_capacity(48);
        for i in 0..16 {
            let v = i as f64 / 16.0;
            data.push(v);       // L
            data.push(v * 0.1); // a
            data.push(v * 0.2); // b
        }
        let result = extract_tile_features(&data, 4, 4, "linear", 2, 64);
        assert!(result.is_ok());
        let tiles = result.unwrap();
        // 2x2 grid = 4 tiles
        assert_eq!(tiles.len(), 4);

        // Verify grid positions
        let positions: Vec<(usize, usize)> = tiles.iter().map(|t| (t.row, t.col)).collect();
        assert!(positions.contains(&(0, 0)));
        assert!(positions.contains(&(0, 1)));
        assert!(positions.contains(&(1, 0)));
        assert!(positions.contains(&(1, 1)));

        // Each tile should have valid features
        for tile in &tiles {
            assert_eq!(tile.features.hist_l.len(), 64);
            assert_eq!(tile.features.hist_a.len(), 64);
            assert_eq!(tile.features.hist_b.len(), 64);
            assert_eq!(tile.features.moments.len(), 12);
        }
    }

    #[test]
    fn test_extract_tile_features_4x4_grid() {
        // 8x8 image = 64 pixels
        let data = vec![0.5; 64 * 3];
        let result = extract_tile_features(&data, 8, 8, "linear", 4, 64);
        assert!(result.is_ok());
        let tiles = result.unwrap();
        // 4x4 grid = 16 tiles
        assert_eq!(tiles.len(), 16);
    }

    #[test]
    fn test_extract_tile_features_non_divisible_dimensions() {
        // 5x5 image with 3x3 grid — dimensions don't divide evenly
        let data = vec![0.5; 25 * 3];
        let result = extract_tile_features(&data, 5, 5, "linear", 3, 64);
        assert!(result.is_ok());
        let tiles = result.unwrap();
        // 3x3 = 9 tiles, all should be present
        assert_eq!(tiles.len(), 9);
    }

    #[test]
    fn test_extract_tile_features_tile_pixel_isolation() {
        // Create image where top-left quadrant has different values from bottom-right
        let mut data = Vec::with_capacity(64 * 3); // 8x8 image
        for y in 0..8 {
            for x in 0..8 {
                let is_top_left = x < 4 && y < 4;
                let l = if is_top_left { 0.9 } else { 0.1 };
                data.push(l);
                data.push(l * 0.1);
                data.push(l * 0.2);
            }
        }
        let result = extract_tile_features(&data, 8, 8, "linear", 2, 64);
        assert!(result.is_ok());
        let tiles = result.unwrap();
        assert_eq!(tiles.len(), 4);

        // Top-left tile should have high mean L, bottom-right should have low
        let tl = tiles.iter().find(|t| t.row == 0 && t.col == 0).unwrap();
        let br = tiles.iter().find(|t| t.row == 1 && t.col == 1).unwrap();
        assert!(tl.features.moments[0] > br.features.moments[0],
            "top-left L mean ({}) should be > bottom-right L mean ({})",
            tl.features.moments[0], br.features.moments[0]);
    }
}
