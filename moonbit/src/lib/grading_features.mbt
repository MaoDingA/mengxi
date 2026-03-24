// grading_features.mbt — Oklab histogram and color moments extraction
// Implements: 3-channel histogram, color moments (mean + std), histogram split
// Reference: architecture-v2.md ADR-v2-2 (FFI compound interface)

// ============================================================
// Helper Functions (prefixed gf_ to avoid cross-file conflicts)
// ============================================================

fn gf_approx_eq(actual : Double, expected : Double) -> Bool {
  let diff = actual - expected
  let abs_diff = if diff < 0.0 { 0.0 - diff } else { diff }
  abs_diff < 0.0001
}

// ============================================================
// Histogram Computation
// ============================================================

/// Compute histogram for a single channel of Oklab values.
/// Input:  data     — single-channel f64 array
///         data_len — number of elements
///         bin_count — number of histogram bins (must be >= 8)
///         out      — output array of size bin_count (bin counts as f64)
///         min_val  — lower bound of bin range
///         max_val  — upper bound of bin range
/// Returns: bin_count on success, negative error code on failure.
fn compute_histogram(
  data: FixedArray[Double],
  data_len: Int,
  bin_count: Int,
  out: FixedArray[Double],
  min_val: Double,
  max_val: Double
) -> Int {
  if data_len < 1 { return -1 }
  if bin_count < 8 { return -2 }

  // Initialize all bins to 0.0
  let mut i = 0
  while i < bin_count {
    out[i] = 0.0
    i = i + 1
  }

  let range = max_val - min_val
  // Avoid division by zero if range is 0 (all values identical)
  // In this case, all values map to bin 0
  let mut j = 0
  while j < data_len {
    let v = data[j]
    let mut bin_idx : Int = 0
    if range > 0.0 {
      if v <= min_val {
        bin_idx = 0
      } else if v >= max_val {
        bin_idx = bin_count - 1
      } else {
        let scaled = (v - min_val) / range * bin_count.to_double()
        bin_idx = scaled.to_int()
      }
    }
    // Clamp to valid range
    if bin_idx < 0 { bin_idx = 0 }
    if bin_idx >= bin_count { bin_idx = bin_count - 1 }
    out[bin_idx] = out[bin_idx] + 1.0
    j = j + 1
  }

  bin_count
}

// ============================================================
// Color Moments Computation
// ============================================================

/// Compute color moments for three Oklab channels.
/// Input:  l_data, a_data, b_data — separate channel arrays
///         data_len — number of pixels per channel
///         out      — output array of size 6: [L_mean, L_std, a_mean, a_std, b_mean, b_std]
/// Returns: 6 on success, negative error code on failure.
fn compute_color_moments(
  l_data: FixedArray[Double],
  a_data: FixedArray[Double],
  b_data: FixedArray[Double],
  data_len: Int,
  out: FixedArray[Double]
) -> Int {
  if data_len < 1 {
    // Return all zeros for empty input
    let mut z = 0
    while z < 6 {
      out[z] = 0.0
      z = z + 1
    }
    return 6
  }

  // Compute mean for each channel
  let mut sum_l = 0.0
  let mut sum_a = 0.0
  let mut sum_b = 0.0
  let mut i = 0
  while i < data_len {
    sum_l = sum_l + l_data[i]
    sum_a = sum_a + a_data[i]
    sum_b = sum_b + b_data[i]
    i = i + 1
  }

  let mean_l = sum_l / data_len.to_double()
  let mean_a = sum_a / data_len.to_double()
  let mean_b = sum_b / data_len.to_double()

  // Compute variance (population) for each channel
  let mut var_l = 0.0
  let mut var_a = 0.0
  let mut var_b = 0.0
  let mut j = 0
  while j < data_len {
    let dl = l_data[j] - mean_l
    let da = a_data[j] - mean_a
    let db = b_data[j] - mean_b
    var_l = var_l + dl * dl
    var_a = var_a + da * da
    var_b = var_b + db * db
    j = j + 1
  }

  var_l = var_l / data_len.to_double()
  var_a = var_a / data_len.to_double()
  var_b = var_b / data_len.to_double()

  // Compute std: sqrt(variance), guard against negative (numerical safety)
  // Use @math.pow(x, 0.5) instead of @math.sqrt (Epic 1 retro: verify sqrt works)
  let std_l = if var_l <= 0.0 { 0.0 } else { @math.pow(var_l, 0.5) }
  let std_a = if var_a <= 0.0 { 0.0 } else { @math.pow(var_a, 0.5) }
  let std_b = if var_b <= 0.0 { 0.0 } else { @math.pow(var_b, 0.5) }

  out[0] = mean_l
  out[1] = std_l
  out[2] = mean_a
  out[3] = std_a
  out[4] = mean_b
  out[5] = std_b

  6
}

