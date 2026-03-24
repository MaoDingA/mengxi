/// Image downsampling for feature extraction pipeline.
///
/// Area-average downsampling preserves color distribution accuracy for
/// histogram-based features while dramatically reducing pixel count
/// (e.g., 4K 4096x2160 → 512x270, ~34x reduction).

/// Maximum dimension for downsampling (FR9: 512x512).
pub const MAX_DIMENSION: usize = 512;

/// Downsample interleaved RGB f64 data using area-average.
///
/// # Arguments
/// * `data` - Interleaved RGB f64 pixel data, length = width * height * 3
/// * `width` - Original image width in pixels
/// * `height` - Original image height in pixels
/// * `max_dim` - Maximum dimension for the output (e.g., 512)
///
/// # Returns
/// * `(Vec<f64>, usize, usize)` - (downsampled interleaved RGB, new_width, new_height)
///
/// # Panics
/// Panics if `data.len() != width * height * 3` or if width/height is 0.
pub fn downsample_rgb(
    data: &[f64],
    width: usize,
    height: usize,
    max_dim: usize,
) -> (Vec<f64>, usize, usize) {
    assert_eq!(
        data.len(),
        width * height * 3,
        "data length {} does not match width {} * height {} * 3",
        data.len(),
        width,
        height
    );
    assert!(width > 0 && height > 0, "width and height must be > 0");

    // No downsampling needed
    let max_side = width.max(height);
    if max_side <= max_dim {
        return (data.to_vec(), width, height);
    }

    // Compute target dimensions preserving aspect ratio
    let scale = max_dim as f64 / max_side as f64;
    let new_w = (width as f64 * scale).round() as usize;
    let new_h = (height as f64 * scale).round() as usize;
    let new_w = new_w.max(1);
    let new_h = new_h.max(1);

    let mut output = vec![0.0_f64; new_w * new_h * 3];

    // Area-average: for each target pixel, average all source pixels in the region
    for ty in 0..new_h {
        let src_y_start = (ty * height) / new_h;
        let src_y_end = ((ty + 1) * height) / new_h;

        for tx in 0..new_w {
            let src_x_start = (tx * width) / new_w;
            let src_x_end = ((tx + 1) * width) / new_w;

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

    (output, new_w, new_h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_op_small_image() {
        let data = vec![0.5, 0.3, 0.1, 0.7, 0.2, 0.9]; // 1x2 image
        let (result, w, h) = downsample_rgb(&data, 1, 2, 512);
        assert_eq!(w, 1);
        assert_eq!(h, 2);
        assert_eq!(result, data);
    }

    #[test]
    fn no_op_exact_boundary() {
        let data = vec![0.0_f64; 512 * 512 * 3];
        let (result, w, h) = downsample_rgb(&data, 512, 512, 512);
        assert_eq!(w, 512);
        assert_eq!(h, 512);
        assert_eq!(result.len(), data.len());
    }

    #[test]
    fn downsample_1024x1024_to_512x512() {
        // Uniform color — all pixels (0.5, 0.3, 0.1)
        let pixel = [0.5_f64, 0.3, 0.1];
        let data: Vec<f64> = pixel.iter().cycle().take(1024 * 1024 * 3).copied().collect();
        let (result, w, h) = downsample_rgb(&data, 1024, 1024, 512);
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
        let (result, w, h) = downsample_rgb(&data, 4096, 2160, 512);
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
        let (_, w, h) = downsample_rgb(&data, 1000, 500, 512);
        assert_eq!(w, 512);
        assert_eq!(h, 256);
    }

    #[test]
    fn downsample_tall_image() {
        // 500x1000 → 256x512 (max_side=1000 > 512)
        let data = vec![0.0_f64; 500 * 1000 * 3];
        let (_, w, h) = downsample_rgb(&data, 500, 1000, 512);
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
        let (result, w, h) = downsample_rgb(&data, 4, 4, 2);
        assert_eq!(w, 2);
        assert_eq!(h, 2);
        // Each target pixel is average of 4 source pixels
        // Block (0,0): pixels (0,0)=(1,0,0), (0,1)=(0,1,1), (1,0)=(0,1,1), (1,1)=(1,0,0) → avg (0.5, 0.5, 0.5)
        assert_eq!(result[0], 0.5); // R
        assert_eq!(result[1], 0.5); // G
        assert_eq!(result[2], 0.5); // B
    }

    #[test]
    fn single_pixel_image() {
        let data = vec![0.25, 0.50, 0.75];
        let (result, w, h) = downsample_rgb(&data, 1, 1, 512);
        assert_eq!(w, 1);
        assert_eq!(h, 1);
        assert_eq!(result, data);
    }

    #[test]
    fn two_pixel_wide_image() {
        let data = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]; // 2x1
        let (result, w, h) = downsample_rgb(&data, 2, 1, 512);
        assert_eq!(w, 2);
        assert_eq!(h, 1);
        assert_eq!(result, data); // no downsampling needed
    }

    #[test]
    fn downsample_3x2_to_2x1() {
        // 3x2 image → max_dim=2 → 2x1
        let data = vec![
            1.0, 0.0, 0.0,  0.0, 1.0, 0.0,  0.0, 0.0, 1.0,
            0.0, 0.0, 1.0,  0.0, 1.0, 0.0,  1.0, 0.0, 0.0,
        ];
        let (result, w, h) = downsample_rgb(&data, 3, 2, 2);
        assert_eq!(w, 2);
        assert_eq!(h, 1);
        eprintln!("result: {:?}", &result[..6]);
        // Target pixel (0,0): all 6 source pixels averaged
        // R: (1+0+0+0+0+1)/6 = 1/3, G: (0+1+0+0+1+0)/6 = 1/3, B: (0+0+1+1+0+0)/6 = 1/3
        assert!((result[0] - 1.0 / 3.0).abs() < 1e-10);
        assert!((result[1] - 1.0 / 3.0).abs() < 1e-10);
        assert!((result[2] - 1.0 / 3.0).abs() < 1e-10);
        // Target pixel (1,0): src_x [1,3), src_y [0,2) → 4 pixels
        // (0,1,0), (0,0,1), (0,1,0), (1,0,0) → R=0.25, G=0.5, B=0.25
        assert!((result[3] - 0.25).abs() < 1e-10);
        assert!((result[4] - 0.5).abs() < 1e-10);
        assert!((result[5] - 0.25).abs() < 1e-10);
    }

    #[test]
    #[should_panic(expected = "data length")]
    fn panics_on_wrong_data_length() {
        let data = vec![0.0_f64; 10]; // not divisible by 3 or wrong for 2x2
        let _ = downsample_rgb(&data, 2, 2, 512);
    }

    #[test]
    #[should_panic(expected = "width and height must be > 0")]
    fn panics_on_zero_dimensions() {
        let data = vec![0.0_f64; 0];
        let _ = downsample_rgb(&data, 0, 0, 512);
    }
}
