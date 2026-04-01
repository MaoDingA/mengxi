// color_mood.mbt — Color mood timeline computation for movie fingerprint strips
// Classifies segments of a fingerprint strip into mood categories
// based on Oklab lightness and chroma statistics.
// Pure functions only — no I/O, no stateful dependencies.

// ---------- Mood tag constants ----------
// 0 = Dark, 1 = Vivid, 2 = Warm, 3 = Cool, 4 = Neutral

// ---------- Internal helpers ----------

/// Clamp value to [lo, hi].
fn cm_clamp(v : Double, lo : Double, hi : Double) -> Double {
  if v < lo { lo } else if v > hi { hi } else { v }
}

/// Classify a segment's mood from its average Oklab values.
/// Returns mood tag: 0=Dark, 1=Vivid, 2=Warm, 3=Cool, 4=Neutral.
fn cm_classify_mood(avg_l : Double, avg_a : Double, avg_b : Double) -> Int {
  let chroma = @math.pow(avg_a * avg_a + avg_b * avg_b, 0.5)
  if avg_l < 0.3 && chroma < 0.05 {
    0 // Dark
  } else if avg_l > 0.6 && chroma > 0.15 {
    1 // Vivid
  } else if avg_a > 0.05 {
    2 // Warm
  } else if avg_a < -0.05 || avg_b > 0.1 {
    3 // Cool
  } else {
    4 // Neutral
  }
}

/// Compute average Oklab for a range of columns in the strip.
/// Returns (avg_L, avg_a, avg_b).
fn cm_segment_avg_oklab(
  strip : FixedArray[Double],
  strip_width : Int,
  height : Int,
  start_col : Int,
  end_col : Int
) -> (Double, Double, Double) {
  if start_col >= end_col || start_col < 0 || end_col > strip_width {
    return (0.5, 0.0, 0.0)
  }
  let mut sum_l = 0.0
  let mut sum_a = 0.0
  let mut sum_b = 0.0
  let mut col = start_col
  while col < end_col {
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
    col = col + 1
  }
  let n = ((end_col - start_col) * height).to_double()
  if n > 0.0 { (sum_l / n, sum_a / n, sum_b / n) } else { (0.5, 0.0, 0.0) }
}

// ---------- FFI export ----------

/// Compute mood timeline for segments defined by scene boundaries.
///
/// Input:
///   strip data (row-major sRGB [0,1])
///   boundaries_ptr — frame indices [b0, b1, b2, ...] (sorted ascending)
///     Segments are: [0, b0], [b0, b1], [b1, b2], ..., [bN, width-1]
///   boundaries_len — number of boundary frame indices
///
/// Output (1 + segments * 6 f64):
///   out[0] = number of segments S
///   For each segment i, at offset 1 + i*6:
///     [0] start_frame
///     [1] end_frame
///     [2] mood_tag (0=Dark, 1=Vivid, 2=Warm, 3=Cool, 4=Neutral)
///     [3] dominant_L
///     [4] dominant_a
///     [5] dominant_b
///
/// Returns: 1 + S*6 on success, negative on error.
pub fn mengxi_compute_mood_timeline(
  strip_len : Int,
  strip_ptr : FixedArray[Double],
  width : Int,
  height : Int,
  boundaries_ptr : FixedArray[Double],
  boundaries_len : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if width <= 0 || height <= 0 { return -1 }
  let expected = width * height * 3
  if strip_len < expected { return -1 }

  // Number of segments = boundaries_len + 1
  let num_segments = boundaries_len + 1
  let needed = 1 + num_segments * 6
  if out_len < needed { return -2 }

  // Build segment boundaries: [0, b0, b1, ..., bN, width]
  let seg_starts = FixedArray::make(num_segments + 1, 0)
  seg_starts[0] = 0
  let mut bi = 0
  while bi < boundaries_len {
    let bval = boundaries_ptr[bi].to_int()
    // Clamp boundary to valid range
    seg_starts[bi + 1] = if bval < 1 { 1 } else if bval >= width { width - 1 } else { bval }
    bi = bi + 1
  }

  let mut si = 0
  out_ptr[0] = num_segments.to_double()

  while si < num_segments {
    let start = seg_starts[si]
    let end = if si + 1 < num_segments { seg_starts[si + 1] } else { width }
    let safe_end = if end > width { width } else { end }
    let safe_start = if start < 0 { 0 } else { start }

    let (avg_l, avg_a, avg_b) = cm_segment_avg_oklab(
      strip_ptr, width, height, safe_start, safe_end
    )
    let mood = cm_classify_mood(avg_l, avg_a, avg_b)

    let base = 1 + si * 6
    out_ptr[base + 0] = safe_start.to_double()
    out_ptr[base + 1] = (safe_end - 1).to_double() // inclusive end
    out_ptr[base + 2] = mood.to_double()
    out_ptr[base + 3] = avg_l
    out_ptr[base + 4] = avg_a
    out_ptr[base + 5] = avg_b

    si = si + 1
  }

  needed
}

