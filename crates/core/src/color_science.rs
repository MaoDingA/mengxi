// color_science.rs — FFI bridge to MoonBit ACES color science engine

/// ACES color space identifiers for FFI boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ACESColorSpace {
    ACES2065_1,
    ACEScg,
    ACEScct,
    Rec709,
}

impl ACESColorSpace {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "aces2065-1" | "aces2065_1" | "ap0" => ACESColorSpace::ACES2065_1,
            "acescg" | "ap1" => ACESColorSpace::ACEScg,
            "acescct" => ACESColorSpace::ACEScct,
            "rec709" | "srgb" | "bt709" => ACESColorSpace::Rec709,
            _ => ACESColorSpace::ACEScg,
        }
    }

    pub fn as_int(&self) -> i32 {
        match self {
            ACESColorSpace::ACES2065_1 => 10,
            ACESColorSpace::ACEScg => 11,
            ACESColorSpace::ACEScct => 12,
            ACESColorSpace::Rec709 => 20,
        }
    }

    pub fn is_log(&self) -> bool {
        matches!(self, ACESColorSpace::ACEScct)
    }
}

/// Errors from ACES color science operations.
#[derive(Debug)]
pub enum ColorScienceError {
    /// MoonBit library not available (not linked).
    FfiUnavailable,
    /// MoonBit returned an error code.
    FfiError(i32, String),
    /// Log-encoded data requires explicit conversion before ACES transform.
    LogDataRequiresConversion(String),
    /// Unsupported color space transform.
    UnsupportedTransform(String),
}

impl std::fmt::Display for ColorScienceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColorScienceError::FfiUnavailable => {
                write!(f, "COLOR_SCIENCE_UNAVAILABLE -- MoonBit FFI library not linked")
            }
            ColorScienceError::FfiError(code, context) => {
                write!(f, "COLOR_SCIENCE_FFI_ERROR -- code {} for {}", code, context)
            }
            ColorScienceError::LogDataRequiresConversion(msg) => {
                write!(f, "COLOR_SCIENCE_LOG_REQUIRED -- {}", msg)
            }
            ColorScienceError::UnsupportedTransform(msg) => {
                write!(f, "COLOR_SCIENCE_UNSUPPORTED -- {}", msg)
            }
        }
    }
}

impl std::error::Error for ColorScienceError {}

