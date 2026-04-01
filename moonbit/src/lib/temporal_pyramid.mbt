// temporal_pyramid.mbt — Temporal feature extraction for movie fingerprint strips
// Extracts per-segment Oklab histograms and color moments from a fingerprint strip.
// Pure functions only — no I/O, no stateful dependencies.

// ---------- Internal helpers ----------

fn tp_clamp(v : Double, lo : Double, hi : Double) -> Double {
  if v < lo { lo } else if v > hi { hi } else { v }
}

fn tp_clamp_int(v : Int, lo : Int, hi : Int) -> Int {
  if v < lo { lo } else if v > hi { hi } else { v }
}

/// Compute Oklab histogram for a segment of the strip.
/// Segment: columns [start_col, end_col), all rows.
/// Output: 3 histograms concatenated (L then a then b), each hist_bins long.
fn tp_segment_histogram(
  strip : FixedArray[Double],
  strip_width : Int,
  height : Int,
  start_col : Int,
  end_col : Int,
  hist_bins : Int,
  out_hist : FixedArray[Double]
) -> Unit {
  // Initialize
  let total = hist_bins * 3
  let mut t = 0
  while t < total { out_hist[t] = 0.0; t = t + 1 }

  if start_col >= end_col || start_col < 0 { return }
  let safe_end = if end_col > strip_width { strip_width } else { end_col }
  let safe_start = if start_col < 0 { 0 } else { start_col }

  // Oklab ranges for binning
  // L: [0, 1], a: [-0.5, 0.5], b: [-0.5, 0.5]
  let l_min = 0.0; let l_max = 1.0
  let a_min = -0.5; let a_max = 0.5
  let b_min = -0.5; let b_max = 0.5
  let l_range = l_max - l_min
  let a_range = a_max - a_min
  let b_range = b_max - b_min
  let bin_max = (hist_bins - 1).to_double()

  let mut col = safe_start
  while col < safe_end {
    let mut y = 0
    while y < height {
      let idx = (y * strip_width + col) * 3
      let r = strip[idx]
      let g = strip[idx + 1]
      let b = strip[idx + 2]
      let (l, a, bv) = srgb_to_oklab(r, g, b)

      // L histogram
      let l_bin = tp_clamp_int(
        ((tp_clamp(l, l_min, l_max) - l_min) / l_range * bin_max).to_int(),
        0, hist_bins - 1
      )
      out_hist[l_bin] = out_hist[l_bin] + 1.0

      // a histogram
      let a_bin = tp_clamp_int(
        ((tp_clamp(a, a_min, a_max) - a_min) / a_range * bin_max).to_int(),
        0, hist_bins - 1
      )
      out_hist[hist_bins + a_bin] = out_hist[hist_bins + a_bin] + 1.0

      // b histogram
      let b_bin = tp_clamp_int(
        ((tp_clamp(bv, b_min, b_max) - b_min) / b_range * bin_max).to_int(),
        0, hist_bins - 1
      )
      out_hist[hist_bins * 2 + b_bin] = out_hist[hist_bins * 2 + b_bin] + 1.0

      y = y + 1
    }
    col = col + 1
  }

  // Normalize histograms
  let num_pixels = ((safe_end - safe_start) * height).to_double()
  if num_pixels > 0.0 {
    let mut h = 0
    while h < total {
      out_hist[h] = out_hist[h] / num_pixels
      h = h + 1
    }
  }
}

