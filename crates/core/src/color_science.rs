// color_science.rs — FFI bridge to MoonBit ACES color science engine

pub use crate::grading_features::GradingFeatures;

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
    /// Failed to deserialize grading features BLOB.
    GradingFeatureDecodeError(String),
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
            ColorScienceError::GradingFeatureDecodeError(msg) => {
                write!(f, "GRADING_FEATURE_DECODE_ERROR -- {}", msg)
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

    fn mengxi_linear_to_oklab(
        data_len: i32,
        data_ptr: *const f64,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;

    fn mengxi_oklab_to_linear(
        data_len: i32,
        data_ptr: *const f64,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;

    fn mengxi_bhattacharyya_distance(
        query_hist: *const f64,
        candidate_hist: *const f64,
        hist_len: i32,
        channels: i32,
        out_score: *mut f64,
    ) -> i32;

    fn mengxi_extract_grading_features(
        pixel_len: i32,
        pixel_ptr: *const f64,
        color_space_tag: i32,
        hist_l_ptr: *mut f64,
        hist_a_ptr: *mut f64,
        hist_b_ptr: *mut f64,
        moments_ptr: *mut f64,
        out_hist_len: *mut f64,
        out_moments_len: *mut f64,
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

/// Convert Linear sRGB pixel data to Oklab color space via FFI.
///
/// # Arguments
/// * `pixel_data` — Interleaved Linear sRGB values [R0,G0,B0, R1,G1,B1, ...], length divisible by 3.
///
/// # Returns
/// * `Ok(Vec<f64>)` with interleaved Oklab values [L0,a0,b0, L1,a1,b1, ...].
/// * `Err(ColorScienceError)` if data is invalid or FFI fails.
pub fn linear_to_oklab(pixel_data: &[f64]) -> Result<Vec<f64>, ColorScienceError> {
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
        mengxi_linear_to_oklab(
            pixel_data.len() as i32,
            pixel_data.as_ptr(),
            output.len() as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScienceError::FfiError(
            result,
            "linear_to_oklab".to_string(),
        ));
    }

    Ok(output)
}

/// Convert Oklab pixel data to Linear sRGB color space via FFI.
///
/// # Arguments
/// * `oklab_data` — Interleaved Oklab values [L0,a0,b0, L1,a1,b1, ...], length divisible by 3.
///
/// # Returns
/// * `Ok(Vec<f64>)` with interleaved Linear sRGB values [R0,G0,B0, R1,G1,B1, ...].
/// * `Err(ColorScienceError)` if data is invalid or FFI fails.
pub fn oklab_to_linear(oklab_data: &[f64]) -> Result<Vec<f64>, ColorScienceError> {
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
        mengxi_oklab_to_linear(
            oklab_data.len() as i32,
            oklab_data.as_ptr(),
            output.len() as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScienceError::FfiError(
            result,
            "oklab_to_linear".to_string(),
        ));
    }

    Ok(output)
}

/// Extract grading features (histograms + color moments) from Oklab pixel data via FFI.
///
/// # Arguments
/// * `oklab_data` — Interleaved Oklab values [L0,a0,b0, L1,a1,b1, ...], length divisible by 3.
/// * `color_space_tag` — Source color space tag (0=Linear, 1=Log/ACEScct, 2=Video/sRGB).
///
/// # Returns
/// * `Ok(GradingFeatures)` with 3 histograms (64 bins each) and 6 moments.
/// * `Err(ColorScienceError)` if data is invalid or FFI fails.
pub fn extract_grading_features(
    oklab_data: &[f64],
    color_space_tag: i32,
) -> Result<GradingFeatures, ColorScienceError> {
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

    // Reject NaN/Inf input at the Rust boundary — MoonBit computation would
    // produce NaN moments and unpredictable histogram bins for non-finite values.
    if !oklab_data.iter().all(|v| v.is_finite()) {
        return Err(ColorScienceError::FfiError(
            -3,
            "oklab data contains NaN or Inf values".to_string(),
        ));
    }

    let mut hist_l = vec![0.0_f64; GradingFeatures::HIST_BINS];
    let mut hist_a = vec![0.0_f64; GradingFeatures::HIST_BINS];
    let mut hist_b = vec![0.0_f64; GradingFeatures::HIST_BINS];
    let mut moments = [0.0_f64; GradingFeatures::MOMENTS_COUNT];
    let mut out_hist_len = [0.0_f64; 1];
    let mut out_moments_len = [0.0_f64; 1];

    let result = unsafe {
        mengxi_extract_grading_features(
            oklab_data.len() as i32,
            oklab_data.as_ptr(),
            color_space_tag,
            hist_l.as_mut_ptr(),
            hist_a.as_mut_ptr(),
            hist_b.as_mut_ptr(),
            moments.as_mut_ptr(),
            out_hist_len.as_mut_ptr(),
            out_moments_len.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScienceError::FfiError(
            result,
            format!(
                "extract_grading_features pixels={} color_space_tag={}",
                oklab_data.len() / 3,
                color_space_tag
            ),
        ));
    }

    Ok(GradingFeatures {
        hist_l,
        hist_a,
        hist_b,
        moments,
    })
}

/// Compute Bhattacharyya similarity between two grading feature sets.
///
/// Normalizes histograms to probability distributions, then delegates to MoonBit FFI.
///
/// # Arguments
/// * `query` — Query grading features (histograms + moments).
/// * `candidate` — Candidate grading features (histograms + moments).
///
/// # Returns
/// * `Ok(f64)` similarity score in [0.0, 1.0] where 1.0 = identical distributions.
/// * `Err(ColorScienceError)` if normalization fails or FFI fails.
pub fn bhattacharyya_distance(
    query: &GradingFeatures,
    candidate: &GradingFeatures,
) -> Result<f64, ColorScienceError> {
    let hist_len = GradingFeatures::HIST_BINS;
    let channels = 3;

    // Normalize each histogram to probability distribution
    let normalize = |hist: &[f64]| -> Result<Vec<f64>, ColorScienceError> {
        let sum: f64 = hist.iter().sum();
        if sum <= 0.0 {
            // Zero-sum histogram: return uniform distribution (no similarity signal)
            let uniform = vec![1.0 / hist_len as f64; hist_len];
            return Ok(uniform);
        }
        if !sum.is_finite() {
            return Err(ColorScienceError::FfiError(
                -4,
                "histogram sum is not finite".to_string(),
            ));
        }
        Ok(hist.iter().map(|&v| v / sum).collect())
    };

    let q_l = normalize(&query.hist_l)?;
    let q_a = normalize(&query.hist_a)?;
    let q_b = normalize(&query.hist_b)?;
    let c_l = normalize(&candidate.hist_l)?;
    let c_a = normalize(&candidate.hist_a)?;
    let c_b = normalize(&candidate.hist_b)?;

    // Interleave into flat arrays: [L0..L63, a0..a63, b0..b63]
    let total_len = hist_len * channels;
    let mut query_flat = Vec::with_capacity(total_len);
    query_flat.extend_from_slice(&q_l);
    query_flat.extend_from_slice(&q_a);
    query_flat.extend_from_slice(&q_b);

    let mut candidate_flat = Vec::with_capacity(total_len);
    candidate_flat.extend_from_slice(&c_l);
    candidate_flat.extend_from_slice(&c_a);
    candidate_flat.extend_from_slice(&c_b);

    // Reject NaN/Inf after normalization
    if !query_flat.iter().all(|v| v.is_finite()) {
        return Err(ColorScienceError::FfiError(
            -4,
            "normalized query histogram contains NaN or Inf".to_string(),
        ));
    }
    if !candidate_flat.iter().all(|v| v.is_finite()) {
        return Err(ColorScienceError::FfiError(
            -4,
            "normalized candidate histogram contains NaN or Inf".to_string(),
        ));
    }

    let mut out_score = [0.0_f64; 1];

    let result = unsafe {
        mengxi_bhattacharyya_distance(
            query_flat.as_ptr(),
            candidate_flat.as_ptr(),
            hist_len as i32,
            channels as i32,
            out_score.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorScienceError::FfiError(
            result,
            "bhattacharyya_distance".to_string(),
        ));
    }

    Ok(out_score[0])
}

/// Convert interleaved RGB f64 pixel data to Oklab color space based on color space tag.
///
/// Dispatches to the appropriate conversion function:
/// - `"linear"` → `linear_to_oklab()`
/// - `"log"` → `acescct_to_oklab()`
/// - `"video"` → `srgb_to_oklab()`
///
/// # Arguments
/// * `pixel_data` - Interleaved RGB f64 pixel data, length must be divisible by 3
/// * `color_space_tag` - Color space identifier: "linear", "log", or "video"
///
/// # Errors
/// Returns `UNSUPPORTED_TRANSFORM` if `color_space_tag` is not one of the supported values.
pub fn rgb_to_oklab_batch(
    pixel_data: &[f64],
    color_space_tag: &str,
) -> Result<Vec<f64>, ColorScienceError> {
    match color_space_tag {
        "linear" => linear_to_oklab(pixel_data),
        "log" => acescct_to_oklab(pixel_data),
        "video" => srgb_to_oklab(pixel_data),
        other => Err(ColorScienceError::UnsupportedTransform(format!(
            "unsupported color space tag '{}' for Oklab conversion",
            other
        ))),
    }
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

    #[test]
    fn test_acescct_negative_input_no_nan() {
        let data = [-0.1_f64, -0.1, -0.1];
        let result = acescct_to_oklab(&data).unwrap();
        for (i, &val) in result.iter().enumerate() {
            assert!(
                val.is_finite(),
                "Negative input channel {} should be finite, got {}",
                i, val
            );
        }
    }

    #[test]
    fn test_acescct_minimum_code_value() {
        // Minimum code value (0.0729) maps to linear 0.0 — no round-trip expected
        let data = [0.0729055341958355_f64, 0.0729055341958355, 0.0729055341958355];
        let result = acescct_to_oklab(&data).unwrap();
        for (i, &val) in result.iter().enumerate() {
            assert!(val.is_finite(), "Min code value channel {} should be finite", i);
        }
        // Achromatic: a and b should be near zero
        assert!(result[1].abs() < 1e-3, "Min code value a should be near 0");
        assert!(result[2].abs() < 1e-3, "Min code value b should be near 0");
    }

    // -- Linear sRGB ↔ Oklab tests --

    #[test]
    fn test_linear_to_oklab_white() {
        let data = [1.0_f64, 1.0, 1.0];
        let result = linear_to_oklab(&data).unwrap();
        assert!((result[0] - 1.0).abs() < 1e-4, "L should be ~1.0, got {}", result[0]);
        assert!(result[1].abs() < 1e-4, "a should be ~0.0, got {}", result[1]);
        assert!(result[2].abs() < 1e-4, "b should be ~0.0, got {}", result[2]);
    }

    #[test]
    fn test_linear_to_oklab_black() {
        let data = [0.0_f64, 0.0, 0.0];
        let result = linear_to_oklab(&data).unwrap();
        assert!(result[0].is_finite(), "L should be finite");
        assert!(result[1].is_finite(), "a should be finite");
        assert!(result[2].is_finite(), "b should be finite");
        assert!(result[0].abs() < 1e-10, "L should be ~0 for black");
    }

    #[test]
    fn test_linear_to_oklab_achromatic() {
        // 18% scene-referred grey
        let data = [0.18_f64, 0.18, 0.18];
        let result = linear_to_oklab(&data).unwrap();
        assert!(result[1].abs() < 1e-3, "a should be near 0 for achromatic, got {}", result[1]);
        assert!(result[2].abs() < 1e-3, "b should be near 0 for achromatic, got {}", result[2]);
    }

    #[test]
    fn test_linear_to_oklab_red() {
        let data = [1.0_f64, 0.0, 0.0];
        let result = linear_to_oklab(&data).unwrap();
        assert!(result[1] > 0.0, "Red should have positive a, got {}", result[1]);
    }

    #[test]
    fn test_linear_to_oklab_green() {
        let data = [0.0_f64, 1.0, 0.0];
        let result = linear_to_oklab(&data).unwrap();
        assert!(result[1] < 0.0, "Green should have negative a, got {}", result[1]);
    }

    #[test]
    fn test_linear_roundtrip() {
        let colors = [
            [0.5_f64, 0.5, 0.5],     // mid-gray
            [1.0_f64, 0.0, 0.0],     // saturated red
            [0.0_f64, 1.0, 0.0],     // saturated green
            [0.0_f64, 0.0, 1.0],     // saturated blue
            [1.0_f64, 1.0, 1.0],     // white
            [0.0_f64, 0.0, 0.0],     // black
            [0.18_f64, 0.18, 0.18],  // 18% grey
            [0.2_f64, 0.4, 0.8],     // arbitrary color
        ];
        for color in &colors {
            let oklab = linear_to_oklab(color).unwrap();
            let back = oklab_to_linear(&oklab).unwrap();
            for i in 0..3 {
                assert!(
                    (back[i] - color[i]).abs() < 1e-4,
                    "Linear round-trip failed for {:?}: channel {} = {} vs {}",
                    color, i, back[i], color[i]
                );
            }
        }
    }

    #[test]
    fn test_linear_hdr_specular() {
        // HDR specular highlight (2.0, 2.0, 2.0) should produce finite achromatic values
        let data = [2.0_f64, 2.0, 2.0];
        let oklab = linear_to_oklab(&data).unwrap();
        for (i, &val) in oklab.iter().enumerate() {
            assert!(val.is_finite(), "HDR specular channel {} should be finite, got {}", i, val);
        }
        // Achromatic: a≈0, b≈0
        assert!(oklab[1].abs() < 1e-3, "HDR specular a should be near 0");
        assert!(oklab[2].abs() < 1e-3, "HDR specular b should be near 0");
        // L should be > 1.0 for HDR
        assert!(oklab[0] > 1.0, "HDR specular L should be > 1.0, got {}", oklab[0]);
        // Round-trip should preserve HDR values
        let back = oklab_to_linear(&oklab).unwrap();
        for i in 0..3 {
            assert!(
                (back[i] - 2.0).abs() < 1e-4,
                "HDR round-trip channel {} = {} vs 2.0",
                i, back[i]
            );
        }
    }

    #[test]
    fn test_linear_to_oklab_multi_pixel() {
        let data = [1.0_f64, 0.0, 0.0, 0.0, 1.0, 0.0];
        let result = linear_to_oklab(&data).unwrap();
        assert_eq!(result.len(), 6);
        assert!(result[1] > 0.0, "Red pixel a should be positive");
        assert!(result[4] < 0.0, "Green pixel a should be negative");
    }

    #[test]
    fn test_linear_to_oklab_too_few_pixels() {
        let result = linear_to_oklab(&[0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_linear_to_oklab_not_divisible_by_3() {
        let result = linear_to_oklab(&[0.5, 0.5, 0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_oklab_to_linear_too_few_pixels() {
        let result = oklab_to_linear(&[0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_oklab_to_linear_not_divisible_by_3() {
        let result = oklab_to_linear(&[0.5, 0.5, 0.5, 0.5]);
        assert!(result.is_err());
    }

    #[test]
    fn test_linear_roundtrip_preserves_black() {
        let data = [0.0_f64, 0.0, 0.0];
        let oklab = linear_to_oklab(&data).unwrap();
        let back = oklab_to_linear(&oklab).unwrap();
        for i in 0..3 {
            assert!(
                back[i].is_finite(),
                "Black round-trip channel {} should be finite, got {}",
                i, back[i]
            );
        }
    }

    #[test]
    fn test_linear_all_ones() {
        // All 1.0 input should produce Oklab ~(1, 0, 0)
        let data = [1.0_f64, 1.0, 1.0];
        let oklab = linear_to_oklab(&data).unwrap();
        let back = oklab_to_linear(&oklab).unwrap();
        for i in 0..3 {
            assert!(
                (back[i] - 1.0).abs() < 1e-4,
                "All-ones round-trip channel {} = {} vs 1.0",
                i, back[i]
            );
        }
    }

    // -- extract_grading_features tests --

    #[test]
    fn test_extract_grading_features_pure_black() {
        // Pure black in Oklab: L=0, a=0, b=0
        let oklab_data = [0.0_f64; 9]; // 3 pixels, all black
        let result = extract_grading_features(&oklab_data, 2).unwrap();
        assert_eq!(result.hist_l.len(), 64);
        assert_eq!(result.hist_a.len(), 64);
        assert_eq!(result.hist_b.len(), 64);
        assert_eq!(result.moments.len(), 6);
        // All black: L histogram should have all counts in bin 0 (L=0.0)
        assert_eq!(result.hist_l[0], 3.0, "All 3 black pixels in L bin 0");
        // a and b at 0.0 map to middle bin (32 in range [-0.5, 0.5])
        assert_eq!(result.hist_a[32], 3.0, "All 3 black pixels in a bin 32");
        assert_eq!(result.hist_b[32], 3.0, "All 3 black pixels in b bin 32");
        // Moments: L_mean=0.0, L_std=0.0
        assert!(result.moments[0].abs() < 1e-10, "L_mean should be 0 for black");
        assert!(result.moments[1].abs() < 1e-10, "L_std should be 0 for black");
    }

    #[test]
    fn test_extract_grading_features_solid_color() {
        // Solid color: all pixels are L=0.5, a=0.1, b=-0.1
        let pixels: [f64; 6] = [0.5, 0.1, -0.1, 0.5, 0.1, -0.1]; // 2 identical pixels
        let result = extract_grading_features(&pixels, 2).unwrap();
        // Moments: mean should match input, std should be 0
        assert!(
            (result.moments[0] - 0.5).abs() < 1e-10,
            "L_mean should be 0.5, got {}",
            result.moments[0]
        );
        assert!(
            (result.moments[2] - 0.1).abs() < 1e-10,
            "a_mean should be 0.1, got {}",
            result.moments[2]
        );
        assert!(
            (result.moments[4] - (-0.1)).abs() < 1e-10,
            "b_mean should be -0.1, got {}",
            result.moments[4]
        );
        // Std should be 0 for solid color
        assert!(result.moments[1].abs() < 1e-10, "L_std should be 0 for solid");
        assert!(result.moments[3].abs() < 1e-10, "a_std should be 0 for solid");
        assert!(result.moments[5].abs() < 1e-10, "b_std should be 0 for solid");
    }

    #[test]
    fn test_extract_grading_features_single_pixel() {
        let oklab_data = [0.75_f64, 0.0, 0.0]; // bright achromatic
        let result = extract_grading_features(&oklab_data, 2).unwrap();
        assert_eq!(result.moments.len(), 6);
        // Single pixel: mean = value, std = 0
        assert!(
            (result.moments[0] - 0.75).abs() < 1e-10,
            "L_mean should be 0.75"
        );
        assert!(result.moments[1].abs() < 1e-10, "L_std should be 0 for single pixel");
    }

    #[test]
    fn test_extract_grading_features_multi_pixel() {
        // 4 pixels: two different colors
        let oklab_data = [
            0.2_f64, 0.0, 0.0,  // dark achromatic
            0.8_f64, 0.0, 0.0,  // bright achromatic
            0.2_f64, 0.0, 0.0,  // dark achromatic
            0.8_f64, 0.0, 0.0,  // bright achromatic
        ];
        let result = extract_grading_features(&oklab_data, 2).unwrap();
        // L_mean = (0.2 + 0.8 + 0.2 + 0.8) / 4 = 0.5
        assert!(
            (result.moments[0] - 0.5).abs() < 1e-10,
            "L_mean should be 0.5, got {}",
            result.moments[0]
        );
        // L_std should be > 0 (pixels have different L values)
        assert!(
            result.moments[1] > 0.0,
            "L_std should be > 0 for varied pixels, got {}",
            result.moments[1]
        );
        // a_mean and b_mean should be 0 (all achromatic)
        assert!(result.moments[2].abs() < 1e-10, "a_mean should be 0 for achromatic");
        assert!(result.moments[4].abs() < 1e-10, "b_mean should be 0 for achromatic");
    }

    #[test]
    fn test_extract_grading_features_histogram_sums() {
        // Histogram bin counts should sum to pixel count
        let oklab_data = [
            0.1_f64, 0.05, -0.05,
            0.3_f64, 0.1, -0.1,
            0.5_f64, 0.2, -0.2,
            0.7_f64, -0.1, 0.1,
            0.9_f64, -0.2, 0.2,
        ];
        let result = extract_grading_features(&oklab_data, 2).unwrap();
        let l_sum: f64 = result.hist_l.iter().sum();
        let a_sum: f64 = result.hist_a.iter().sum();
        let b_sum: f64 = result.hist_b.iter().sum();
        assert!(
            (l_sum - 5.0).abs() < 1e-10,
            "L histogram sum should be 5.0 (pixel count), got {}",
            l_sum
        );
        assert!(
            (a_sum - 5.0).abs() < 1e-10,
            "a histogram sum should be 5.0, got {}",
            a_sum
        );
        assert!(
            (b_sum - 5.0).abs() < 1e-10,
            "b histogram sum should be 5.0, got {}",
            b_sum
        );
    }

    #[test]
    fn test_extract_grading_features_too_few_pixels() {
        let result = extract_grading_features(&[0.5, 0.5], 2);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_grading_features_not_divisible_by_3() {
        let result = extract_grading_features(&[0.5, 0.5, 0.5, 0.5], 2);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_grading_features_all_finite() {
        // All output values should be finite (no NaN/Inf)
        let oklab_data = [
            0.0_f64, 0.0, 0.0,
            1.0_f64, 0.0, 0.0,
            0.5_f64, 0.3, -0.2,
            0.5_f64, -0.3, 0.2,
            0.5_f64, 0.0, 0.0,
            0.1_f64, 0.001, -0.001,
        ];
        let result = extract_grading_features(&oklab_data, 2).unwrap();
        for (i, &val) in result.hist_l.iter().enumerate() {
            assert!(val.is_finite(), "hist_l[{}] should be finite, got {}", i, val);
        }
        for (i, &val) in result.hist_a.iter().enumerate() {
            assert!(val.is_finite(), "hist_a[{}] should be finite, got {}", i, val);
        }
        for (i, &val) in result.hist_b.iter().enumerate() {
            assert!(val.is_finite(), "hist_b[{}] should be finite, got {}", i, val);
        }
        for (i, &val) in result.moments.iter().enumerate() {
            assert!(val.is_finite(), "moments[{}] should be finite, got {}", i, val);
        }
    }

    #[test]
    fn test_extract_grading_features_large_dataset() {
        // 1000 pixels with varied values
        let mut oklab_data = Vec::with_capacity(3000);
        for i in 0..1000 {
            let l = (i as f64) / 999.0;
            let a = 0.3 * ((i as f64) / 999.0 * 2.0 - 1.0);
            let b = -0.3 * ((i as f64) / 999.0 * 2.0 - 1.0);
            oklab_data.push(l);
            oklab_data.push(a);
            oklab_data.push(b);
        }
        let result = extract_grading_features(&oklab_data, 2).unwrap();
        assert_eq!(result.hist_l.len(), 64);
        assert_eq!(result.moments.len(), 6);
        // L_mean should be ~0.5
        assert!(
            (result.moments[0] - 0.5).abs() < 0.01,
            "L_mean should be ~0.5, got {}",
            result.moments[0]
        );
        // L_std should be > 0 (values range from 0 to 1)
        assert!(result.moments[1] > 0.1, "L_std should be large for spread");
        // Histogram sums to 1000
        let l_sum: f64 = result.hist_l.iter().sum();
        assert!(
            (l_sum - 1000.0).abs() < 1e-10,
            "L histogram sum should be 1000.0, got {}",
            l_sum
        );
    }

    #[test]
    fn test_extract_grading_features_nan_input_rejected() {
        // NaN input should be rejected by the FFI function (returns -3)
        let oklab_data = [f64::NAN, 0.0, 0.0];
        let result = extract_grading_features(&oklab_data, 2);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_grading_features_mixed_nan_rejected() {
        // Any NaN in the input causes rejection (safer than silently dropping pixels)
        let oklab_data = [0.5_f64, 0.0, 0.0, f64::NAN, 0.0, 0.0];
        let result = extract_grading_features(&oklab_data, 2);
        assert!(result.is_err());
    }

    // --- rgb_to_oklab_batch tests ---

    #[test]
    fn test_bhattacharyya_distance_identical() {
        let gf = GradingFeatures {
            hist_l: vec![10.0; 64],
            hist_a: vec![5.0; 64],
            hist_b: vec![5.0; 64],
            moments: [0.5, 0.1, 0.0, 0.05, 0.0, 0.05],
        };
        let score = bhattacharyya_distance(&gf, &gf).unwrap();
        assert!((score - 1.0).abs() < 1e-10, "Identical features should score 1.0, got {}", score);
    }

    #[test]
    fn test_bhattacharyya_distance_orthogonal() {
        let mut q_l = vec![0.0; 64];
        let mut c_l = vec![0.0; 64];
        q_l[0] = 100.0;  // All mass in bin 0
        c_l[63] = 100.0; // All mass in last bin
        let query = GradingFeatures {
            hist_l: q_l,
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.5, 0.1, 0.0, 0.05, 0.0, 0.05],
        };
        let candidate = GradingFeatures {
            hist_l: c_l,
            hist_a: vec![1.0; 64],
            hist_b: vec![1.0; 64],
            moments: [0.5, 0.1, 0.0, 0.05, 0.0, 0.05],
        };
        let score = bhattacharyya_distance(&query, &candidate).unwrap();
        // L channel: no overlap → BC_L ≈ 0
        // a, b channels: identical → BC = 1.0
        // Average ≈ (0 + 1.0 + 1.0) / 3 ≈ 0.667
        assert!(score < 0.7, "L-orthogonal should score < 0.7, got {}", score);
        assert!(score > 0.6, "a/b identical should score > 0.6, got {}", score);
    }

    #[test]
    fn test_bhattacharyya_distance_all_zeros() {
        let gf = GradingFeatures {
            hist_l: vec![0.0; 64],
            hist_a: vec![0.0; 64],
            hist_b: vec![0.0; 64],
            moments: [0.0; 6],
        };
        // Zero-sum histograms → uniform normalization → identical → score 1.0
        let score = bhattacharyya_distance(&gf, &gf).unwrap();
        assert!((score - 1.0).abs() < 1e-10, "Both zero should normalize to uniform, got {}", score);
    }

    #[test]
    fn test_bhattacharyya_distance_with_nan_rejected() {
        let mut hist_l = vec![10.0; 64];
        hist_l[0] = f64::NAN;
        let gf = GradingFeatures {
            hist_l,
            hist_a: vec![5.0; 64],
            hist_b: vec![5.0; 64],
            moments: [0.0; 6],
        };
        let result = bhattacharyya_distance(&gf, &gf);
        assert!(result.is_err());
    }

    #[test]
    fn test_rgb_to_oklab_batch_linear() {
        let pixel = [0.5_f64, 0.5, 0.5];
        let result = rgb_to_oklab_batch(&pixel, "linear").unwrap();
        assert_eq!(result.len(), 3);
        // Linear 0.5 → Oklab L near 0.794 (sqrt(0.5^3) per Oklab formula)
        assert!((result[0] - 0.7937).abs() < 0.01, "L={}", result[0]);
    }

    #[test]
    fn test_rgb_to_oklab_batch_video() {
        let pixel = [0.5_f64, 0.5, 0.5];
        let result = rgb_to_oklab_batch(&pixel, "video").unwrap();
        assert_eq!(result.len(), 3);
        // sRGB 0.5 → Oklab L near 0.598
        assert!((result[0] - 0.5982).abs() < 0.01, "L={}", result[0]);
    }

    #[test]
    fn test_rgb_to_oklab_batch_log() {
        let pixel = [0.5_f64, 0.5, 0.5];
        let result = rgb_to_oklab_batch(&pixel, "log").unwrap();
        assert_eq!(result.len(), 3);
        // ACEScct 0.5 → Oklab L should be positive
        assert!(result[0] > 0.0, "L={}", result[0]);
    }

    #[test]
    fn test_rgb_to_oklab_batch_empty_returns_error() {
        // FFI functions require at least 1 pixel (3 values)
        let result = rgb_to_oklab_batch(&[], "linear");
        assert!(result.is_err());
    }

    #[test]
    fn test_rgb_to_oklab_batch_single_pixel() {
        let pixel = [0.25_f64, 0.5, 0.75];
        for tag in &["linear", "video", "log"] {
            let result = rgb_to_oklab_batch(&pixel, tag).unwrap();
            assert_eq!(result.len(), 3);
        }
    }

    #[test]
    fn test_rgb_to_oklab_batch_invalid_tag() {
        let pixel = [0.5_f64, 0.5, 0.5];
        let result = rgb_to_oklab_batch(&pixel, "unknown");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("COLOR_SCIENCE_UNSUPPORTED"), "error: {}", err);
    }
}
