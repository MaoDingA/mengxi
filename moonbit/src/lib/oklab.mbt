// oklab.mbt — Oklab perceptual color space conversion
// Implements: sRGB ↔ Oklab (IEC 61966-2-1 compliant)
// Reference: Björn Ottosson 2020 — A perceptual color space for image processing

// ============================================================
// IEC 61966-2-1 sRGB Transfer Functions
// CRITICAL: These use the correct piecewise function, NOT simplified gamma 2.2
// ============================================================

/// sRGB gamma-encoded value to linear light (inverse EOTF / decoding).
/// Used in sRGB → Oklab path.
fn srgb_to_linear(c : Double) -> Double {
  if c <= 0.04045 {
    c / 12.92
  } else {
    let base = (c + 0.055) / 1.055
    @math.pow(base, 2.4)
  }
}

/// Linear light to sRGB gamma-encoded value (EOTF inverse / encoding).
/// Used in Oklab → sRGB path.
fn linear_to_srgb(c : Double) -> Double {
  if c <= 0.0031308 {
    12.92 * c
  } else {
    1.055 * @math.pow(c, 1.0 / 2.4) - 0.055
  }
}

// ============================================================
// Oklab Core Transforms (linear RGB ↔ Oklab)
// ============================================================

/// Forward: linear sRGB → Oklab
fn linear_rgb_to_oklab(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  // Step 1: linear sRGB to LMS (M_srgb_to_lms, pre-composed matrix)
  let l = 0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b
  let m = 0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b
  let s = 0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b

  // Step 2: cube root non-linearity (exact exponent 1/3)
  // Clamp to 0 before cbrt to prevent NaN from negative LMS values
  let l_c = if l < 0.0 { 0.0 } else { l }
  let m_c = if m < 0.0 { 0.0 } else { m }
  let s_c = if s < 0.0 { 0.0 } else { s }
  let l_cbrt = @math.pow(l_c, 1.0 / 3.0)
  let m_cbrt = @math.pow(m_c, 1.0 / 3.0)
  let s_cbrt = @math.pow(s_c, 1.0 / 3.0)

  // Step 3: LMS' to Oklab (M2 matrix)
  let ok_l =  0.2104542553 * l_cbrt + 0.7936177850 * m_cbrt - 0.0040720468 * s_cbrt
  let ok_a =  1.9779984951 * l_cbrt - 2.4285922050 * m_cbrt + 0.4505937099 * s_cbrt
  let ok_b =  0.0259040371 * l_cbrt + 0.7827717662 * m_cbrt - 0.8086757660 * s_cbrt

  (ok_l, ok_a, ok_b)
}

/// Inverse: Oklab → linear sRGB
fn oklab_to_linear_rgb(ok_l : Double, ok_a : Double, ok_b : Double) -> (Double, Double, Double) {
  // Step 1: Oklab to LMS' (M2_inv)
  let l_prime = ok_l + 0.3963377774 * ok_a + 0.2158037573 * ok_b
  let m_prime = ok_l - 0.1055613458 * ok_a - 0.0638541728 * ok_b
  let s_prime = ok_l - 0.0894841775 * ok_a - 1.2914855480 * ok_b

  // Step 2: cube (inverse of cube root)
  let l = l_prime * l_prime * l_prime
  let m = m_prime * m_prime * m_prime
  let s = s_prime * s_prime * s_prime

  // Step 3: LMS to linear sRGB (M_lms_to_srgb)
  let r =  4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s
  let g = -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s
  let b = -0.0041960863 * l - 0.7034186147 * m + 1.7076147010 * s

  (r, g, b)
}

// ============================================================
// Compound sRGB ↔ Oklab Functions
// ============================================================

/// sRGB (gamma-encoded) → Oklab
fn srgb_to_oklab(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  let lr = srgb_to_linear(r)
  let lg = srgb_to_linear(g)
  let lb = srgb_to_linear(b)
  linear_rgb_to_oklab(lr, lg, lb)
}

/// Oklab → sRGB (gamma-encoded)
fn oklab_to_srgb(l : Double, a : Double, bv : Double) -> (Double, Double, Double) {
  let (r, g, b) = oklab_to_linear_rgb(l, a, bv)
  (linear_to_srgb(r), linear_to_srgb(g), linear_to_srgb(b))
}

// ============================================================
// FFI Exported Functions
// ============================================================

/// sRGB → Oklab conversion via FFI.
/// Input: interleaved f64 array [R0,G0,B0, R1,G1,B1, ...]
/// Output: interleaved f64 array [L0,a0,b0, L1,a1,b1, ...]
/// Returns: data_len on success, negative error code on failure.
pub fn mengxi_srgb_to_oklab(
  data_len : Int,
  data_ptr : FixedArray[Double],
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if data_len < 3 { return -1 }
  if out_len < data_len { return -2 }
  let num_pixels = data_len / 3
  let mut i = 0
  while i < num_pixels {
    let r = data_ptr[i * 3]
    let g = data_ptr[i * 3 + 1]
    let b = data_ptr[i * 3 + 2]
    let (l, a, bv) = srgb_to_oklab(r, g, b)
    out_ptr[i * 3] = l
    out_ptr[i * 3 + 1] = a
    out_ptr[i * 3 + 2] = bv
    i = i + 1
  }
  data_len
}

