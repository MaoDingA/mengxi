// lut_engine.mbt — 3D LUT generation via ACES color space transforms (FR15, FR18)
// Pure functions only — no I/O, no stateful dependencies.
//
// Generates a 3D LUT by sampling ACES color space transforms across a uniform grid.
// The output is a flat array of grid_size^3 * 3 RGB values in red-fastest order:
//   index = r + grid_size * g + grid_size^2 * b

// ---------- LUT generation ----------

/// Generate a 3D LUT by applying an ACES color space transform across a uniform grid.
///
/// Parameters:
///   grid_size — number of samples per axis (e.g., 17, 33)
///   src_cs    — source color space: 10=ACES2065-1, 11=ACEScg, 20=Rec709
///   dst_cs    — destination color space: 10=ACES2065-1, 11=ACEScg, 20=Rec709
///   out_ptr   — output buffer (must hold at least grid_size^3 * 3 doubles)
///   out_len   — length of output buffer
///
/// Returns: number of values written (grid_size^3 * 3), or negative error code.
///   -1 = invalid grid_size (< 2 or > 256)
///   -2 = output buffer too small
///   -3 = log-encoded source (not supported for LUT generation)
///   -4 = invalid color space code
pub fn mengxi_generate_lut(
  grid_size : Int,
  src_cs : Int,
  dst_cs : Int,
  out_ptr : FixedArray[Double],
  out_len : Int
) -> Int {
  // Validate grid_size
  if grid_size < 2 { return -1 }
  if grid_size > 256 { return -1 }

  // Reject log-encoded sources
  if is_log_space(src_cs) { return -3 }

  // Reject invalid color space codes
  if !is_valid_cs(src_cs) { return -4 }
  if !is_valid_cs(dst_cs) { return -4 }

  // Calculate total values needed
  let total = grid_size * grid_size * grid_size * 3
  if out_len < total { return -2 }
  if out_ptr.length() < total { return -2 }

  // Generate LUT by iterating over the 3D grid
  let n = grid_size
  let n_minus_1 = n - 1
  // Use Double for division to avoid integer truncation
  let inv_n = 1.0 / (n_minus_1.to_double())

  let mut idx = 0
  while idx < n {
    let b_f = (idx.to_double()) * inv_n
    let mut g_idx = 0
    while g_idx < n {
      let g_f = (g_idx.to_double()) * inv_n
      let mut r_idx = 0
      while r_idx < n {
        let r_f = (r_idx.to_double()) * inv_n

        // Apply ACES transform
        let (or, og, ob) = apply_transform(r_f, g_f, b_f, src_cs, dst_cs)

        let i = idx * n * n * 3 + g_idx * n * 3 + r_idx * 3
        if i < out_ptr.length() { out_ptr[i] = or }
        let i1 = i + 1
        if i1 < out_ptr.length() { out_ptr[i1] = og }
        let i2 = i + 2
        if i2 < out_ptr.length() { out_ptr[i2] = ob }

        r_idx = r_idx + 1
      }
      g_idx = g_idx + 1
    }
    idx = idx + 1
  }

  total
}

// ---------- Helpers ----------

fn is_valid_cs(cs : Int) -> Bool {
  cs == cs_aces2065_1() || cs == cs_acescg() || cs == cs_acescct() || cs == cs_rec709()
}

// ---------- DCE prevention ----------

fn init {
  // mengxi_generate_lut is referenced via FFI exports in moon.pkg.json
  // but we also reference the transform function to prevent tree-shaking
  let _ = apply_transform(0.0, 0.0, 0.0, 0, 0)
}

// ---------- Unit tests ----------

test {
  // LUT size 2: total output should be 2^3 * 3 = 24
  let out = FixedArray::make(24, 0.0)
  let result = mengxi_generate_lut(2, cs_acescg(), cs_rec709(), out, 24)
  assert_eq(result, 24)
}

test {
  // LUT size 3: total output should be 27 * 3 = 81
  let out = FixedArray::make(81, 0.0)
  let result = mengxi_generate_lut(3, cs_acescg(), cs_rec709(), out, 81)
  assert_eq(result, 81)
}

