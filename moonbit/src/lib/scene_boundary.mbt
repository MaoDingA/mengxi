// scene_boundary.mbt — Scene boundary detection for movie fingerprint strips
// Detects abrupt scene changes via inter-frame L1 distance in Oklab space.
// Pure functions only — no I/O, no stateful dependencies.

// ---------- Internal helpers ----------

/// Clamp value to [lo, hi].
fn sb_clamp(v : Double, lo : Double, hi : Double) -> Double {
  if v < lo { lo } else if v > hi { hi } else { v }
}

/// Compute average Oklab color of a single column in the strip.
/// Strip is row-major: pixel at (x, y) is at index (y * strip_width + x) * 3.
fn sb_column_avg_oklab(
  strip : FixedArray[Double],
  strip_width : Int,
  col : Int,
  height : Int
) -> (Double, Double, Double) {
  let mut sum_l = 0.0
  let mut sum_a = 0.0
  let mut sum_b = 0.0
  let mut y = 0
  while y < height {
    let idx = (y * strip_width + col) * 3
    let r = strip[idx]
    let g = strip[idx + 1]
    let b = strip[idx + 2]
    let (l, a, bv) = srgb_to_oklab(r, g, b)
    sum_l = sum_l + l
    sum_a = sum_a + a
    sum_b = sum_b + bv
    y = y + 1
  }
  let n = height.to_double()
  (sum_l / n, sum_a / n, sum_b / n)
}

/// Compute L1 distance between two adjacent columns in Oklab space.
/// Returns normalized distance (0.0 to ~1.0).
fn sb_column_distance(
  strip : FixedArray[Double],
  strip_width : Int,
  col_a : Int,
  col_b : Int,
  height : Int
) -> Double {
  let mut total_dist = 0.0
  let mut y = 0
  while y < height {
    let idx_a = (y * strip_width + col_a) * 3
    let idx_b = (y * strip_width + col_b) * 3
    let r_a = strip[idx_a]
    let g_a = strip[idx_a + 1]
    let b_a = strip[idx_a + 2]
    let r_b = strip[idx_b]
    let g_b = strip[idx_b + 1]
    let b_b = strip[idx_b + 2]
    let (l_a, a_a, b_a_ok) = srgb_to_oklab(r_a, g_a, b_a)
    let (l_b, a_b, b_b_ok) = srgb_to_oklab(r_b, g_b, b_b)
    let dl = if l_a > l_b { l_a - l_b } else { l_b - l_a }
    let da = if a_a > a_b { a_a - a_b } else { a_b - a_a }
    let db = if b_a_ok > b_b_ok { b_a_ok - b_b_ok } else { b_b_ok - b_a_ok }
    total_dist = total_dist + dl + da + db
    y = y + 1
  }
  // Normalize by height * 3 channels * max possible delta (~1.0 per channel)
  total_dist / (height.to_double() * 3.0)
}

// ---------- FFI export ----------