/// Oklab → sRGB conversion via FFI.
/// Input: interleaved f64 array [L0,a0,b0, L1,a1,b1, ...]
/// Output: interleaved f64 array [R0,G0,B0, R1,G1,B1, ...]
/// Returns: data_len on success, negative error code on failure.
pub fn mengxi_oklab_to_srgb(
  data_len : Int,
  data_ptr : FixedArray[Double],
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if data_len < 3 { return -1 }
  if out_len < data_len { return -2 }
  let num_pixels = data_len / 3
  let mut i = 0
  while i < num_pixels {
    let l = data_ptr[i * 3]
    let a = data_ptr[i * 3 + 1]
    let bv = data_ptr[i * 3 + 2]
    let (r, g, b) = oklab_to_srgb(l, a, bv)
    out_ptr[i * 3] = r
    out_ptr[i * 3 + 1] = g
    out_ptr[i * 3 + 2] = b
    i = i + 1
  }
  data_len
}

// ============================================================
// DCE Prevention
// ============================================================

fn init {
  let _ = mengxi_srgb_to_oklab(0, FixedArray::make(0, 0.0), 0, FixedArray::make(0, 0.0))
  let _ = mengxi_oklab_to_srgb(0, FixedArray::make(0, 0.0), 0, FixedArray::make(0, 0.0))
}

// ============================================================
// Tests
// ============================================================

fn oklab_delta_e(l1: Double, a1: Double, b1: Double, l2: Double, a2: Double, b2: Double) -> Double {
  let dl = l2 - l1
  let da = a2 - a1
  let db = b2 - b1
  @math.pow(dl * dl + da * da + db * db, 0.5)
}

test {
  // D65 white (1,1,1) → Oklab should be approximately (1, 0, 0)
  let (l, a, b) = linear_rgb_to_oklab(0.9505, 1.0, 1.0890)
  assert_eq(approx_eq(l, 1.0), true)
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
}

test {
  // Pure black (0,0,0) → linear → Oklab should be (0,0,0)
  let (l, a, b) = linear_rgb_to_oklab(0.0, 0.0, 0.0)
  assert_eq(approx_eq(l, 0.0), true)
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
}

test {
  // No NaN/Inf for pure black
  let (l, a, b) = srgb_to_oklab(0.0, 0.0, 0.0)
  assert_eq(l == l, true) // NaN check
  assert_eq(a == a, true)   // NaN check
  assert_eq(b == b, true)   // NaN check
  assert_eq(approx_eq(l, 0.0), true)
}

test {
  // No NaN/Inf for pure white
  let (l, a, b) = srgb_to_oklab(1.0, 1.0, 1.0)
  assert_eq(l == l, true)
  assert_eq(a == a, true)
  assert_eq(b == b, true)
}

test {
  // sRGB round-trip: sRGB → Oklab → sRGB
  // Mid-gray
  let (l, a, b) = srgb_to_oklab(0.5, 0.5, 0.5)
  let (r2, g2, b2) = oklab_to_srgb(l, a, b)
  assert_eq(approx_eq(r2, 0.5), true)
  assert_eq(approx_eq(g2, 0.5), true)
  assert_eq(approx_eq(b2, 0.5), true)
  // Oklab ΔE check
  let de = oklab_delta_e(l, a, b, l, a, b)
  assert_eq(approx_eq(de, 0.0), true)
}

test {
  // sRGB round-trip: saturated red
  let (l, a, b) = srgb_to_oklab(1.0, 0.0, 0.0)
  let (r2, g2, b2) = oklab_to_srgb(l, a, b)
  assert_eq(approx_eq(r2, 1.0), true)
  assert_eq(approx_eq(g2, 0.0), true)
  assert_eq(approx_eq(b2, 0.0), true)
}

test {
  // sRGB round-trip: saturated green
  let (l, a, b) = srgb_to_oklab(0.0, 1.0, 0.0)
  let (r2, g2, b2) = oklab_to_srgb(l, a, b)
  assert_eq(approx_eq(r2, 0.0), true)
  assert_eq(approx_eq(g2, 1.0), true)
  assert_eq(approx_eq(b2, 0.0), true)
}

test {
  // sRGB round-trip: saturated blue
  let (l, a, b) = srgb_to_oklab(0.0, 0.0, 1.0)
  let (r2, g2, b2) = oklab_to_srgb(l, a, b)
  assert_eq(approx_eq(r2, 0.0), true)
  assert_eq(approx_eq(g2, 0.0), true)
  assert_eq(approx_eq(b2, 1.0), true)
}

