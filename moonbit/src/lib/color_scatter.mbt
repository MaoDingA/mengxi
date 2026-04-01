// color_scatter.mbt — Frame-level color scatter distribution on Oklab a-b plane
//
// Extracts per-frame center pixel color projected to Oklab (L, a, b) space.
// This enables visualization similar to the "Tenet case" from the WeChat article:
// each frame becomes a point on the a-b chromaticity plane, revealing the movie's
// color palette distribution and complementary color usage.
//
// Pure functions only — no I/O, no stateful dependencies.

// ---------- Internal helpers ----------

/// Clamp value to [lo, hi].
fn cs_clamp(v : Double, lo : Double, hi : Double) -> Double {
  if v < lo { lo } else if v > hi { hi } else { v }
}

// ---------- FFI exports ----------

/// Extract per-frame Oklab (L, a, b) from a fingerprint strip.
///
/// For each frame column, samples the center row pixel and converts to Oklab.
/// This produces a scatter plot on the a-b chromaticity plane.
///
/// Input: strip image (row-major interleaved sRGB [0,1])
///   strip_len  — total f64 count (width * height * 3)
///   strip_ptr  — pixel data
///   width      — strip width (number of frames)
///   height     — strip height
///   out_len    — must be >= width * 3
///   out_ptr    — output buffer
///
/// Output (width * 3 f64):
///   For each frame i: out[i*3] = L, out[i*3+1] = a, out[i*3+2] = b
///
/// Returns: number of output values (width * 3) on success, negative on error.
pub fn mengxi_extract_frame_scatter(
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
  let out_expected = width * 3
  if out_len < out_expected { return -2 }

  // Sample center row for each frame column
  let sample_y = (height / 2).min(height - 1)

  let mut col = 0
  while col < width {
    let idx = (sample_y * width + col) * 3
    let r = strip_ptr[idx]
    let g = strip_ptr[idx + 1]
    let bv = strip_ptr[idx + 2]
    let (l, a, b) = srgb_to_oklab(r, g, bv)
    out_ptr[col * 3] = l
    out_ptr[col * 3 + 1] = a
    out_ptr[col * 3 + 2] = b
    col = col + 1
  }

  out_expected
}

/// Compute a-b scatter density grid for visualization.
///
/// Divides the Oklab a-b plane into a grid and counts how many frames
/// fall into each cell. Useful for rendering a density heatmap.
///
/// Input: strip image + grid resolution
///   grid_size   — grid resolution (e.g., 32 for 32×32 grid)
///   a_range_permille — range of a axis in permille (e.g., 500 = [-0.5, 0.5])
///   b_range_permille — range of b axis in permille (e.g., 500 = [-0.5, 0.5])
///
/// Output: grid_size * grid_size f64 (density values, normalized to [0,1])
///
/// Returns: grid_size * grid_size on success, negative on error.
pub fn mengxi_compute_scatter_density(
  strip_len : Int,
  strip_ptr : FixedArray[Double],
  width : Int,
  height : Int,
  grid_size : Int,
  a_range_permille : Int,
  b_range_permille : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if width <= 0 || height <= 0 || grid_size <= 0 { return -1 }
  let expected = width * height * 3
  if strip_len < expected { return -1 }
  let out_expected = grid_size * grid_size
  if out_len < out_expected { return -2 }

  let a_range = a_range_permille.to_double() / 1000.0  // e.g., 0.5
  let b_range = b_range_permille.to_double() / 1000.0
  let a_min = -a_range
  let b_min = -b_range
  let a_step = 2.0 * a_range / grid_size.to_double()
  let b_step = 2.0 * b_range / grid_size.to_double()

  // Initialize grid
  let mut i = 0
  while i < out_expected {
    out_ptr[i] = 0.0
    i = i + 1
  }

  // Sample center row pixels and accumulate
  let sample_y = (height / 2).min(height - 1)
  let mut col = 0
  while col < width {
    let idx = (sample_y * width + col) * 3
    let r = strip_ptr[idx]
    let g = strip_ptr[idx + 1]
    let bv = strip_ptr[idx + 2]
    let (_, a, b) = srgb_to_oklab(r, g, bv)

    // Map a,b to grid coordinates
    let ga = ((a - a_min) / a_step).to_int()
    let gb = ((b - b_min) / b_step).to_int()
    let ga_c = cs_clamp(ga.to_double(), 0.0, (grid_size - 1).to_double()).to_int()
    let gb_c = cs_clamp(gb.to_double(), 0.0, (grid_size - 1).to_double()).to_int()
    out_ptr[gb_c * grid_size + ga_c] = out_ptr[gb_c * grid_size + ga_c] + 1.0

    col = col + 1
  }

  // Normalize to [0, 1]
  let mut max_val = 0.0
  let mut j = 0
  while j < out_expected {
    if out_ptr[j] > max_val { max_val = out_ptr[j] }
    j = j + 1
  }
  if max_val > 0.0 {
    let mut k = 0
    while k < out_expected {
      out_ptr[k] = out_ptr[k] / max_val
      k = k + 1
    }
  }

  out_expected
}

