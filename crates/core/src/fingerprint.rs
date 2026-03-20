// fingerprint.rs — FFI bridge to MoonBit color fingerprint extraction

/// Number of histogram bins per channel.
pub const BINS_PER_CHANNEL: usize = 64;

/// Total output size: 64 bins R + 64 bins G + 64 bins B + mean + stddev.
pub const OUTPUT_SIZE: usize = BINS_PER_CHANNEL * 3 + 2;

/// Color space tag at the FFI boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpaceTag {
    Linear,
    Log,
    Video,
}

impl ColorSpaceTag {
    pub fn from_str(s: &str) -> Self {
        match s {
            "log" => ColorSpaceTag::Log,
            "video" => ColorSpaceTag::Video,
            _ => ColorSpaceTag::Linear,
        }
    }

    pub fn as_int(&self) -> i32 {
        match self {
            ColorSpaceTag::Linear => 0,
            ColorSpaceTag::Log => 1,
            ColorSpaceTag::Video => 2,
        }
    }
}

/// Extracted color fingerprint from a file.
#[derive(Debug, Clone)]
pub struct Fingerprint {
    pub histogram_r: Vec<f64>,
    pub histogram_g: Vec<f64>,
    pub histogram_b: Vec<f64>,
    pub luminance_mean: f64,
    pub luminance_stddev: f64,
    pub color_space_tag: String,
}

/// Errors from fingerprint extraction.
#[derive(Debug)]
pub enum FingerprintError {
    /// MoonBit library not available (not linked).
    FfiUnavailable,
    /// MoonBit returned an error code.
    FfiError(i32, String),
    /// Invalid input data.
    InvalidInput(String),
}

impl std::fmt::Display for FingerprintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FingerprintError::FfiUnavailable => {
                write!(f, "FINGERPRINT_UNAVAILABLE -- MoonBit FFI library not linked")
            }
            FingerprintError::FfiError(code, context) => {
                write!(f, "FINGERPRINT_FFI_ERROR -- code {} for {}", code, context)
            }
            FingerprintError::InvalidInput(msg) => {
                write!(f, "FINGERPRINT_INVALID_INPUT -- {}", msg)
            }
        }
    }
}

impl std::error::Error for FingerprintError {}