// ============================================================
// Histogram Split (Shadows/Midtones/Highlights)
// ============================================================

/// Split L-channel histogram into shadows/midtones/highlights sub-histograms.
/// Input:  hist, bin_count — source histogram
///         min_val, max_val — bin range (for computing bin centers)
///         shadow_boundary — L value separating shadows from midtones
///         highlight_boundary — L value separating midtones from highlights
///         shadow_out, midtone_out, highlight_out — output sub-histograms
/// Returns: 0 on success, negative error code on failure.
fn split_l_histogram(
  hist: FixedArray[Double],
  bin_count: Int,
  min_val: Double,
  max_val: Double,
  shadow_boundary: Double,
  highlight_boundary: Double,
  shadow_out: FixedArray[Double],
  midtone_out: FixedArray[Double],
  highlight_out: FixedArray[Double]
) -> Int {
  if bin_count < 1 { return -1 }

  let range = max_val - min_val

  // Initialize all output bins to 0.0
  let mut z = 0
  while z < bin_count {
    shadow_out[z] = 0.0
    midtone_out[z] = 0.0
    highlight_out[z] = 0.0
    z = z + 1
  }

  let mut i = 0
  while i < bin_count {
    // Compute bin center
    let center = if range > 0.0 {
      min_val + (i.to_double() + 0.5) * range / bin_count.to_double()
    } else {
      min_val
    }

    let count = hist[i]

    if center < shadow_boundary {
      // Shadow bin
      shadow_out[i] = count
    } else if center >= highlight_boundary {
      // Highlight bin
      highlight_out[i] = count
    } else {
      // Midtone bin
      midtone_out[i] = count
    }

    i = i + 1
  }

  0
}

// ============================================================
// Tests
// ============================================================

test {
  // compute_histogram: uniform distribution test
  // 4 values: 0.0, 0.25, 0.5, 0.75 with range [0.0, 1.0] and 8 bins
  // Bins: [0, 0.125), [0.125, 0.25), ..., [0.875, 1.0]
  // 0.0 → bin 0, 0.25 → bin 2, 0.5 → bin 4, 0.75 → bin 6
  let data = FixedArray::make(4, 0.0)
  data[0] = 0.0; data[1] = 0.25; data[2] = 0.5; data[3] = 0.75
  let out = FixedArray::make(8, 0.0)
  let result = compute_histogram(data, 4, 8, out, 0.0, 1.0)
  assert_eq(result, 8)
  assert_eq(gf_approx_eq(out[0], 1.0), true)
  assert_eq(gf_approx_eq(out[2], 1.0), true)
  assert_eq(gf_approx_eq(out[4], 1.0), true)
  assert_eq(gf_approx_eq(out[6], 1.0), true)
}

test {
  // compute_histogram: single-value (pure black) distribution
  // All values = 0.0, should all go to bin 0
  let data = FixedArray::make(5, 0.0)
  let out = FixedArray::make(8, 0.0)
  let result = compute_histogram(data, 5, 8, out, 0.0, 1.0)
  assert_eq(result, 8)
  assert_eq(gf_approx_eq(out[0], 5.0), true)
  let mut z = 1
  while z < 8 {
    assert_eq(gf_approx_eq(out[z], 0.0), true)
    z = z + 1
  }
}

test {
  // compute_histogram: empty input returns error
  let data = FixedArray::make(0, 0.0)
  let out = FixedArray::make(8, 0.0)
  let result = compute_histogram(data, 0, 8, out, 0.0, 1.0)
  assert_eq(result, -1)
}

test {
  // compute_histogram: bin_count too small returns error
  let data = FixedArray::make(8, 0.0)
  let out = FixedArray::make(4, 0.0)
  let result = compute_histogram(data, 8, 4, out, 0.0, 1.0)
  assert_eq(result, -2)
}

