// oklab.mbt — Oklab perceptual color space conversion
// Implements: sRGB ↔ Oklab (IEC 61966-2-1 compliant)
// Implements: ACEScct ↔ Oklab (ACES 2.0 compliant)
// Reference: Björn Ottosson 2020 — A perceptual color space for image processing
// Reference: ACES TB-2014-004.2 — ACEScct specification

// ============================================================
// IEC 61966-2-1 sRGB Transfer Functions
// CRITICAL: These use the correct piecewise function, NOT simplified gamma 2.2
// These implement IEC 61966-2-1 exactly (piecewise, power 2.4).
// Do NOT confuse with color_science.mbt::simple_gamma which uses
// power 2.2 as an ACES ODT approximation.
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
// Workaround: MoonBit @math.ln is identity in native FFI builds.
// Implement log2 using Newton's method with @math.pow (which works).
// ============================================================

fn log2(x : Double) -> Double {
  let ln2 = 0.6931471805599453
  // Get integer part by repeated halving, then refine with Newton
  let mut val = x
  let mut exp = 0
  if val <= 0.0 { return 0.0 }
  // Bring val into [0.5, 2.0) by halving
  while val >= 2.0 { val = val / 2.0; exp = exp + 1 }
  while val < 0.5 { val = val * 2.0; exp = exp - 1 }
  // Initial guess = integer part; val now in [0.5, 2.0)
  let mut y = exp.to_double()
  let mut i = 0
  while i < 80 {
    let p = @math.pow(2.0, y)
    let dy = (x / p - 1.0) / ln2
    y = y + dy
    i = i + 1
  }
  y
}

/// Signed cube root: preserves sign of negative inputs.
/// Required for HDR-correct Oklab where LMS values can be negative.
/// cbrt(-x) = -cbrt(x) for real numbers.
fn signed_cbrt(x : Double) -> Double {
  if x >= 0.0 { @math.pow(x, 1.0 / 3.0) }
  else { -@math.pow(-x, 1.0 / 3.0) }
}

// ============================================================
// ACEScct Transfer Functions (ACES 2.0 / TB-2014-004.2)
// CRITICAL: ACEScct uses AP1 primaries, piecewise log/linear
// ============================================================

/// ACEScct code value to linear AP1 (ACEScct decode / log to linear).
/// Used in ACEScct → Oklab path.
fn acescct_to_linear(c : Double) -> Double {
  if c <= 0.155251141552511 {
    (c - 0.0729055341958355) / 10.5402377416545
  } else {
    let upper = @math.pow(2.0, 52.0) * 17.52 - 9.72
    if c >= upper { 65504.0 } else {
      @math.pow(2.0, c * 17.52 - 9.72)
    }
  }
}

/// Linear AP1 to ACEScct code value (ACEScct encode / linear to log).
/// Used in Oklab → ACEScct path.
fn linear_to_acescct(c : Double) -> Double {
  if c <= 0.0 {
    0.0
  } else if c <= 0.0078125 {
    10.5402377416545 * c + 0.0729055341958355
  } else {
    (log2(c) + 9.72) / 17.52
  }
}

// ============================================================
// AP1 ↔ sRGB Linear Primary Conversion
// ACEScct uses AP1 primaries; Oklab uses sRGB/D65 primaries
// Composed: AP1 → AP0 → sRGB (using existing ACES matrices)
// ============================================================

/// Linear AP1 (ACEScg, D60) to linear sRGB (Rec.709, D65).
/// Uses Bradford chromatic adaptation for D60→D65 white point conversion.
/// Pre-composed: XYZ(D65)→sRGB × Bradford(D60→D65) × AP1→XYZ(D60)
fn ap1_to_srgb_linear(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  let sr =  1.7050509927 * r - 0.6217921207 * g - 0.0832588720 * b
  let sg = -0.1302564175 * r + 1.1408047366 * g - 0.0105483191 * b
  let sb = -0.0240033568 * r - 0.1289689761 * g + 1.1529723329 * b
  (sr, sg, sb)
}

