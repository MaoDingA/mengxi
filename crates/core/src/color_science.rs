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

    fn mengxi_generate_lut(
        grid_size: i32,
        src_cs: i32,
        dst_cs: i32,
        out_ptr: *mut f64,
        out_len: i32,
    ) -> i32;

    fn mengxi_srgb_to_oklab(
        data_len: i32,
        data_ptr: *const f64,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;

    fn mengxi_oklab_to_srgb(
        data_len: i32,
        data_ptr: *const f64,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;

    fn mengxi_acescct_to_oklab(
        data_len: i32,
        data_ptr: *const f64,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;

    fn mengxi_oklab_to_acescct(
        data_len: i32,
        data_ptr: *const f64,
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

/// Generate a 3D LUT by applying an ACES color space transform across a uniform grid.
///
/// # Arguments
/// * `grid_size` — Number of samples per axis (e.g., 17, 33). Must be 2–256.
/// * `src` — Source color space.
/// * `dst` — Destination color space.
///
/// # Returns
/// * `Ok(Vec<f64>)` with `grid_size^3 * 3` values in red-fastest order on success.
/// * `Err(ColorScienceError)` if parameters are invalid or FFI fails.
pub fn generate_lut(
    grid_size: u32,
    src: ACESColorSpace,
    dst: ACESColorSpace,
) -> Result<Vec<f64>, ColorScienceError> {
    if grid_size < 2 {
        return Err(ColorScienceError::FfiError(
            -1,
            format!("grid_size {} must be >= 2", grid_size),
        ));
    }
    if grid_size > 256 {
        return Err(ColorScienceError::FfiError(
            -1,
            format!("grid_size {} must be <= 256", grid_size),
        ));
    }
    if src.is_log() {
        return Err(ColorScienceError::LogDataRequiresConversion(format!(
            "source color space {:?} is log-encoded; LUT generation requires linear input",
            src
        )));
    }

    let total = (grid_size as usize) * (grid_size as usize) * (grid_size as usize) * 3;
    let mut output = vec![0.0_f64; total];

    let result = unsafe {
        mengxi_generate_lut(
            grid_size as i32,
            src.as_int(),
            dst.as_int(),
            output.as_mut_ptr(),
            total as i32,
        )
    };

    if result < 0 {
        return Err(ColorScienceError::FfiError(
            result,
            format!("generate_lut grid_size={} {:?} -> {:?}", grid_size, src, dst),
        ));
    }

    Ok(output)
}

/// Convert sRGB pixel data to Oklab color space via FFI.
///
/// # Arguments
/// * `pixel_data` — Interleaved sRGB values [R0,G0,B0, R1,G1,B1, ...], length divisible by 3.
///
/// # Returns
/// * `Ok(Vec<f64>)` with interleaved Oklab values [L0,a0,b0, L1,a1,b1, ...].
/// * `Err(ColorScienceError)` if data is invalid or FFI fails.
pub fn srgb_to_oklab(pixel_data: &[f64]) -> Result<Vec<f64>, ColorScienceError> {
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

    let mut output = vec![0.0_f64; pixel_data.len()];

    let result = unsafe {
        mengxi_srgb_to_oklab(
            pixel_data.len() as i32,
            pixel_data.as_ptr(),
            output.len() as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScienceError::FfiError(
            result,
            "srgb_to_oklab".to_string(),
        ));
    }

    Ok(output)
}

/// Convert Oklab pixel data to sRGB color space via FFI.
///
/// # Arguments
/// * `oklab_data` — Interleaved Oklab values [L0,a0,b0, L1,a1,b1, ...], length divisible by 3.
///
/// # Returns
/// * `Ok(Vec<f64>)` with interleaved sRGB values [R0,G0,B0, R1,G1,B1, ...].
/// * `Err(ColorScienceError)` if data is invalid or FFI fails.
pub fn oklab_to_srgb(oklab_data: &[f64]) -> Result<Vec<f64>, ColorScienceError> {
    if oklab_data.len() < 3 {
        return Err(ColorScienceError::FfiError(
            -1,
            "oklab data must contain at least 3 values (1 pixel)".to_string(),
        ));
    }
    if oklab_data.len() > i32::MAX as usize {
        return Err(ColorScienceError::FfiError(
            -1,
            format!(
                "oklab data too large for FFI ({} elements, max {})",
                oklab_data.len(),
                i32::MAX
            ),
        ));
    }
    if oklab_data.len() % 3 != 0 {
        return Err(ColorScienceError::FfiError(
            -1,
            "oklab data length must be divisible by 3 (L,a,b)".to_string(),
        ));
    }

    let mut output = vec![0.0_f64; oklab_data.len()];

    let result = unsafe {
        mengxi_oklab_to_srgb(
            oklab_data.len() as i32,
            oklab_data.as_ptr(),
            output.len() as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScienceError::FfiError(
            result,
            "oklab_to_srgb".to_string(),
        ));
    }

    Ok(output)
}

/// Convert ACEScct pixel data to Oklab color space via FFI.
///
/// # Arguments
/// * `pixel_data` — Interleaved ACEScct values [R0,G0,B0, R1,G1,B1, ...], length divisible by 3.
///
/// # Returns
/// * `Ok(Vec<f64>)` with interleaved Oklab values [L0,a0,b0, L1,a1,b1, ...].
/// * `Err(ColorScienceError)` if data is invalid or FFI fails.
pub fn acescct_to_oklab(pixel_data: &[f64]) -> Result<Vec<f64>, ColorScienceError> {
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

    let mut output = vec![0.0_f64; pixel_data.len()];

    let result = unsafe {
        mengxi_acescct_to_oklab(
            pixel_data.len() as i32,
            pixel_data.as_ptr(),
            output.len() as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScienceError::FfiError(
            result,
            "acescct_to_oklab".to_string(),
        ));
    }

    Ok(output)
}

/// Convert Oklab pixel data to ACEScct color space via FFI.
///
/// # Arguments
/// * `oklab_data` — Interleaved Oklab values [L0,a0,b0, L1,a1,b1, ...], length divisible by 3.
///
/// # Returns
/// * `Ok(Vec<f64>)` with interleaved ACEScct values [R0,G0,B0, R1,G1,B1, ...].
/// * `Err(ColorScienceError)` if data is invalid or FFI fails.
pub fn oklab_to_acescct(oklab_data: &[f64]) -> Result<Vec<f64>, ColorScienceError> {
    if oklab_data.len() < 3 {
        return Err(ColorScienceError::FfiError(
            -1,
            "oklab data must contain at least 3 values (1 pixel)".to_string(),
        ));
    }
    if oklab_data.len() > i32::MAX as usize {
        return Err(ColorScienceError::FfiError(
            -1,
            format!(
                "oklab data too large for FFI ({} elements, max {})",
                oklab_data.len(),
                i32::MAX
            ),
        ));
    }
    if oklab_data.len() % 3 != 0 {
        return Err(ColorScienceError::FfiError(
            -1,
            "oklab data length must be divisible by 3 (L,a,b)".to_string(),
        ));
    }

    let mut output = vec![0.0_f64; oklab_data.len()];

    let result = unsafe {
        mengxi_oklab_to_acescct(
            oklab_data.len() as i32,
            oklab_data.as_ptr(),
            output.len() as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScienceError::FfiError(
            result,
            "oklab_to_acescct".to_string(),
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

    // -- generate_lut tests --

    #[test]
    fn test_generate_lut_size_2() {
        let result = generate_lut(2, ACESColorSpace::ACEScg, ACESColorSpace::Rec709);
        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 24); // 2^3 * 3
    }

    #[test]
    fn test_generate_lut_size_33() {
        let result = generate_lut(33, ACESColorSpace::ACEScg, ACESColorSpace::Rec709);
        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 107811); // 33^3 * 3
    }

    #[test]
    fn test_generate_lut_identity() {
        // Grid size 2: indices normalize to 0.0 and 1.0
        let result = generate_lut(2, ACESColorSpace::ACEScg, ACESColorSpace::ACEScg);
        assert!(result.is_ok());
        let v = result.unwrap();
        // (0,0,0) → (0,0,0)
        assert!((v[0]).abs() < 1e-10);
        assert!((v[1]).abs() < 1e-10);
        assert!((v[2]).abs() < 1e-10);
        // (1,0,0) → (1,0,0)
        assert!((v[3] - 1.0).abs() < 1e-10);
        assert!((v[4]).abs() < 1e-10);
        assert!((v[5]).abs() < 1e-10);
        // (1,1,1) → (1,1,1)
        assert!((v[21] - 1.0).abs() < 1e-10);
        assert!((v[22] - 1.0).abs() < 1e-10);
        assert!((v[23] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_generate_lut_black() {
        let result = generate_lut(2, ACESColorSpace::ACEScg, ACESColorSpace::Rec709);
        assert!(result.is_ok());
        let v = result.unwrap();
        assert!((v[0]).abs() < 1e-10);
        assert!((v[1]).abs() < 1e-10);
        assert!((v[2]).abs() < 1e-10);
    }

    #[test]
    fn test_generate_lut_grid_size_too_small() {
        let result = generate_lut(1, ACESColorSpace::ACEScg, ACESColorSpace::Rec709);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_lut_grid_size_too_large() {
        let result = generate_lut(257, ACESColorSpace::ACEScg, ACESColorSpace::Rec709);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_lut_log_rejected() {
        let result = generate_lut(2, ACESColorSpace::ACEScct, ACESColorSpace::Rec709);
        assert!(result.is_err());
        match result.unwrap_err() {
            ColorScienceError::LogDataRequiresConversion(msg) => {
                assert!(msg.contains("log-encoded"));
            }
            other => panic!("Expected LogDataRequiresConversion, got: {:?}", other),
        }
    }

    #[test]
    fn test_generate_lut_values_in_range() {
        let result = generate_lut(5, ACESColorSpace::ACEScg, ACESColorSpace::Rec709);
        assert!(result.is_ok());
        let v = result.unwrap();
        for &val in &v {
            assert!(val.is_finite(), "LUT value is not finite: {}", val);
        }
    }

    // -- sRGB ↔ Oklab tests --

    #[test]
    fn test_srgb_to_oklab_white() {
        let data = [1.0_f64, 1.0, 1.0];
        let result = srgb_to_oklab(&data).unwrap();
        assert!((result[0] - 1.0).abs() < 1e-4, "L should be ~1.0, got {}", result[0]);
        assert!(result[1].abs() < 1e-4, "a should be ~0.0, got {}", result[1]);
        assert!(result[2].abs() < 1e-4, "b should be ~0.0, got {}", result[2]);
    }

    #[test]
    fn test_srgb_to_oklab_black() {
        let data = [0.0_f64, 0.0, 0.0];
        let result = srgb_to_oklab(&data).unwrap();
        assert!(result[0].is_finite(), "L should be finite");
        assert!(result[1].is_finite(), "a should be finite");
        assert!(result[2].is_finite(), "b should be finite");
    }

    #[test]
    fn test_srgb_to_oklab_red() {
        let data = [1.0_f64, 0.0, 0.0];
        let result = srgb_to_oklab(&data).unwrap();
        assert!(result[1] > 0.0, "Red should have positive a, got {}", result[1]);
    }

    #[test]
    fn test_srgb_to_oklab_green() {
        let data = [0.0_f64, 1.0, 0.0];
        let result = srgb_to_oklab(&data).unwrap();
        assert!(result[1] < 0.0, "Green should have negative a, got {}", result[1]);
    }

    #[test]
    fn test_oklab_to_srgb_roundtrip() {
        let colors = [
            [0.5_f64, 0.5, 0.5],
            [1.0_f64, 0.0, 0.0],
            [0.0_f64, 1.0, 0.0],
            [0.0_f64, 0.0, 1.0],
            [1.0_f64, 1.0, 1.0],
            [0.0_f64, 0.0, 0.0],
            [0.2_f64, 0.4, 0.8],
        ];
        for color in &colors {
            let oklab = srgb_to_oklab(color).unwrap();
            let back = oklab_to_srgb(&oklab).unwrap();
            for i in 0..3 {
                assert!(
                    (back[i] - color[i]).abs() < 1e-4,
                    "Round-trip failed for {:?}: channel {} = {} vs {}",
                    color,
                    i,
                    back[i],
                    color[i]
                );
            }
        }
    }

    #[test]
    fn test_srgb_to_oklab_multi_pixel() {
        let data = [1.0_f64, 0.0, 0.0, 0.0, 1.0, 0.0];
        let result = srgb_to_oklab(&data).unwrap();
        assert_eq!(result.len(), 6);
        assert!(result[1] > 0.0, "Red pixel a should be positive");
        assert!(result[4] < 0.0, "Green pixel a should be negative");
    }

    #[test]
    fn test_srgb_to_oklab_too_few_pixels() {
        let result = srgb_to_oklab(&[0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_srgb_to_oklab_not_divisible_by_3() {
        let result = srgb_to_oklab(&[0.5, 0.5, 0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_oklab_to_srgb_too_few_pixels() {
        let result = oklab_to_srgb(&[0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_oklab_to_srgb_roundtrip_preserves_black() {
        let data = [0.0_f64, 0.0, 0.0];
        let oklab = srgb_to_oklab(&data).unwrap();
        let back = oklab_to_srgb(&oklab).unwrap();
        for i in 0..3 {
            assert!(
                back[i].is_finite(),
                "Black round-trip channel {} should be finite, got {}",
                i,
                back[i]
            );
        }
    }

    #[test]
    fn test_oklab_to_srgb_white_identity() {
        let data = [1.0_f64, 0.0, 0.0];
        let back = oklab_to_srgb(&data).unwrap();
        assert!((back[0] - 1.0).abs() < 1e-4, "White should round-trip");
    }

    // -- ACEScct ↔ Oklab tests --

    #[test]
    fn test_acescct_to_oklab_achromatic() {
        let data = [0.5_f64, 0.5, 0.5];
        let result = acescct_to_oklab(&data).unwrap();
        assert!(result.len() == 3);
        // Achromatic: a and b should be near zero
        assert!(result[1].abs() < 1e-3, "a should be near 0 for achromatic, got {}", result[1]);
        assert!(result[2].abs() < 1e-3, "b should be near 0 for achromatic, got {}", result[2]);
    }

    #[test]
    fn test_acescct_to_oklab_black() {
        let data = [0.0_f64, 0.0, 0.0];
        let result = acescct_to_oklab(&data).unwrap();
        assert!(result[0].is_finite(), "L should be finite");
        assert!(result[1].is_finite(), "a should be finite");
        assert!(result[2].is_finite(), "b should be finite");
    }

    #[test]
    fn test_acescct_roundtrip() {
        let colors = [
            [0.413_f64, 0.413, 0.413],  // mid-gray
            [1.0_f64, 0.0, 0.0],           // saturated red
            [0.0_f64, 1.0, 0.0],           // saturated green
            [0.0_f64, 0.0, 1.0],           // saturated blue
            [0.5_f64, 0.5, 0.5],           // gray
            [0.0_f64, 0.0, 0.0],           // black
        ];
        for color in &colors {
            let oklab = acescct_to_oklab(color).unwrap();
            let back = oklab_to_acescct(&oklab).unwrap();
            for i in 0..3 {
                assert!(
                    (back[i] - color[i]).abs() < 1e-4,
                    "ACEScct round-trip failed for {:?}: channel {} = {} vs {}",
                    color, i, back[i], color[i]
                );
            }
        }
    }

    #[test]
    fn test_acescct_to_oklab_multi_pixel() {
        let data = [0.5_f64, 0.5, 0.5, 1.0, 0.0, 0.0];
        let result = acescct_to_oklab(&data).unwrap();
        assert_eq!(result.len(), 6);
        // Achromatic: a and b should be near zero
        assert!(result[1].abs() < 1e-3);
        assert!(result[2].abs() < 1e-3);
    }

    #[test]
    fn test_acescct_to_oklab_too_few_pixels() {
        let result = acescct_to_oklab(&[0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_acescct_to_oklab_not_divisible_by_3() {
        let result = acescct_to_oklab(&[0.5, 0.5, 0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_oklab_to_acescct_too_few_pixels() {
        let result = oklab_to_acescct(&[0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_oklab_to_acescct_not_divisible_by_3() {
        let result = oklab_to_acescct(&[0.5, 0.5, 0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_acescct_roundtrip_preserves_black() {
        let data = [0.0_f64, 0.0, 0.0];
        let oklab = acescct_to_oklab(&data).unwrap();
        let back = oklab_to_acescct(&oklab).unwrap();
        for i in 0..3 {
            assert!(
                back[i].is_finite(),
                "Black round-trip channel {} should be finite, got {}",
                i, back[i]
            );
        }
    }
}