test {
  // compute_histogram: values clamped to bin range
  // Values outside [0.0, 1.0] should go to edge bins
  let data = FixedArray::make(4, 0.0)
  data[0] = -0.5  // below min → bin 0
  data[1] = 1.5   // above max → bin 7
  data[2] = 0.0   // at min → bin 0
  data[3] = 1.0   // at max → bin 7
  let out = FixedArray::make(8, 0.0)
  let result = compute_histogram(data, 4, 8, out, 0.0, 1.0)
  assert_eq(result, 8)
  assert_eq(gf_approx_eq(out[0], 2.0), true)   // -0.5 and 0.0
  assert_eq(gf_approx_eq(out[7], 2.0), true)   // 1.5 and 1.0
}

test {
  // compute_color_moments: all same value → mean=value, std=0.0
  let l_data = FixedArray::make(3, 0.5)
  let a_data = FixedArray::make(3, 0.1)
  let b_data = FixedArray::make(3, -0.2)
  let out = FixedArray::make(6, 0.0)
  let result = compute_color_moments(l_data, a_data, b_data, 3, out)
  assert_eq(result, 6)
  assert_eq(gf_approx_eq(out[0], 0.5), true)   // L_mean
  assert_eq(gf_approx_eq(out[1], 0.0), true)   // L_std
  assert_eq(gf_approx_eq(out[2], 0.1), true)   // a_mean
  assert_eq(gf_approx_eq(out[3], 0.0), true)   // a_std
  assert_eq(gf_approx_eq(out[4], -0.2), true)  // b_mean
  assert_eq(gf_approx_eq(out[5], 0.0), true)   // b_std
}

test {
  // compute_color_moments: known mean and std
  // Values [0.0, 2.0]: mean=1.0, var=((0-1)^2 + (2-1)^2)/2 = 1.0, std=1.0
  let l_data = FixedArray::make(2, 0.0)
  l_data[0] = 0.0; l_data[1] = 2.0
  let a_data = FixedArray::make(2, 0.0)
  a_data[0] = -1.0; a_data[1] = 1.0
  let b_data = FixedArray::make(2, 0.0)
  b_data[0] = 0.5; b_data[1] = 0.5
  let out = FixedArray::make(6, 0.0)
  let result = compute_color_moments(l_data, a_data, b_data, 2, out)
  assert_eq(result, 6)
  assert_eq(gf_approx_eq(out[0], 1.0), true)    // L_mean
  assert_eq(gf_approx_eq(out[1], 1.0), true)    // L_std
  assert_eq(gf_approx_eq(out[2], 0.0), true)    // a_mean
  assert_eq(gf_approx_eq(out[3], 1.0), true)    // a_std
  assert_eq(gf_approx_eq(out[4], 0.5), true)    // b_mean
  assert_eq(gf_approx_eq(out[5], 0.0), true)    // b_std (all same → 0.0)
}

test {
  // compute_color_moments: empty input returns all zeros
  let l_data = FixedArray::make(0, 0.0)
  let a_data = FixedArray::make(0, 0.0)
  let b_data = FixedArray::make(0, 0.0)
  let out = FixedArray::make(6, 0.0)
  let result = compute_color_moments(l_data, a_data, b_data, 0, out)
  assert_eq(result, 6)
  let mut z = 0
  while z < 6 {
    assert_eq(gf_approx_eq(out[z], 0.0), true)
    z = z + 1
  }
}

test {
  // compute_color_moments: pure black → mean=0.0, std=0.0
  let l_data = FixedArray::make(3, 0.0)
  let a_data = FixedArray::make(3, 0.0)
  let b_data = FixedArray::make(3, 0.0)
  let out = FixedArray::make(6, 0.0)
  let result = compute_color_moments(l_data, a_data, b_data, 3, out)
  assert_eq(result, 6)
  // All finite
  assert_eq(out[0] == out[0], true)
  assert_eq(out[1] == out[1], true)
  assert_eq(out[2] == out[2], true)
  assert_eq(out[3] == out[3], true)
  assert_eq(out[4] == out[4], true)
  assert_eq(out[5] == out[5], true)
  assert_eq(gf_approx_eq(out[0], 0.0), true)
  assert_eq(gf_approx_eq(out[1], 0.0), true)
}

