// color_distribution.rs — Color distribution classification for movie fingerprints
//
// Classifies strip pixels into 7 color categories based on Oklab hue angle.
// Produces per-category fraction + average RGB, plus neutral fraction.
//
// Classification is implemented in MoonBit and exposed through FFI.

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

#[cfg(moonbit_ffi)]
extern "C" {
    fn mengxi_classify_color_distribution(
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
// Types
// ---------------------------------------------------------------------------

/// 7 color categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorCategory {
    Red = 0,
    Skin = 1,
    Yellow = 2,
    Green = 3,
    Cyan = 4,
    Blue = 5,
    Magenta = 6,
}

pub const NUM_CATEGORIES: usize = 7;

/// Color distribution result
#[derive(Debug, Clone)]
pub struct ColorDistribution {
    /// Per-category: [fraction, avg_r, avg_g, avg_b]
    pub categories: [[f64; 4]; NUM_CATEGORIES],
    /// Fraction of achromatic (neutral) pixels
    pub neutral_fraction: f64,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

const RAW_OUT_LEN: usize = NUM_CATEGORIES * 4 + 1;

fn empty_distribution() -> ColorDistribution {
    ColorDistribution {
        categories: [[0.0; 4]; NUM_CATEGORIES],
        neutral_fraction: 0.0,
    }
}

fn parse_raw_distribution(raw: &[f64]) -> ColorDistribution {
    let mut categories = [[0.0f64; 4]; NUM_CATEGORIES];
    for (i, category) in categories.iter_mut().enumerate() {
        let base = i * 4;
        category.copy_from_slice(&raw[base..base + 4]);
    }
    ColorDistribution {
        categories,
        neutral_fraction: raw[NUM_CATEGORIES * 4],
    }
}

/// Classify color distribution from fingerprint strip data.
///
/// # Arguments
/// * `strip` — Interleaved sRGB [0,1] strip data (width * height * 3)
/// * `width` — Strip width (number of frame columns)
/// * `height` — Strip height (frame pixel height)
/// * `min_chroma` — Minimum chroma threshold for classification
pub fn classify_color_distribution(
    strip: &[f64],
    width: usize,
    height: usize,
    min_chroma: f64,
) -> ColorDistribution {
    if width == 0 || height == 0 || strip.len() < width * height * 3 {
        return empty_distribution();
    }

    classify_color_distribution_impl(strip, width, height, min_chroma)
}

#[cfg(moonbit_ffi)]
fn classify_color_distribution_impl(
    strip: &[f64],
    width: usize,
    height: usize,
    min_chroma: f64,
) -> ColorDistribution {
    let mut output = [0.0_f64; RAW_OUT_LEN];
    let min_chroma_permille = (min_chroma * 1000.0).round() as i32;
    let result = unsafe {
        mengxi_classify_color_distribution(
            strip.len() as i32,
            strip.as_ptr(),
            width as i32,
            height as i32,
            min_chroma_permille,
            RAW_OUT_LEN as i32,
            output.as_mut_ptr(),
        )
    };
    if result == RAW_OUT_LEN as i32 {
        parse_raw_distribution(&output)
    } else {
        empty_distribution()
    }
}

#[cfg(not(moonbit_ffi))]
fn classify_color_distribution_impl(
    _strip: &[f64],
    _width: usize,
    _height: usize,
    _min_chroma: f64,
) -> ColorDistribution {
    empty_distribution()
}

// ---------------------------------------------------------------------------
// Category metadata
// ---------------------------------------------------------------------------

impl ColorCategory {
    pub fn name(self) -> &'static str {
        match self {
            ColorCategory::Red => "RED",
            ColorCategory::Skin => "SKIN",
            ColorCategory::Yellow => "YELLOW",
            ColorCategory::Green => "GREEN",
            ColorCategory::Cyan => "CYAN",
            ColorCategory::Blue => "BLUE",
            ColorCategory::Magenta => "MAGENTA",
        }
    }

    pub fn display_rgb(self) -> (u8, u8, u8) {
        match self {
            ColorCategory::Red => (220, 50, 50),
            ColorCategory::Skin => (230, 170, 130),
            ColorCategory::Yellow => (230, 210, 50),
            ColorCategory::Green => (50, 200, 70),
            ColorCategory::Cyan => (50, 200, 210),
            ColorCategory::Blue => (50, 80, 220),
            ColorCategory::Magenta => (200, 50, 200),
        }
    }

    pub fn all() -> [ColorCategory; NUM_CATEGORIES] {
        [
            ColorCategory::Red,
            ColorCategory::Skin,
            ColorCategory::Yellow,
            ColorCategory::Green,
            ColorCategory::Cyan,
            ColorCategory::Blue,
            ColorCategory::Magenta,
        ]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_distribution_all_red() {
        let strip: Vec<f64> = vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let dist = classify_color_distribution(&strip, 2, 2, 0.03);
        assert!(
            (dist.categories[ColorCategory::Red as usize][0] - 1.0).abs() < 0.01,
            "Red fraction: {}",
            dist.categories[ColorCategory::Red as usize][0]
        );
        assert!((dist.neutral_fraction).abs() < 0.01);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_distribution_all_gray() {
        let strip: Vec<f64> = vec![0.5; 12];
        let dist = classify_color_distribution(&strip, 2, 2, 0.03);
        assert!(
            (dist.neutral_fraction - 1.0).abs() < 0.01,
            "Neutral fraction: {}",
            dist.neutral_fraction
        );
    }

    #[cfg(not(moonbit_ffi))]
    #[test]
    fn test_distribution_without_ffi_is_empty() {
        let strip: Vec<f64> = vec![1.0, 0.0, 0.0];
        let dist = classify_color_distribution(&strip, 1, 1, 0.03);
        assert_eq!(dist.neutral_fraction, 0.0);
        assert_eq!(dist.categories, [[0.0; 4]; NUM_CATEGORIES]);
    }
}