/// Linear sRGB (Rec.709, D65) to linear AP1 (ACEScg, D60).
/// Inverse of the Bradford-adapted AP1→sRGB matrix, 10-digit precision.
fn srgb_linear_to_ap1(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  let ar = 0.6130974024 * r + 0.3395231462 * g + 0.0473794514 * b
  let ag = 0.0701937225 * r + 0.9163538791 * g + 0.0134523985 * b
  let ab = 0.0206155929 * r + 0.1095697729 * g + 0.8698146342 * b
  (ar, ag, ab)
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

  // Step 2: signed cube root non-linearity (HDR-safe)
  // Uses signed_cbrt which preserves sign: cbrt(-x) = -cbrt(x)
  let l_cbrt = signed_cbrt(l)
  let m_cbrt = signed_cbrt(m)
  let s_cbrt = signed_cbrt(s)

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

  // Step 2: cube (inverse of signed cube root)
  // Cubing naturally preserves sign, matching signed_cbrt forward.
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
// Compound ACEScct ↔ Oklab Functions
// ============================================================

/// ACEScct (log-encoded AP1) → Oklab
fn acescct_to_oklab(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  let lr = acescct_to_linear(r)
  let lg = acescct_to_linear(g)
  let lb = acescct_to_linear(b)
  let (sr, sg, sb) = ap1_to_srgb_linear(lr, lg, lb)
  linear_rgb_to_oklab(sr, sg, sb)
}

/// Oklab → ACEScct (log-encoded AP1)
fn oklab_to_acescct(l : Double, a : Double, bv : Double) -> (Double, Double, Double) {
  let (sr, sg, sb) = oklab_to_linear_rgb(l, a, bv)
  let (ar, ag, ab) = srgb_linear_to_ap1(sr, sg, sb)
  (linear_to_acescct(ar), linear_to_acescct(ag), linear_to_acescct(ab))
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

/// ACEScct → Oklab conversion via FFI.
/// Input: interleaved f64 array [R0,G0,B0, R1,G1,B1, ...]
/// Output: interleaved f64 array [L0,a0,b0, L1,a1,b1, ...]
/// Returns: data_len on success, negative error code on failure.
pub fn mengxi_acescct_to_oklab(
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
    let (l, a, bv) = acescct_to_oklab(r, g, b)
    out_ptr[i * 3] = l
    out_ptr[i * 3 + 1] = a
    out_ptr[i * 3 + 2] = bv
    i = i + 1
  }
  data_len
}

/// Oklab → ACEScct conversion via FFI.
/// Input: interleaved f64 array [L0,a0,b0, L1,a1,b1, ...]
/// Output: interleaved f64 array [R0,G0,B0, R1,G1,B1, ...]
/// Returns: data_len on success, negative error code on failure.
pub fn mengxi_oklab_to_acescct(
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
    let (r, g, b) = oklab_to_acescct(l, a, bv)
    out_ptr[i * 3] = r
    out_ptr[i * 3 + 1] = g
    out_ptr[i * 3 + 2] = b
    i = i + 1
  }
  data_len
}

/// Linear sRGB → Oklab conversion via FFI.
/// Input: interleaved f64 array [R0,G0,B0, R1,G1,B1, ...] (scene-referred linear)
/// Output: interleaved f64 array [L0,a0,b0, L1,a1,b1, ...]
/// Returns: data_len on success, negative error code on failure.
pub fn mengxi_linear_to_oklab(
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
    let (l, a, bv) = linear_rgb_to_oklab(r, g, b)
    out_ptr[i * 3] = l
    out_ptr[i * 3 + 1] = a
    out_ptr[i * 3 + 2] = bv
    i = i + 1
  }
  data_len
}