test {
  // split_l_histogram: correct partitioning at boundaries
  // 8 bins over [0.0, 1.0]: bin centers at 0.0625, 0.1875, 0.3125, 0.4375, 0.5625, 0.6875, 0.8125, 0.9375
  // With shadow=0.25, highlight=0.75:
  //   shadow bins (center < 0.25): 0, 1, 2 (centers 0.0625, 0.1875, 0.3125... wait 0.3125 >= 0.25)
  //   Actually: bin 0 (0.0625 < 0.25), bin 1 (0.1875 < 0.25) → shadow
  //            bin 2 (0.3125 >= 0.25, < 0.75), bin 3 (0.4375), bin 4 (0.5625), bin 5 (0.6875) → midtone
  //            bin 6 (0.8125 >= 0.75), bin 7 (0.9375) → highlight
  let hist = FixedArray::make(8, 1.0)  // 1 pixel in each bin
  let shadow_out = FixedArray::make(8, 0.0)
  let midtone_out = FixedArray::make(8, 0.0)
  let highlight_out = FixedArray::make(8, 0.0)
  let result = split_l_histogram(hist, 8, 0.0, 1.0, 0.25, 0.75, shadow_out, midtone_out, highlight_out)
  assert_eq(result, 0)
  // Shadows: bins 0, 1
  assert_eq(gf_approx_eq(shadow_out[0], 1.0), true)
  assert_eq(gf_approx_eq(shadow_out[1], 1.0), true)
  assert_eq(gf_approx_eq(shadow_out[2], 0.0), true)
  // Midtones: bins 2, 3, 4, 5
  assert_eq(gf_approx_eq(midtone_out[2], 1.0), true)
  assert_eq(gf_approx_eq(midtone_out[3], 1.0), true)
  assert_eq(gf_approx_eq(midtone_out[4], 1.0), true)
  assert_eq(gf_approx_eq(midtone_out[5], 1.0), true)
  assert_eq(gf_approx_eq(midtone_out[6], 0.0), true)
  // Highlights: bins 6, 7
  assert_eq(gf_approx_eq(highlight_out[6], 1.0), true)
  assert_eq(gf_approx_eq(highlight_out[7], 1.0), true)
  assert_eq(gf_approx_eq(highlight_out[5], 0.0), true)
}

test {
  // split_l_histogram: all-shadows edge case
  // All values in shadows (bins with center < 0.25)
  let hist = FixedArray::make(8, 0.0)
  hist[0] = 5.0; hist[1] = 3.0  // only shadow bins have data
  let shadow_out = FixedArray::make(8, 0.0)
  let midtone_out = FixedArray::make(8, 0.0)
  let highlight_out = FixedArray::make(8, 0.0)
  let result = split_l_histogram(hist, 8, 0.0, 1.0, 0.25, 0.75, shadow_out, midtone_out, highlight_out)
  assert_eq(result, 0)
  assert_eq(gf_approx_eq(shadow_out[0], 5.0), true)
  assert_eq(gf_approx_eq(shadow_out[1], 3.0), true)
  // Sum of midtone and highlight should be 0
  let mut total_mt = 0.0
  let mut total_hl = 0.0
  let mut k = 0
  while k < 8 {
    total_mt = total_mt + midtone_out[k]
    total_hl = total_hl + highlight_out[k]
    k = k + 1
  }
  assert_eq(gf_approx_eq(total_mt, 0.0), true)
  assert_eq(gf_approx_eq(total_hl, 0.0), true)
}

test {
  // split_l_histogram: all-highlights edge case
  let hist = FixedArray::make(8, 0.0)
  hist[6] = 10.0; hist[7] = 7.0
  let shadow_out = FixedArray::make(8, 0.0)
  let midtone_out = FixedArray::make(8, 0.0)
  let highlight_out = FixedArray::make(8, 0.0)
  let result = split_l_histogram(hist, 8, 0.0, 1.0, 0.25, 0.75, shadow_out, midtone_out, highlight_out)
  assert_eq(result, 0)
  assert_eq(gf_approx_eq(highlight_out[6], 10.0), true)
  assert_eq(gf_approx_eq(highlight_out[7], 7.0), true)
  let mut total_sh = 0.0
  let mut total_mt = 0.0
  let mut k = 0
  while k < 8 {
    total_sh = total_sh + shadow_out[k]
    total_mt = total_mt + midtone_out[k]
    k = k + 1
  }
  assert_eq(gf_approx_eq(total_sh, 0.0), true)
  assert_eq(gf_approx_eq(total_mt, 0.0), true)
}

// ============================================================
// FFI Exported Compound Function
// ============================================================

