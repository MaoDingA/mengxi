// color_dna.rs — Color DNA extraction and comparison via FFI
//
// Extracts a compact 18-element color signature from a fingerprint strip,
// and compares two signatures for visual similarity.

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ColorDnaError {
    #[error("COLOR_DNA_FFI_ERROR -- {0}")]
    FfiError(String),
    #[error("COLOR_DNA_INVALID_INPUT -- {0}")]
    InvalidInput(String),
}

type Result<T> = std::result::Result<T, ColorDnaError>;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

#[cfg(moonbit_ffi)]
extern "C" {
    fn mengxi_extract_color_dna(
        strip_len: i32,
        strip_ptr: *const f64,
        width: i32,
        height: i32,
        dna_a_ptr: *const f64,
        dna_b_ptr: *const f64,
        dna_len: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;

    fn mengxi_compare_color_dna(
        dna_a_ptr: *const f64,
        dna_b_ptr: *const f64,
        dna_len: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Compact 18-element color signature extracted from a fingerprint strip.
///
/// Layout: avg_L, avg_a, avg_b (3) + hue_distribution[12] + contrast + warmth + saturation
#[derive(Debug, Clone)]
pub struct ColorDna {
    pub avg_l: f64,
    pub avg_a: f64,
    pub avg_b: f64,
    pub hue_distribution: [f64; 12],
    pub contrast: f64,
    pub warmth: f64,
    pub saturation: f64,
}

impl ColorDna {
    /// Number of f64 elements in the raw representation.
    pub const RAW_LEN: usize = 18;

    /// Parse from raw f64 array (18 elements).
    pub fn from_raw(data: &[f64]) -> Result<Self> {
        if data.len() != Self::RAW_LEN {
            return Err(ColorDnaError::InvalidInput(format!(
                "expected {} elements, got {}",
                Self::RAW_LEN,
                data.len()
            )));
        }
        let mut hue = [0.0f64; 12];
        hue.copy_from_slice(&data[3..15]);
        Ok(ColorDna {
            avg_l: data[0],
            avg_a: data[1],
            avg_b: data[2],
            hue_distribution: hue,
            contrast: data[15],
            warmth: data[16],
            saturation: data[17],
        })
    }

    /// Convert to raw f64 array.
    pub fn to_raw(&self) -> Vec<f64> {
        let mut v = Vec::with_capacity(Self::RAW_LEN);
        v.push(self.avg_l);
        v.push(self.avg_a);
        v.push(self.avg_b);
        v.extend_from_slice(&self.hue_distribution);
        v.push(self.contrast);
        v.push(self.warmth);
        v.push(self.saturation);
        v
    }
}

/// Comparison result between two color DNA signatures.
#[derive(Debug, Clone)]
pub struct ColorDnaComparison {
    /// Overall similarity [0, 1].
    pub overall_similarity: f64,
    /// Hue histogram similarity [0, 1].
    pub hue_similarity: f64,
    /// Absolute contrast difference.
    pub contrast_diff: f64,
    /// Absolute warmth difference.
    pub warmth_diff: f64,
}

impl ColorDnaComparison {
    /// Number of f64 elements in the raw representation.
    pub const RAW_LEN: usize = 4;

    /// Parse from raw f64 array (4 elements).
    pub fn from_raw(data: &[f64]) -> Result<Self> {
        if data.len() != Self::RAW_LEN {
            return Err(ColorDnaError::InvalidInput(format!(
                "expected {} elements, got {}",
                Self::RAW_LEN,
                data.len()
            )));
        }
        Ok(ColorDnaComparison {
            overall_similarity: data[0],
            hue_similarity: data[1],
            contrast_diff: data[2],
            warmth_diff: data[3],
        })
    }
}

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

/// Extract color DNA signature from a fingerprint strip.
///
/// # Arguments
/// * `strip` — Interleaved sRGB [0,1] strip data
/// * `width` — Strip width (number of frames)
/// * `height` — Strip height
#[cfg(moonbit_ffi)]
pub fn extract_color_dna(strip: &[f64], width: usize, height: usize) -> Result<ColorDna> {
    if width == 0 || height == 0 {
        return Err(ColorDnaError::InvalidInput(
            "width and height must be non-zero".to_string(),
        ));
    }
    let expected = width * height * 3;
    if strip.len() < expected {
        return Err(ColorDnaError::InvalidInput(format!(
            "strip length {} < width {} * height {} * 3 = {}",
            strip.len(),
            width,
            height,
            expected
        )));
    }

    let out_size = ColorDna::RAW_LEN;
    let mut output = vec![0.0_f64; out_size];

    let result = unsafe {
        mengxi_extract_color_dna(
            strip.len() as i32,
            strip.as_ptr(),
            width as i32,
            height as i32,
            out_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorDnaError::FfiError(format!(
            "mengxi_extract_color_dna returned {}",
            result
        )));
    }

    ColorDna::from_raw(&output)
}

#[cfg(not(moonbit_ffi))]
pub fn extract_color_dna(_strip: &[f64], _width: usize, _height: usize) -> Result<ColorDna> {
    Err(ColorDnaError::FfiError("MoonBit FFI not available".to_string()))
}

/// Compare two color DNA signatures for visual similarity.
///
/// Both DNA arrays must have the same length (18 elements each).
#[cfg(moonbit_ffi)]
pub fn compare_color_dna(dna_a: &ColorDna, dna_b: &ColorDna) -> Result<ColorDnaComparison> {
    let raw_a = dna_a.to_raw();
    let raw_b = dna_b.to_raw();
    let dna_len = ColorDna::RAW_LEN;

    let out_size = ColorDnaComparison::RAW_LEN;
    let mut output = vec![0.0_f64; out_size];

    let result = unsafe {
        mengxi_compare_color_dna(
            raw_a.as_ptr(),
            raw_b.as_ptr(),
            dna_len as i32,
            out_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorDnaError::FfiError(format!(
            "mengxi_compare_color_dna returned {}",
            result
        )));
    }

    ColorDnaComparison::from_raw(&output)
}

#[cfg(not(moonbit_ffi))]
pub fn compare_color_dna(_dna_a: &ColorDna, _dna_b: &ColorDna) -> Result<ColorDnaComparison> {
    Err(ColorDnaError::FfiError("MoonBit FFI not available".to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_extract_color_dna_uniform() {
        // 4x2 strip, all gray
        let strip = vec![0.5; 4 * 2 * 3];
        let dna = extract_color_dna(&strip, 4, 2).unwrap();
        // Uniform gray → Oklab L should be ~0.5-ish
        assert!(dna.avg_l > 0.3 && dna.avg_l < 0.8);
        // Contrast should be near zero for uniform data
        assert!(dna.contrast < 0.1);
    }

    #[test]
    fn test_extract_color_dna_zero_dims() {
        let strip = vec![0.5; 12];
        let result = extract_color_dna(&strip, 0, 2);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_color_dna_short_strip() {
        let strip = vec![0.5; 3]; // too short
        let result = extract_color_dna(&strip, 2, 2);
        assert!(result.is_err());
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_compare_identical_dna() {
        let strip = vec![0.5; 4 * 2 * 3];
        let dna_a = extract_color_dna(&strip, 4, 2).unwrap();
        let dna_b = extract_color_dna(&strip, 4, 2).unwrap();
        let comp = compare_color_dna(&dna_a, &dna_b).unwrap();
        // Identical → similarity should be very high
        assert!(comp.overall_similarity > 0.9);
    }

    #[test]
    fn test_color_dna_roundtrip() {
        let dna = ColorDna {
            avg_l: 0.5,
            avg_a: 0.1,
            avg_b: -0.05,
            hue_distribution: [0.1; 12],
            contrast: 0.3,
            warmth: 0.15,
            saturation: 0.4,
        };
        let raw = dna.to_raw();
        assert_eq!(raw.len(), 18);
        let parsed = ColorDna::from_raw(&raw).unwrap();
        assert!((parsed.avg_l - 0.5).abs() < 1e-10);
        assert!((parsed.contrast - 0.3).abs() < 1e-10);
    }

    #[test]
    fn test_color_dna_from_raw_wrong_len() {
        let bad = vec![0.0; 10];
        assert!(ColorDna::from_raw(&bad).is_err());
    }

    #[test]
    fn test_comparison_from_raw_wrong_len() {
        let bad = vec![0.0; 3];
        assert!(ColorDnaComparison::from_raw(&bad).is_err());
    }
}
