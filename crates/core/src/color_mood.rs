// color_mood.rs — Color mood timeline computation via FFI
//
// Classifies segments of a fingerprint strip into mood categories
// based on Oklab lightness and chroma statistics.

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ColorMoodError {
    #[error("COLOR_MOOD_FFI_ERROR -- {0}")]
    FfiError(String),
    #[error("COLOR_MOOD_INVALID_INPUT -- {0}")]
    InvalidInput(String),
}

type Result<T> = std::result::Result<T, ColorMoodError>;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

#[cfg(moonbit_ffi)]
extern "C" {
    fn mengxi_compute_mood_timeline(
        strip_len: i32,
        strip_ptr: *const f64,
        width: i32,
        height: i32,
        boundaries_ptr: *const f64,
        boundaries_len: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Mood category based on color analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoodCategory {
    Dark = 0,
    Vivid = 1,
    Warm = 2,
    Cool = 3,
    Neutral = 4,
}

impl MoodCategory {
    /// Parse from the integer tag returned by FFI.
    pub fn from_tag(tag: i32) -> Option<Self> {
        match tag {
            0 => Some(MoodCategory::Dark),
            1 => Some(MoodCategory::Vivid),
            2 => Some(MoodCategory::Warm),
            3 => Some(MoodCategory::Cool),
            4 => Some(MoodCategory::Neutral),
            _ => None,
        }
    }

    /// Chinese description of the mood.
    pub fn description_zh(&self) -> &'static str {
        match self {
            MoodCategory::Dark => "暗调",
            MoodCategory::Vivid => "鲜艳",
            MoodCategory::Warm => "暖调",
            MoodCategory::Cool => "冷调",
            MoodCategory::Neutral => "中性",
        }
    }

    /// English description of the mood.
    pub fn description_en(&self) -> &'static str {
        match self {
            MoodCategory::Dark => "Dark",
            MoodCategory::Vivid => "Vivid",
            MoodCategory::Warm => "Warm",
            MoodCategory::Cool => "Cool",
            MoodCategory::Neutral => "Neutral",
        }
    }
}

/// A mood segment with start/end frames and dominant color.
#[derive(Debug, Clone)]
pub struct MoodSegment {
    pub start_frame: usize,
    pub end_frame: usize,
    pub mood: MoodCategory,
    pub dominant_l: f64,
    pub dominant_a: f64,
    pub dominant_b: f64,
}

/// Elements per segment in the raw output.
const SEGMENT_ELEMENTS: usize = 6;

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

/// Compute mood timeline for segments defined by scene boundaries.
///
/// # Arguments
/// * `strip` — Interleaved sRGB [0,1] strip data
/// * `width` — Strip width (number of frames)
/// * `height` — Strip height
/// * `boundaries` — Frame indices where scene boundaries occur (sorted ascending)
#[cfg(moonbit_ffi)]
pub fn compute_mood_timeline(
    strip: &[f64],
    width: usize,
    height: usize,
    boundaries: &[usize],
) -> Result<Vec<MoodSegment>> {
    if width == 0 || height == 0 {
        return Err(ColorMoodError::InvalidInput(
            "width and height must be non-zero".to_string(),
        ));
    }
    let expected = width * height * 3;
    if strip.len() < expected {
        return Err(ColorMoodError::InvalidInput(format!(
            "strip length {} < expected {}",
            strip.len(),
            expected
        )));
    }

    // Convert usize boundaries to f64 for FFI
    let bounds_f64: Vec<f64> = boundaries.iter().map(|&b| b as f64).collect();

    let num_segments = boundaries.len() + 1;
    let out_size = 1 + num_segments * SEGMENT_ELEMENTS;
    let mut output = vec![0.0_f64; out_size];

    let result = unsafe {
        mengxi_compute_mood_timeline(
            strip.len() as i32,
            strip.as_ptr(),
            width as i32,
            height as i32,
            bounds_f64.as_ptr(),
            bounds_f64.len() as i32,
            out_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorMoodError::FfiError(format!(
            "mengxi_compute_mood_timeline returned {}",
            result
        )));
    }

    // Parse output
    let count = output[0] as usize;
    let mut segments = Vec::with_capacity(count);
    for i in 0..count {
        let base = 1 + i * SEGMENT_ELEMENTS;
        if base + SEGMENT_ELEMENTS > output.len() {
            break;
        }
        let mood_tag = output[base + 2] as i32;
        segments.push(MoodSegment {
            start_frame: output[base] as usize,
            end_frame: output[base + 1] as usize,
            mood: MoodCategory::from_tag(mood_tag).unwrap_or(MoodCategory::Neutral),
            dominant_l: output[base + 3],
            dominant_a: output[base + 4],
            dominant_b: output[base + 5],
        });
    }

    Ok(segments)
}

#[cfg(not(moonbit_ffi))]
pub fn compute_mood_timeline(
    _strip: &[f64],
    _width: usize,
    _height: usize,
    _boundaries: &[usize],
) -> Result<Vec<MoodSegment>> {
    Err(ColorMoodError::FfiError("MoonBit FFI not available".to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_mood_timeline_single_segment() {
        let strip = vec![0.5; 4 * 2 * 3]; // 4 frames, 2 rows, gray
        let segments = compute_mood_timeline(&strip, 4, 2, &[]).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].start_frame, 0);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_mood_timeline_warm_dark() {
        // 6 frames, 1 row: first 3 warm red, last 3 dark
        let mut strip = vec![0.0; 6 * 1 * 3];
        // Warm red frames
        strip[0] = 0.9; strip[1] = 0.2; strip[2] = 0.1;
        strip[3] = 0.9; strip[4] = 0.2; strip[5] = 0.1;
        strip[6] = 0.9; strip[7] = 0.2; strip[8] = 0.1;
        // Dark frames stay at 0.0

        let segments = compute_mood_timeline(&strip, 6, 1, &[3]).unwrap();
        assert_eq!(segments.len(), 2);
        // Bright saturated red → Vivid (L>0.6 && chroma>0.15 takes priority over Warm)
        assert_eq!(segments[0].mood, MoodCategory::Vivid);
        assert_eq!(segments[1].mood, MoodCategory::Dark);
    }

    #[test]
    fn test_mood_timeline_zero_dims() {
        let strip = vec![0.5; 12];
        let result = compute_mood_timeline(&strip, 0, 2, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_mood_category_descriptions() {
        assert_eq!(MoodCategory::Dark.description_zh(), "暗调");
        assert_eq!(MoodCategory::Vivid.description_en(), "Vivid");
        assert_eq!(MoodCategory::from_tag(2), Some(MoodCategory::Warm));
        assert_eq!(MoodCategory::from_tag(99), None);
    }
}
