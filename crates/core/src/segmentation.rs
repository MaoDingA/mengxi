// segmentation.rs — SLIC superpixel segmentation in pure Rust
//
// Divides Oklab image data into N superpixels using the SLIC algorithm
// (Simple Linear Iterative Clustering). Each superpixel is a connected
// region of similar color, providing semantically meaningful segmentation
// instead of arbitrary grid cells.
//
// Reference: Achanta et al., "SLIC Superpixels Compared to State-of-the-Art
// Superpixel Methods", PAMI 2012.

// ---------------------------------------------------------------------------
// Cluster center
// ---------------------------------------------------------------------------

/// A SLIC cluster center in Oklab + spatial space [L, a, b, x, y].
#[derive(Debug, Clone, Copy)]
struct Center {
    l: f64,
    a: f64,
    b: f64,
    x: f64,
    y: f64,
}

// ---------------------------------------------------------------------------
// SLIC segmentation
// ---------------------------------------------------------------------------

/// SLIC segmentation parameters.
#[derive(Debug, Clone)]
pub struct SlicParams {
    /// Desired number of superpixels.
    pub num_superpixels: usize,
    /// Compactness factor (higher = more regular shapes, lower = more color-sensitive).
    pub compactness: f64,
    /// Maximum iterations.
    pub max_iterations: usize,
}

impl Default for SlicParams {
    fn default() -> Self {
        Self {
            num_superpixels: 16,
            compactness: 10.0,
            max_iterations: 10,
        }
    }
}

/// Segment an Oklab image into superpixels using SLIC.
///
/// # Arguments
/// * `oklab_data` — Interleaved Oklab values [L0,a0,b0, L1,a1,b1, ...]
/// * `width` — Image width in pixels
/// * `height` — Image height in pixels
/// * `params` — SLIC parameters
///
/// # Returns
/// A label map where each pixel is assigned a superpixel ID (0..K).
/// Returns `None` if the image is too small or parameters are invalid.
pub fn slic_segment(
    oklab_data: &[f64],
    width: usize,
    height: usize,
    params: &SlicParams,
) -> Option<Vec<usize>> {
    let n = width * height;
    if n == 0 || oklab_data.len() < n * 3 || params.num_superpixels == 0 {
        return None;
    }

    let k = params.num_superpixels.min(n);
    let s = ((n as f64 / k as f64).sqrt().ceil() as usize).max(1);
    let inv_s = 1.0 / s as f64;
    let m = params.compactness;
    let m_over_s = m * inv_s;

    // Initialize centers on a regular grid
    let mut centers = initialize_centers(oklab_data, width, height, s, k)?;

    let mut labels = vec![0usize; n];
    let mut distances = vec![f64::INFINITY; n];

    for _iter in 0..params.max_iterations {
        // Reset distances
        distances.fill(f64::INFINITY);

        // Assignment: for each center, search within 2S window
        for (ki, center) in centers.iter().enumerate() {
            let x_min = ((center.x - 2.0 * s as f64).floor() as usize).saturating_sub(0).min(width - 1);
            let x_max = ((center.x + 2.0 * s as f64).ceil() as usize).min(width);
            let y_min = ((center.y - 2.0 * s as f64).floor() as usize).saturating_sub(0).min(height - 1);
            let y_max = ((center.y + 2.0 * s as f64).ceil() as usize).min(height);

            for y in y_min..y_max {
                for x in x_min..x_max {
                    let idx = y * width + x;
                    let l = oklab_data[idx * 3];
                    let a = oklab_data[idx * 3 + 1];
                    let b = oklab_data[idx * 3 + 2];

                    let dl = l - center.l;
                    let da = a - center.a;
                    let db = b - center.b;
                    let d_color = dl * dl + da * da + db * db;

                    let dx = x as f64 - center.x;
                    let dy = y as f64 - center.y;
                    let d_spatial = dx * dx + dy * dy;

                    let dist = d_color + (m_over_s * m_over_s) * d_spatial;

                    if dist < distances[idx] {
                        distances[idx] = dist;
                        labels[idx] = ki;
                    }
                }
            }
        }

        // Update centers
        let mut sums = vec![0.0f64; k * 5]; // [l, a, b, x, y] per center
        let mut counts = vec![0usize; k];

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let ki = labels[idx];
                let off = ki * 5;
                sums[off] += oklab_data[idx * 3];
                sums[off + 1] += oklab_data[idx * 3 + 1];
                sums[off + 2] += oklab_data[idx * 3 + 2];
                sums[off + 3] += x as f64;
                sums[off + 4] += y as f64;
                counts[ki] += 1;
            }
        }

        for (ki, center) in centers.iter_mut().enumerate() {
            if counts[ki] > 0 {
                let off = ki * 5;
                let c = counts[ki] as f64;
                center.l = sums[off] / c;
                center.a = sums[off + 1] / c;
                center.b = sums[off + 2] / c;
                center.x = sums[off + 3] / c;
                center.y = sums[off + 4] / c;
            }
        }
    }

    // Enforce connectivity: relabel small isolated regions (< S/4 pixels)
    enforce_connectivity(&mut labels, width, height, s);

    Some(labels)
}

