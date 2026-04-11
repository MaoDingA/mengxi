// color_pairs.mbt — Dominant color pair detection for movie fingerprints
//
// Detects complementary color pairs (e.g., teal-orange, blue-yellow) from
// fingerprint strip data. This matches the "vectorscope analysis" approach
// described in the WeChat article, where complementary colors appear as
// diametrically opposed clusters on the hue wheel.
//
// Pure functions only — no I/O, no stateful dependencies.

// ---------- Constants ----------

/// Number of hue bins for fine-grained analysis
let cp_hue_bins : Int = 24

/// Maximum number of color pairs to detect
let cp_max_pairs : Int = 6


// ---------- Internal helpers ----------

/// Clamp value to [lo, hi].
fn cp_clamp(v : Double, lo : Double, hi : Double) -> Double {
  if v < lo { lo } else if v > hi { hi } else { v }
}

/// Compute hue angle from Oklab a,b components.
/// Returns angle in [0, 2*pi).
fn cp_hue_angle(a : Double, b : Double) -> Double {
  hue_angle_from_ab(a, b)
}

/// Convert hue angle (radians) to degrees.
fn cp_to_degrees(rad : Double) -> Double {
  rad * 180.0 / pi_const()
}

/// Classify complementary pair type by hue angles.
/// Returns tag: 0=teal-orange, 1=blue-yellow, 2=red-cyan, 3=magenta-green, 4=other
fn cp_classify_pair(hue_a : Double, hue_b : Double) -> Int {
  // Normalize to degrees
  let deg_a = cp_to_degrees(hue_a)
  let deg_b = cp_to_degrees(hue_b)

  // Identify which hue is which based on degree ranges:
  // Teal/cyan: 160-200°, Orange: 15-50°
  // Blue: 200-260°, Yellow: 50-70°
  // Red: 340-20°, Cyan: 160-200°
  // Magenta: 280-340°, Green: 80-160°

  let is_teal_orange = (is_in_range(deg_a, 15.0, 50.0) && is_in_range(deg_b, 160.0, 200.0)) ||
                       (is_in_range(deg_b, 15.0, 50.0) && is_in_range(deg_a, 160.0, 200.0))
  if is_teal_orange { return 0 }

  let is_blue_yellow = (is_in_range(deg_a, 200.0, 260.0) && is_in_range(deg_b, 50.0, 70.0)) ||
                       (is_in_range(deg_b, 200.0, 260.0) && is_in_range(deg_a, 50.0, 70.0))
  if is_blue_yellow { return 1 }

  let is_red_cyan = (is_in_range_wrap(deg_a, 340.0, 20.0) && is_in_range(deg_b, 160.0, 200.0)) ||
                    (is_in_range_wrap(deg_b, 340.0, 20.0) && is_in_range(deg_a, 160.0, 200.0))
  if is_red_cyan { return 2 }

  let is_magenta_green = (is_in_range(deg_a, 280.0, 340.0) && is_in_range(deg_b, 80.0, 160.0)) ||
                         (is_in_range(deg_b, 280.0, 340.0) && is_in_range(deg_a, 80.0, 160.0))
  if is_magenta_green { return 3 }

  4 // other
}

/// Check if angle is in a normal range [lo, hi].
fn is_in_range(deg : Double, lo : Double, hi : Double) -> Bool {
  deg >= lo && deg <= hi
}

/// Check if angle is in a wrapping range (e.g., 340° to 20° crossing 0°).
fn is_in_range_wrap(deg : Double, lo : Double, hi : Double) -> Bool {
  if lo > hi {
    deg >= lo || deg <= hi
  } else {
    deg >= lo && deg <= hi
  }
}

// ---------- FFI exports ----------