test {
  // LUT size 33: total output should be 35937 * 3 = 107811
  let total = 33 * 33 * 33 * 3
  let out = FixedArray::make(total, 0.0)
  let result = mengxi_generate_lut(33, cs_acescg(), cs_rec709(), out, total)
  assert_eq(result, total)
}

test {
  // Identity LUT: ACEScg → ACEScg should pass through unchanged
  let out = FixedArray::make(24, 0.0)
  let result = mengxi_generate_lut(2, cs_acescg(), cs_acescg(), out, 24)
  assert_eq(result, 24)
  // (0,0,0) → (0,0,0)
  assert_eq(approx_eq(out[0], 0.0), true)
  assert_eq(approx_eq(out[1], 0.0), true)
  assert_eq(approx_eq(out[2], 0.0), true)
  // (1,0,0) → (1,0,0)
  assert_eq(approx_eq(out[3], 1.0), true)
  assert_eq(approx_eq(out[4], 0.0), true)
  assert_eq(approx_eq(out[5], 0.0), true)
  // (0,1,0) → (0,1,0)
  assert_eq(approx_eq(out[6], 0.0), true)
  assert_eq(approx_eq(out[7], 1.0), true)
  assert_eq(approx_eq(out[8], 0.0), true)
  // (1,1,1) → (1,1,1)
  assert_eq(approx_eq(out[21], 1.0), true)
  assert_eq(approx_eq(out[22], 1.0), true)
  assert_eq(approx_eq(out[23], 1.0), true)
}

test {
  // Black value: ACEScg(0,0,0) → Rec.709 should be (0,0,0)
  let out = FixedArray::make(24, 0.0)
  let _ = mengxi_generate_lut(2, cs_acescg(), cs_rec709(), out, 24)
  assert_eq(approx_eq(out[0], 0.0), true)
  assert_eq(approx_eq(out[1], 0.0), true)
  assert_eq(approx_eq(out[2], 0.0), true)
}

test {
  // Grid size 0 rejected
  let out = FixedArray::make(24, 0.0)
  let result = mengxi_generate_lut(0, cs_acescg(), cs_rec709(), out, 24)
  assert_eq(result, -1)
}

test {
  // Grid size 1 rejected
  let out = FixedArray::make(3, 0.0)
  let result = mengxi_generate_lut(1, cs_acescg(), cs_rec709(), out, 3)
  assert_eq(result, -1)
}

test {
  // Grid size 257 rejected
  let out = FixedArray::make(3, 0.0)
  let result = mengxi_generate_lut(257, cs_acescg(), cs_rec709(), out, 3)
  assert_eq(result, -1)
}

test {
  // Output buffer too small
  let out = FixedArray::make(3, 0.0)
  let result = mengxi_generate_lut(2, cs_acescg(), cs_rec709(), out, 3)
  assert_eq(result, -2)
}

test {
  // Log-encoded source rejected
  let out = FixedArray::make(24, 0.0)
  let result = mengxi_generate_lut(2, cs_acescct(), cs_rec709(), out, 24)
  assert_eq(result, -3)
}

test {
  // Invalid source color space code rejected
  let out = FixedArray::make(24, 0.0)
  let result = mengxi_generate_lut(2, 99, cs_rec709(), out, 24)
  assert_eq(result, -4)
}

test {
  // Invalid destination color space code rejected
  let out = FixedArray::make(24, 0.0)
  let result = mengxi_generate_lut(2, cs_acescg(), 99, out, 24)
  assert_eq(result, -4)
}

test {
  // ACEScg → Rec.709 LUT: grey point (0.5, 0.5, 0.5) should produce a valid Rec.709 value
  // Grid size 3: index 1 normalizes to 0.5
  let total = 27 * 3
  let out = FixedArray::make(total, 0.0)
  let _ = mengxi_generate_lut(3, cs_acescg(), cs_rec709(), out, total)
  // Red-fastest: index = r + n*g + n*n*b
  // For (1,1,1): idx = 1 + 3*1 + 3*3*1 = 13
  let grey_idx = 1 + 3 * 1 + 3 * 3 * 1
  let r = out[grey_idx * 3]
  let g = out[grey_idx * 3 + 1]
  let b = out[grey_idx * 3 + 2]
  // Grey should map to a consistent value (same for R, G, B)
  assert_eq(approx_eq(r, g), true)
  assert_eq(approx_eq(g, b), true)
}