extern "C" {
    fn mengxi_aces_transform(
        data_len: i32,
        data_ptr: *const f64,
        src_tag: i32,
        dst_tag: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

/// Apply ACES color space transform to interleaved RGB pixel data.
///
/// # Arguments
/// * `pixel_data` — Interleaved RGB values, length must be divisible by 3.
/// * `src` — Source color space.
/// * `dst` — Destination color space.
///
/// # Returns
/// * `Ok(Vec<f64>)` with transformed pixel data on success.
/// * `Err(ColorScienceError)` if data is invalid or transform fails.
pub fn apply_aces_transform(
    pixel_data: &[f64],
    src: ACESColorSpace,
    dst: ACESColorSpace,
) -> Result<Vec<f64>, ColorScienceError> {
    if pixel_data.len() < 3 {
        return Err(ColorScienceError::FfiError(
            -1,
            "pixel data must contain at least 3 values (1 pixel)".to_string(),
        ));
    }
    if pixel_data.len() > i32::MAX as usize {
        return Err(ColorScienceError::FfiError(
            -1,
            format!(
                "pixel data too large for FFI ({} elements, max {})",
                pixel_data.len(),
                i32::MAX
            ),
        ));
    }
    if pixel_data.len() % 3 != 0 {
        return Err(ColorScienceError::FfiError(
            -1,
            "pixel data length must be divisible by 3 (RGB)".to_string(),
        ));
    }

    // Enforce type safety: reject log-encoded sources
    if src.is_log() {
        return Err(ColorScienceError::LogDataRequiresConversion(format!(
            "source color space {:?} is log-encoded; convert to linear first",
            src
        )));
    }

    let mut output = vec![0.0_f64; pixel_data.len()];

    let result = unsafe {
        mengxi_aces_transform(
            pixel_data.len() as i32,
            pixel_data.as_ptr(),
            src.as_int(),
            dst.as_int(),
            output.len() as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScienceError::FfiError(
            result,
            format!("{:?} -> {:?}", src, dst),
        ));
    }

    Ok(output)
}

/// Check if MoonBit ACES FFI is available by testing a trivial transform.
pub fn is_aces_ffi_available() -> bool {
    let data = [0.5_f64, 0.5, 0.5];
    let mut output = [0.0_f64; 3];
    let result = unsafe {
        mengxi_aces_transform(
            3,
            data.as_ptr(),
            ACESColorSpace::ACEScg.as_int(),
            ACESColorSpace::ACEScg.as_int(),
            3,
            output.as_mut_ptr(),
        )
    };
    result == 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_space_from_str() {
        assert_eq!(ACESColorSpace::from_str("aces2065-1"), ACESColorSpace::ACES2065_1);
        assert_eq!(ACESColorSpace::from_str("aces2065_1"), ACESColorSpace::ACES2065_1);
        assert_eq!(ACESColorSpace::from_str("ap0"), ACESColorSpace::ACES2065_1);
        assert_eq!(ACESColorSpace::from_str("ACEScg"), ACESColorSpace::ACEScg);
        assert_eq!(ACESColorSpace::from_str("ap1"), ACESColorSpace::ACEScg);
        assert_eq!(ACESColorSpace::from_str("acescct"), ACESColorSpace::ACEScct);
        assert_eq!(ACESColorSpace::from_str("rec709"), ACESColorSpace::Rec709);
        assert_eq!(ACESColorSpace::from_str("srgb"), ACESColorSpace::Rec709);
        assert_eq!(ACESColorSpace::from_str("unknown"), ACESColorSpace::ACEScg);
    }

    #[test]
    fn test_color_space_as_int() {
        assert_eq!(ACESColorSpace::ACES2065_1.as_int(), 10);
        assert_eq!(ACESColorSpace::ACEScg.as_int(), 11);
        assert_eq!(ACESColorSpace::ACEScct.as_int(), 12);
        assert_eq!(ACESColorSpace::Rec709.as_int(), 20);
    }

    #[test]
    fn test_color_space_is_log() {
        assert!(ACESColorSpace::ACEScct.is_log());
        assert!(!ACESColorSpace::ACEScg.is_log());
        assert!(!ACESColorSpace::ACES2065_1.is_log());
        assert!(!ACESColorSpace::Rec709.is_log());
    }

    #[test]
    fn test_log_data_rejected() {
        let data = [0.5_f64, 0.5, 0.5];
        let result = apply_aces_transform(&data, ACESColorSpace::ACEScct, ACESColorSpace::Rec709);
        assert!(result.is_err());
        match result.unwrap_err() {
            ColorScienceError::LogDataRequiresConversion(msg) => {
                assert!(msg.contains("log-encoded"));
            }
            other => panic!("Expected LogDataRequiresConversion, got: {:?}", other),
        }
    }

    #[test]
    fn test_too_few_pixels() {
        let result = apply_aces_transform(&[0.5], ACESColorSpace::ACEScg, ACESColorSpace::Rec709);
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_transform() {
        let data = [0.3_f64, 0.6, 0.9];
        let result = apply_aces_transform(&data, ACESColorSpace::ACEScg, ACESColorSpace::ACEScg).unwrap();
        assert_eq!(result.len(), 3);
        assert!((result[0] - 0.3).abs() < 1e-10);
        assert!((result[1] - 0.6).abs() < 1e-10);
        assert!((result[2] - 0.9).abs() < 1e-10);
    }

    #[test]
    fn test_aces2065_to_acescg_roundtrip() {
        let data = [0.5_f64, 0.25, 0.75];
        let to_acescg = apply_aces_transform(&data, ACESColorSpace::ACES2065_1, ACESColorSpace::ACEScg).unwrap();
        let back = apply_aces_transform(&to_acescg, ACESColorSpace::ACEScg, ACESColorSpace::ACES2065_1).unwrap();
        assert!((back[0] - 0.5).abs() < 1e-4);
        assert!((back[1] - 0.25).abs() < 1e-4);
        assert!((back[2] - 0.75).abs() < 1e-4);
    }

    #[test]
    fn test_acescg_to_rec709_black() {
        let data = [0.0_f64, 0.0, 0.0];
        let result = apply_aces_transform(&data, ACESColorSpace::ACEScg, ACESColorSpace::Rec709).unwrap();
        assert!((result[0]).abs() < 1e-10);
        assert!((result[1]).abs() < 1e-10);
        assert!((result[2]).abs() < 1e-10);
    }

    #[test]
    fn test_acescg_to_rec709_grey() {
        let data = [0.18_f64, 0.18, 0.18];
        let result = apply_aces_transform(&data, ACESColorSpace::ACEScg, ACESColorSpace::Rec709).unwrap();
        // 18% grey should produce reasonable display values
        assert!(result[0] > 0.4 && result[0] < 0.7);
        assert!(result[1] > 0.4 && result[1] < 0.7);
        assert!(result[2] > 0.4 && result[2] < 0.7);
    }

    #[test]
    fn test_is_aces_ffi_available() {
        assert!(is_aces_ffi_available());
    }

    #[test]
    fn test_error_display() {
        let err = ColorScienceError::FfiUnavailable;
        assert!(format!("{}", err).contains("COLOR_SCIENCE_UNAVAILABLE"));

        let err = ColorScienceError::FfiError(-3, "test".to_string());
        assert!(format!("{}", err).contains("COLOR_SCIENCE_FFI_ERROR"));

        let err = ColorScienceError::LogDataRequiresConversion("acescct input".to_string());
        assert!(format!("{}", err).contains("COLOR_SCIENCE_LOG_REQUIRED"));

        let err = ColorScienceError::UnsupportedTransform("test".to_string());
        assert!(format!("{}", err).contains("COLOR_SCIENCE_UNSUPPORTED"));
    }
}