/// Extract grading features (histograms + moments) from Oklab pixel data.
/// Input:  pixel_ptr — interleaved Oklab data [L0,a0,b0, L1,a1,b1, ...]
///         pixel_len — number of f64 values (must be >= 3, divisible by 3)
///         color_space_tag — reserved for future use (unused in Story 2.1)
/// Output: hist_l_ptr, hist_a_ptr, hist_b_ptr — 64-bin histograms
///         moments_ptr — 6 values [L_mean, L_std, a_mean, a_std, b_mean, b_std]
///         hist_len_ptr — output: bin_count (64)
///         moments_len_ptr — output: 6
/// Returns: 0 on success, negative error code on failure.
pub fn mengxi_extract_grading_features(
  pixel_ptr : FixedArray[Double],
  pixel_len : Int,
  color_space_tag : Int,
  hist_l_ptr : FixedArray[Double],
  hist_a_ptr : FixedArray[Double],
  hist_b_ptr : FixedArray[Double],
  moments_ptr : FixedArray[Double],
  hist_len_ptr : FixedArray[Double],
  moments_len_ptr : FixedArray[Double]
) -> Int {
  if pixel_len < 3 { return -1 }
  if pixel_len % 3 != 0 { return -2 }

  let num_pixels = pixel_len / 3
  let bin_count = 64

  // Extract L, a, b channels from interleaved input
  let l_data = FixedArray::make(num_pixels, 0.0)
  let a_data = FixedArray::make(num_pixels, 0.0)
  let b_data = FixedArray::make(num_pixels, 0.0)

  let mut i = 0
  while i < num_pixels {
    l_data[i] = pixel_ptr[i * 3]
    a_data[i] = pixel_ptr[i * 3 + 1]
    b_data[i] = pixel_ptr[i * 3 + 2]
    i = i + 1
  }

  // Compute L-channel histogram with range [0.0, 1.0]
  let l_result = compute_histogram(l_data, num_pixels, bin_count, hist_l_ptr, 0.0, 1.0)
  if l_result < 0 { return l_result }

  // Compute a-channel histogram with range [-0.5, 0.5]
  let a_result = compute_histogram(a_data, num_pixels, bin_count, hist_a_ptr, -0.5, 0.5)
  if a_result < 0 { return a_result }

  // Compute b-channel histogram with range [-0.5, 0.5]
  let b_result = compute_histogram(b_data, num_pixels, bin_count, hist_b_ptr, -0.5, 0.5)
  if b_result < 0 { return b_result }

  // Compute color moments
  let m_result = compute_color_moments(l_data, a_data, b_data, num_pixels, moments_ptr)
  if m_result < 0 { return m_result }

  // Write output sizes
  hist_len_ptr[0] = bin_count.to_double()
  moments_len_ptr[0] = 6.0

  0
}

// ============================================================
// DCE Prevention
// ============================================================

fn init {
  // DCE prevention: reference all exported functions to prevent dead code elimination
  let _ = mengxi_extract_grading_features(
    FixedArray::make(0, 0.0), 0, 0,
    FixedArray::make(0, 0.0), FixedArray::make(0, 0.0), FixedArray::make(0, 0.0),
    FixedArray::make(0, 0.0), FixedArray::make(0, 0.0), FixedArray::make(0, 0.0)
  )
}

// ============================================================
// FFI Function Tests
// ============================================================

test {
  // mengxi_extract_grading_features: basic extraction test
  // 2 pixels: (0.5, 0.0, 0.0) and (0.0, 0.0, 0.5)
  let pixel = FixedArray::make(6, 0.0)
  pixel[0] = 0.5; pixel[1] = 0.0; pixel[2] = 0.0  // achromatic mid-gray
  pixel[3] = 0.0; pixel[4] = 0.0; pixel[5] = 0.5  // blue
  let hist_l = FixedArray::make(64, 0.0)
  let hist_a = FixedArray::make(64, 0.0)
  let hist_b = FixedArray::make(64, 0.0)
  let moments = FixedArray::make(6, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 6, 0,
    hist_l, hist_a, hist_b, moments, hist_len, moments_len
  )
  assert_eq(result, 0)
  // Verify output sizes
  assert_eq(gf_approx_eq(hist_len[0], 64.0), true)
  assert_eq(gf_approx_eq(moments_len[0], 6.0), true)
  // Moments should be finite
  assert_eq(moments[0] == moments[0], true)  // L_mean NaN check
  assert_eq(moments[1] == moments[1], true)  // L_std NaN check
  assert_eq(moments[2] == moments[2], true)  // a_mean NaN check
  assert_eq(moments[3] == moments[3], true)  // a_std NaN check
  assert_eq(moments[4] == moments[4], true)  // b_mean NaN check
  assert_eq(moments[5] == moments[5], true)  // b_std NaN check
}

