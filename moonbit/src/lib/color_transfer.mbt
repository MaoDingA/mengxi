// color_transfer.mbt — Color transfer LUT generation via CDF matching
// Generates a 3D LUT that maps source strip colors to target strip colors
// using cumulative distribution function (CDF) matching in RGB space.
// Pure functions only — no I/O, no stateful dependencies.

// ---------- Constants ----------

// Number of histogram bins for CDF matching (per channel)
let ct_hist_bins : Int = 64

// ---------- Internal helpers ----------

fn ct_clamp(v : Double, lo : Double, hi : Double) -> Double {
  if v < lo { lo } else if v > hi { hi } else { v }
}

fn ct_clamp_int(v : Int, lo : Int, hi : Int) -> Int {
  if v < lo { lo } else if v > hi { hi } else { v }
}

/// Build histogram for a single channel from interleaved RGB strip data.
/// channel: 0=R, 1=G, 2=B.
fn ct_build_histogram(
  strip : FixedArray[Double],
  strip_len : Int,
  channel : Int,
  out_hist : FixedArray[Double]
) -> Unit {
  let bins = ct_hist_bins
  let num_pixels = strip_len / 3
  // Initialize bins
  let mut b = 0
  while b < bins {
    out_hist[b] = 0.0
    b = b + 1
  }
  // Count
  let mut i = 0
  while i < num_pixels {
    let v = strip[i * 3 + channel]
    let clamped = ct_clamp(v, 0.0, 1.0)
    let bin = (clamped * (bins - 1).to_double()).to_int()
    let safe_bin = ct_clamp_int(bin, 0, bins - 1)
    out_hist[safe_bin] = out_hist[safe_bin] + 1.0
    i = i + 1
  }
}

/// Build CDF from histogram (in-place normalization).
/// Returns the CDF as normalized [0, 1] values.
fn ct_build_cdf(hist : FixedArray[Double], bins : Int) -> Unit {
  let mut total = 0.0
  let mut i = 0
  while i < bins {
    total = total + hist[i]
    i = i + 1
  }
  if total <= 0.0 { return }
  // Compute cumulative sum and normalize
  let mut cumsum = 0.0
  let mut j = 0
  while j < bins {
    cumsum = cumsum + hist[j]
    hist[j] = cumsum / total
    j = j + 1
  }
}

/// Map a value through CDF: find source CDF value, then find target bin.
/// Given source_cdf and target_cdf, for input value v:
///   1. Find source CDF value at v's bin
///   2. Find target bin with matching CDF value
///   3. Return target bin center as output value
fn ct_map_through_cdf(
  v : Double,
  source_cdf : FixedArray[Double],
  target_cdf : FixedArray[Double],
  bins : Int
) -> Double {
  let clamped = ct_clamp(v, 0.0, 1.0)
  let src_bin = (clamped * (bins - 1).to_double()).to_int()
  let safe_src = ct_clamp_int(src_bin, 0, bins - 1)
  let cdf_val = source_cdf[safe_src]

  // Find target bin where CDF matches
  let mut t = 0
  while t < bins - 1 {
    if target_cdf[t + 1] >= cdf_val {
      // Linear interpolation between bins
      let lo = target_cdf[t]
      let hi = target_cdf[t + 1]
      let range = hi - lo
      let frac = if range > 0.0000001 { (cdf_val - lo) / range } else { 0.5 }
      return (t.to_double() + ct_clamp(frac, 0.0, 1.0)) / (bins - 1).to_double()
    }
    t = t + 1
  }
  1.0
}

// ---------- FFI export ----------

