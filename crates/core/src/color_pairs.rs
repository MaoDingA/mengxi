// color_pairs.rs — Dominant complementary color pair detection via FFI
//
// Detects complementary color pairs (e.g., teal-orange, blue-yellow) from
// fingerprint strip data using hue histogram analysis.

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ColorPairsError {
    #[error("COLOR_PAIRS_FFI_ERROR -- {0}")]
    FfiError(String),
    #[error("COLOR_PAIRS_INVALID_INPUT -- {0}")]
    InvalidInput(String),
}

type Result<T> = std::result::Result<T, ColorPairsError>;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

extern "C" {
    fn mengxi_detect_dominant_pairs(
        strip_len: i32,
        strip_ptr: *const f64,
        width: i32,
        height: i32,
        min_chroma_permille: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Pair type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairType {
    TealOrange,
    BlueYellow,
    RedCyan,
    MagentaGreen,
    Other,
}

impl PairType {
    fn from_tag(tag: i32) -> Self {
        match tag {
            0 => PairType::TealOrange,
            1 => PairType::BlueYellow,
            2 => PairType::RedCyan,
            3 => PairType::MagentaGreen,
            _ => PairType::Other,
        }
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            PairType::TealOrange => "teal-orange",
            PairType::BlueYellow => "blue-yellow",
            PairType::RedCyan => "red-cyan",
            PairType::MagentaGreen => "magenta-green",
            PairType::Other => "other",
        }
    }
}

/// A detected complementary color pair.
#[derive(Debug, Clone)]
pub struct DominantPair {
    /// Hue angle of the first color (radians).
    pub hue_a: f64,
    /// Hue angle of the complementary color (radians).
    pub hue_b: f64,
    /// Strength of the pair [0, 1] (product of normalized bin frequencies).
    pub strength: f64,
    /// Classification of the pair.
    pub pair_type: PairType,
}

/// Result of dominant pair detection.
#[derive(Debug, Clone)]
pub struct DominantPairsResult {
    /// Detected complementary pairs, sorted by strength descending.
    pub pairs: Vec<DominantPair>,
}

/// Maximum number of pairs the FFI can return.
const MAX_PAIRS: usize = 6;
/// Elements per pair in the raw output.
const ELEMENTS_PER_PAIR: usize = 4;
/// Header element (count).
const HEADER_LEN: usize = 1;
/// Total raw output buffer size.
const RAW_OUT_LEN: usize = HEADER_LEN + MAX_PAIRS * ELEMENTS_PER_PAIR;

impl DominantPairsResult {
    /// Parse from raw f64 output buffer.
    pub fn from_raw(data: &[f64]) -> Result<Self> {
        if data.is_empty() {
            return Err(ColorPairsError::InvalidInput("empty output".to_string()));
        }
        let count = data[0] as usize;
        if count > MAX_PAIRS {
            return Err(ColorPairsError::InvalidInput(format!(
                "pair count {} exceeds max {}",
                count, MAX_PAIRS
            )));
        }
        let needed = HEADER_LEN + count * ELEMENTS_PER_PAIR;
        if data.len() < needed {
            return Err(ColorPairsError::InvalidInput(format!(
                "need {} elements for {} pairs, got {}",
                needed,
                count,
                data.len()
            )));
        }

        let mut pairs = Vec::with_capacity(count);
        for i in 0..count {
            let base = HEADER_LEN + i * ELEMENTS_PER_PAIR;
            pairs.push(DominantPair {
                hue_a: data[base],
                hue_b: data[base + 1],
                strength: data[base + 2],
                pair_type: PairType::from_tag(data[base + 3] as i32),
            });
        }
        Ok(DominantPairsResult { pairs })
    }
}

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

/// Detect dominant complementary color pairs from a fingerprint strip.
///
/// # Arguments
/// * `strip` — Interleaved sRGB [0,1] strip data
/// * `width` — Strip width (number of frames)
/// * `height` — Strip height
/// * `min_chroma_permille` — Minimum chroma threshold * 1000 (default: 20 = 0.02)
pub fn detect_dominant_pairs(
    strip: &[f64],
    width: usize,
    height: usize,
    min_chroma_permille: i32,
) -> Result<DominantPairsResult> {
    if width == 0 || height == 0 {
        return Err(ColorPairsError::InvalidInput(
            "width and height must be non-zero".to_string(),
        ));
    }
    let expected = width * height * 3;
    if strip.len() < expected {
        return Err(ColorPairsError::InvalidInput(format!(
            "strip length {} < width {} * height {} * 3 = {}",
            strip.len(),
            width,
            height,
            expected
        )));
    }

    let mut output = vec![0.0_f64; RAW_OUT_LEN];

    let result = unsafe {
        mengxi_detect_dominant_pairs(
            strip.len() as i32,
            strip.as_ptr(),
            width as i32,
            height as i32,
            min_chroma_permille,
            RAW_OUT_LEN as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorPairsError::FfiError(format!(
            "mengxi_detect_dominant_pairs returned {}",
            result
        )));
    }

    DominantPairsResult::from_raw(&output)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_pairs_uniform_gray() {
        // All gray → no chromatic pixels → 0 pairs
        let strip = vec![0.5; 4 * 2 * 3];
        let result = detect_dominant_pairs(&strip, 4, 2, 20).unwrap();
        assert_eq!(result.pairs.len(), 0);
    }

    #[test]
    fn test_detect_pairs_zero_dims() {
        let strip = vec![0.5; 12];
        let result = detect_dominant_pairs(&strip, 0, 2, 20);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_pairs_short_strip() {
        let strip = vec![0.5; 3];
        let result = detect_dominant_pairs(&strip, 2, 2, 20);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_pairs_teal_orange() {
        // 4x2 strip: first 4 pixels orange-ish, next 4 pixels teal-ish
        let mut strip = vec![0.0; 4 * 2 * 3];
        // Orange (warm): high red, medium green, low blue
        for i in 0..4 {
            strip[i * 3] = 0.9;
            strip[i * 3 + 1] = 0.5;
            strip[i * 3 + 2] = 0.1;
        }
        // Teal (cool): low red, medium green, high blue
        for j in 0..4 {
            let idx = (4 + j) * 3;
            strip[idx] = 0.0;
            strip[idx + 1] = 0.5;
            strip[idx + 2] = 0.7;
        }
        let result = detect_dominant_pairs(&strip, 4, 2, 10).unwrap();
        // Should detect at least one pair
        assert!(!result.pairs.is_empty());
        assert!(result.pairs[0].strength > 0.0);
    }

    #[test]
    fn test_pair_type_from_tag() {
        assert_eq!(PairType::from_tag(0), PairType::TealOrange);
        assert_eq!(PairType::from_tag(1), PairType::BlueYellow);
        assert_eq!(PairType::from_tag(2), PairType::RedCyan);
        assert_eq!(PairType::from_tag(3), PairType::MagentaGreen);
        assert_eq!(PairType::from_tag(4), PairType::Other);
        assert_eq!(PairType::from_tag(99), PairType::Other);
    }

    #[test]
    fn test_pair_type_names() {
        assert_eq!(PairType::TealOrange.name(), "teal-orange");
        assert_eq!(PairType::BlueYellow.name(), "blue-yellow");
        assert_eq!(PairType::RedCyan.name(), "red-cyan");
        assert_eq!(PairType::MagentaGreen.name(), "magenta-green");
        assert_eq!(PairType::Other.name(), "other");
    }

    #[test]
    fn test_dominant_pairs_result_from_raw_empty() {
        let mut data = vec![0.0; RAW_OUT_LEN];
        data[0] = 0.0; // 0 pairs
        let result = DominantPairsResult::from_raw(&data).unwrap();
        assert!(result.pairs.is_empty());
    }

    #[test]
    fn test_dominant_pairs_result_from_raw_too_many() {
        let mut data = vec![0.0; RAW_OUT_LEN];
        data[0] = 7.0; // more than MAX_PAIRS
        assert!(DominantPairsResult::from_raw(&data).is_err());
    }
}