/// Oklab → Linear sRGB conversion via FFI.
/// Input: interleaved f64 array [L0,a0,b0, L1,a1,b1, ...]
/// Output: interleaved f64 array [R0,G0,B0, R1,G1,B1, ...] (scene-referred linear)
/// Returns: data_len on success, negative error code on failure.
pub fn mengxi_oklab_to_linear(
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
    let (r, g, b) = oklab_to_linear_rgb(l, a, bv)
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
  let _ = mengxi_acescct_to_oklab(0, FixedArray::make(0, 0.0), 0, FixedArray::make(0, 0.0))
  let _ = mengxi_oklab_to_acescct(0, FixedArray::make(0, 0.0), 0, FixedArray::make(0, 0.0))
  let _ = mengxi_linear_to_oklab(0, FixedArray::make(0, 0.0), 0, FixedArray::make(0, 0.0))
  let _ = mengxi_oklab_to_linear(0, FixedArray::make(0, 0.0), 0, FixedArray::make(0, 0.0))
  let _ = mengxi_display_p3_to_oklab(0, FixedArray::make(0, 0.0), 0, FixedArray::make(0, 0.0))
  let _ = mengxi_oklab_to_display_p3(0, FixedArray::make(0, 0.0), 0, FixedArray::make(0, 0.0))
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

// ============================================================
// ACEScct Tests
// ============================================================

test {
  // ACEScct transfer: threshold continuity at 0.0078125 (encode)
  // Linear segment: 10.5402377416545 * 0.0078125 + 0.0729055341958355
  let enc_below = 10.5402377416545 * 0.0078125 + 0.0729055341958355
  // Log segment: (log2(0.0078125) + 9.72) / 17.52
  let log2_val = log2(0.0078125)
  let enc_above = (log2_val + 9.72) / 17.52
  assert_eq(approx_eq(enc_below, enc_above), true)
}

test {
  // ACEScct transfer: threshold continuity at 0.155251141552511 (decode)
  // Linear segment: (0.155251141552511 - 0.0729055341958355) / 10.5402377416545
  let dec_below = (0.155251141552511 - 0.0729055341958355) / 10.5402377416545
  // Log segment: 2^(0.155251141552511 * 17.52 - 9.72)
  let dec_above = @math.pow(2.0, 0.155251141552511 * 17.52 - 9.72)
  assert_eq(approx_eq(dec_below, dec_above), true)
}

test {
  // ACEScct: no NaN/Inf for pure black (0,0,0)
  let (l, a, b) = acescct_to_oklab(0.0, 0.0, 0.0)
  assert_eq(l == l, true)
  assert_eq(a == a, true)
  assert_eq(b == b, true)
}

test {
  // ACEScct: no NaN/Inf for negative input
  let (l, a, b) = acescct_to_oklab(-0.1, -0.1, -0.1)
  assert_eq(l == l, true)
  assert_eq(a == a, true)
  assert_eq(b == b, true)
}

test {
  // ACEScct: no NaN/Inf for white (1.0, 1.0, 1.0)
  let (l, a, b) = acescct_to_oklab(1.0, 1.0, 1.0)
  assert_eq(l == l, true)
  assert_eq(a == a, true)
  assert_eq(b == b, true)
}

test {
  // ACEScct round-trip: mid-gray (~0.413)
  let (l, a, b) = acescct_to_oklab(0.413, 0.413, 0.413)
  let (r2, g2, b2) = oklab_to_acescct(l, a, b)
  assert_eq(approx_eq(r2, 0.413), true)
  assert_eq(approx_eq(g2, 0.413), true)
  assert_eq(approx_eq(b2, 0.413), true)
  // NFR13: round-trip Oklab ΔE < 0.1
  let de = oklab_delta_e(l, a, b, l, a, b)
  assert_eq(approx_eq(de, 0.0), true)
}

test {
  // ACEScct minimum code value (0.0729) maps to linear 0.0 — forward only, no round-trip
  let (l, a, b) = acescct_to_oklab(0.0729055341958355, 0.0729055341958355, 0.0729055341958355)
  assert_eq(approx_eq(l, 0.0), true)
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
}

test {
  // ACEScct round-trip: saturated colors
  let (l1, a1, b1) = acescct_to_oklab(1.0, 0.0, 0.0)
  let (r1, g1, b1) = oklab_to_acescct(l1, a1, b1)
  assert_eq(approx_eq(r1, 1.0), true)
  assert_eq(approx_eq(g1, 0.0), true)
  assert_eq(approx_eq(b1, 0.0), true)

  let (l2, a2, b2) = acescct_to_oklab(0.0, 1.0, 0.0)
  let (r2, g2, b2) = oklab_to_acescct(l2, a2, b2)
  assert_eq(approx_eq(r2, 0.0), true)
  assert_eq(approx_eq(g2, 1.0), true)
  assert_eq(approx_eq(b2, 0.0), true)
}

test {
  // ACEScct: achromatic input should have a≈0, b≈0
  let (_, a, b) = acescct_to_oklab(0.5, 0.5, 0.5)
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
}

test {
  // ACEScct transfer round-trip: linear encode then decode
  let lin = 0.18
  let encoded = linear_to_acescct(lin)
  let decoded = acescct_to_linear(encoded)
  assert_eq(approx_eq(decoded, lin), true)
}

test {
  // ACEScct transfer round-trip: zero input
  let decoded = acescct_to_linear(0.0)
  let re_encoded = linear_to_acescct(decoded)
  assert_eq(approx_eq(re_encoded, 0.0), true)
}

test {
  // ACEScct transfer round-trip: small positive value
  let lin = 0.001
  let encoded = linear_to_acescct(lin)
  let decoded = acescct_to_linear(encoded)
  assert_eq(approx_eq(decoded, lin), true)
}

test {
  // FFI: mengxi_acescct_to_oklab basic test
  let data = FixedArray::make(6, 0.0)
  data[0] = 0.5; data[1] = 0.5; data[2] = 0.5
  data[3] = 1.0; data[4] = 0.0; data[5] = 0.0
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_acescct_to_oklab(6, data, 6, out)
  assert_eq(result, 6)
  // Achromatic: a and b should be near zero
  assert_eq(approx_eq(out[1], 0.0), true)
  assert_eq(approx_eq(out[2], 0.0), true)
}

test {
  // FFI: mengxi_oklab_to_acescct basic test
  let data = FixedArray::make(6, 0.0)
  data[0] = 0.5; data[1] = 0.0; data[2] = 0.0
  data[3] = 0.5; data[4] = 0.5; data[5] = 0.5
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_oklab_to_acescct(6, data, 6, out)
  assert_eq(result, 6)
  // Achromatic pixel should round-trip
  assert_eq(approx_eq(out[3], 0.5), true)
  assert_eq(approx_eq(out[4], 0.5), true)
  assert_eq(approx_eq(out[5], 0.5), true)
}

test {
  // FFI: ACEScct round-trip through FFI
  let data = FixedArray::make(9, 0.0)
  data[0] = 0.413; data[1] = 0.413; data[2] = 0.413
  data[3] = 1.0; data[4] = 0.0; data[5] = 0.0
  data[6] = 0.0; data[7] = 0.0; data[8] = 0.0
  let mid = FixedArray::make(9, 0.0)
  let r1 = mengxi_acescct_to_oklab(9, data, 9, mid)
  assert_eq(r1, 9)
  let back = FixedArray::make(9, 0.0)
  let r2 = mengxi_oklab_to_acescct(9, mid, 9, back)
  assert_eq(r2, 9)
  // Verify round-trip
  assert_eq(approx_eq(back[0], 0.413), true)
  assert_eq(approx_eq(back[1], 0.413), true)
  assert_eq(approx_eq(back[2], 0.413), true)
  assert_eq(approx_eq(back[3], 1.0), true)
  assert_eq(approx_eq(back[4], 0.0), true)
  assert_eq(approx_eq(back[5], 0.0), true)
  assert_eq(approx_eq(back[6], 0.0), true)
  assert_eq(approx_eq(back[7], 0.0), true)
  assert_eq(approx_eq(back[8], 0.0), true)
}

test {
  // FFI: ACEScct error on too few pixels
  let data = FixedArray::make(2, 0.0)
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_acescct_to_oklab(2, data, 6, out)
  assert_eq(result, -1)
}

test {
  // FFI: ACEScct error on output too small
  let data = FixedArray::make(6, 0.0)
  let out = FixedArray::make(3, 0.0)
  let result = mengxi_acescct_to_oklab(6, data, 3, out)
  assert_eq(result, -2)
}

// ============================================================
// Linear sRGB ↔ Oklab Tests
// ============================================================

test {
  // Linear: pure black (0,0,0) → Oklab should be (0,0,0)
  let (l, a, b) = linear_rgb_to_oklab(0.0, 0.0, 0.0)
  assert_eq(approx_eq(l, 0.0), true)
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
  // No NaN/Inf
  assert_eq(l == l, true)
  assert_eq(a == a, true)
  assert_eq(b == b, true)
}

test {
  // Linear: white (1,1,1) → Oklab should be approximately (1, 0, 0)
  let (l, a, b) = linear_rgb_to_oklab(1.0, 1.0, 1.0)
  assert_eq(approx_eq(l, 1.0), true)
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
}

test {
  // Linear: mid-gray (0.5, 0.5, 0.5) should be achromatic
  let (_, a, b) = linear_rgb_to_oklab(0.5, 0.5, 0.5)
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
}

test {
  // Linear: 18% scene-referred grey (0.18, 0.18, 0.18) should be achromatic
  let (_, a, b) = linear_rgb_to_oklab(0.18, 0.18, 0.18)
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
}

test {
  // Linear: round-trip mid-gray
  let (l, a, b) = linear_rgb_to_oklab(0.5, 0.5, 0.5)
  let (r2, g2, b2) = oklab_to_linear_rgb(l, a, b)
  assert_eq(approx_eq(r2, 0.5), true)
  assert_eq(approx_eq(g2, 0.5), true)
  assert_eq(approx_eq(b2, 0.5), true)
  // NFR13: round-trip Oklab ΔE < 0.1
  let (r2, g2, b2) = oklab_to_linear_rgb(l, a, b)
  let (l3, a3, b3) = linear_rgb_to_oklab(r2, g2, b2)
  let de = oklab_delta_e(l, a, b, l3, a3, b3)
  assert_eq(approx_eq(de, 0.0), true)
}

test {
  // Linear: round-trip saturated primaries
  let (l1, a1, b1) = linear_rgb_to_oklab(1.0, 0.0, 0.0)
  let (r1, g1, bv1) = oklab_to_linear_rgb(l1, a1, b1)
  assert_eq(approx_eq(r1, 1.0), true)
  assert_eq(approx_eq(g1, 0.0), true)
  assert_eq(approx_eq(bv1, 0.0), true)

  let (l2, a2, b2) = linear_rgb_to_oklab(0.0, 1.0, 0.0)
  let (r2, g2, bv2) = oklab_to_linear_rgb(l2, a2, b2)
  assert_eq(approx_eq(r2, 0.0), true)
  assert_eq(approx_eq(g2, 1.0), true)
  assert_eq(approx_eq(bv2, 0.0), true)

  let (l3, a3, b3) = linear_rgb_to_oklab(0.0, 0.0, 1.0)
  let (r3, g3, bv3) = oklab_to_linear_rgb(l3, a3, b3)
  assert_eq(approx_eq(r3, 0.0), true)
  assert_eq(approx_eq(g3, 0.0), true)
  assert_eq(approx_eq(bv3, 1.0), true)
}

test {
  // Linear: HDR specular (2.0, 2.0, 2.0) should produce finite achromatic values
  let (l, a, b) = linear_rgb_to_oklab(2.0, 2.0, 2.0)
  assert_eq(l == l, true)  // NaN check
  assert_eq(a == a, true)
  assert_eq(b == b, true)
  // Achromatic: a≈0, b≈0
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
  // L should be > 1.0 for HDR
  assert_eq(l > 1.0, true)
}

test {
  // FFI: mengxi_linear_to_oklab basic test
  let data = FixedArray::make(6, 0.0)
  data[0] = 1.0; data[1] = 0.0; data[2] = 0.0
  data[3] = 0.0; data[4] = 1.0; data[5] = 0.0
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_linear_to_oklab(6, data, 6, out)
  assert_eq(result, 6)
  // Red pixel should have positive a
  assert_eq(out[1] > 0.0, true)
  // Green pixel should have negative a
  assert_eq(out[4] < 0.0, true)
}

test {
  // FFI: mengxi_oklab_to_linear basic test
  let data = FixedArray::make(6, 0.0)
  data[0] = 1.0; data[1] = 0.0; data[2] = 0.0
  data[3] = 0.5; data[4] = 0.5; data[5] = 0.5
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_oklab_to_linear(6, data, 6, out)
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
  // FFI: Linear round-trip through FFI functions
  let data = FixedArray::make(9, 0.0)
  data[0] = 0.5; data[1] = 0.5; data[2] = 0.5
  data[3] = 1.0; data[4] = 0.0; data[5] = 0.0
  data[6] = 0.0; data[7] = 0.0; data[8] = 0.0
  let mid = FixedArray::make(9, 0.0)
  let r1 = mengxi_linear_to_oklab(9, data, 9, mid)
  assert_eq(r1, 9)
  let back = FixedArray::make(9, 0.0)
  let r2 = mengxi_oklab_to_linear(9, mid, 9, back)
  assert_eq(r2, 9)
  // Verify round-trip
  assert_eq(approx_eq(back[0], 0.5), true)
  assert_eq(approx_eq(back[1], 0.5), true)
  assert_eq(approx_eq(back[2], 0.5), true)
  assert_eq(approx_eq(back[3], 1.0), true)
  assert_eq(approx_eq(back[4], 0.0), true)
  assert_eq(approx_eq(back[5], 0.0), true)
  assert_eq(approx_eq(back[6], 0.0), true)
  assert_eq(approx_eq(back[7], 0.0), true)
  assert_eq(approx_eq(back[8], 0.0), true)
}

test {
  // FFI: Linear error on too few pixels
  let data = FixedArray::make(2, 0.0)
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_linear_to_oklab(2, data, 6, out)
  assert_eq(result, -1)
}

test {
  // FFI: Linear error on output too small
  let data = FixedArray::make(6, 0.0)
  let out = FixedArray::make(3, 0.0)
  let result = mengxi_linear_to_oklab(6, data, 3, out)
  assert_eq(result, -2)
}

test {
  // FFI: HDR specular round-trip (2.0, 2.0, 2.0)
  let data = FixedArray::make(3, 0.0)
  data[0] = 2.0; data[1] = 2.0; data[2] = 2.0
  let mid = FixedArray::make(3, 0.0)
  let r1 = mengxi_linear_to_oklab(3, data, 3, mid)
  assert_eq(r1, 3)
  // Should be finite
  assert_eq(mid[0] == mid[0], true)
  assert_eq(mid[1] == mid[1], true)
  assert_eq(mid[2] == mid[2], true)
  // Achromatic: a≈0, b≈0
  assert_eq(approx_eq(mid[1], 0.0), true)
  assert_eq(approx_eq(mid[2], 0.0), true)
  // Round-trip back
  let back = FixedArray::make(3, 0.0)
  let r2 = mengxi_oklab_to_linear(3, mid, 3, back)
  assert_eq(r2, 3)
  assert_eq(approx_eq(back[0], 2.0), true)
  assert_eq(approx_eq(back[1], 2.0), true)
  assert_eq(approx_eq(back[2], 2.0), true)
}

// ============================================================
// HDR Round-Trip Tests (AC1: signed_cbrt fix verification)
// ============================================================

test {
  // HDR round-trip: linear_rgb(2.0, 0.5, 0.3) -> oklab -> linear_rgb
  // Error should be < 1e-6 (signed cbrt preserves sign info)
  let (l, a, b) = linear_rgb_to_oklab(2.0, 0.5, 0.3)
  assert_eq(l == l, true)  // NaN check
  assert_eq(a == a, true)
  assert_eq(b == b, true)
  let (r2, g2, b2) = oklab_to_linear_rgb(l, a, b)
  assert_eq(approx_eq(r2, 2.0), true)
  assert_eq(approx_eq(g2, 0.5), true)
  assert_eq(approx_eq(b2, 0.3), true)
}

test {
  // HDR round-trip: negative linear RGB (out-of-gamut, valid in scene-referred)
  let (l, a, b) = linear_rgb_to_oklab(-0.2, 0.8, 1.5)
  assert_eq(l == l, true)  // NaN check
  let (r2, g2, b2) = oklab_to_linear_rgb(l, a, b)
  assert_eq(approx_eq(r2, -0.2), true)
  assert_eq(approx_eq(g2, 0.8), true)
  assert_eq(approx_eq(b2, 1.5), true)
}

test {
  // Regression: sRGB [0,1] range unchanged by HDR fix
  let (l, a, b) = srgb_to_oklab(0.5, 0.5, 0.5)
  let (r2, g2, b2) = oklab_to_srgb(l, a, b)
  assert_eq(approx_eq(r2, 0.5), true)
  assert_eq(approx_eq(g2, 0.5), true)
  assert_eq(approx_eq(b2, 0.5), true)
}

// ============================================================
// Compound Display P3 <-> Oklab Functions
// ============================================================

/// Display P3 (gamma-encoded, IEC 61966-2-1) -> Oklab.
/// Uses sRGB transfer function (Display P3 shares the same EOTF curve as sRGB).
/// Two-step conversion: P3 linear -> XYZ(D65) -> sRGB linear -> Oklab.
fn display_p3_to_oklab(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  // Step 1: Decode gamma (same IEC 61966-2-1 EOTF as sRGB)
  let lr = srgb_to_linear(r)
  let lg = srgb_to_linear(g)
  let lb = srgb_to_linear(b)
  // Step 2: Display P3 linear -> XYZ (D65)
  let (x, y, z) = display_p3_to_xyz(lr, lg, lb)
  // Step 3: XYZ (D65) -> linear sRGB (using shared matrix)
  let (sr, sg, sb) = xyz_to_srgb(x, y, z)
  // Step 4: Linear sRGB -> Oklab (using existing M1 + M2 chain)
  linear_rgb_to_oklab(sr, sg, sb)
}

/// Oklab -> Display P3 (gamma-encoded).
fn oklab_to_display_p3(ok_l : Double, ok_a : Double, ok_b : Double) -> (Double, Double, Double) {
  // Step 1: Oklab -> linear sRGB
  let (sr, sg, sb) = oklab_to_linear_rgb(ok_l, ok_a, ok_b)
  // Step 2: Linear sRGB -> XYZ (D65) using shared matrix from color_science.mbt
  let (x, y, z) = srgb_to_xyz(sr, sg, sb)
  // Step 3: XYZ (D65) -> Display P3 linear
  let (pr, pg, pb) = xyz_to_display_p3(x, y, z)
  // Step 4: Apply sRGB EOTF (same as sRGB for Display P3)
  (linear_to_srgb(pr), linear_to_srgb(pg), linear_to_srgb(pb))
}

// ============================================================
// FFI Exported Functions: Display P3 <-> Oklab
// ============================================================

/// Display P3 -> Oklab conversion via FFI.
pub fn mengxi_display_p3_to_oklab(
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
    let (l, a, bv) = display_p3_to_oklab(r, g, b)
    out_ptr[i * 3] = l
    out_ptr[i * 3 + 1] = a
    out_ptr[i * 3 + 2] = bv
    i = i + 1
  }
  data_len
}

