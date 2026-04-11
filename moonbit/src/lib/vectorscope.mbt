///|
/// Compute vectorscope density grid from fingerprint strip data.
///
/// For each pixel in the strip, converts to Oklab, computes hue angle and chroma,
/// bins into a polar grid (angle_bins x radius_bins), and normalizes to [0,1].
///
/// Input: strip data (row-major interleaved sRGB [0,1])
///   width = number of frame columns, height = frame pixel height
/// Output: angle_bins * radius_bins f64 values (density, normalized to [0,1])
pub fn mengxi_compute_vectorscope_density(
  strip_len : Int,
  strip_ptr : FixedArray[Double],
  width : Int,
  height : Int,
  angle_bins : Int,
  radius_bins : Int,
  max_chroma_permille : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if width <= 0 || height <= 0 || angle_bins <= 0 || radius_bins <= 0 {
    return -1
  }
  let expected_strip = width * height * 3
  if strip_len < expected_strip {
    return -1
  }
  let expected_out = angle_bins * radius_bins
  if out_len < expected_out {
    return -2
  }

  let max_chroma = max_chroma_permille.to_double() / 1000.0
  if max_chroma <= 0.0 {
    return -1
  }

  let radius_step = max_chroma / radius_bins.to_double()

  // Initialize grid as double array
  let grid_size = angle_bins * radius_bins
  let grid = FixedArray::make(grid_size, 0.0)
  let mut max_val = 0.0

  // Iterate over all pixels in the strip
  for col in 0..<width {
    for row in 0..<height {
      let idx = (col * height + row) * 3
      let r = strip_ptr[idx]
      let g = strip_ptr[idx + 1]
      let bv = strip_ptr[idx + 2]
      let (_l_val, a_val, b_val) = srgb_to_oklab(r, g, bv)

      let chroma = my_sqrt(a_val * a_val + b_val * b_val)
      if chroma < 0.001 {
        continue  // skip near-achromatic pixels
      }
      let hue = cp_hue_angle(a_val, b_val) // [0, 2*pi)
      let angle_deg = hue * 180.0 / pi_const()
      let mut angle_bin = (angle_deg / 360.0 * angle_bins.to_double()).to_int()
      if angle_bin >= angle_bins {
        angle_bin = angle_bins - 1
      }
      let mut radius_bin = (chroma / radius_step).to_int()
      if radius_bin >= radius_bins {
        radius_bin = radius_bins - 1
      }
      let cell = angle_bin * radius_bins + radius_bin
      let new_val = grid[cell] + 1.0
      grid[cell] = new_val
      if new_val > max_val {
        max_val = new_val
      }
    }
  }

  // Normalize to [0, 1]
  if max_val > 0.0 {
    for i in 0..<grid_size {
      out_ptr[i] = grid[i] / max_val
    }
  } else {
    // All zeros — leave output as-is
  }

  grid_size
}

///|
fn init {
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(1440, 0.0)
  let _ = mengxi_compute_vectorscope_density(
    12, strip, 2, 2,
    72, 20, 400,
    1440, out
  )
}