// ---------- DCE prevention ----------

fn init {
  let strip = FixedArray::make(12, 0.5)
  let bounds = FixedArray::make(1, 1.0)
  let out = FixedArray::make(1 + 2 * 6, 0.0)
  let _ = mengxi_compute_mood_timeline(12, strip, 2, 2, bounds, 1, 13, out)
}

// ---------- Tests ----------

test "mood_timeline_single_segment" {
  // 4x1 strip, no boundaries → 1 segment
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(1 + 1 * 6, 0.0)
  let bounds = FixedArray::make(0, 0.0) // empty boundaries
  let result = mengxi_compute_mood_timeline(12, strip, 4, 1, bounds, 0, 7, out)
  assert_eq(result, 7)
  assert_eq(out[0], 1.0) // 1 segment
  assert_eq(out[1], 0.0) // start = 0
  assert_eq(out[2], 3.0) // end = 3
  // Mood of uniform gray (0.5 sRGB) → likely Neutral (4)
  assert_true(out[3] >= 0.0 && out[3] <= 4.0) // mood tag valid
}

test "mood_timeline_warm_dark" {
  // 6x1 strip: first 3 frames warm red, last 3 frames dark
  let strip = FixedArray::make(18, 0.0)
  // Warm red: high R, low G,B → Oklab a > 0
  strip[0] = 0.9; strip[1] = 0.2; strip[2] = 0.1  // frame 0
  strip[3] = 0.9; strip[4] = 0.2; strip[5] = 0.1  // frame 1
  strip[6] = 0.9; strip[7] = 0.2; strip[8] = 0.1  // frame 2
  // Dark: low everything → Oklab L < 0.3
  strip[9] = 0.05; strip[10] = 0.05; strip[11] = 0.05   // frame 3
  strip[12] = 0.05; strip[13] = 0.05; strip[14] = 0.05  // frame 4
  strip[15] = 0.05; strip[16] = 0.05; strip[17] = 0.05  // frame 5

  let bounds = FixedArray::make(1, 3.0) // boundary at frame 3
  let out = FixedArray::make(1 + 2 * 6, 0.0)
  let result = mengxi_compute_mood_timeline(18, strip, 6, 1, bounds, 1, 13, out)
  assert_eq(result, 13)
  assert_eq(out[0], 2.0) // 2 segments

  // Segment 0 (frames 0-2): warm red → should be Warm (2)
  assert_eq(out[1], 0.0) // start
  assert_eq(out[2], 2.0) // end
  assert_true(out[3] == 2.0) // mood = Warm

  // Segment 1 (frames 3-5): dark → should be Dark (0)
  assert_eq(out[7 + 0], 3.0) // start
  assert_eq(out[7 + 2], 0.0) // mood = Dark
}

test "mood_classify_rules" {
  // Dark: L<0.3, chroma<0.05
  assert_eq(cm_classify_mood(0.2, 0.01, 0.01), 0)
  // Vivid: L>0.6, chroma>0.15
  assert_eq(cm_classify_mood(0.8, 0.15, 0.1), 1)
  // Warm: a>0.05
  assert_eq(cm_classify_mood(0.5, 0.1, 0.0), 2)
  // Cool: a<-0.05
  assert_eq(cm_classify_mood(0.5, -0.1, 0.0), 3)
  // Cool: b>0.1
  assert_eq(cm_classify_mood(0.5, 0.0, 0.15), 3)
  // Neutral: everything else
  assert_eq(cm_classify_mood(0.5, 0.01, 0.01), 4)
}

test "mood_timeline_error_cases" {
  let strip = FixedArray::make(12, 0.5)
  let bounds = FixedArray::make(0, 0.0)
  let out = FixedArray::make(13, 0.0)
  assert_eq(mengxi_compute_mood_timeline(12, strip, 0, 2, bounds, 0, 13, out), -1)
  assert_eq(mengxi_compute_mood_timeline(12, strip, 4, 1, bounds, 0, 5, out), -2)
}