// ---------------------------------------------------------------------------
// Center initialization with gradient perturbation
// ---------------------------------------------------------------------------

fn initialize_centers(
    oklab_data: &[f64],
    width: usize,
    height: usize,
    s: usize,
    k: usize,
) -> Option<Vec<Center>> {
    let mut centers = Vec::with_capacity(k);
    let mut ki = 0;

    let grid_cols = width.div_ceil(s);
    let grid_rows = height.div_ceil(s);

    for gr in 0..grid_rows {
        for gc in 0..grid_cols {
            if ki >= k {
                break;
            }
            let cx = (gc * s + s / 2).min(width - 1);
            let cy = (gr * s + s / 2).min(height - 1);

            // Perturb to lowest-gradient position in 3x3 neighborhood
            let (best_x, best_y) = find_lowest_gradient(oklab_data, width, height, cx, cy);
            let idx = best_y * width + best_x;

            centers.push(Center {
                l: oklab_data[idx * 3],
                a: oklab_data[idx * 3 + 1],
                b: oklab_data[idx * 3 + 2],
                x: best_x as f64,
                y: best_y as f64,
            });
            ki += 1;
        }
    }

    if centers.is_empty() { None } else { Some(centers) }
}

/// Find the position with the lowest gradient in a 3x3 neighborhood.
fn find_lowest_gradient(
    oklab_data: &[f64],
    width: usize,
    height: usize,
    cx: usize,
    cy: usize,
) -> (usize, usize) {
    let mut best_grad = f64::INFINITY;
    let mut best_pos = (cx, cy);

    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let nx = (cx as i32 + dx) as usize;
            let ny = (cy as i32 + dy) as usize;
            if nx >= width || ny >= height {
                continue;
            }
            // Gradient = sum of squared differences with neighbors
            let grad = compute_gradient(oklab_data, width, height, nx, ny);
            if grad < best_grad {
                best_grad = grad;
                best_pos = (nx, ny);
            }
        }
    }
    best_pos
}

fn compute_gradient(
    oklab_data: &[f64],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
) -> f64 {
    let idx = y * width + x;
    let l = oklab_data[idx * 3];
    let a = oklab_data[idx * 3 + 1];
    let b = oklab_data[idx * 3 + 2];

    let mut grad = 0.0f64;

    if x + 1 < width {
        let i2 = idx + 1;
        let dl = oklab_data[i2 * 3] - l;
        let da = oklab_data[i2 * 3 + 1] - a;
        let db = oklab_data[i2 * 3 + 2] - b;
        grad += dl * dl + da * da + db * db;
    }
    if y + 1 < height {
        let i2 = idx + width;
        let dl = oklab_data[i2 * 3] - l;
        let da = oklab_data[i2 * 3 + 1] - a;
        let db = oklab_data[i2 * 3 + 2] - b;
        grad += dl * dl + da * da + db * db;
    }

    grad
}

// ---------------------------------------------------------------------------
// Connectivity enforcement
// ---------------------------------------------------------------------------