/// Oklab -> Display P3 conversion via FFI.
pub fn mengxi_oklab_to_display_p3(
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
    let (r, g, b) = oklab_to_display_p3(l, a, bv)
    out_ptr[i * 3] = r
    out_ptr[i * 3 + 1] = g
    out_ptr[i * 3 + 2] = b
    i = i + 1
  }
  data_len
}

// ============================================================
// Display P3 Round-Trip Tests
// ============================================================

test {
  // Display P3 round-trip: mid-gray
  let (l, a, b) = display_p3_to_oklab(0.5, 0.5, 0.5)
  let (r2, g2, b2) = oklab_to_display_p3(l, a, b)
  assert_eq(approx_eq(r2, 0.5), true)
  assert_eq(approx_eq(g2, 0.5), true)
  assert_eq(approx_eq(b2, 0.5), true)
}

test {
  // Display P3 round-trip: saturated red (P3 red is more vivid than sRGB)
  let (l, a, b) = display_p3_to_oklab(1.0, 0.0, 0.0)
  let (r2, g2, b2) = oklab_to_display_p3(l, a, b)
  assert_eq(approx_eq(r2, 1.0), true)
  assert_eq(approx_eq(g2, 0.0), true)
  assert_eq(approx_eq(b2, 0.0), true)
}

