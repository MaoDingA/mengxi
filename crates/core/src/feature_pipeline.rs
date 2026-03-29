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
}