// ---------- DCE prevention ----------

fn init {
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(6, 0.0)
  let _ = mengxi_extract_frame_scatter(12, strip, 2, 2, 6, out)

  let strip2 = FixedArray::make(12, 0.5)
  let dout = FixedArray::make(16, 0.0)
  let _ = mengxi_compute_scatter_density(12, strip2, 2, 2, 4, 500, 500, 16, dout)
}

// ---------- Tests ----------

test "extract_frame_scatter_uniform" {
  // 2x2 strip, all white
  let strip = FixedArray::make(12, 1.0)
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_extract_frame_scatter(12, strip, 2, 2, 6, out)
  assert_eq(result, 6)
  // White → L≈1.0, a≈0, b≈0
  assert_true(out[0] > 0.99)  // L near 1.0
  assert_true(out[1] < 0.01)  // a near 0
  assert_true(out[2] < 0.01)  // b near 0
  // Second frame same
  assert_true(out[3] > 0.99)
  assert_true(out[4] < 0.01)
}

test "extract_frame_scatter_colored" {
  // 2x1 strip: frame 0 = red (1,0,0), frame 1 = green (0,1,0)
  let strip = FixedArray::make(6, 0.0)
  strip[0] = 1.0; strip[1] = 0.0; strip[2] = 0.0  // red
  strip[3] = 0.0; strip[4] = 1.0; strip[5] = 0.0  // green
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_extract_frame_scatter(6, strip, 2, 1, 6, out)
  assert_eq(result, 6)
  // Red → positive a
  assert_true(out[1] > 0.1)
  // Green → negative a
  assert_true(out[4] < -0.1)
}

test "extract_frame_scatter_error_cases" {
  let strip = FixedArray::make(12, 0.5)
  let out = FixedArray::make(6, 0.0)
  assert_eq(mengxi_extract_frame_scatter(12, strip, 0, 2, 6, out), -1)
  assert_eq(mengxi_extract_frame_scatter(12, strip, 2, 0, 6, out), -1)
  assert_eq(mengxi_extract_frame_scatter(6, strip, 2, 2, 6, out), -1)
  assert_eq(mengxi_extract_frame_scatter(12, strip, 2, 2, 3, out), -2)
}

test "compute_scatter_density_basic" {
  // 4x1 strip with two colors
  let strip = FixedArray::make(12, 0.0)
  strip[0] = 1.0; strip[1] = 0.0; strip[2] = 0.0  // red
  strip[3] = 0.0; strip[4] = 1.0; strip[5] = 0.0  // green
  strip[6] = 0.0; strip[7] = 0.0; strip[8] = 1.0  // blue
  strip[9] = 1.0; strip[10] = 1.0; strip[11] = 1.0  // white
  let dout = FixedArray::make(4, 0.0)
  let result = mengxi_compute_scatter_density(12, strip, 4, 1, 2, 500, 500, 4, dout)
  assert_eq(result, 4)
  // At least some cells should be non-zero
  let mut has_nonzero = false
  let mut i = 0
  while i < 4 {
    if dout[i] > 0.0 { has_nonzero = true }
    i = i + 1
  }
  assert_true(has_nonzero)
}

test "compute_scatter_density_uniform" {
  // Uniform gray → all points near center
  let strip = FixedArray::make(12, 0.5)
  let dout = FixedArray::make(4, 0.0)
  let result = mengxi_compute_scatter_density(12, strip, 2, 2, 2, 500, 500, 4, dout)
  assert_eq(result, 4)
  // All frames at same point → one cell = 1.0, others = 0.0
  let mut sum = 0.0
  let mut i = 0
  while i < 4 {
    sum = sum + dout[i]
    i = i + 1
  }
  assert_true(sum > 0.0 && sum <= 1.01)
}
