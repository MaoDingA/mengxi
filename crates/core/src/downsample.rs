/// Image downsampling for feature extraction pipeline.
///
/// Area-average downsampling preserves color distribution accuracy for
/// histogram-based features while dramatically reducing pixel count
/// (e.g., 4K 4096x2160 → 512x270, ~34x reduction).

/// Maximum dimension for downsampling (FR9: 512x512).
pub const MAX_DIMENSION: usize = 512;

/// Errors for downsampling operations.
#[derive(Debug)]
pub enum DownsampleError {
    InvalidDimensions { width: usize, height: usize, reason: String },
    DataLengthMismatch { expected: usize, actual: usize },
}

impl std::fmt::Display for DownsampleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownsampleError::InvalidDimensions { width, height, reason } => {
                write!(f, "DOWNSAMPLE_INVALID_DIMENSIONS -- width={} height={} {}", width, height, reason)
            }
            DownsampleError::DataLengthMismatch { expected, actual } => {
                write!(f, "DOWNSAMPLE_DATA_LENGTH_MISMATCH -- expected {} bytes, got {}", expected, actual)
            }
        }
    }
}

impl std::error::Error for DownsampleError {}

/// Downsample interleaved RGB f64 data using area-average.
///
/// # Arguments
/// * `data` - Interleaved RGB f64 pixel data, length = width * height * 3
/// * `width` - Original image width in pixels
/// * `height` - Original image height in pixels
/// * `max_dim` - Maximum dimension for the output (e.g., 512)
///
/// # Returns
/// * `Ok((Vec<f64>, usize, usize))` - (downsampled interleaved RGB, new_width, new_height)
///
/// # Errors
/// Returns `DownsampleError` if width/height is 0 or data length doesn't match.
pub fn downsample_rgb(
    data: &[f64],
    width: usize,
    height: usize,
    max_dim: usize,
) -> Result<(Vec<f64>, usize, usize), DownsampleError> {
    if width == 0 || height == 0 {
        return Err(DownsampleError::InvalidDimensions {
            width,
            height,
            reason: "width and height must be > 0".to_string(),
        });
    }

    let expected_len = width * height * 3;
    if data.len() != expected_len {
        return Err(DownsampleError::DataLengthMismatch {
            expected: expected_len,
            actual: data.len(),
        });
    }

    // No downsampling needed
    let max_side = width.max(height);
    if max_side <= max_dim {
        return Ok((data.to_vec(), width, height));
    }

    // Compute target dimensions preserving aspect ratio
    let scale = max_dim as f64 / max_side as f64;
    let new_w = (width as f64 * scale).round() as usize;
    let new_h = (height as f64 * scale).round() as usize;
    let new_w = new_w.max(1);
    let new_h = new_h.max(1);

    let mut output = vec![0.0_f64; new_w * new_h * 3];

    // Area-average: for each target pixel, average all source pixels in the region
    // Ceiling division for end boundary ensures all source pixels are covered
    for ty in 0..new_h {
        let src_y_start = (ty * height) / new_h;
        let src_y_end = ((ty + 1) * height + new_h - 1) / new_h.min(height);

        for tx in 0..new_w {
            let src_x_start = (tx * width) / new_w;
            let src_x_end = ((tx + 1) * width + new_w - 1) / new_w.min(width);

            let mut sum_r = 0.0_f64;
            let mut sum_g = 0.0_f64;
            let mut sum_b = 0.0_f64;
            let mut count = 0usize;

            for sy in src_y_start..src_y_end {
                for sx in src_x_start..src_x_end {
                    let src_idx = (sy * width + sx) * 3;
                    sum_r += data[src_idx];
                    sum_g += data[src_idx + 1];
                    sum_b += data[src_idx + 2];
                    count += 1;
                }
            }

            let out_idx = (ty * new_w + tx) * 3;
            output[out_idx] = sum_r / count as f64;
            output[out_idx + 1] = sum_g / count as f64;
            output[out_idx + 2] = sum_b / count as f64;
        }
    }

    Ok((output, new_w, new_h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_op_small_image() {
        let data = vec![0.5, 0.3, 0.1, 0.7, 0.2, 0.9]; // 1x2 image
        let (result, w, h) = downsample_rgb(&data, 1, 2, 512).unwrap();
        assert_eq!(w, 1);
        assert_eq!(h, 2);
        assert_eq!(result, data);
    }

    #[test]
    fn no_op_exact_boundary() {
        let data = vec![0.0_f64; 512 * 512 * 3];
        let (result, w, h) = downsample_rgb(&data, 512, 512, 512).unwrap();
        assert_eq!(w, 512);
        assert_eq!(h, 512);
        assert_eq!(result.len(), data.len());
    }

    #[test]
    fn downsample_1024x1024_to_512x512() {
        // Uniform color — all pixels (0.5, 0.3, 0.1)
        let pixel = [0.5_f64, 0.3, 0.1];
        let data: Vec<f64> = pixel.iter().cycle().take(1024 * 1024 * 3).copied().collect();
        let (result, w, h) = downsample_rgb(&data, 1024, 1024, 512).unwrap();
        assert_eq!(w, 512);
        assert_eq!(h, 512);
        assert_eq!(result.len(), 512 * 512 * 3);
        // Uniform image stays uniform
        for i in 0..result.len() {
            assert!(
                (result[i] - [0.5, 0.3, 0.1][i % 3]).abs() < 1e-10,
                "pixel {} deviated: {}",
                i,
                result[i]
            );
        }
    }

    #[test]
    fn downsample_non_square_4096x2160() {
        // 4K DCI: 4096x2160 → 512x270
        let data = vec![1.0_f64; 4096 * 2160 * 3];
        let (result, w, h) = downsample_rgb(&data, 4096, 2160, 512).unwrap();
        assert_eq!(w, 512);
        assert_eq!(h, 270);
        assert_eq!(result.len(), 512 * 270 * 3);
        // Uniform image stays uniform
        for &v in &result {
            assert!((v - 1.0).abs() < 1e-10);
        }
    }

    #[test]
    fn downsample_preserves_aspect_ratio() {
        // 1000x500 → 512x256 (max_side=1000 > 512)
        let data = vec![0.0_f64; 1000 * 500 * 3];
        let (_, w, h) = downsample_rgb(&data, 1000, 500, 512).unwrap();
        assert_eq!(w, 512);
        assert_eq!(h, 256);
    }

    #[test]
    fn downsample_tall_image() {
        // 500x1000 → 256x512 (max_side=1000 > 512)
        let data = vec![0.0_f64; 500 * 1000 * 3];
        let (_, w, h) = downsample_rgb(&data, 500, 1000, 512).unwrap();
        assert_eq!(w, 256);
        assert_eq!(h, 512);
    }

    #[test]
    fn area_average_checkerboard() {
        // 4x4 checkerboard: even pixels (1.0, 0.0, 0.0), odd pixels (0.0, 1.0, 1.0)
        let mut data = Vec::with_capacity(4 * 4 * 3);
        for i in 0..16 {
            if (i + (i / 4)) % 2 == 0 {
                data.extend_from_slice(&[1.0, 0.0, 0.0]);
            } else {
                data.extend_from_slice(&[0.0, 1.0, 1.0]);
            }
        }
        // Downsample 4x4 → 2x2: each 2x2 block has 2 of each color
        let (result, w, h) = downsample_rgb(&data, 4, 4, 2).unwrap();
        assert_eq!(w, 2);
        assert_eq!(h, 2);
        assert_eq!(result.len(), 2 * 2 * 3);
        // All 4 output pixels should be (0.5, 0.5, 0.5) — checkerboard symmetry
        for pixel in 0..4 {
            let base = pixel * 3;
            assert_eq!(result[base], 0.5, "pixel {} R", pixel);
            assert_eq!(result[base + 1], 0.5, "pixel {} G", pixel);
            assert_eq!(result[base + 2], 0.5, "pixel {} B", pixel);
        }
    }

    #[test]
    fn single_pixel_image() {
        let data = vec![0.25, 0.50, 0.75];
        let (result, w, h) = downsample_rgb(&data, 1, 1, 512).unwrap();
        assert_eq!(w, 1);
        assert_eq!(h, 1);
        assert_eq!(result, data);
    }

    #[test]
    fn two_pixel_wide_image() {
        let data = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]; // 2x1
        let (result, w, h) = downsample_rgb(&data, 2, 1, 512).unwrap();
        assert_eq!(w, 2);
        assert_eq!(h, 1);
        assert_eq!(result, data); // no downsampling needed
    }

    #[test]
    fn downsample_4x2_to_2x1() {
        // 4x2 image → max_dim=2 → 2x1
        // Row 0: (1,0,0) (0,1,0) (0,0,1) (0,0,0)
        // Row 1: (0,0,0) (0,0,0) (0,0,0) (0,0,0)
        let data = vec![
            1.0, 0.0, 0.0,  0.0, 1.0, 0.0,  0.0, 0.0, 1.0,  0.0, 0.0, 0.0,
            0.0, 0.0, 0.0,  0.0, 0.0, 0.0,  0.0, 0.0, 0.0,  0.0, 0.0, 0.0,
        ];
        let (result, w, h) = downsample_rgb(&data, 4, 2, 2).unwrap();
        assert_eq!(w, 2);
        assert_eq!(h, 1);
        assert_eq!(result.len(), 2 * 1 * 3);
        // Pixel (0,0): cols [0,2), rows [0,2) → 4 pixels: (1,0,0) (0,1,0) (0,0,0) (0,0,0)
        assert!((result[0] - 0.25).abs() < 1e-10, "R of pixel 0");
        assert!((result[1] - 0.25).abs() < 1e-10, "G of pixel 0");
        assert!((result[2] - 0.0).abs() < 1e-10, "B of pixel 0");
        // Pixel (1,0): cols [2,4), rows [0,2) → 4 pixels: (0,0,1) (0,0,0) (0,0,0) (0,0,0)
        assert!((result[3] - 0.0).abs() < 1e-10, "R of pixel 1");
        assert!((result[4] - 0.0).abs() < 1e-10, "G of pixel 1");
        assert!((result[5] - 0.25).abs() < 1e-10, "B of pixel 1");
    }

    #[test]
    fn error_on_wrong_data_length() {
        let data = vec![0.0_f64; 10]; // not divisible by 3 or wrong for 2x2
        let result = downsample_rgb(&data, 2, 2, 512);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("DOWNSAMPLE_DATA_LENGTH_MISMATCH"), "error: {}", err);
    }

    #[test]
    fn error_on_zero_dimensions() {
        let data = vec![0.0_f64; 0];
        let result = downsample_rgb(&data, 0, 0, 512);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("DOWNSAMPLE_INVALID_DIMENSIONS"), "error: {}", err);
    }

    #[test]
    fn error_display_format() {
        let err = DownsampleError::InvalidDimensions {
            width: 0,
            height: 5,
            reason: "test".to_string(),
        };
        assert!(err.to_string().starts_with("DOWNSAMPLE_INVALID_DIMENSIONS"));

        let err = DownsampleError::DataLengthMismatch {
            expected: 12,
            actual: 10,
        };
        assert!(err.to_string().starts_with("DOWNSAMPLE_DATA_LENGTH_MISMATCH"));
    }
}