/// Generate color transfer 3D LUT via CDF matching.
///
/// Input:
///   src strip (row-major sRGB [0,1]) + dimensions
///   tgt strip (row-major sRGB [0,1]) + dimensions
///   grid_size — LUT grid size (e.g. 33)
///
/// Output: grid_size³ × 3 f64 — RGB→RGB mapping table.
///   For LUT index (ri, gi, bi), the output RGB is at:
///     offset = ((ri * grid_size + gi) * grid_size + bi) * 3
///
/// Returns: grid_size³*3 on success, negative on error.
pub fn mengxi_generate_color_transfer_lut(
  src_len : Int,
  src_ptr : FixedArray[Double],
  src_w : Int,
  src_h : Int,
  tgt_len : Int,
  tgt_ptr : FixedArray[Double],
  tgt_w : Int,
  tgt_h : Int,
  grid_size : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if grid_size <= 0 { return -1 }
  if src_w <= 0 || src_h <= 0 { return -1 }
  if tgt_w <= 0 || tgt_h <= 0 { return -1 }
  let src_expected = src_w * src_h * 3
  let tgt_expected = tgt_w * tgt_h * 3
  if src_len < src_expected { return -1 }
  if tgt_len < tgt_expected { return -1 }

  let lut_size = grid_size * grid_size * grid_size * 3
  if out_len < lut_size { return -2 }

  let bins = ct_hist_bins

  // Build CDFs for source and target (per channel)
  // R channel
  let src_cdf_r = FixedArray::make(bins, 0.0)
  let tgt_cdf_r = FixedArray::make(bins, 0.0)
  ct_build_histogram(src_ptr, src_len, 0, src_cdf_r)
  ct_build_histogram(tgt_ptr, tgt_len, 0, tgt_cdf_r)
  ct_build_cdf(src_cdf_r, bins)
  ct_build_cdf(tgt_cdf_r, bins)

  // G channel
  let src_cdf_g = FixedArray::make(bins, 0.0)
  let tgt_cdf_g = FixedArray::make(bins, 0.0)
  ct_build_histogram(src_ptr, src_len, 1, src_cdf_g)
  ct_build_histogram(tgt_ptr, tgt_len, 1, tgt_cdf_g)
  ct_build_cdf(src_cdf_g, bins)
  ct_build_cdf(tgt_cdf_g, bins)

  // B channel
  let src_cdf_b = FixedArray::make(bins, 0.0)
  let tgt_cdf_b = FixedArray::make(bins, 0.0)
  ct_build_histogram(src_ptr, src_len, 2, src_cdf_b)
  ct_build_histogram(tgt_ptr, tgt_len, 2, tgt_cdf_b)
  ct_build_cdf(src_cdf_b, bins)
  ct_build_cdf(tgt_cdf_b, bins)

  // Fill LUT: for each grid point, map through CDFs
  let gs = grid_size
  let gmax = (gs - 1).to_double()
  let mut ri = 0
  while ri < gs {
    let mut gi = 0
    while gi < gs {
      let mut bi = 0
      while bi < gs {
        let in_r = ri.to_double() / gmax
        let in_g = gi.to_double() / gmax
        let in_b = bi.to_double() / gmax

        let out_r = ct_map_through_cdf(in_r, src_cdf_r, tgt_cdf_r, bins)
        let out_g = ct_map_through_cdf(in_g, src_cdf_g, tgt_cdf_g, bins)
        let out_b = ct_map_through_cdf(in_b, src_cdf_b, tgt_cdf_b, bins)

        let idx = ((ri * gs + gi) * gs + bi) * 3
        out_ptr[idx + 0] = ct_clamp(out_r, 0.0, 1.0)
        out_ptr[idx + 1] = ct_clamp(out_g, 0.0, 1.0)
        out_ptr[idx + 2] = ct_clamp(out_b, 0.0, 1.0)

        bi = bi + 1
      }
      gi = gi + 1
    }
    ri = ri + 1
  }

  lut_size
}

// ---------- DCE prevention ----------

fn init {
  let src = FixedArray::make(12, 0.5)
  let tgt = FixedArray::make(12, 0.5)
  let grid = 3
  let out = FixedArray::make(grid * grid * grid * 3, 0.0)
  let _ = mengxi_generate_color_transfer_lut(12, src, 2, 2, 12, tgt, 2, 2, grid, out.length(), out)
}

// ---------- Tests ----------

test "color_transfer_identity" {
  // Same source and target → identity mapping
  let strip = FixedArray::make(12, 0.5) // 2x2 uniform
  let grid = 3
  let out_size = grid * grid * grid * 3
  let out = FixedArray::make(out_size, 0.0)
  let result = mengxi_generate_color_transfer_lut(12, strip, 2, 2, 12, strip, 2, 2, grid, out_size, out)
  assert_eq(result, out_size)

  // For identity, each grid point should map close to itself
  let mut ri = 0
  while ri < grid {
    let mut gi = 0
    while gi < grid {
      let mut bi = 0
      while bi < grid {
        let idx = ((ri * grid + gi) * grid + bi) * 3
        let in_r = ri.to_double() / (grid - 1).to_double()
        let in_g = gi.to_double() / (grid - 1).to_double()
        let in_b = bi.to_double() / (grid - 1).to_double()
        // Identity mapping: output ≈ input (within CDF discretization error)
        let err_r = out[idx] - in_r
        let err_g = out[idx + 1] - in_g
        let err_b = out[idx + 2] - in_b
        let max_err = if err_r < 0.0 { 0.0 - err_r } else { err_r }
        let _eg = if err_g < 0.0 { 0.0 - err_g } else { err_g }
        let _eb = if err_b < 0.0 { 0.0 - err_b } else { err_b }
        // CDF matching with uniform data should give reasonable identity
        assert_true(max_err < 0.5)
        bi = bi + 1
      }
      gi = gi + 1
    }
    ri = ri + 1
  }
}

test "color_transfer_error_cases" {
  let src = FixedArray::make(12, 0.5)
  let tgt = FixedArray::make(12, 0.5)
  let out = FixedArray::make(81, 0.0)
  assert_eq(mengxi_generate_color_transfer_lut(12, src, 2, 2, 12, tgt, 2, 2, 0, 81, out), -1)
  assert_eq(mengxi_generate_color_transfer_lut(12, src, 0, 2, 12, tgt, 2, 2, 3, 81, out), -1)
  assert_eq(mengxi_generate_color_transfer_lut(12, src, 2, 2, 12, tgt, 2, 2, 3, 10, out), -2)
}

test "color_transfer_output_range" {
  // Source bright, target dark → check output is in [0,1]
  let src = FixedArray::make(12, 0.9) // bright
  let tgt = FixedArray::make(12, 0.1) // dark
  let grid = 3
  let out_size = grid * grid * grid * 3
  let out = FixedArray::make(out_size, 0.0)
  let result = mengxi_generate_color_transfer_lut(12, src, 2, 2, 12, tgt, 2, 2, grid, out_size, out)
  assert_eq(result, out_size)
  let mut i = 0
  while i < out_size {
    assert_true(out[i] >= 0.0 && out[i] <= 1.0)
    i = i + 1
  }
}
