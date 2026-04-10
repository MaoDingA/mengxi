// scene_boundary.rs — Scene boundary detection via FFI
//
// Detects scene changes in a fingerprint strip by analyzing
// per-column Oklab L1 distances between adjacent frames.

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SceneBoundaryError {
    #[error("SCENE_BOUNDARY_FFI_ERROR -- {0}")]
    FfiError(String),
    #[error("SCENE_BOUNDARY_INVALID_INPUT -- {0}")]
    InvalidInput(String),
}

type Result<T> = std::result::Result<T, SceneBoundaryError>;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

#[cfg(moonbit_ffi)]
extern "C" {
    fn mengxi_detect_scene_boundaries(
        strip_len: i32,
        strip_ptr: *const f64,
        width: i32,
        height: i32,
        threshold_permille: i32,
        min_scene_frames: i32,
        max_boundaries: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A detected scene boundary with surrounding color context.
#[derive(Debug, Clone)]
pub struct SceneBoundary {
    /// Frame index where the boundary occurs.
    pub frame_idx: usize,
    /// Confidence of the detection [0, 1].
    pub confidence: f64,
    /// Average Oklab L of the segment before the boundary.
    pub prev_l: f64,
    /// Average Oklab a of the segment before the boundary.
    pub prev_a: f64,
    /// Average Oklab b of the segment before the boundary.
    pub prev_b: f64,
    /// Average Oklab L of the segment after the boundary.
    pub next_l: f64,
    /// Average Oklab a of the segment after the boundary.
    pub next_a: f64,
    /// Average Oklab b of the segment after the boundary.
    pub next_b: f64,
}

/// Maximum boundaries to prevent excessive memory allocation.
const MAX_BOUNDARIES: usize = 1024;
/// Elements per boundary in the raw output.
const BOUNDARY_ELEMENTS: usize = 8;

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

/// Detect scene boundaries in a fingerprint strip.
///
/// # Arguments
/// * `strip` — Interleaved sRGB [0,1] strip data
/// * `width` — Strip width (number of frames)
/// * `height` — Strip height
/// * `threshold` — Change threshold [0.0, 1.0], e.g. 0.3
/// * `min_scene_frames` — Minimum frames between boundaries
/// * `max_boundaries` — Maximum number of boundaries to detect (0 = use default 50)
#[cfg(moonbit_ffi)]
pub fn detect_scene_boundaries(
    strip: &[f64],
    width: usize,
    height: usize,
    threshold: f64,
    min_scene_frames: usize,
    max_boundaries: usize,
) -> Result<Vec<SceneBoundary>> {
    if width == 0 || height == 0 {
        return Err(SceneBoundaryError::InvalidInput(
            "width and height must be non-zero".to_string(),
        ));
    }
    let expected = width * height * 3;
    if strip.len() < expected {
        return Err(SceneBoundaryError::InvalidInput(format!(
            "strip length {} < expected {}",
            strip.len(),
            expected
        )));
    }

    let max_b = if max_boundaries == 0 { 50 } else { max_boundaries };
    if max_b > MAX_BOUNDARIES {
        return Err(SceneBoundaryError::InvalidInput(format!(
            "max_boundaries {} exceeds limit {}",
            max_b, MAX_BOUNDARIES
        )));
    }

    let threshold_permille = (threshold * 1000.0).round() as i32;
    let out_size = 1 + max_b * BOUNDARY_ELEMENTS;
    let mut output = vec![0.0_f64; out_size];

    let result = unsafe {
        mengxi_detect_scene_boundaries(
            strip.len() as i32,
            strip.as_ptr(),
            width as i32,
            height as i32,
            threshold_permille,
            min_scene_frames as i32,
            max_b as i32,
            out_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(SceneBoundaryError::FfiError(format!(
            "mengxi_detect_scene_boundaries returned {}",
            result
        )));
    }

    // Parse output: first element = count, then count * 8 elements
    let count = output[0] as usize;
    let mut boundaries = Vec::with_capacity(count);
    for i in 0..count {
        let base = 1 + i * BOUNDARY_ELEMENTS;
        if base + BOUNDARY_ELEMENTS > output.len() {
            break;
        }
        boundaries.push(SceneBoundary {
            frame_idx: output[base] as usize,
            confidence: output[base + 1],
            prev_l: output[base + 2],
            prev_a: output[base + 3],
            prev_b: output[base + 4],
            next_l: output[base + 5],
            next_a: output[base + 6],
            next_b: output[base + 7],
        });
    }

    Ok(boundaries)
}

#[cfg(not(moonbit_ffi))]
pub fn detect_scene_boundaries(
    _strip: &[f64],
    _width: usize,
    _height: usize,
    _threshold: f64,
    _min_scene_frames: usize,
    _max_boundaries: usize,
) -> Result<Vec<SceneBoundary>> {
    Err(SceneBoundaryError::FfiError("MoonBit FFI not available".to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_detect_uniform_no_boundaries() {
        // Uniform strip → no boundaries
        let strip = vec![0.5; 10 * 4 * 3];
        let boundaries = detect_scene_boundaries(&strip, 10, 4, 0.3, 2, 50).unwrap();
        assert!(boundaries.is_empty());
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_detect_abrupt_change() {
        // 10x1 strip: first 5 white, last 5 black
        let mut strip = vec![0.0; 10 * 1 * 3];
        for i in 0..5 {
            strip[i * 3] = 1.0;
            strip[i * 3 + 1] = 1.0;
            strip[i * 3 + 2] = 1.0;
        }
        let boundaries = detect_scene_boundaries(&strip, 10, 1, 0.3, 1, 50).unwrap();
        assert!(!boundaries.is_empty(), "should detect boundary between white and black");
    }

    #[test]
    fn test_detect_zero_dims() {
        let strip = vec![0.5; 12];
        let result = detect_scene_boundaries(&strip, 0, 2, 0.3, 2, 50);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_too_many_boundaries() {
        let strip = vec![0.5; 12];
        let result = detect_scene_boundaries(&strip, 2, 2, 0.3, 2, 2000);
        assert!(result.is_err());
    }
}