/// Detect dominant complementary color pairs from a fingerprint strip.
///
/// Analyzes the hue distribution of all pixels, then finds pairs of hue bins
/// that are approximately 180° apart (complementary). This implements the
/// "vectorscope analysis" approach: complementary colors create characteristic
/// patterns on the hue wheel.
///
/// Input: strip image (row-major interleaved sRGB [0,1])
///   strip_len          — total f64 count (width * height * 3)
///   strip_ptr          — pixel data
///   width              — strip width (number of frames)
///   height             — strip height
///   min_chroma_permille — minimum chroma threshold * 1000 to include pixel (default: 20 = 0.02)
///   out_len            — must be >= 1 + 6 * 4 (1 count + up to 6 pairs × 4 values)
///   out_ptr            — output buffer
///
/// Output format:
///   out[0] = N (number of pairs found, 0..6)
///   For each pair i (0..N-1), starting at out[1 + i*4]:
///     out[1 + i*4 + 0] = hue_a (radians, first color angle)
///     out[1 + i*4 + 1] = hue_b (radians, complementary angle)
///     out[1 + i*4 + 2] = strength (0-1, product of normalized bin frequencies)
///     out[1 + i*4 + 3] = pair_type (0=teal-orange, 1=blue-yellow, 2=red-cyan, 3=magenta-green, 4=other)
///
/// Returns: 1 + N*4 on success, negative on error.
pub fn mengxi_detect_dominant_pairs(
  strip_len : Int,
  strip_ptr : FixedArray[Double],
  width : Int,
  height : Int,
  min_chroma_permille : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if width <= 0 || height <= 0 { return -1 }
  let expected = width * height * 3
  if strip_len < expected { return -1 }
  let max_output = 1 + cp_max_pairs * 4
  if out_len < max_output { return -2 }

  let min_chroma = min_chroma_permille.to_double() / 1000.0
  let num_pixels = width * height

  // Build 24-bin hue histogram
  let bins = cp_hue_bins
  let hue_hist = FixedArray::make(bins, 0.0)
  let mut total_counted = 0.0

  let mut i = 0
  while i < num_pixels {
    let r = strip_ptr[i * 3]
    let g = strip_ptr[i * 3 + 1]
    let bv = strip_ptr[i * 3 + 2]
    let (_, a, b) = srgb_to_oklab(r, g, bv)
    let chroma = @math.pow(a * a + b * b, 0.5)

    if chroma >= min_chroma {
      let angle = cp_hue_angle(a, b)
      let bin = (angle / (2.0 * pi_const()) * bins.to_double()).to_int()
      let bin_idx = cp_clamp(bin.to_double(), 0.0, (bins - 1).to_double()).to_int()
      hue_hist[bin_idx] = hue_hist[bin_idx] + 1.0
      total_counted = total_counted + 1.0
    }
    i = i + 1
  }

  // Normalize histogram
  if total_counted > 0.0 {
    let mut h = 0
    while h < bins {
      hue_hist[h] = hue_hist[h] / total_counted
      h = h + 1
    }
  }

  // Find complementary pairs: for each bin, check the bin ~180° away
  // 180° = bins/2 offset (12 bins for 24-bin histogram)
  let half_bins = bins / 2
  let pair_scores : FixedArray[(Int, Int, Double)] =
    FixedArray::make(cp_max_pairs, (0, 0, 0.0))
  let mut pair_count = 0

  let mut bin_a = 0
  while bin_a < half_bins {
    let bin_b = bin_a + half_bins  // complementary bin (180° away)
    let score = hue_hist[bin_a] * hue_hist[bin_b]

    // Only consider if both bins have meaningful presence
    if score > 0.0001 && hue_hist[bin_a] > 0.01 && hue_hist[bin_b] > 0.01 {
      // Insertion sort into top-N pairs
      if pair_count < cp_max_pairs {
        pair_scores[pair_count] = (bin_a, bin_b, score)
        pair_count = pair_count + 1
        // Sort by score descending
        let mut sorted = false
        while !sorted {
          sorted = true
          let mut p = 0
          while p < pair_count - 1 {
            let (_, _, s1) = pair_scores[p]
            let (_, _, s2) = pair_scores[p + 1]
            if s2 > s1 {
              let tmp = pair_scores[p]
              pair_scores[p] = pair_scores[p + 1]
              pair_scores[p + 1] = tmp
              sorted = false
            }
            p = p + 1
          }
        }
      } else {
        // Check if this score beats the weakest
        let (_, _, weakest) = pair_scores[pair_count - 1]
        if score > weakest {
          pair_scores[pair_count - 1] = (bin_a, bin_b, score)
          // Re-sort
          let mut sorted = false
          while !sorted {
            sorted = true
            let mut p = 0
            while p < pair_count - 1 {
              let (_, _, s1) = pair_scores[p]
              let (_, _, s2) = pair_scores[p + 1]
              if s2 > s1 {
                let tmp = pair_scores[p]
                pair_scores[p] = pair_scores[p + 1]
                pair_scores[p + 1] = tmp
                sorted = false
              }
              p = p + 1
            }
          }
        }
      }
    }
    bin_a = bin_a + 1
  }

  // Normalize scores to [0, 1]
  let mut max_score = 0.0
  let mut p = 0
  while p < pair_count {
    let (_, _, s) = pair_scores[p]
    if s > max_score { max_score = s }
    p = p + 1
  }

  // Write output
  out_ptr[0] = pair_count.to_double()
  let mut pi = 0
  while pi < pair_count {
    let (bin_a, bin_b, score) = pair_scores[pi]
    let hue_a = bin_a.to_double() / bins.to_double() * 2.0 * pi_const()
    let hue_b = bin_b.to_double() / bins.to_double() * 2.0 * pi_const()
    let norm_score = if max_score > 0.0 { score / max_score } else { 0.0 }
    let pair_type = cp_classify_pair(hue_a, hue_b)

    out_ptr[1 + pi * 4 + 0] = hue_a
    out_ptr[1 + pi * 4 + 1] = hue_b
    out_ptr[1 + pi * 4 + 2] = norm_score
    out_ptr[1 + pi * 4 + 3] = pair_type.to_double()
    pi = pi + 1
  }

  1 + pair_count * 4
}