/// Detect scene boundaries in a fingerprint strip.
///
/// Input: strip image (row-major interleaved sRGB [0,1])
///   strip_len         — total f64 count
///   strip_ptr         — pixel data
///   width             — strip width (number of frames)
///   height            — strip height
///   threshold_permille — threshold * 1000 (e.g. 300 = 0.3)
///   min_scene_frames  — minimum frames between boundaries (merge closer ones)
///   max_boundaries    — maximum number of boundaries to return
///   out_len           — must be >= 1 + max_boundaries * 8
///   out_ptr           — output buffer
///
/// Output format:
///   out[0] = number of boundaries N
///   For each boundary i (0..N-1), at offset 1 + i*8:
///     [0] frame_index      — boundary position (frame column index)
///     [1] confidence       — 0.0 to 1.0 (how far above threshold)
///     [2] prev_avg_L       — average Oklab L of preceding segment
///     [3] prev_avg_a       — average Oklab a
///     [4] prev_avg_b       — average Oklab b
///     [5] next_avg_L       — average Oklab L of following segment
///     [6] next_avg_a       — average Oklab a
///     [7] next_avg_b       — average Oklab b
///
/// Returns: 1 + N*8 on success, negative error code on failure.
pub fn mengxi_detect_scene_boundaries(
  strip_len : Int,
  strip_ptr : FixedArray[Double],
  width : Int,
  height : Int,
  threshold_permille : Int,
  min_scene_frames : Int,
  max_boundaries : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if width <= 1 || height <= 0 { return -1 }
  let expected = width * height * 3
  if strip_len < expected { return -1 }
  if max_boundaries <= 0 { return -1 }
  if out_len < 1 + max_boundaries * 8 { return -2 }

  let threshold = threshold_permille.to_double() / 1000.0
  let min_gap = if min_scene_frames < 1 { 1 } else { min_scene_frames }

  // Step 1: Compute inter-frame distances
  // Store candidates as (frame_index, distance) pairs in a flat array
  let candidates = FixedArray::make((width - 1) * 2, 0.0)
  let mut num_candidates = 0

  let mut col = 0
  while col < width - 1 {
    let dist = sb_column_distance(strip_ptr, width, col, col + 1, height)
    if dist > threshold {
      let ci = num_candidates * 2
      candidates[ci] = col.to_double()
      candidates[ci + 1] = dist
      num_candidates = num_candidates + 1
    }
    col = col + 1
  }

  // Step 2: Merge candidates too close together (keep the one with higher distance)
  let merged = FixedArray::make(num_candidates * 2, 0.0)
  let mut num_merged = 0
  let mut c = 0
  while c < num_candidates {
    let ci = c * 2
    let mut best_frame = candidates[ci]
    let mut best_dist = candidates[ci + 1]
    // Look ahead for candidates within min_gap
    let mut next = c + 1
    while next < num_candidates {
      let ni = next * 2
      let next_frame = candidates[ni]
      if next_frame - best_frame > min_gap.to_double() {
        break
      }
      // Within gap — keep the stronger boundary
      if candidates[ni + 1] > best_dist {
        best_frame = next_frame
        best_dist = candidates[ni + 1]
      }
      next = next + 1
    }
    let mi = num_merged * 2
    merged[mi] = best_frame
    merged[mi + 1] = best_dist
    num_merged = num_merged + 1
    c = next // skip past merged group
  }

  // Step 3: Cap to max_boundaries
  let n = if num_merged > max_boundaries { max_boundaries } else { num_merged }

  // Step 4: Compute prev/next average colors and write output
  out_ptr[0] = n.to_double()
  let mut i = 0
  while i < n {
    let mi = i * 2
    let frame_idx = merged[mi].to_int()
    let dist = merged[mi + 1]

    // Confidence: how far above threshold (capped at 1.0)
    let confidence = sb_clamp((dist - threshold) / (if threshold > 0.001 { threshold } else { 0.001 }), 0.0, 1.0)

    // Average color of preceding segment (from previous boundary or start)
    let prev_start = if i > 0 { merged[(i - 1) * 2].to_int() + 1 } else { 0 }
    let prev_end = frame_idx
    let (prev_l, prev_a, prev_b) = if prev_end > prev_start {
      // Average across columns in segment
      let mut sl = 0.0; let mut sa = 0.0; let mut sb = 0.0
      let mut col2 = prev_start
      while col2 <= prev_end && col2 < width {
        let (l, a, bv) = sb_column_avg_oklab(strip_ptr, width, col2, height)
        sl = sl + l; sa = sa + a; sb = sb + bv
        col2 = col2 + 1
      }
      let cnt = (col2 - prev_start).to_double()
      (sl / cnt, sa / cnt, sb / cnt)
    } else {
      sb_column_avg_oklab(strip_ptr, width, if frame_idx > 0 { frame_idx } else { 0 }, height)
    }

    // Average color of following segment
    let next_start = frame_idx + 1
    let next_end = if i < n - 1 { merged[(i + 1) * 2].to_int() - 1 } else { width - 1 }
    let (next_l, next_a, next_b) = if next_end >= next_start && next_start < width {
      let mut sl = 0.0; let mut sa = 0.0; let mut sb = 0.0
      let mut col2 = next_start
      while col2 <= next_end && col2 < width {
        let (l, a, bv) = sb_column_avg_oklab(strip_ptr, width, col2, height)
        sl = sl + l; sa = sa + a; sb = sb + bv
        col2 = col2 + 1
      }
      let cnt = (col2 - next_start).to_double()
      (sl / cnt, sa / cnt, sb / cnt)
    } else if next_start < width {
      sb_column_avg_oklab(strip_ptr, width, next_start, height)
    } else {
      sb_column_avg_oklab(strip_ptr, width, width - 1, height)
    }

    let base = 1 + i * 8
    out_ptr[base + 0] = frame_idx.to_double()
    out_ptr[base + 1] = confidence
    out_ptr[base + 2] = prev_l
    out_ptr[base + 3] = prev_a
    out_ptr[base + 4] = prev_b
    out_ptr[base + 5] = next_l
    out_ptr[base + 6] = next_a
    out_ptr[base + 7] = next_b

    i = i + 1
  }

  1 + n * 8
}

// ---------- DCE prevention ----------

fn init {
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(1 + 10 * 8, 0.0)
  let _ = mengxi_detect_scene_boundaries(12, strip, 2, 2, 300, 5, 10, 81, out)
}

// ---------- Tests ----------

test "detect_boundaries_uniform" {
  // Uniform strip — no boundaries expected
  let strip = FixedArray::make(30, 0.5) // 10x1 strip
  let out = FixedArray::make(1 + 5 * 8, 0.0)
  let result = mengxi_detect_scene_boundaries(30, strip, 10, 1, 300, 5, 5, 41, out)
  assert_eq(result, 1) // 1 + 0*8 = just count = 0 boundaries
  assert_eq(out[0], 0.0)
}

test "detect_boundaries_abrupt_change" {
  // 10x1 strip: first 5 frames white, last 5 frames black
  let strip = FixedArray::make(30, 0.0)
  let mut i = 0
  while i < 15 { // first 5 columns * 3 channels = 15 values
    strip[i] = 1.0
    i = i + 1
  }
  let out = FixedArray::make(1 + 5 * 8, 0.0)
  let result = mengxi_detect_scene_boundaries(30, strip, 10, 1, 100, 2, 5, 41, out)
  assert_eq(result, 9) // 1 + 1*8 = 9
  assert_eq(out[0], 1.0) // 1 boundary found
  // Boundary at frame 4 (between col 4 and 5)
  assert_true(out[1] >= 4.0 && out[1] <= 5.0) // frame index near the transition
  assert_true(out[2] > 0.0) // confidence > 0
}

test "detect_boundaries_error_cases" {
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(41, 0.0)
  assert_eq(mengxi_detect_scene_boundaries(12, strip, 1, 2, 300, 5, 5, 41, out), -1) // width <= 1
  assert_eq(mengxi_detect_scene_boundaries(12, strip, 4, 1, 300, 5, 0, 41, out), -1) // max_boundaries <= 0
  assert_eq(mengxi_detect_scene_boundaries(6, strip, 4, 1, 300, 5, 5, 41, out), -1)  // strip_len < expected
  assert_eq(mengxi_detect_scene_boundaries(12, strip, 4, 1, 300, 5, 5, 10, out), -2) // out_len too small
}