fn enforce_connectivity(labels: &mut [usize], width: usize, height: usize, s: usize) {
    let min_size = (s as f64 / 4.0).ceil() as usize;
    if min_size <= 1 {
        return;
    }

    // Count pixels per label
    let n = width * height;
    let max_label = *labels.iter().max().unwrap_or(&0);
    let mut counts = vec![0usize; max_label + 1];
    for &l in labels.iter() {
        counts[l] += 1;
    }

    // Replace small regions with nearest large neighbor using flood fill
    let mut visited = vec![false; n];
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if visited[idx] {
                continue;
            }
            let label = labels[idx];
            if counts[label] >= min_size {
                continue;
            }

            // Find neighboring label (BFS to find border)
            let neighbor_label = find_adjacent_label(labels, width, height, x, y, label, &mut visited);
            if let Some(nl) = neighbor_label {
                // Relabel this small region
                relabel_region(labels, width, height, x, y, label, nl);
                counts[label] = 0;
            }
        }
    }
}

fn find_adjacent_label(
    labels: &[usize],
    width: usize,
    height: usize,
    start_x: usize,
    start_y: usize,
    label: usize,
    visited: &mut [bool],
) -> Option<usize> {
    let mut stack = vec![(start_x, start_y)];
    let mut found_neighbor = None;

    while let Some((x, y)) = stack.pop() {
        if x >= width || y >= height {
            continue;
        }
        let idx = y * width + x;
        if visited[idx] {
            continue;
        }
        if labels[idx] != label {
            if found_neighbor.is_none() {
                found_neighbor = Some(labels[idx]);
            }
            continue;
        }
        visited[idx] = true;

        if x > 0 { stack.push((x - 1, y)); }
        if x + 1 < width { stack.push((x + 1, y)); }
        if y > 0 { stack.push((x, y - 1)); }
        if y + 1 < height { stack.push((x, y + 1)); }
    }

    found_neighbor
}

fn relabel_region(
    labels: &mut [usize],
    width: usize,
    height: usize,
    start_x: usize,
    start_y: usize,
    old_label: usize,
    new_label: usize,
) {
    let mut stack = vec![(start_x, start_y)];
    while let Some((x, y)) = stack.pop() {
        if x >= width || y >= height {
            continue;
        }
        let idx = y * width + x;
        if labels[idx] != old_label {
            continue;
        }
        labels[idx] = new_label;

        if x > 0 { stack.push((x - 1, y)); }
        if x + 1 < width { stack.push((x + 1, y)); }
        if y > 0 { stack.push((x, y - 1)); }
        if y + 1 < height { stack.push((x, y + 1)); }
    }
}

// ---------------------------------------------------------------------------
// Extract per-segment features
// ---------------------------------------------------------------------------

/// Per-segment features with bounding box.
#[derive(Debug, Clone)]
pub struct SegmentFeatures {
    pub segment_id: usize,
    pub min_row: usize,
    pub min_col: usize,
    pub max_row: usize,
    pub max_col: usize,
    pub pixel_count: usize,
    pub features: crate::grading_features::GradingFeatures,
}