/// Compute color moments (mean, std, skewness, kurtosis) for each Oklab channel.
/// Output: 12 f64 = [mean_L, std_L, skew_L, kurt_L, mean_a, std_a, skew_a, kurt_a, mean_b, std_b, skew_b, kurt_b]
fn tp_segment_moments(
  strip : FixedArray[Double],
  strip_width : Int,
  height : Int,
  start_col : Int,
  end_col : Int,
  out_moments : FixedArray[Double]
) -> Unit {
  if start_col >= end_col || start_col < 0 {
    let mut i = 0
    while i < 12 { out_moments[i] = 0.0; i = i + 1 }
    return
  }
  let safe_end = if end_col > strip_width { strip_width } else { end_col }
  let safe_start = if start_col < 0 { 0 } else { start_col }
  let num_pixels = ((safe_end - safe_start) * height).to_double()
  if num_pixels <= 0.0 {
    let mut i = 0
    while i < 12 { out_moments[i] = 0.0; i = i + 1 }
    return
  }

  // Pass 1: means
  let mut sum_l = 0.0; let mut sum_a = 0.0; let mut sum_b = 0.0
  let mut col = safe_start
  while col < safe_end {
    let mut y = 0
    while y < height {
      let idx = (y * strip_width + col) * 3
      let (l, a, bv) = srgb_to_oklab(strip[idx], strip[idx + 1], strip[idx + 2])
      sum_l = sum_l + l; sum_a = sum_a + a; sum_b = sum_b + bv
      y = y + 1
    }
    col = col + 1
  }
  let mean_l = sum_l / num_pixels
  let mean_a = sum_a / num_pixels
  let mean_b = sum_b / num_pixels

  // Pass 2: central moments (variance, skewness, kurtosis)
  let mut m2_l = 0.0; let mut m3_l = 0.0; let mut m4_l = 0.0
  let mut m2_a = 0.0; let mut m3_a = 0.0; let mut m4_a = 0.0
  let mut m2_b = 0.0; let mut m3_b = 0.0; let mut m4_b = 0.0
  let mut col2 = safe_start
  while col2 < safe_end {
    let mut y2 = 0
    while y2 < height {
      let idx = (y2 * strip_width + col2) * 3
      let (l, a, bv) = srgb_to_oklab(strip[idx], strip[idx + 1], strip[idx + 2])
      let dl = l - mean_l; let da = a - mean_a; let db = bv - mean_b
      let dl2 = dl * dl; let da2 = da * da; let db2 = db * db
      m2_l = m2_l + dl2; m2_a = m2_a + da2; m2_b = m2_b + db2
      m3_l = m3_l + dl2 * dl; m3_a = m3_a + da2 * da; m3_b = m3_b + db2 * db
      m4_l = m4_l + dl2 * dl2; m4_a = m4_a + da2 * da2; m4_b = m4_b + db2 * db2
      y2 = y2 + 1
    }
    col2 = col2 + 1
  }
  m2_l = m2_l / num_pixels; m2_a = m2_a / num_pixels; m2_b = m2_b / num_pixels
  m3_l = m3_l / num_pixels; m3_a = m3_a / num_pixels; m3_b = m3_b / num_pixels
  m4_l = m4_l / num_pixels; m4_a = m4_a / num_pixels; m4_b = m4_b / num_pixels

  let std_l = @math.pow(m2_l, 0.5)
  let std_a = @math.pow(m2_a, 0.5)
  let std_b = @math.pow(m2_b, 0.5)

  // Skewness = m3 / std^3 (guard against div-by-zero)
  let skew_l = if std_l > 0.0000001 { m3_l / (std_l * std_l * std_l) } else { 0.0 }
  let skew_a = if std_a > 0.0000001 { m3_a / (std_a * std_a * std_a) } else { 0.0 }
  let skew_b = if std_b > 0.0000001 { m3_b / (std_b * std_b * std_b) } else { 0.0 }

  // Kurtosis = m4 / std^4 - 3 (excess kurtosis)
  let kurt_l = if std_l > 0.0000001 { m4_l / (std_l * std_l * std_l * std_l) - 3.0 } else { 0.0 }
  let kurt_a = if std_a > 0.0000001 { m4_a / (std_a * std_a * std_a * std_a) - 3.0 } else { 0.0 }
  let kurt_b = if std_b > 0.0000001 { m4_b / (std_b * std_b * std_b * std_b) - 3.0 } else { 0.0 }

  out_moments[0] = mean_l; out_moments[1] = std_l; out_moments[2] = skew_l; out_moments[3] = kurt_l
  out_moments[4] = mean_a; out_moments[5] = std_a; out_moments[6] = skew_a; out_moments[7] = kurt_a
  out_moments[8] = mean_b; out_moments[9] = std_b; out_moments[10] = skew_b; out_moments[11] = kurt_b
}

// ---------- FFI export ----------

/// Extract temporal features for defined segments of a fingerprint strip.
///
/// Input:
///   strip data (row-major sRGB [0,1]) + dimensions
///   segments_ptr — [start0, end0, start1, end1, ...] pairs of frame indices
///   segments_len — number of values (must be even, pairs * 2)
///   hist_bins    — histogram bins per channel (e.g. 64)
///
/// Output per segment: hist_bins*3 (Oklab L/a/b histograms) + 12 (4 moments × 3 channels)
/// Total: num_segments * (hist_bins*3 + 12) f64
///
/// Returns: total output count on success, negative on error.
pub fn mengxi_extract_temporal_features(
  strip_len : Int,
  strip_ptr : FixedArray[Double],
  width : Int,
  height : Int,
  segments_ptr : FixedArray[Double],
  segments_len : Int,
  hist_bins : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if width <= 0 || height <= 0 { return -1 }
  let expected = width * height * 3
  if strip_len < expected { return -1 }
  if segments_len < 2 || segments_len % 2 != 0 { return -1 }
  if hist_bins < 8 { return -1 }

  let num_segments = segments_len / 2
  let per_segment = hist_bins * 3 + 12
  let total_needed = num_segments * per_segment
  if out_len < total_needed { return -2 }

  let hist_buf = FixedArray::make(hist_bins * 3, 0.0)
  let moments_buf = FixedArray::make(12, 0.0)

  let mut seg = 0
  while seg < num_segments {
    let start_col = segments_ptr[seg * 2].to_int()
    let end_col = segments_ptr[seg * 2 + 1].to_int()
    let safe_start = tp_clamp_int(start_col, 0, width)
    let safe_end = tp_clamp_int(end_col, safe_start + 1, width)

    // Extract histograms
    tp_segment_histogram(strip_ptr, width, height, safe_start, safe_end, hist_bins, hist_buf)
    // Extract moments
    tp_segment_moments(strip_ptr, width, height, safe_start, safe_end, moments_buf)

    // Copy to output
    let base = seg * per_segment
    let mut h = 0
    while h < hist_bins * 3 {
      out_ptr[base + h] = hist_buf[h]
      h = h + 1
    }
    let mut m = 0
    while m < 12 {
      out_ptr[base + hist_bins * 3 + m] = moments_buf[m]
      m = m + 1
    }

    seg = seg + 1
  }

  total_needed
}

