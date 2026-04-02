// color_distribution.rs — Color distribution classification for movie fingerprints
//
// Classifies strip pixels into 7 color categories based on Oklab hue angle.
// Produces per-category fraction + average RGB, plus neutral fraction.
//
// Uses inline Oklab conversion to avoid per-pixel FFI overhead.

// ---------------------------------------------------------------------------
// Inline Oklab conversion (matches MoonBit srgb_to_oklab exactly)
// ---------------------------------------------------------------------------

/// sRGB gamma decode (IEC 61966-2-1 piecewise)
#[inline]
fn srgb_gamma_decode(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear sRGB → Oklab (single pixel)
#[inline]
fn srgb_to_oklab_pixel(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let lr = srgb_gamma_decode(r);
    let lg = srgb_gamma_decode(g);
    let lb = srgb_gamma_decode(b);

    // Linear sRGB → LMS
    let l = 0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb;
    let m = 0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb;
    let s = 0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb;

    // Cube root (clamp to 0 to avoid NaN)
    let l_c = if l < 0.0 { 0.0 } else { l };
    let m_c = if m < 0.0 { 0.0 } else { m };
    let s_c = if s < 0.0 { 0.0 } else { s };
    let l3 = l_c.powf(1.0 / 3.0);
    let m3 = m_c.powf(1.0 / 3.0);
    let s3 = s_c.powf(1.0 / 3.0);

    // LMS' → Oklab
    let ok_l = 0.2104542553 * l3 + 0.7936177850 * m3 - 0.0040720468 * s3;
    let ok_a = 1.9779984951 * l3 - 2.4285922050 * m3 + 0.4505937099 * s3;
    let ok_b = 0.0259040371 * l3 + 0.7827717662 * m3 - 0.8086757660 * s3;

    (ok_l, ok_a, ok_b)
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
// Classification
// ---------------------------------------------------------------------------

fn classify_pixel(l: f64, a: f64, b: f64, min_chroma: f64) -> Option<ColorCategory> {
    let chroma = (a * a + b * b).sqrt();
    if chroma < min_chroma {
        return None;
    }

    let deg = b.atan2(a).to_degrees();
    let deg = if deg < 0.0 { deg + 360.0 } else { deg };

    // SKIN: warm hue + moderate chroma + adequate lightness
    if deg >= 15.0 && deg < 45.0 && chroma < 0.15 && l > 0.3 {
        return Some(ColorCategory::Skin);
    }

    if deg >= 345.0 || deg < 15.0 {
        Some(ColorCategory::Red)
    } else if deg < 45.0 {
        Some(ColorCategory::Red) // warm orange-red
    } else if deg < 70.0 {
        Some(ColorCategory::Yellow)
    } else if deg < 165.0 {
        Some(ColorCategory::Green)
    } else if deg < 200.0 {
        Some(ColorCategory::Cyan)
    } else if deg < 270.0 {
        Some(ColorCategory::Blue)
    } else {
        Some(ColorCategory::Magenta)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

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
    let total_pixels = width * height;

    let mut counts = [0usize; NUM_CATEGORIES];
    let mut sum_r = [0.0f64; NUM_CATEGORIES];
    let mut sum_g = [0.0f64; NUM_CATEGORIES];
    let mut sum_b = [0.0f64; NUM_CATEGORIES];
    let mut neutral_count = 0usize;

    for col in 0..width {
        for row in 0..height {
            let idx = (col * height + row) * 3;
            if idx + 2 >= strip.len() {
                break;
            }
            let r = strip[idx];
            let g = strip[idx + 1];
            let bv = strip[idx + 2];

            let (l_val, a_val, b_val) = srgb_to_oklab_pixel(r, g, bv);

            match classify_pixel(l_val, a_val, b_val, min_chroma) {
                None => neutral_count += 1,
                Some(cat) => {
                    let i = cat as usize;
                    counts[i] += 1;
                    sum_r[i] += r;
                    sum_g[i] += g;
                    sum_b[i] += bv;
                }
            }
        }
    }

    let total_f = total_pixels as f64;
    let mut categories = [[0.0f64; 4]; NUM_CATEGORIES];

    for i in 0..NUM_CATEGORIES {
        if counts[i] > 0 {
            let cf = counts[i] as f64;
            categories[i][0] = cf / total_f;
            categories[i][1] = sum_r[i] / cf;
            categories[i][2] = sum_g[i] / cf;
            categories[i][3] = sum_b[i] / cf;
        }
    }

    let neutral_fraction = if total_pixels > 0 {
        neutral_count as f64 / total_f
    } else {
        0.0
    };

    ColorDistribution {
        categories,
        neutral_fraction,
    }
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

    #[test]
    fn test_classify_red_pixel() {
        let (l, a, b) = srgb_to_oklab_pixel(1.0, 0.0, 0.0);
        let result = classify_pixel(l, a, b, 0.03);
        assert_eq!(result, Some(ColorCategory::Red));
    }

    #[test]
    fn test_classify_blue_pixel() {
        let (l, a, b) = srgb_to_oklab_pixel(0.0, 0.0, 1.0);
        let result = classify_pixel(l, a, b, 0.03);
        assert_eq!(result, Some(ColorCategory::Blue));
    }

    #[test]
    fn test_classify_gray_is_neutral() {
        let (l, a, b) = srgb_to_oklab_pixel(0.5, 0.5, 0.5);
        let result = classify_pixel(l, a, b, 0.03);
        assert_eq!(result, None);
    }

    #[test]
    fn test_classify_green_pixel() {
        let (l, a, b) = srgb_to_oklab_pixel(0.0, 1.0, 0.0);
        let result = classify_pixel(l, a, b, 0.03);
        assert_eq!(result, Some(ColorCategory::Green));
    }

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

    #[test]
    fn test_oklab_white_roundtrip() {
        let (l, a, b) = srgb_to_oklab_pixel(1.0, 1.0, 1.0);
        assert!((l - 1.0).abs() < 0.001, "L = {}", l);
        assert!(a.abs() < 0.001, "a = {}", a);
        assert!(b.abs() < 0.001, "b = {}", b);
    }

    #[test]
    fn test_oklab_black() {
        let (l, a, b) = srgb_to_oklab_pixel(0.0, 0.0, 0.0);
        assert!(l.abs() < 0.001, "L = {}", l);
        assert!(a.abs() < 0.001);
        assert!(b.abs() < 0.001);
    }
}