/// Extract per-segment grading features from segmented Oklab data.
///
/// For each segment (identified by label), collects all Oklab pixels
/// and extracts grading features via the FFI pipeline.
pub fn extract_segment_features(
    oklab_data: &[f64],
    width: usize,
    height: usize,
    labels: &[usize],
    color_tag: &str,
    hist_bins: usize,
) -> Result<Vec<SegmentFeatures>, crate::feature_pipeline::FeaturePipelineError> {
    if !oklab_data.len().is_multiple_of(3) || labels.len() != width * height {
        return Err(crate::feature_pipeline::FeaturePipelineError::ExtractionError(
            "invalid input dimensions".to_string(),
        ));
    }

    let max_label = labels.iter().copied().max().unwrap_or(0);
    let n_segments = max_label + 1;

    let mut segments: Vec<(Vec<f64>, usize, usize, usize, usize, usize)> = vec![
        (Vec::new(), usize::MAX, usize::MAX, 0, 0, 0);
        n_segments
    ];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let label = labels[idx];
            let seg = &mut segments[label];
            seg.0.push(oklab_data[idx * 3]);
            seg.0.push(oklab_data[idx * 3 + 1]);
            seg.0.push(oklab_data[idx * 3 + 2]);
            seg.1 = seg.1.min(y); // min_row
            seg.2 = seg.2.min(x); // min_col
            seg.3 = seg.3.max(y); // max_row
            seg.4 = seg.4.max(x); // max_col
            seg.5 += 1;           // pixel_count
        }
    }

    let mut results = Vec::new();
    for (label, (pixels, min_row, min_col, max_row, max_col, pixel_count)) in segments.into_iter().enumerate() {
        if pixel_count == 0 {
            continue;
        }

        let color_space_tag_int = crate::feature_pipeline::color_tag_to_int(color_tag);
        let features = crate::color_science::extract_grading_features(
            &pixels, color_space_tag_int, hist_bins,
        ).map_err(|e| crate::feature_pipeline::FeaturePipelineError::ExtractionError(e.to_string()))?;

        results.push(SegmentFeatures {
            segment_id: label,
            min_row,
            min_col,
            max_row,
            max_col,
            pixel_count,
            features,
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_uniform_oklab(w: usize, h: usize, l: f64, a: f64, b: f64) -> Vec<f64> {
        let mut data = Vec::with_capacity(w * h * 3);
        for _ in 0..w * h {
            data.push(l);
            data.push(a);
            data.push(b);
        }
        data
    }

    #[test]
    fn test_slic_uniform_image() {
        // Uniform image — all pixels same color, should produce 1 label effectively
        let data = make_uniform_oklab(10, 10, 0.5, 0.0, 0.0);
        let params = SlicParams {
            num_superpixels: 4,
            compactness: 10.0,
            max_iterations: 5,
        };
        let labels = slic_segment(&data, 10, 10, &params).unwrap();
        assert_eq!(labels.len(), 100);
        // All labels should be valid
        assert!(labels.iter().all(|&l| l < 4));
    }

    #[test]
    fn test_slic_two_region_image() {
        // Left half bright, right half dark
        let mut data = Vec::with_capacity(20 * 10 * 3);
        for _y in 0..10 {
            for x in 0..20 {
                let l = if x < 10 { 0.9 } else { 0.1 };
                data.push(l);
                data.push(0.0);
                data.push(0.0);
            }
        }
        let params = SlicParams {
            num_superpixels: 2,
            compactness: 5.0,
            max_iterations: 20,
        };
        let labels = slic_segment(&data, 20, 10, &params).unwrap();
        assert_eq!(labels.len(), 200);

        // Left half should mostly have one label, right half another
        let mut left_labels = Vec::new();
        let mut right_labels = Vec::new();
        for y in 0..10 {
            let row = y * 20;
            for x in 0..10 {
                left_labels.push(labels[row + x]);
            }
            for x in 10..20 {
                right_labels.push(labels[row + x]);
            }
        }

        let left_mode = mode(&left_labels);
        let right_mode = mode(&right_labels);
        assert_ne!(left_mode, right_mode, "left and right regions should have different dominant labels");
    }

    fn mode(v: &[usize]) -> usize {
        let mut counts = std::collections::HashMap::new();
        for &x in v {
            *counts.entry(x).or_insert(0) += 1;
        }
        counts.into_iter().max_by_key(|(_, c)| *c).unwrap().0
    }

    #[test]
    fn test_slic_empty_image() {
        let result = slic_segment(&[], 0, 0, &SlicParams::default());
        assert!(result.is_none());
    }

    #[test]
    fn test_slic_zero_superpixels() {
        let data = make_uniform_oklab(4, 4, 0.5, 0.0, 0.0);
        let params = SlicParams { num_superpixels: 0, ..Default::default() };
        let result = slic_segment(&data, 4, 4, &params);
        assert!(result.is_none());
    }

    #[test]
    fn test_slic_small_image() {
        let data = make_uniform_oklab(2, 2, 0.5, 0.0, 0.0);
        let params = SlicParams {
            num_superpixels: 2,
            ..Default::default()
        };
        let labels = slic_segment(&data, 2, 2, &params).unwrap();
        assert_eq!(labels.len(), 4);
    }

    #[test]
    fn test_default_params() {
        let params = SlicParams::default();
        assert_eq!(params.num_superpixels, 16);
        assert_eq!(params.compactness, 10.0);
        assert_eq!(params.max_iterations, 10);
    }

    #[cfg(moonbit_ffi)]
    #[test]
    fn test_extract_segment_features_empty() {
        let data = vec![0.5, 0.0, 0.0];
        let result = extract_segment_features(&data, 1, 1, &[0], "linear", 64);
        assert!(result.is_ok());
        let segs = result.unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].pixel_count, 1);
        assert_eq!(segs[0].features.hist_l.len(), 64);
    }
}