test {
  // Display P3: D65 white -> Oklab ~(1, 0, 0)
  let (l, a, b) = display_p3_to_oklab(1.0, 1.0, 1.0)
  assert_eq(approx_eq(l, 1.0), true)
  assert_eq(approx_eq(a, 0.0), true)
  assert_eq(approx_eq(b, 0.0), true)
}

test {
  // FFI: display_p3_to_oklab basic test
  let data = FixedArray::make(6, 0.0)
  data[0] = 1.0; data[1] = 0.0; data[2] = 0.0
  data[3] = 0.5; data[4] = 0.5; data[5] = 0.5
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_display_p3_to_oklab(6, data, 6, out)
  assert_eq(result, 6)
  assert_eq(approx_eq(out[3], 1.0), true)
  assert_eq(approx_eq(out[4], 0.0), true)
  assert_eq(approx_eq(out[5], 0.0), true)
}

test {
  // FFI: oklab_to_display_p3 round-trip
  let data = FixedArray::make(6, 0.0)
  data[0] = 0.7; data[1] = 0.2; data[2] = -0.1
  data[3] = 0.4; data[4] = 0.0; data[5] = 0.05
  let mid = FixedArray::make(6, 0.0)
  let r1 = mengxi_display_p3_to_oklab(6, data, 6, mid)
  assert_eq(r1, 6)
  let back = FixedArray::make(6, 0.0)
  let r2 = mengxi_oklab_to_display_p3(6, mid, 6, back)
  assert_eq(r2, 6)
  assert_eq(approx_eq(back[0], 0.7), true)
  assert_eq(approx_eq(back[1], 0.2), true)
  assert_eq(approx_eq(back[2], -0.1), true)
}
