// temporal_pyramid.rs — Multi-resolution temporal feature extraction via FFI
//
// Extracts per-segment Oklab histograms and color moments from a
// fingerprint strip, supporting temporal analysis of movie content.

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum TemporalPyramidError {
    #[error("TEMPORAL_PYRAMID_FFI_ERROR -- {0}")]
    FfiError(String),
    #[error("TEMPORAL_PYRAMID_INVALID_INPUT -- {0}")]
    InvalidInput(String),
}

type Result<T> = std::result::Result<T, TemporalPyramidError>;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

#[cfg(moonbit_ffi)]
extern "C" {
    fn mengxi_extract_temporal_features(
        strip_len: i32,
        strip_ptr: *const f64,
        width: i32,
        height: i32,
        segments_ptr: *const f64,
        segments_len: i32,
        hist_bins: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Default histogram bins per channel.
const DEFAULT_HIST_BINS: usize = 64;

/// Temporal features for a single segment.
#[derive(Debug, Clone)]
pub struct SegmentFeatures {
    /// Oklab L histogram (hist_bins elements).
    pub hist_l: Vec<f64>,
    /// Oklab a histogram (hist_bins elements).
    pub hist_a: Vec<f64>,
    /// Oklab b histogram (hist_bins elements).
    pub hist_b: Vec<f64>,
    /// Color moments: [mean, std, skewness, kurtosis] for L, a, b (12 elements).
    pub moments: Vec<f64>,
}

/// Multi-resolution temporal feature set extracted from a fingerprint strip.
#[derive(Debug, Clone)]
pub struct TemporalFeatures {
    /// Features per segment.
    pub segments: Vec<SegmentFeatures>,
    /// Number of histogram bins per channel.
    pub hist_bins: usize,
}

impl TemporalFeatures {
    /// Get the average lightness (mean L) for a segment.
    pub fn avg_lightness(&self, segment_idx: usize) -> Option<f64> {
        self.segments.get(segment_idx).map(|s| s.moments[0])
    }

    /// Get the lightness standard deviation for a segment.
    pub fn lightness_std(&self, segment_idx: usize) -> Option<f64> {
        self.segments.get(segment_idx).map(|s| s.moments[1])
    }
}

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

/// Extract temporal features for defined segments of a fingerprint strip.
///
/// # Arguments
/// * `strip` — Interleaved sRGB [0,1] strip data
/// * `width` — Strip width (number of frames)
/// * `height` — Strip height
/// * `segments` — Slice of (start_frame, end_frame) pairs
/// * `hist_bins` — Histogram bins per channel (0 = default 64)
#[cfg(moonbit_ffi)]
pub fn extract_temporal_features(
    strip: &[f64],
    width: usize,
    height: usize,
    segments: &[(usize, usize)],
    hist_bins: usize,
) -> Result<TemporalFeatures> {
    if width == 0 || height == 0 {
        return Err(TemporalPyramidError::InvalidInput(
            "width and height must be non-zero".to_string(),
        ));
    }
    let expected = width * height * 3;
    if strip.len() < expected {
        return Err(TemporalPyramidError::InvalidInput(format!(
            "strip length {} < expected {}",
            strip.len(),
            expected
        )));
    }
    if segments.is_empty() {
        return Err(TemporalPyramidError::InvalidInput(
            "segments must not be empty".to_string(),
        ));
    }

    let bins = if hist_bins == 0 { DEFAULT_HIST_BINS } else { hist_bins };
    if bins < 8 {
        return Err(TemporalPyramidError::InvalidInput(
            "hist_bins must be >= 8".to_string(),
        ));
    }

    // Flatten segments to f64 array [start0, end0, start1, end1, ...]
    let segs_f64: Vec<f64> = segments
        .iter()
        .flat_map(|&(s, e)| [s as f64, e as f64])
        .collect();

    let num_segments = segments.len();
    let per_segment = bins * 3 + 12;
    let out_size = num_segments * per_segment;
    let mut output = vec![0.0_f64; out_size];

    let result = unsafe {
        mengxi_extract_temporal_features(
            strip.len() as i32,
            strip.as_ptr(),
            width as i32,
            height as i32,
            segs_f64.as_ptr(),
            segs_f64.len() as i32,
            bins as i32,
            out_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(TemporalPyramidError::FfiError(format!(
            "mengxi_extract_temporal_features returned {}",
            result
        )));
    }

    // Parse output into segments
    let mut seg_features = Vec::with_capacity(num_segments);
    for i in 0..num_segments {
        let base = i * per_segment;
        let mut hist_l = vec![0.0; bins];
        let mut hist_a = vec![0.0; bins];
        let mut hist_b = vec![0.0; bins];
        hist_l.copy_from_slice(&output[base..base + bins]);
        hist_a.copy_from_slice(&output[base + bins..base + bins * 2]);
        hist_b.copy_from_slice(&output[base + bins * 2..base + bins * 3]);
        let moments = output[base + bins * 3..base + per_segment].to_vec();

        seg_features.push(SegmentFeatures {
            hist_l,
            hist_a,
            hist_b,
            moments,
        });
    }

    Ok(TemporalFeatures {
        segments: seg_features,
        hist_bins: bins,
    })
}

#[cfg(not(moonbit_ffi))]
pub fn extract_temporal_features(
    _strip: &[f64],
    _width: usize,
    _height: usize,
    _segments: &[(usize, usize)],
    _hist_bins: usize,
) -> Result<TemporalFeatures> {
    Err(TemporalPyramidError::FfiError("MoonBit FFI not available".to_string()))
}

/// Convenience: extract temporal features for equal-width segments.
///
/// Divides the strip into `num_segments` equal parts.
#[cfg(moonbit_ffi)]
pub fn extract_temporal_features_uniform(
    strip: &[f64],
    width: usize,
    height: usize,
    num_segments: usize,
    hist_bins: usize,
) -> Result<TemporalFeatures> {
    if num_segments == 0 {
        return Err(TemporalPyramidError::InvalidInput(
            "num_segments must be > 0".to_string(),
        ));
    }
    let seg_width = width / num_segments;
    if seg_width == 0 {
        return Err(TemporalPyramidError::InvalidInput(format!(
            "width {} too small for {} segments",
            width, num_segments
        )));
    }
    let segments: Vec<(usize, usize)> = (0..num_segments)
        .map(|i| {
            let start = i * seg_width;
            let end = if i == num_segments - 1 { width } else { (i + 1) * seg_width };
            (start, end)
        })
        .collect();

    extract_temporal_features(strip, width, height, &segments, hist_bins)
}

#[cfg(not(moonbit_ffi))]
pub fn extract_temporal_features_uniform(
    _strip: &[f64],
    _width: usize,
    _height: usize,
    _num_segments: usize,
    _hist_bins: usize,
) -> Result<TemporalFeatures> {
    Err(TemporalPyramidError::FfiError("MoonBit FFI not available".to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_temporal_single_segment() {
        let strip = vec![0.5; 4 * 1 * 3]; // 4x1 strip, gray
        let features = extract_temporal_features(&strip, 4, 1, &[(0, 4)], 8).unwrap();
        assert_eq!(features.segments.len(), 1);
        assert_eq!(features.hist_bins, 8);
        // Mean L should be around 0.5
        let mean_l = features.segments[0].moments[0];
        assert!(mean_l > 0.3 && mean_l < 0.8, "mean_l = {}", mean_l);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_temporal_two_segments() {
        let mut strip = vec![0.0; 6 * 1 * 3]; // 6x1 strip
        // First 3 white, last 3 black
        for i in 0..3 {
            strip[i * 3] = 1.0;
            strip[i * 3 + 1] = 1.0;
            strip[i * 3 + 2] = 1.0;
        }
        let features = extract_temporal_features(&strip, 6, 1, &[(0, 3), (3, 6)], 8).unwrap();
        assert_eq!(features.segments.len(), 2);
        // White segment should have high L
        assert!(features.segments[0].moments[0] > 0.7);
        // Black segment should have low L
        assert!(features.segments[1].moments[0] < 0.3);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_temporal_uniform() {
        let strip = vec![0.5; 12 * 1 * 3];
        let features = extract_temporal_features_uniform(&strip, 12, 1, 3, 8).unwrap();
        assert_eq!(features.segments.len(), 3);
    }

    #[test]
    fn test_temporal_zero_dims() {
        let strip = vec![0.5; 12];
        let result = extract_temporal_features(&strip, 0, 2, &[(0, 2)], 8);
        assert!(result.is_err());
    }

    #[test]
    fn test_temporal_empty_segments() {
        let strip = vec![0.5; 12];
        let result = extract_temporal_features(&strip, 4, 1, &[], 8);
        assert!(result.is_err());
    }

    #[test]
    fn test_temporal_bad_bins() {
        let strip = vec![0.5; 12];
        let result = extract_temporal_features(&strip, 4, 1, &[(0, 4)], 4);
        assert!(result.is_err());
    }
}