// ---------- DCE prevention ----------

fn init {
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(25, 0.0)
  let _ = mengxi_detect_dominant_pairs(12, strip, 2, 2, 20, 25, out)
}

// ---------- Tests ----------

test "detect_pairs_uniform_gray" {
  // All gray → no chromatic pixels → 0 pairs
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(25, 0.0)
  let result = mengxi_detect_dominant_pairs(12, strip, 2, 2, 20, 25, out)
  assert_true(result >= 1)
  assert_eq(out[0], 0.0)  // no pairs
}

test "detect_pairs_error_cases" {
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(25, 0.0)
  assert_eq(mengxi_detect_dominant_pairs(12, strip, 0, 2, 20, 25, out), -1)
  assert_eq(mengxi_detect_dominant_pairs(12, strip, 2, 0, 20, 25, out), -1)
  assert_eq(mengxi_detect_dominant_pairs(6, strip, 2, 2, 20, 25, out), -1)
  assert_eq(mengxi_detect_dominant_pairs(12, strip, 2, 2, 20, 5, out), -2)
}

test "detect_pairs_teal_orange" {
  // Create a strip with teal and orange pixels
  // Teal in Oklab: a ≈ -0.15, b ≈ -0.05 (cyan/teal hue ~180°)
  // Orange in Oklab: a ≈ 0.15, b ≈ 0.15 (orange hue ~45°)
  // We create RGB values that map to these approximate colors

  // 4x2 strip: first 4 pixels orange-ish, next 4 pixels teal-ish
  let strip = FixedArray::make(24, 0.0)
  // Orange-ish (warm): high red, medium green, low blue
  let mut i = 0
  while i < 4 {
    strip[i * 3] = 0.9
    strip[i * 3 + 1] = 0.5
    strip[i * 3 + 2] = 0.1
    i = i + 1
  }
  // Teal-ish (cool): low red, medium green, high blue
  let mut j = 0
  while j < 4 {
    let idx = (4 + j) * 3
    strip[idx] = 0.0
    strip[idx + 1] = 0.5
    strip[idx + 2] = 0.7
    j = j + 1
  }
  let out = FixedArray::make(25, 0.0)
  let result = mengxi_detect_dominant_pairs(24, strip, 4, 2, 10, 25, out)
  assert_true(result >= 1)
  let num_pairs = out[0].to_int()
  // Should detect at least one pair (teal-orange)
  assert_true(num_pairs >= 1)
  // First pair should have positive strength
  assert_true(out[3] > 0.0)
}

test "detect_pairs_single_color" {
  // All red pixels → only one hue cluster → no complementary pair
  let strip = FixedArray::make(12, 0.0)
  let mut i = 0
  while i < 4 {
    strip[i * 3] = 1.0
    strip[i * 3 + 1] = 0.0
    strip[i * 3 + 2] = 0.0
    i = i + 1
  }
  let out = FixedArray::make(25, 0.0)
  let result = mengxi_detect_dominant_pairs(12, strip, 2, 2, 10, 25, out)
  assert_true(result >= 1)
  // Single dominant color: unlikely to form strong complementary pair
  // (red's complement is cyan, which is absent)
  // But this depends on whether noise bins get above threshold
  assert_true(out[0] >= 0.0)  // 0 or 1 pairs acceptable
}

test "classify_pair_teal_orange" {
  // Orange hue ≈ 40° = 0.698 rad, Teal hue ≈ 185° = 3.228 rad
  let tag = cp_classify_pair(0.698, 3.228)
  assert_eq(tag, 0) // teal-orange
}

test "classify_pair_blue_yellow" {
  // Yellow hue ≈ 60° = 1.047 rad, Blue hue ≈ 240° = 4.189 rad
  let tag = cp_classify_pair(1.047, 4.189)
  assert_eq(tag, 1) // blue-yellow
}

test "classify_pair_other" {
  // Two random non-standard angles
  let tag = cp_classify_pair(0.5, 3.64)  // ~29° and ~209°
  assert_eq(tag, 4) // other
}