test {
  // mengxi_extract_grading_features: pure black test
  let pixel = FixedArray::make(3, 0.0)
  let hist_l = FixedArray::make(64, 0.0)
  let hist_a = FixedArray::make(64, 0.0)
  let hist_b = FixedArray::make(64, 0.0)
  let moments = FixedArray::make(6, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 3, 0,
    hist_l, hist_a, hist_b, moments, hist_len, moments_len
  )
  assert_eq(result, 0)
  // L histogram: 0.0 in [0.0, 1.0] → bin 0
  assert_eq(gf_approx_eq(hist_l[0], 1.0), true)
  // a/b histogram: 0.0 in [-0.5, 0.5] → bin 32 (middle bin)
  assert_eq(gf_approx_eq(hist_a[32], 1.0), true)
  assert_eq(gf_approx_eq(hist_b[32], 1.0), true)
  // Moments should be all zeros
  assert_eq(gf_approx_eq(moments[0], 0.0), true)  // L_mean
  assert_eq(gf_approx_eq(moments[1], 0.0), true)  // L_std
  assert_eq(gf_approx_eq(moments[2], 0.0), true)  // a_mean
  assert_eq(gf_approx_eq(moments[3], 0.0), true)  // a_std
  assert_eq(gf_approx_eq(moments[4], 0.0), true)  // b_mean
  assert_eq(gf_approx_eq(moments[5], 0.0), true)  // b_std
}

test {
  // mengxi_extract_grading_features: solid color test
  // 3 identical pixels (0.3, 0.3, 0.3)
  let pixel = FixedArray::make(9, 0.0)
  let mut k = 0
  while k < 9 {
    pixel[k] = 0.3
    k = k + 1
  }
  let hist_l = FixedArray::make(64, 0.0)
  let hist_a = FixedArray::make(64, 0.0)
  let hist_b = FixedArray::make(64, 0.0)
  let moments = FixedArray::make(6, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 9, 0,
    hist_l, hist_a, hist_b, moments, hist_len, moments_len
  )
  assert_eq(result, 0)
  // All histograms should have all pixels in one bin (0.3 in L range [0,1] → bin 19)
  let expected_bin = (0.3 / 1.0 * 64.0).to_int()
  assert_eq(gf_approx_eq(hist_l[expected_bin], 3.0), true)
  // a and b: 0.3 in range [-0.5, 0.5] → bin (0.3+0.5)/1.0 * 64 = bin 51
  let expected_ab_bin = (0.8 / 1.0 * 64.0).to_int()
  assert_eq(gf_approx_eq(hist_a[expected_ab_bin], 3.0), true)
  assert_eq(gf_approx_eq(hist_b[expected_ab_bin], 3.0), true)
  // std should be 0.0 for solid color
  assert_eq(gf_approx_eq(moments[1], 0.0), true)  // L_std
  assert_eq(gf_approx_eq(moments[3], 0.0), true)  // a_std
  assert_eq(gf_approx_eq(moments[5], 0.0), true)  // b_std
}

test {
  // mengxi_extract_grading_features: too few pixels error
  let pixel = FixedArray::make(2, 0.0)
  let hist_l = FixedArray::make(64, 0.0)
  let hist_a = FixedArray::make(64, 0.0)
  let hist_b = FixedArray::make(64, 0.0)
  let moments = FixedArray::make(6, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 2, 0,
    hist_l, hist_a, hist_b, moments, hist_len, moments_len
  )
  assert_eq(result, -1)
}

test {
  // mengxi_extract_grading_features: not divisible by 3 error
  let pixel = FixedArray::make(4, 0.0)
  let hist_l = FixedArray::make(64, 0.0)
  let hist_a = FixedArray::make(64, 0.0)
  let hist_b = FixedArray::make(64, 0.0)
  let moments = FixedArray::make(6, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 4, 0,
    hist_l, hist_a, hist_b, moments, hist_len, moments_len
  )
  assert_eq(result, -2)
}
