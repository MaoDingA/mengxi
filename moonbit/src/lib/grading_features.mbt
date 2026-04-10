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

/// Check if a Double value is finite (not NaN, not Inf, not -Inf).
/// Uses the IEEE 754 property: x * 0.0 == 0.0 only for finite x.
/// NaN * 0.0 = NaN, Inf * 0.0 = NaN — both fail the equality check.
fn gf_is_finite(v : Double) -> Bool {
  let product = v * 0.0
  product == 0.0
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
  let bc = bin_count.to_double()
  // Soft clamping: instead of dumping edge values into a single bin,
  // distribute them proportionally to adjacent bins via linear interpolation.
  // This prevents artificial spikes at histogram boundaries.
  let mut j = 0
  while j < data_len {
    let v = data[j]
    if range > 0.0 {
      // Soft clamp: compute fractional bin index
      let scaled = (v - min_val) / range * bc
      if scaled <= 0.0 {
        // Below min: entire weight goes to bin 0
        out[0] = out[0] + 1.0
      } else if scaled >= bc {
        // Above max: entire weight goes to last bin
        out[bin_count - 1] = out[bin_count - 1] + 1.0
      } else {
        let lo_bin = scaled.to_int()
        let frac = scaled - lo_bin.to_double()
        if lo_bin < bin_count - 1 {
          out[lo_bin] = out[lo_bin] + (1.0 - frac)
          out[lo_bin + 1] = out[lo_bin + 1] + frac
        } else {
          // At upper boundary: all weight to last bin
          out[bin_count - 1] = out[bin_count - 1] + 1.0
        }
      }
    } else {
      // All values identical: everything in bin 0
      out[0] = out[0] + 1.0
    }
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
///         out      — output array of size 12: [L_mean, L_std, L_skew, L_kurt, a_mean, a_std, a_skew, a_kurt, b_mean, b_std, b_skew, b_kurt]
/// Returns: 12 on success, negative error code on failure.
fn compute_color_moments(
  l_data: FixedArray[Double],
  a_data: FixedArray[Double],
  b_data: FixedArray[Double],
  data_len: Int,
  out: FixedArray[Double]
) -> Int {
  let out_size = 12
  if data_len < 1 {
    // Return all zeros for empty input
    let mut z = 0
    while z < out_size {
      out[z] = 0.0
      z = z + 1
    }
    return out_size
  }

  let n = data_len.to_double()

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

  let mean_l = sum_l / n
  let mean_a = sum_a / n
  let mean_b = sum_b / n

  // Compute variance, m3 (skewness), m4 (kurtosis) for each channel
  let mut var_l = 0.0; let mut var_a = 0.0; let mut var_b = 0.0
  let mut m3_l = 0.0;  let mut m3_a = 0.0;  let mut m3_b = 0.0
  let mut m4_l = 0.0;  let mut m4_a = 0.0;  let mut m4_b = 0.0
  let mut j = 0
  while j < data_len {
    let dl = l_data[j] - mean_l
    let da = a_data[j] - mean_a
    let db = b_data[j] - mean_b
    let dl2 = dl * dl
    let da2 = da * da
    let db2 = db * db
    var_l = var_l + dl2
    var_a = var_a + da2
    var_b = var_b + db2
    m3_l = m3_l + dl2 * dl
    m3_a = m3_a + da2 * da
    m3_b = m3_b + db2 * db
    m4_l = m4_l + dl2 * dl2
    m4_a = m4_a + da2 * da2
    m4_b = m4_b + db2 * db2
    j = j + 1
  }

  var_l = var_l / n
  var_a = var_a / n
  var_b = var_b / n
  m3_l = m3_l / n
  m3_a = m3_a / n
  m3_b = m3_b / n
  m4_l = m4_l / n
  m4_a = m4_a / n
  m4_b = m4_b / n

  // Std: sqrt(variance), guard against negative (numerical safety)
  let std_l = if var_l <= 0.0 { 0.0 } else { @math.pow(var_l, 0.5) }
  let std_a = if var_a <= 0.0 { 0.0 } else { @math.pow(var_a, 0.5) }
  let std_b = if var_b <= 0.0 { 0.0 } else { @math.pow(var_b, 0.5) }

  // Skewness: m3 / std^3 (guard against zero std)
  let skew_l = if std_l <= 0.0 { 0.0 } else { m3_l / (std_l * std_l * std_l) }
  let skew_a = if std_a <= 0.0 { 0.0 } else { m3_a / (std_a * std_a * std_a) }
  let skew_b = if std_b <= 0.0 { 0.0 } else { m3_b / (std_b * std_b * std_b) }

  // Excess Kurtosis: m4 / std^4 - 3 (guard against zero std)
  let kurt_l = if std_l <= 0.0 { 0.0 } else { m4_l / (std_l * std_l * std_l * std_l) - 3.0 }
  let kurt_a = if std_a <= 0.0 { 0.0 } else { m4_a / (std_a * std_a * std_a * std_a) - 3.0 }
  let kurt_b = if std_b <= 0.0 { 0.0 } else { m4_b / (std_b * std_b * std_b * std_b) - 3.0 }

  out[0] = mean_l
  out[1] = std_l
  out[2] = skew_l
  out[3] = kurt_l
  out[4] = mean_a
  out[5] = std_a
  out[6] = skew_a
  out[7] = kurt_a
  out[8] = mean_b
  out[9] = std_b
  out[10] = skew_b
  out[11] = kurt_b

  out_size
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
  // mengxi_extract_grading_features: NaN input returns error
  let pixel = FixedArray::make(3, 0.0)
  pixel[0] = 0.0 / 0.0  // NaN via division by zero (produces NaN in MoonBit)
  pixel[1] = 0.0
  pixel[2] = 0.0
  let hist_l = FixedArray::make(64, 0.0)
  let hist_a = FixedArray::make(64, 0.0)
  let hist_b = FixedArray::make(64, 0.0)
  let moments = FixedArray::make(12, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 3, 0, 64,
    hist_l, hist_a, hist_b, moments, hist_len, moments_len
  )
  assert_eq(result, -3)
}

test {
  // mengxi_extract_grading_features: mixed valid + NaN pixels, only valid counted
  let pixel = FixedArray::make(6, 0.0)
  pixel[0] = 0.5; pixel[1] = 0.0; pixel[2] = 0.0   // valid
  pixel[3] = 0.0 / 0.0; pixel[4] = 0.0; pixel[5] = 0.0  // NaN pixel
  let hist_l = FixedArray::make(64, 0.0)
  let hist_a = FixedArray::make(64, 0.0)
  let hist_b = FixedArray::make(64, 0.0)
  let moments = FixedArray::make(12, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 6, 0, 64,
    hist_l, hist_a, hist_b, moments, hist_len, moments_len
  )
  assert_eq(result, 0)
  // Only 1 valid pixel: L=0.5, a=0.0, b=0.0
  assert_eq(gf_approx_eq(moments[0], 0.5), true)   // L_mean
  assert_eq(gf_approx_eq(moments[1], 0.0), true)   // L_std (single pixel)
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
  let out = FixedArray::make(12, 0.0)
  let result = compute_color_moments(l_data, a_data, b_data, 3, out)
  assert_eq(result, 12)
  assert_eq(gf_approx_eq(out[0], 0.5), true)   // L_mean
  assert_eq(gf_approx_eq(out[1], 0.0), true)   // L_std
  assert_eq(gf_approx_eq(out[2], 0.0), true)   // L_skew (all same value → 0)
  assert_eq(gf_approx_eq(out[3], 0.0), true)   // L_kurt (all same value → 0)
  assert_eq(gf_approx_eq(out[4], 0.1), true)   // a_mean
  assert_eq(gf_approx_eq(out[5], 0.0), true)   // a_std
  assert_eq(gf_approx_eq(out[6], 0.0), true)   // a_skew
  assert_eq(gf_approx_eq(out[7], 0.0), true)   // a_kurt
  assert_eq(gf_approx_eq(out[8], -0.2), true)  // b_mean
  assert_eq(gf_approx_eq(out[9], 0.0), true)   // b_std
  assert_eq(gf_approx_eq(out[10], 0.0), true)  // b_skew
  assert_eq(gf_approx_eq(out[11], 0.0), true)  // b_kurt
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
  let out = FixedArray::make(12, 0.0)
  let result = compute_color_moments(l_data, a_data, b_data, 2, out)
  assert_eq(result, 12)
  assert_eq(gf_approx_eq(out[0], 1.0), true)    // L_mean
  assert_eq(gf_approx_eq(out[1], 1.0), true)    // L_std
  assert_eq(gf_approx_eq(out[4], 0.0), true)    // a_mean
  assert_eq(gf_approx_eq(out[5], 1.0), true)    // a_std
  assert_eq(gf_approx_eq(out[8], 0.5), true)    // b_mean
  assert_eq(gf_approx_eq(out[9], 0.0), true)    // b_std (all same → 0.0)
}

test {
  // compute_color_moments: empty input returns all zeros
  let l_data = FixedArray::make(0, 0.0)
  let a_data = FixedArray::make(0, 0.0)
  let b_data = FixedArray::make(0, 0.0)
  let out = FixedArray::make(12, 0.0)
  let result = compute_color_moments(l_data, a_data, b_data, 0, out)
  assert_eq(result, 12)
  let mut z = 0
  while z < 12{
    assert_eq(gf_approx_eq(out[z], 0.0), true)
    z = z + 1
  }
}

test {
  // compute_color_moments: pure black → mean=0.0, std=0.0
  let l_data = FixedArray::make(3, 0.0)
  let a_data = FixedArray::make(3, 0.0)
  let b_data = FixedArray::make(3, 0.0)
  let out = FixedArray::make(12, 0.0)
  let result = compute_color_moments(l_data, a_data, b_data, 3, out)
  assert_eq(result, 12)
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
///         hist_len_ptr — output: bin_count
///         moments_len_ptr — output: 6
/// Returns: 0 on success, negative error code on failure.
pub fn mengxi_extract_grading_features(
  pixel_ptr : FixedArray[Double],
  pixel_len : Int,
  _color_space_tag : Int,
  hist_bins : Int,
  hist_l_ptr : FixedArray[Double],
  hist_a_ptr : FixedArray[Double],
  hist_b_ptr : FixedArray[Double],
  moments_ptr : FixedArray[Double],
  hist_len_ptr : FixedArray[Double],
  moments_len_ptr : FixedArray[Double]
) -> Int {
  if pixel_len < 3 { return -1 }
  if pixel_len % 3 != 0 { return -2 }
  if hist_bins < 8 { return -4 }

  let num_pixels = pixel_len / 3
  let bin_count = hist_bins

  // Extract L, a, b channels from interleaved input, filtering non-finite values
  let l_data = FixedArray::make(num_pixels, 0.0)
  let a_data = FixedArray::make(num_pixels, 0.0)
  let b_data = FixedArray::make(num_pixels, 0.0)

  let mut valid_count = 0
  let mut i = 0
  while i < num_pixels {
    let l = pixel_ptr[i * 3]
    let a = pixel_ptr[i * 3 + 1]
    let b = pixel_ptr[i * 3 + 2]
    // Skip pixels with any non-finite channel (NaN/Inf)
    if gf_is_finite(l) && gf_is_finite(a) && gf_is_finite(b) {
      l_data[valid_count] = l
      a_data[valid_count] = a
      b_data[valid_count] = b
      valid_count = valid_count + 1
    }
    i = i + 1
  }

  // All pixels were non-finite — cannot compute features
  if valid_count < 1 { return -3 }

  // Compute L-channel histogram with range [0.0, 1.0]
  let l_result = compute_histogram(l_data, valid_count, bin_count, hist_l_ptr, 0.0, 1.0)
  if l_result < 0 { return l_result }

  // Compute a-channel histogram with range [-0.5, 0.5]
  let a_result = compute_histogram(a_data, valid_count, bin_count, hist_a_ptr, -0.5, 0.5)
  if a_result < 0 { return a_result }

  // Compute b-channel histogram with range [-0.5, 0.5]
  let b_result = compute_histogram(b_data, valid_count, bin_count, hist_b_ptr, -0.5, 0.5)
  if b_result < 0 { return b_result }

  // Compute color moments
  let m_result = compute_color_moments(l_data, a_data, b_data, valid_count, moments_ptr)
  if m_result < 0 { return m_result }

  // Write output sizes
  hist_len_ptr[0] = bin_count.to_double()
  moments_len_ptr[0] = 12.0

  0
}

// ============================================================
// DCE Prevention
// ============================================================

fn init {
  // DCE prevention: reference all exported functions to prevent dead code elimination
  let _ = mengxi_extract_grading_features(
    FixedArray::make(0, 0.0), 0, 0, 64,
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
  let moments = FixedArray::make(12, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 6, 0, 64,
    hist_l, hist_a, hist_b, moments, hist_len, moments_len
  )
  assert_eq(result, 0)
  // Verify output sizes
  assert_eq(gf_approx_eq(hist_len[0], 64.0), true)
  assert_eq(gf_approx_eq(moments_len[0], 12.0), true)
  // Moments should be finite (check all 12)
  let mut mi = 0
  while mi < 12 {
    assert_eq(moments[mi] == moments[mi], true)  // NaN check
    mi = mi + 1
  }
}

test {
  // mengxi_extract_grading_features: pure black test
  let pixel = FixedArray::make(3, 0.0)
  let hist_l = FixedArray::make(64, 0.0)
  let hist_a = FixedArray::make(64, 0.0)
  let hist_b = FixedArray::make(64, 0.0)
  let moments = FixedArray::make(12, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 3, 0, 64,
    hist_l, hist_a, hist_b, moments, hist_len, moments_len
  )
  assert_eq(result, 0)
  // L histogram: 0.0 in [0.0, 1.0] → bin 0
  assert_eq(gf_approx_eq(hist_l[0], 1.0), true)
  // a/b histogram: 0.0 in [-0.5, 0.5] → bin 32 (middle bin)
  assert_eq(gf_approx_eq(hist_a[32], 1.0), true)
  assert_eq(gf_approx_eq(hist_b[32], 1.0), true)
  // Moments should be all zeros (black: all channels = 0)
  let mut mi = 0
  while mi < 12 {
    assert_eq(gf_approx_eq(moments[mi], 0.0), true)
    mi = mi + 1
  }
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
  let moments = FixedArray::make(12, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 9, 0, 64,
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
  assert_eq(gf_approx_eq(moments[5], 0.0), true)  // a_std
  assert_eq(gf_approx_eq(moments[9], 0.0), true)  // b_std
}

test {
  // mengxi_extract_grading_features: too few pixels error
  let pixel = FixedArray::make(2, 0.0)
  let hist_l = FixedArray::make(64, 0.0)
  let hist_a = FixedArray::make(64, 0.0)
  let hist_b = FixedArray::make(64, 0.0)
  let moments = FixedArray::make(12, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 2, 0, 64,
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
  let moments = FixedArray::make(12, 0.0)
  let hist_len = FixedArray::make(1, 0.0)
  let moments_len = FixedArray::make(1, 0.0)
  let result = mengxi_extract_grading_features(
    pixel, 4, 0, 64,
    hist_l, hist_a, hist_b, moments, hist_len, moments_len
  )
  assert_eq(result, -2)
}