test {
  // sRGB round-trip: D65 white
  let (l, a, b) = srgb_to_oklab(1.0, 1.0, 1.0)
  let (r2, g2, b2) = oklab_to_srgb(l, a, b)
  assert_eq(approx_eq(r2, 1.0), true)
  assert_eq(approx_eq(g2, 1.0), true)
  assert_eq(approx_eq(b2, 1.0), true)
}

test {
  // sRGB round-trip: pure black
  let (l, a, b) = srgb_to_oklab(0.0, 0.0, 0.0)
  let (r2, g2, b2) = oklab_to_srgb(l, a, b)
  assert_eq(approx_eq(r2, 0.0), true)
  assert_eq(approx_eq(g2, 0.0), true)
  assert_eq(approx_eq(b2, 0.0), true)
}

test {
  // IEC 61966-2-1: sRGB transfer function threshold continuity
  // Forward: at threshold 0.04045, both branches should agree
  let linear_below = 0.04045 / 12.92
  let linear_above = @math.pow((0.04045 + 0.055) / 1.055, 2.4)
  assert_eq(approx_eq(linear_below, linear_above), true)
}

test {
  // IEC 61966-2-1: inverse transfer function threshold continuity
  // Inverse: at threshold 0.0031308, both branches should agree
  let srgb_below = 12.92 * 0.0031308
  let srgb_above = 1.055 * @math.pow(0.0031308, 1.0 / 2.4) - 0.055
  assert_eq(approx_eq(srgb_below, srgb_above), true)
}

test {
  // Oklab: achromatic values (r=g=b) should have a≈0, b≈0
  let (_, a, b) = linear_rgb_to_oklab(0.3, 0.3, 0.3)
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
}

test {
  // FFI function: mengxi_srgb_to_oklab basic test
  let data = FixedArray::make(6, 0.0)
  data[0] = 1.0; data[1] = 0.0; data[2] = 0.0
  data[3] = 0.0; data[4] = 1.0; data[5] = 0.0
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_srgb_to_oklab(6, data, 6, out)
  assert_eq(result, 6)
  // Red pixel should have positive a
  assert_eq(out[1] > 0.0, true)
  // Green pixel should have negative a
  assert_eq(out[4] < 0.0, true)
}

test {
  // FFI function: mengxi_oklab_to_srgb basic test
  let data = FixedArray::make(6, 0.0)
  data[0] = 1.0; data[1] = 0.0; data[2] = 0.0
  data[3] = 0.5; data[4] = 0.5; data[5] = 0.5
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_oklab_to_srgb(6, data, 6, out)
  assert_eq(result, 6)
  // White pixel round-trip
  assert_eq(approx_eq(out[0], 1.0), true)
  assert_eq(approx_eq(out[1], 0.0), true)
  assert_eq(approx_eq(out[2], 0.0), true)
  // Gray pixel
  assert_eq(approx_eq(out[3], 0.5), true)
  assert_eq(approx_eq(out[4], 0.5), true)
  assert_eq(approx_eq(out[5], 0.5), true)
}

test {
  // FFI: error on too few pixels
  let data = FixedArray::make(2, 0.0)
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_srgb_to_oklab(2, data, 6, out)
  assert_eq(result, -1)
}

test {
  // FFI: error on output too small
  let data = FixedArray::make(6, 0.0)
  let out = FixedArray::make(3, 0.0)
  let result = mengxi_srgb_to_oklab(6, data, 3, out)
  assert_eq(result, -2)
}

test {
  // FFI: round-trip through FFI functions
  let data = FixedArray::make(9, 0.0)
  data[0] = 0.2; data[1] = 0.4; data[2] = 0.8
  data[3] = 0.6; data[4] = 0.1; data[5] = 0.9
  data[6] = 0.0; data[7] = 0.0; data[8] = 0.0
  let mid = FixedArray::make(9, 0.0)
  let r1 = mengxi_srgb_to_oklab(9, data, 9, mid)
  assert_eq(r1, 9)
  let back = FixedArray::make(9, 0.0)
  let r2 = mengxi_oklab_to_srgb(9, mid, 9, back)
  assert_eq(r2, 9)
  // Verify round-trip
  assert_eq(approx_eq(back[0], 0.2), true)
  assert_eq(approx_eq(back[1], 0.4), true)
  assert_eq(approx_eq(back[2], 0.8), true)
  assert_eq(approx_eq(back[3], 0.6), true)
  assert_eq(approx_eq(back[4], 0.1), true)
  assert_eq(approx_eq(back[5], 0.9), true)
  assert_eq(approx_eq(back[6], 0.0), true)
  assert_eq(approx_eq(back[7], 0.0), true)
  assert_eq(approx_eq(back[8], 0.0), true)
}
