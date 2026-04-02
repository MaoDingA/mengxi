// color_distribution.mbt — Color distribution classification for movie fingerprints
//
// Classifies strip pixels into 7 color categories based on Oklab hue angle.
// Produces per-category fraction + average RGB, plus neutral fraction.
//
// Categories (by hue angle in degrees):
//   0: RED      — 345-15 (wraps around 0)
//   1: SKIN     — 15-45 + chroma<0.15 + L>0.3
//   2: YELLOW   — 45-70
//   3: GREEN    — 70-165
//   4: CYAN     — 165-200
//   5: BLUE     — 200-270
//   6: MAGENTA  — 270-345
//
// Pixels with chroma below the threshold are classified as NEUTRAL (not counted).
//
// Output layout (29 doubles):
//   For each category i in 0..6: out[i*4+0]=fraction, out[i*4+1]=avg_r, out[i*4+2]=avg_g, out[i*4+3]=avg_b
//   out[28] = neutral_fraction
//
// Pure functions only — no I/O, no stateful dependencies.

/// Number of color categories
let cd_num_categories : Int = 7

/// Pi constant
let cd_pi : Double = 3.14159265358979323846

// ---------- Internal helpers ----------

/// Classify a pixel by Oklab hue angle, lightness, and chroma.
/// Returns category index (0-6) or -1 for NEUTRAL.
fn cd_classify_pixel(l : Double, a : Double, b : Double, min_chroma : Double) -> Int {
  let chroma = my_sqrt(a * a + b * b)
  if chroma < min_chroma {
    return -1  // NEUTRAL
  }
  let hue = cp_hue_angle(a, b) // [0, 2*pi)
  let deg = hue * 180.0 / cd_pi

  // SKIN check: warm hue + low chroma + adequate lightness
  if deg >= 15.0 && deg < 45.0 && chroma < 0.15 && l > 0.3 {
    return 1  // SKIN
  }

  // Standard hue-based classification
  if deg >= 345.0 || deg < 15.0 {
    0   // RED (wraps)
  } else if deg >= 15.0 && deg < 45.0 {
    // Already checked SKIN above; if not skin, it's warm orange-red
    0   // RED
  } else if deg >= 45.0 && deg < 70.0 {
    2   // YELLOW
  } else if deg >= 70.0 && deg < 165.0 {
    3   // GREEN
  } else if deg >= 165.0 && deg < 200.0 {
    4   // CYAN
  } else if deg >= 200.0 && deg < 270.0 {
    5   // BLUE
  } else {
    6   // MAGENTA (270-345)
  }
}

// ---------- FFI Export ----------

/// Classify color distribution from fingerprint strip data.
///
/// Input: strip data (row-major interleaved sRGB [0,1])
///   width = number of frame columns, height = frame pixel height
///   min_chroma_permille = minimum chroma threshold (e.g. 30 = 0.030)
/// Output: 29 f64 values (7 categories * 4 values + 1 neutral fraction)
pub fn mengxi_classify_color_distribution(
  strip_len : Int,
  strip_ptr : FixedArray[Double],
  width : Int,
  height : Int,
  min_chroma_permille : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if width <= 0 || height <= 0 {
    return -1
  }
  let expected_strip = width * height * 3
  if strip_len < expected_strip {
    return -1
  }
  let expected_out = cd_num_categories * 4 + 1  // 29
  if out_len < expected_out {
    return -2
  }

  let min_chroma = min_chroma_permille.to_double() / 1000.0

  // Per-category accumulators: count, sum_r, sum_g, sum_b
  let counts = FixedArray::make(cd_num_categories, 0)
  let sum_r = FixedArray::make(cd_num_categories, 0.0)
  let sum_g = FixedArray::make(cd_num_categories, 0.0)
  let sum_b = FixedArray::make(cd_num_categories, 0.0)
  let mut total_pixels = 0
  let mut neutral_count = 0

  // Iterate over all pixels in the strip
  for col = 0; col < width; col = col + 1 {
    for row = 0; row < height; row = row + 1 {
      let idx = (col * height + row) * 3
      let r = strip_ptr[idx]
      let g = strip_ptr[idx + 1]
      let bv = strip_ptr[idx + 2]
      let (l_val, a_val, b_val) = srgb_to_oklab(r, g, bv)

      total_pixels = total_pixels + 1
      let cat = cd_classify_pixel(l_val, a_val, b_val, min_chroma)

      if cat < 0 {
        neutral_count = neutral_count + 1
      } else {
        counts[cat] = counts[cat] + 1
        sum_r[cat] = sum_r[cat] + r
        sum_g[cat] = sum_g[cat] + g
        sum_b[cat] = sum_b[cat] + bv
      }
    }
  }

  // Write output: for each category, [fraction, avg_r, avg_g, avg_b]
  let total_f = total_pixels.to_double()
  for i = 0; i < cd_num_categories; i = i + 1 {
    let base = i * 4
    let c = counts[i]
    if c > 0 {
      let cf = c.to_double()
      out_ptr[base] = cf / total_f
      out_ptr[base + 1] = sum_r[i] / cf
      out_ptr[base + 2] = sum_g[i] / cf
      out_ptr[base + 3] = sum_b[i] / cf
    } else {
      out_ptr[base] = 0.0
      out_ptr[base + 1] = 0.0
      out_ptr[base + 2] = 0.0
      out_ptr[base + 3] = 0.0
    }
  }

  // Neutral fraction
  out_ptr[28] = neutral_count.to_double() / total_f

  expected_out
}

// ---------- DCE Prevention ----------

fn init {
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(29, 0.0)
  let _ = mengxi_classify_color_distribution(
    12, strip, 2, 2,
    30,
    29, out
  )
}

// ---------- Tests ----------

test {
  // All-same color should classify into one category
  let strip = FixedArray::make(12, 0.0)  // 2x2, 4 pixels
  // Set all pixels to pure red
  strip[0] = 1.0; strip[1] = 0.0; strip[2] = 0.0
  strip[3] = 1.0; strip[4] = 0.0; strip[5] = 0.0
  strip[6] = 1.0; strip[7] = 0.0; strip[8] = 0.0
  strip[9] = 1.0; strip[10] = 0.0; strip[11] = 0.0
  let out = FixedArray::make(29, 0.0)
  let result = mengxi_classify_color_distribution(12, strip, 2, 2, 30, 29, out)
  assert_eq(result, 29)
  // RED fraction should be 1.0
  assert_eq(approx_eq(out[0], 1.0), true)
  // All other categories should be 0
  assert_eq(approx_eq(out[4], 0.0), true)  // SKIN
  assert_eq(approx_eq(out[8], 0.0), true)  // YELLOW
}

test {
  // All gray pixels should be neutral
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(29, 0.0)
  let result = mengxi_classify_color_distribution(12, strip, 2, 2, 30, 29, out)
  assert_eq(result, 29)
  // Neutral fraction should be 1.0
  assert_eq(approx_eq(out[28], 1.0), true)
  // All categories should be 0
  assert_eq(approx_eq(out[0], 0.0), true)
}

test {
  // Error on invalid dimensions
  let strip = FixedArray::make(12, 0.0)
  let out = FixedArray::make(29, 0.0)
  let result = mengxi_classify_color_distribution(12, strip, 0, 2, 30, 29, out)
  assert_eq(result, -1)
}

test {
  // Error on output too small
  let strip = FixedArray::make(12, 0.0)
  let out = FixedArray::make(10, 0.0)
  let result = mengxi_classify_color_distribution(12, strip, 2, 2, 30, 10, out)
  assert_eq(result, -2)
}
