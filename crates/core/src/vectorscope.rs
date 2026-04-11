// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum VectorscopeError {
    #[error("VECTORSCOPE_FFI_ERROR -- {0}")]
    FfiError(String),
    #[error("VECTORSCOPE_INVALID_INPUT -- {0}")]
    InvalidInput(String),
}

type Result<T> = std::result::Result<T, VectorscopeError>;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

#[cfg(moonbit_ffi)]
extern "C" {
    fn mengxi_compute_vectorscope_density(
        strip_len: i32,
        strip_ptr: *const f64,
        width: i32,
        height: i32,
        angle_bins: i32,
        radius_bins: i32,
        max_chroma_permille: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

 // ---------------------------------------------------------------------------
 // Structs
 // ---------------------------------------------------------------------------

/// Polar density grid from vectorscope analysis.
#[derive(Debug, Clone)]
pub struct VectorscopeDensity {
    pub angle_bins: usize,
    pub radius_bins: usize,
    pub grid: Vec<f64>, // normalized [0, 1]
}

 // ---------------------------------------------------------------------------
 // Safe wrappers
 // ---------------------------------------------------------------------------

 /// Compute vectorscope polar density grid from fingerprint strip data.
 ///
 /// Each pixel is converted to Oklab, its hue angle and chroma are computed,
 /// and binned into a polar grid of `angle_bins × radius_bins`.
 /// The result is normalized to [0, 1].
 #[cfg(moonbit_ffi)]
 pub fn compute_vectorscope_density(
     strip: &[f64],
     width: usize,
     height: usize,
     angle_bins: usize,
     radius_bins: usize,
     max_chroma_permille: usize,
 ) -> Result<VectorscopeDensity> {
     let expected = width * height * 3;
     if strip.len() < expected {
         return Err(VectorscopeError::InvalidInput(format!(
             "strip has {} elements, expected {} (width={} height={} * 3)",
             strip.len(), expected, width, height
         )));
     }
     if angle_bins == 0 || radius_bins == 0 {
         return Err(VectorscopeError::InvalidInput(
             "angle_bins and radius_bins must be > 0".to_string(),
         ));
     }

     let out_size = angle_bins * radius_bins;
     let mut output = vec![0.0_f64; out_size];

     let result = unsafe {
         mengxi_compute_vectorscope_density(
             strip.len() as i32,
             strip.as_ptr(),
             width as i32,
             height as i32,
             angle_bins as i32,
             radius_bins as i32,
             max_chroma_permille as i32,
             out_size as i32,
             output.as_mut_ptr(),
         )
     };

     if result < 0 {
         return Err(VectorscopeError::FfiError(format!(
             "MoonBit vectorscope returned error code {}",
             result
         )));
     }

     Ok(VectorscopeDensity {
         angle_bins,
         radius_bins,
         grid: output,
     })
 }

 #[cfg(not(moonbit_ffi))]
 pub fn compute_vectorscope_density(
     _strip: &[f64],
     _width: usize,
     _height: usize,
     _angle_bins: usize,
     _radius_bins: usize,
     _max_chroma_permille: usize,
 ) -> Result<VectorscopeDensity> {
     Err(VectorscopeError::FfiError("MoonBit FFI not available".to_string()))
 }

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vectorscope_ffi_error_display() {
        let err = VectorscopeError::FfiError("test error".to_string());
        let display = format!("{}", err);
        assert!(display.contains("VECTORSCOPE_FFI_ERROR"));
        assert!(display.contains("test error"));
    }

    #[test]
    fn test_vectorscope_invalid_input_display() {
        let err = VectorscopeError::InvalidInput("invalid strip".to_string());
        let display = format!("{}", err);
        assert!(display.contains("VECTORSCOPE_INVALID_INPUT"));
        assert!(display.contains("invalid strip"));
    }

    #[test]
    fn test_density_struct_default() {
        let density = VectorscopeDensity {
            angle_bins: 36,
            radius_bins: 16,
            grid: vec![0.0_f64; 36 * 16],
        };
        assert_eq!(density.angle_bins, 36);
        assert_eq!(density.radius_bins, 16);
        assert_eq!(density.grid.len(), 36 * 16);
        assert!(density.grid.iter().all(|&x| x == 0.0));
    }

    #[test]
    #[cfg(not(moonbit_ffi))]
    fn test_unavailable_returns_err() {
        let result = compute_vectorscope_density(&[], 1, 1, 36, 16, 100);
        assert!(result.is_err());
        match result.unwrap_err() {
            VectorscopeError::FfiError(msg) => {
                assert!(msg.contains("MoonBit FFI not available"));
            }
            _ => panic!("Expected FfiError"),
        }
    }
}