// ---------- DCE prevention ----------

fn init {
  let strip = FixedArray::make(12, 0.5)
  let segs = FixedArray::make(2, 0.0)
  segs[0] = 0.0; segs[1] = 2.0
  let out = FixedArray::make(8 * 3 + 12, 0.0)
  let _ = mengxi_extract_temporal_features(12, strip, 2, 2, segs, 2, 8, out.length(), out)
}

// ---------- Tests ----------

test "temporal_features_single_segment" {
  // 4x1 strip, 1 segment covering all frames
  let strip = FixedArray::make(12, 0.5)
  let segs = FixedArray::make(2, 0.0)
  segs[0] = 0.0; segs[1] = 4.0
  let hist_bins = 8
  let per_seg = hist_bins * 3 + 12 // 24 + 12 = 36
  let out = FixedArray::make(per_seg, 0.0)

  let result = mengxi_extract_temporal_features(12, strip, 4, 1, segs, 2, hist_bins, per_seg, out)
  assert_eq(result, per_seg)

  // Uniform gray → all hist in middle bin, std ≈ 0
  assert_true(out[hist_bins / 2] > 0.0) // L middle bin has count
  // Mean L for sRGB 0.5 should be ~0.5-ish in Oklab
  let mean_l = out[hist_bins * 3] // first moment
  assert_true(mean_l > 0.3 && mean_l < 0.8)
  let std_l = out[hist_bins * 3 + 1] // std
  assert_true(std_l < 0.1) // uniform → low std
}

test "temporal_features_two_segments" {
  // 6x1 strip: first 3 white, last 3 black
  let strip = FixedArray::make(18, 0.0)
  strip[0] = 1.0; strip[1] = 1.0; strip[2] = 1.0
  strip[3] = 1.0; strip[4] = 1.0; strip[5] = 1.0
  strip[6] = 1.0; strip[7] = 1.0; strip[8] = 1.0
  // frames 3-5 stay at 0.0

  let segs = FixedArray::make(4, 0.0)
  segs[0] = 0.0; segs[1] = 3.0 // segment 0: white
  segs[2] = 3.0; segs[3] = 6.0 // segment 1: black

  let hist_bins = 8
  let per_seg = hist_bins * 3 + 12
  let out = FixedArray::make(per_seg * 2, 0.0)

  let result = mengxi_extract_temporal_features(18, strip, 6, 1, segs, 4, hist_bins, per_seg * 2, out)
  assert_eq(result, per_seg * 2)

  // Segment 0 mean_L should be high (white)
  let mean_l_seg0 = out[hist_bins * 3]
  assert_true(mean_l_seg0 > 0.8)

  // Segment 1 mean_L should be low (black)
  let mean_l_seg1 = out[per_seg + hist_bins * 3]
  assert_true(mean_l_seg1 < 0.2)
}

test "temporal_features_error_cases" {
  let strip = FixedArray::make(12, 0.5)
  let segs = FixedArray::make(2, 0.0)
  segs[0] = 0.0; segs[1] = 2.0
  let out = FixedArray::make(200, 0.0)
  assert_eq(mengxi_extract_temporal_features(12, strip, 0, 2, segs, 2, 8, 200, out), -1)
  assert_eq(mengxi_extract_temporal_features(12, strip, 4, 1, segs, 1, 8, 200, out), -1) // odd segments_len
  assert_eq(mengxi_extract_temporal_features(12, strip, 4, 1, segs, 2, 4, 200, out), -1) // hist_bins < 8
  assert_eq(mengxi_extract_temporal_features(12, strip, 4, 1, segs, 2, 8, 10, out), -2) // out too small
}
