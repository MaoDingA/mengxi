// color_dna.mbt — Color DNA extraction and comparison for movie fingerprints
// Extracts a compact color signature from strip data (Oklab-based),
// then computes similarity between two signatures.
// Pure functions only — no I/O, no stateful dependencies.

// ---------- Internal helpers ----------

/// Clamp value to [lo, hi].
fn cd_clamp(v : Double, lo : Double, hi : Double) -> Double {
  if v < lo { lo } else if v > hi { hi } else { v }
}

/// Compute hue angle from Oklab a,b components.
/// Returns angle in [0, 2*pi).
fn cd_hue_angle(a : Double, b : Double) -> Double {
  hue_angle_from_ab(a, b)
}

// ---------- FFI exports ----------

/// Extract color DNA from a fingerprint strip.
///
/// Input: strip image (row-major interleaved sRGB [0,1])
///   strip_len  — total f64 count (width * height * 3)
///   strip_ptr  — pixel data
///   width      — strip width (number of frames)
///   height     — strip height (frame height)
///   out_len    — must be >= 18
///   out_ptr    — output buffer
///
/// Output (18 f64):
///   [0..2]   avg_L, avg_a, avg_b (mean Oklab)
///   [3..14]  hue_distribution[12] (normalized, 12 sectors × 30°)
///   [15]     contrast (L channel standard deviation)
///   [16]     warmth (mean Oklab a channel)
///   [17]     saturation (mean chroma sqrt(a²+b²))
///
/// Returns: 18 on success, negative error code on failure.
pub fn mengxi_extract_color_dna(
  strip_len : Int,
  strip_ptr : FixedArray[Double],
  width : Int,
  height : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if width <= 0 || height <= 0 { return -1 }
  let expected = width * height * 3
  if strip_len < expected { return -1 }
  if out_len < 18 { return -2 }

  let num_pixels = width * height
  let n = num_pixels.to_double()

  // Single pass: accumulate Oklab statistics
  let mut sum_l = 0.0
  let mut sum_l2 = 0.0
  let mut sum_a = 0.0
  let mut sum_b = 0.0
  let mut chroma_sum = 0.0
  let hue_bins = FixedArray::make(12, 0.0)

  let mut i = 0
  while i < num_pixels {
    let r = strip_ptr[i * 3]
    let g = strip_ptr[i * 3 + 1]
    let b = strip_ptr[i * 3 + 2]
    let (l, a, bv) = srgb_to_oklab(r, g, b)
    sum_l = sum_l + l
    sum_l2 = sum_l2 + l * l
    sum_a = sum_a + a
    sum_b = sum_b + bv

    // Chroma = sqrt(a² + b²)
    let chroma = @math.pow(a * a + bv * bv, 0.5)
    chroma_sum = chroma_sum + chroma

    // Hue angle → bin (skip low-chroma pixels)
    if chroma > 0.001 {
      let angle = cd_hue_angle(a, bv)
      let bin = (angle / (2.0 * pi_const()) * 12.0).to_int()
      let bin_idx = cd_clamp(bin.to_double(), 0.0, 11.0).to_int()
      hue_bins[bin_idx] = hue_bins[bin_idx] + 1.0
    }

    i = i + 1
  }

  let avg_l = sum_l / n
  let avg_a = sum_a / n
  let avg_b = sum_b / n

  // Contrast = stddev(L) = sqrt(E[L²] - E[L]²)
  let variance = sum_l2 / n - avg_l * avg_l
  let contrast = @math.pow(if variance < 0.0 { 0.0 } else { variance }, 0.5)

  // Normalize hue distribution
  let mut hue_total = 0.0
  let mut h = 0
  while h < 12 {
    hue_total = hue_total + hue_bins[h]
    h = h + 1
  }
  let mut h2 = 0
  while h2 < 12 {
    hue_bins[h2] = if hue_total > 0.0 { hue_bins[h2] / hue_total } else { 1.0 / 12.0 }
    h2 = h2 + 1
  }

  // Write output
  out_ptr[0] = avg_l
  out_ptr[1] = avg_a
  out_ptr[2] = avg_b
  let mut hi = 0
  while hi < 12 {
    out_ptr[3 + hi] = hue_bins[hi]
    hi = hi + 1
  }
  out_ptr[15] = contrast
  out_ptr[16] = avg_a   // warmth = mean a
  out_ptr[17] = chroma_sum / n // saturation

  18
}

