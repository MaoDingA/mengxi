// color_scatter.rs — Frame-level color scatter distribution in Oklab space via FFI
//
// Extracts per-frame Oklab data points and computes density grids for
// visualizing the chromatic distribution of a fingerprint strip.

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ColorScatterError {
    #[error("COLOR_SCATTER_FFI_ERROR -- {0}")]
    FfiError(String),
    #[error("COLOR_SCATTER_INVALID_INPUT -- {0}")]
    InvalidInput(String),
}

type Result<T> = std::result::Result<T, ColorScatterError>;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

#[cfg(moonbit_ffi)]
extern "C" {
    fn mengxi_extract_frame_scatter(
        strip_len: i32,
        strip_ptr: *const f64,
        width: i32,
        height: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;

    fn mengxi_compute_scatter_density(
        strip_len: i32,
        strip_ptr: *const f64,
        width: i32,
        height: i32,
        grid_size: i32,
        a_range_permille: i32,
        b_range_permille: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Per-frame Oklab scatter point (L, a, b).
#[derive(Debug, Clone)]
pub struct ScatterPoint {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

/// Frame-level scatter data extracted from a fingerprint strip.
#[derive(Debug, Clone)]
pub struct ColorScatter {
    /// One scatter point per frame column.
    pub points: Vec<ScatterPoint>,
}

impl ColorScatter {
    /// Number of f64 per scatter point (L, a, b).
    pub const ELEMENTS_PER_POINT: usize = 3;

    /// Parse from raw f64 array (width * 3 elements).
    pub fn from_raw(data: &[f64], width: usize) -> Result<Self> {
        let expected = width * Self::ELEMENTS_PER_POINT;
        if data.len() < expected {
            return Err(ColorScatterError::InvalidInput(format!(
                "expected {} elements for {} frames, got {}",
                expected,
                width,
                data.len()
            )));
        }
        let mut points = Vec::with_capacity(width);
        for i in 0..width {
            let base = i * Self::ELEMENTS_PER_POINT;
            points.push(ScatterPoint {
                l: data[base],
                a: data[base + 1],
                b: data[base + 2],
            });
        }
        Ok(ColorScatter { points })
    }
}

/// Density grid computed on the Oklab a-b plane.
#[derive(Debug, Clone)]
pub struct ScatterDensity {
    /// Grid size (grid_size x grid_size).
    pub grid_size: usize,
    /// Row-major density values normalized to [0, 1].
    pub grid: Vec<f64>,
}

impl ScatterDensity {
    /// Raw element count.
    pub fn raw_len(&self) -> usize {
        self.grid_size * self.grid_size
    }
}

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

/// Extract per-frame Oklab scatter data from a fingerprint strip.
///
/// # Arguments
/// * `strip` — Interleaved sRGB [0,1] strip data
/// * `width` — Strip width (number of frames)
/// * `height` — Strip height
#[cfg(moonbit_ffi)]
pub fn extract_frame_scatter(strip: &[f64], width: usize, height: usize) -> Result<ColorScatter> {
    if width == 0 || height == 0 {
        return Err(ColorScatterError::InvalidInput(
            "width and height must be non-zero".to_string(),
        ));
    }
    let expected = width * height * 3;
    if strip.len() < expected {
        return Err(ColorScatterError::InvalidInput(format!(
            "strip length {} < width {} * height {} * 3 = {}",
            strip.len(),
            width,
            height,
            expected
        )));
    }

    let out_size = width * ColorScatter::ELEMENTS_PER_POINT;
    let mut output = vec![0.0_f64; out_size];

    let result = unsafe {
        mengxi_extract_frame_scatter(
            strip.len() as i32,
            strip.as_ptr(),
            width as i32,
            height as i32,
            out_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScatterError::FfiError(format!(
            "mengxi_extract_frame_scatter returned {}",
            result
        )));
    }

    ColorScatter::from_raw(&output, width)
}

#[cfg(not(moonbit_ffi))]
pub fn extract_frame_scatter(
    _strip: &[f64],
    _width: usize,
    _height: usize,
) -> Result<ColorScatter> {
    Err(ColorScatterError::FfiError("MoonBit FFI not available".to_string()))
}

/// Compute a-b scatter density grid from a fingerprint strip.
///
/// # Arguments
/// * `strip` — Interleaved sRGB [0,1] strip data
/// * `width` — Strip width (number of frames)
/// * `height` — Strip height
/// * `grid_size` — Resolution of the density grid (e.g., 32)
/// * `a_range_permille` — a-axis range * 1000 (e.g., 500 = ±0.5)
/// * `b_range_permille` — b-axis range * 1000 (e.g., 500 = ±0.5)
#[cfg(moonbit_ffi)]
pub fn compute_scatter_density(
    strip: &[f64],
    width: usize,
    height: usize,
    grid_size: usize,
    a_range_permille: i32,
    b_range_permille: i32,
) -> Result<ScatterDensity> {
    if width == 0 || height == 0 || grid_size == 0 {
        return Err(ColorScatterError::InvalidInput(
            "width, height, and grid_size must be non-zero".to_string(),
        ));
    }
    let expected = width * height * 3;
    if strip.len() < expected {
        return Err(ColorScatterError::InvalidInput(format!(
            "strip length {} < width {} * height {} * 3 = {}",
            strip.len(),
            width,
            height,
            expected
        )));
    }

    let out_size = grid_size * grid_size;
    let mut output = vec![0.0_f64; out_size];

    let result = unsafe {
        mengxi_compute_scatter_density(
            strip.len() as i32,
            strip.as_ptr(),
            width as i32,
            height as i32,
            grid_size as i32,
            a_range_permille,
            b_range_permille,
            out_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScatterError::FfiError(format!(
            "mengxi_compute_scatter_density returned {}",
            result
        )));
    }

    Ok(ScatterDensity {
        grid_size,
        grid: output,
    })
}

#[cfg(not(moonbit_ffi))]
pub fn compute_scatter_density(
    _strip: &[f64],
    _width: usize,
    _height: usize,
    _grid_size: usize,
    _a_range_permille: i32,
    _b_range_permille: i32,
) -> Result<ScatterDensity> {
    Err(ColorScatterError::FfiError("MoonBit FFI not available".to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_extract_frame_scatter_uniform() {
        // 4x2 strip, all gray
        let strip = vec![0.5; 4 * 2 * 3];
        let scatter = extract_frame_scatter(&strip, 4, 2).unwrap();
        assert_eq!(scatter.points.len(), 4);
        // Gray pixels should have near-zero a,b
        for pt in &scatter.points {
            assert!(pt.a.abs() < 0.1);
            assert!(pt.b.abs() < 0.1);
        }
    }

    #[test]
    fn test_extract_frame_scatter_zero_dims() {
        let strip = vec![0.5; 12];
        let result = extract_frame_scatter(&strip, 0, 2);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_frame_scatter_short_strip() {
        let strip = vec![0.5; 3];
        let result = extract_frame_scatter(&strip, 2, 2);
        assert!(result.is_err());
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_compute_scatter_density_uniform() {
        let strip = vec![0.5; 4 * 2 * 3];
        let density = compute_scatter_density(&strip, 4, 2, 16, 500, 500).unwrap();
        assert_eq!(density.grid.len(), 16 * 16);
        // Uniform gray → density concentrated in center region
        let max_val = density.grid.iter().cloned().fold(0.0_f64, f64::max);
        assert!(max_val > 0.0);
    }

    #[test]
    fn test_compute_scatter_density_zero_grid() {
        let strip = vec![0.5; 12];
        let result = compute_scatter_density(&strip, 2, 2, 0, 500, 500);
        assert!(result.is_err());
    }

    #[test]
    fn test_color_scatter_from_raw_wrong_len() {
        let data = vec![0.0; 3]; // only 1 point, but claim width=2
        assert!(ColorScatter::from_raw(&data, 2).is_err());
    }

    #[test]
    fn test_color_scatter_from_raw_valid() {
        let data = vec![0.5, 0.1, -0.05, 0.6, -0.1, 0.08];
        let scatter = ColorScatter::from_raw(&data, 2).unwrap();
        assert_eq!(scatter.points.len(), 2);
        assert!((scatter.points[0].l - 0.5).abs() < 1e-10);
        assert!((scatter.points[1].b - 0.08).abs() < 1e-10);
    }
}