extern "C" {
    fn mengxi_compute_fingerprint(
        data_len: i32,
        data_ptr: *const f64,
        color_tag: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

/// Extract color fingerprint from interleaved RGB pixel data via MoonBit FFI.
///
/// # Arguments
/// * `pixel_data` — Interleaved RGB values normalized to [0.0, 1.0], length must be divisible by 3.
/// * `color_space_tag` — Color space: "linear", "log", or "video".
///
/// # Returns
/// * `Ok(Fingerprint)` on success with histogram bins and luminance statistics.
/// * `Err(FingerprintError)` if data is invalid or MoonBit returns an error.
pub fn extract_fingerprint(
    pixel_data: &[f64],
    color_space_tag: &str,
) -> Result<Fingerprint, FingerprintError> {
    if pixel_data.len() < 3 {
        return Err(FingerprintError::InvalidInput(
            "pixel data must contain at least 3 values (1 pixel)".to_string(),
        ));
    }
    if pixel_data.len() > i32::MAX as usize {
        return Err(FingerprintError::InvalidInput(
            format!("pixel data too large for FFI ({} elements, max {})", pixel_data.len(), i32::MAX),
        ));
    }
    if pixel_data.len() % 3 != 0 {
        return Err(FingerprintError::InvalidInput(
            "pixel data length must be divisible by 3 (RGB)".to_string(),
        ));
    }

    let tag = ColorSpaceTag::from_str(color_space_tag);
    let mut output = vec![0.0_f64; OUTPUT_SIZE];

    let result = unsafe {
        mengxi_compute_fingerprint(
            pixel_data.len() as i32,
            pixel_data.as_ptr(),
            tag.as_int(),
            OUTPUT_SIZE as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(FingerprintError::FfiError(
            result,
            color_space_tag.to_string(),
        ));
    }

    let histogram_r = output[0..BINS_PER_CHANNEL].to_vec();
    let histogram_g = output[BINS_PER_CHANNEL..BINS_PER_CHANNEL * 2].to_vec();
    let histogram_b = output[BINS_PER_CHANNEL * 2..BINS_PER_CHANNEL * 3].to_vec();
    let luminance_mean = output[BINS_PER_CHANNEL * 3];
    let luminance_stddev = output[BINS_PER_CHANNEL * 3 + 1];

    Ok(Fingerprint {
        histogram_r,
        histogram_g,
        histogram_b,
        luminance_mean,
        luminance_stddev,
        color_space_tag: color_space_tag.to_string(),
    })
}

/// Check if MoonBit FFI is available by testing a trivial call.
/// Returns true if the library is linked and responsive.
pub fn is_ffi_available() -> bool {
    let data = [0.5_f64, 0.5, 0.5];
    let mut output = [0.0_f64; OUTPUT_SIZE];
    let result = unsafe {
        mengxi_compute_fingerprint(
            3,
            data.as_ptr(),
            ColorSpaceTag::Linear.as_int(),
            OUTPUT_SIZE as i32,
            output.as_mut_ptr(),
        )
    };
    result == OUTPUT_SIZE as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_space_tag_from_str() {
        assert_eq!(ColorSpaceTag::from_str("linear"), ColorSpaceTag::Linear);
        assert_eq!(ColorSpaceTag::from_str("log"), ColorSpaceTag::Log);
        assert_eq!(ColorSpaceTag::from_str("video"), ColorSpaceTag::Video);
        assert_eq!(ColorSpaceTag::from_str("unknown"), ColorSpaceTag::Linear);
    }

    #[test]
    fn test_color_space_tag_as_int() {
        assert_eq!(ColorSpaceTag::Linear.as_int(), 0);
        assert_eq!(ColorSpaceTag::Log.as_int(), 1);
        assert_eq!(ColorSpaceTag::Video.as_int(), 2);
    }

    #[test]
    fn test_extract_fingerprint_too_few_pixels() {
        let result = extract_fingerprint(&[0.5], "linear");
        assert!(result.is_err());
        match result.unwrap_err() {
            FingerprintError::InvalidInput(msg) => {
                assert!(msg.contains("at least 3"));
            }
            other => panic!("Expected InvalidInput, got: {:?}", other),
        }
    }

    #[test]
    fn test_extract_fingerprint_not_divisible_by_3() {
        let result = extract_fingerprint(&[0.5, 0.5, 0.5, 0.5], "linear");
        assert!(result.is_err());
        match result.unwrap_err() {
            FingerprintError::InvalidInput(msg) => {
                assert!(msg.contains("divisible by 3"));
            }
            other => panic!("Expected InvalidInput, got: {:?}", other),
        }
    }

    #[test]
    fn test_extract_fingerprint_uniform_color() {
        // Single pixel, all channels at 0.5
        let data = [0.5_f64, 0.5, 0.5];
        let fp = extract_fingerprint(&data, "linear").unwrap();

        assert_eq!(fp.histogram_r.len(), BINS_PER_CHANNEL);
        assert_eq!(fp.histogram_g.len(), BINS_PER_CHANNEL);
        assert_eq!(fp.histogram_b.len(), BINS_PER_CHANNEL);
        assert_eq!(fp.color_space_tag, "linear");
        // All values at 0.5 should land in bin 32 (0-indexed)
        assert_eq!(fp.histogram_r[32], 1.0);
        assert_eq!(fp.histogram_g[32], 1.0);
        assert_eq!(fp.histogram_b[32], 1.0);
        // Luminance of (0.5, 0.5, 0.5) = 0.5
        assert!((fp.luminance_mean - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_extract_fingerprint_two_pixels() {
        // Red pixel + Green pixel
        let data = [
            1.0_f64, 0.0, 0.0, // red
            0.0_f64, 1.0, 0.0, // green
        ];
        let fp = extract_fingerprint(&data, "linear").unwrap();

        // Red channel: one at 1.0 (bin 63), one at 0.0 (bin 0)
        assert_eq!(fp.histogram_r[63], 0.5);
        assert_eq!(fp.histogram_r[0], 0.5);
        // Green channel: one at 0.0 (bin 0), one at 1.0 (bin 63)
        assert_eq!(fp.histogram_g[0], 0.5);
        assert_eq!(fp.histogram_g[63], 0.5);
        // Blue channel: both at 0.0 (bin 0)
        assert_eq!(fp.histogram_b[0], 1.0);
    }

    #[test]
    fn test_extract_fingerprint_output_buffer_too_small() {
        let data = [0.5_f64, 0.5, 0.5];
        let mut output = [0.0_f64; 10];
        let result = unsafe {
            mengxi_compute_fingerprint(
                3,
                data.as_ptr(),
                0,
                10,
                output.as_mut_ptr(),
            )
        };
        assert_eq!(result, -2);
    }

    #[test]
    fn test_is_ffi_available() {
        assert!(is_ffi_available());
    }

    #[test]
    fn test_fingerprint_error_display() {
        let err = FingerprintError::FfiUnavailable;
        let msg = format!("{}", err);
        assert!(msg.contains("FINGERPRINT_UNAVAILABLE"));

        let err = FingerprintError::FfiError(-1, "test".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("FINGERPRINT_FFI_ERROR"));

        let err = FingerprintError::InvalidInput("bad data".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("FINGERPRINT_INVALID_INPUT"));
    }
}