/// Compare two color DNA signatures.
///
/// Input: two DNA arrays of dna_len each (18 f64, output from extract)
/// Output (4 f64):
///   [0] overall_similarity (0-1, 1 = identical)
///   [1] hue_similarity (0-1, Bhattacharyya coefficient)
///   [2] contrast_diff (0-1, absolute difference)
///   [3] warmth_diff (0-1, absolute difference)
///
/// Returns: 4 on success, negative error code on failure.
pub fn mengxi_compare_color_dna(
  dna_a_ptr : FixedArray[Double],
  dna_b_ptr : FixedArray[Double],
  dna_len : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if dna_len < 18 { return -1 }
  if out_len < 4 { return -2 }

  // Oklab Delta-E between average colors
  let dl = dna_a_ptr[0] - dna_b_ptr[0]
  let da = dna_a_ptr[1] - dna_b_ptr[1]
  let db = dna_a_ptr[2] - dna_b_ptr[2]
  let delta_e = @math.pow(dl * dl + da * da + db * db, 0.5)
  let overall_sim = 1.0 - cd_clamp(delta_e, 0.0, 1.0)

  // Hue histogram Bhattacharyya coefficient
  let mut bc = 0.0
  let mut i = 0
  while i < 12 {
    let a = dna_a_ptr[3 + i]
    let b = dna_b_ptr[3 + i]
    // Clamp to prevent sqrt(0) issues
    let a_c = if a < 0.0000000001 { 0.0000000001 } else { a }
    let b_c = if b < 0.0000000001 { 0.0000000001 } else { b }
    bc = bc + @math.pow(a_c * b_c, 0.5)
    i = i + 1
  }
  let hue_sim = cd_clamp(bc, 0.0, 1.0)

  // Contrast diff
  let cdiff = @math.pow(
    (dna_a_ptr[15] - dna_b_ptr[15]) * (dna_a_ptr[15] - dna_b_ptr[15]),
    0.5
  )
  let cdiff_norm = cd_clamp(cdiff, 0.0, 1.0)

  // Warmth diff
  let wdiff = @math.pow(
    (dna_a_ptr[16] - dna_b_ptr[16]) * (dna_a_ptr[16] - dna_b_ptr[16]),
    0.5
  )
  let wdiff_norm = cd_clamp(wdiff, 0.0, 1.0)

  out_ptr[0] = overall_sim
  out_ptr[1] = hue_sim
  out_ptr[2] = cdiff_norm
  out_ptr[3] = wdiff_norm

  4
}

// ---------- DCE prevention ----------

fn init {
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(18, 0.0)
  let _ = mengxi_extract_color_dna(12, strip, 2, 2, 18, out)

  let dna = FixedArray::make(18, 0.5)
  let cout = FixedArray::make(4, 0.0)
  let _ = mengxi_compare_color_dna(dna, dna, 18, 4, cout)
}

// ---------- Tests ----------

test "extract_color_dna_uniform" {
  // 2x2 uniform strip: all pixels (1.0, 1.0, 1.0) = white
  let strip = FixedArray::make(12, 1.0)
  let out = FixedArray::make(18, 0.0)

  let result = mengxi_extract_color_dna(12, strip, 2, 2, 18, out)
  assert_eq(result, 18)

  // White pixel → Oklab L≈1.0, a≈0, b≈0
  assert_true(out[0] > 0.99)   // avg_L near 1.0
  assert_true(out[1] < 0.01)   // avg_a near 0
  assert_true(out[2] < 0.01)   // avg_b near 0
  assert_true(out[15] < 0.01)  // contrast near 0 (uniform)
  assert_true(out[17] < 0.01)  // saturation near 0 (achromatic)
}

test "extract_color_dna_black" {
  let strip = FixedArray::make(12, 0.0) // all black
  let out = FixedArray::make(18, 0.0)
  let result = mengxi_extract_color_dna(12, strip, 2, 2, 18, out)
  assert_eq(result, 18)
  assert_true(out[0] < 0.01) // L near 0
}

test "extract_color_dna_error_cases" {
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(18, 0.0)
  assert_eq(mengxi_extract_color_dna(12, strip, 0, 2, 18, out), -1)
  assert_eq(mengxi_extract_color_dna(12, strip, 2, 0, 18, out), -1)
  assert_eq(mengxi_extract_color_dna(6, strip, 2, 2, 18, out), -1)
  assert_eq(mengxi_extract_color_dna(12, strip, 2, 2, 10, out), -2)
}

test "compare_color_dna_identical" {
  let dna = FixedArray::make(18, 0.0)
  dna[0] = 0.5; dna[1] = 0.1; dna[2] = 0.05
  dna[3] = 1.0 // all hue in first bin
  let out = FixedArray::make(4, 0.0)
  let result = mengxi_compare_color_dna(dna, dna, 18, 4, out)
  assert_eq(result, 4)
  assert_true(out[0] > 0.99) // identical → similarity ~1.0
  assert_true(out[1] > 0.99) // identical histograms → BC ~1.0
  assert_true(out[2] < 0.01) // no contrast diff
  assert_true(out[3] < 0.01) // no warmth diff
}

test "compare_color_dna_different" {
  let dna_a = FixedArray::make(18, 0.0)
  let dna_b = FixedArray::make(18, 0.0)
  // Very different average colors
  dna_a[0] = 0.9; dna_a[1] = 0.0; dna_a[2] = 0.0  // bright, neutral
  dna_b[0] = 0.1; dna_b[1] = 0.3; dna_b[2] = 0.3   // dark, colorful
  // Different hue distributions
  dna_a[3] = 1.0  // all hue in bin 0
  dna_b[8] = 1.0  // all hue in bin 5

  let out = FixedArray::make(4, 0.0)
  let result = mengxi_compare_color_dna(dna_a, dna_b, 18, 4, out)
  assert_eq(result, 4)
  assert_true(out[0] < 0.3) // low overall similarity
}

test "compare_color_dna_error" {
  let dna = FixedArray::make(18, 0.5)
  let out = FixedArray::make(4, 0.0)
  assert_eq(mengxi_compare_color_dna(dna, dna, 10, 4, out), -1)
  assert_eq(mengxi_compare_color_dna(dna, dna, 18, 2, out), -2)
}
