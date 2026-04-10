// color_science.mbt — ACES 1.3 color space transforms (FR20-FR22)
// Pure functions only — no I/O, no stateful dependencies.
//
// Supported transforms:
//   ACES2065-1 (AP0) ↔ ACEScg (AP1) — 3×3 matrix
//   ACEScg → Rec.709 — RRT + ODT chain
//   Rec.709 → ACEScg — inverse IDT

// ---------- AP0 ↔ AP1 matrices ----------

fn ap0_to_ap1_m00() -> Double { 0.6954522414 }
fn ap0_to_ap1_m01() -> Double { 0.1406786965 }
fn ap0_to_ap1_m02() -> Double { 0.1638690622 }
fn ap0_to_ap1_m10() -> Double { 0.0447945634 }
fn ap0_to_ap1_m11() -> Double { 0.8596711185 }
fn ap0_to_ap1_m12() -> Double { 0.0955343182 }
fn ap0_to_ap1_m20() -> Double { -0.0055258826 }
fn ap0_to_ap1_m21() -> Double { 0.0040252103 }
fn ap0_to_ap1_m22() -> Double { 1.0015006723 }

fn ap1_to_ap0_m00() -> Double { 1.4514393161 }
fn ap1_to_ap0_m01() -> Double { -0.2365107469 }
fn ap1_to_ap0_m02() -> Double { -0.2149285693 }
fn ap1_to_ap0_m10() -> Double { -0.0765537734 }
fn ap1_to_ap0_m11() -> Double { 1.1762296998 }
fn ap1_to_ap0_m12() -> Double { -0.0996959264 }
fn ap1_to_ap0_m20() -> Double { 0.0083161484 }
fn ap1_to_ap0_m21() -> Double { -0.0060324498 }
fn ap1_to_ap0_m22() -> Double { 0.9977163014 }

// ---------- Matrix × Vector ----------

fn mat3_mul_vec3(
  m00 : Double, m01 : Double, m02 : Double,
  m10 : Double, m11 : Double, m12 : Double,
  m20 : Double, m21 : Double, m22 : Double,
  r : Double, g : Double, b : Double
) -> (Double, Double, Double) {
  let out_r = m00 * r + m01 * g + m02 * b
  let out_g = m10 * r + m11 * g + m12 * b
  let out_b = m20 * r + m21 * g + m22 * b
  (out_r, out_g, out_b)
}

// ---------- ACES2065-1 ↔ ACEScg ----------

/// Convert from ACES2065-1 (AP0) to ACEScg (AP1).
pub fn aces2065_to_acescg(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  mat3_mul_vec3(
    ap0_to_ap1_m00(), ap0_to_ap1_m01(), ap0_to_ap1_m02(),
    ap0_to_ap1_m10(), ap0_to_ap1_m11(), ap0_to_ap1_m12(),
    ap0_to_ap1_m20(), ap0_to_ap1_m21(), ap0_to_ap1_m22(),
    r, g, b
  )
}

/// Convert from ACEScg (AP1) to ACES2065-1 (AP0).
pub fn acescg_to_aces2065(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  mat3_mul_vec3(
    ap1_to_ap0_m00(), ap1_to_ap0_m01(), ap1_to_ap0_m02(),
    ap1_to_ap0_m10(), ap1_to_ap0_m11(), ap1_to_ap0_m12(),
    ap1_to_ap0_m20(), ap1_to_ap0_m21(), ap1_to_ap0_m22(),
    r, g, b
  )
}

// ---------- ACES RRT (Reference Rendering Transform) ----------

/// ACES RRT per-channel curve.
fn rrt_curve(x : Double) -> Double {
  let a = 2.51
  let b = 0.03
  let c = 2.43
  let d = 0.59
  let e = 0.14
  (x * (a * x + b)) / (x * (c * x + d) + e)
}

/// Apply ACES RRT to a single pixel.
fn apply_rrt(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  (rrt_curve(r), rrt_curve(g), rrt_curve(b))
}

// ---------- ACES ODT (Output Device Transform) for Rec.709 ----------

/// Simple gamma (power 2.2 approximation) for ACES ODT output.
/// NOTE: This is NOT IEC 61966-2-1 sRGB. It is a simplified transfer function
/// used in the ACES Reference Rendering Transform output stage. For correct
/// sRGB OETF/EOTF, see oklab.mbt::srgb_to_linear / linear_to_srgb.
fn simple_gamma(x : Double) -> Double {
  if x <= 0.0 { return 0.0 }
  @math.pow(x, 1.0 / 2.2)
}

/// Inverse simple gamma (display to linear): x^2.2
/// NOTE: This is NOT IEC 61966-2-1 sRGB. It is a simplified transfer function
/// used in the ACES Reference Rendering Transform output stage. For correct
/// sRGB OETF/EOTF, see oklab.mbt::srgb_to_linear / linear_to_srgb.
fn simple_gamma_inverse(x : Double) -> Double {
  if x <= 0.0 { return 0.0 }
  @math.pow(x, 2.2)
}

// ---------- Shared sRGB <-> XYZ (D65) matrices (IEC 61966-2-1) ----------
// These are used by both Oklab core transforms AND Display P3 compound conversions.
// Extracted to single source of truth per Principle 4.

fn srgb_to_xyz_m00() -> Double { 0.4124564 }
fn srgb_to_xyz_m01() -> Double { 0.3575761 }
fn srgb_to_xyz_m02() -> Double { 0.1804375 }
fn srgb_to_xyz_m10() -> Double { 0.2126729 }
fn srgb_to_xyz_m11() -> Double { 0.7151522 }
fn srgb_to_xyz_m12() -> Double { 0.0721750 }
fn srgb_to_xyz_m20() -> Double { 0.0193339 }
fn srgb_to_xyz_m21() -> Double { 0.1191920 }
fn srgb_to_xyz_m22() -> Double { 0.9503041 }

fn xyz_to_srgb_m00() -> Double {  3.2404542 }
fn xyz_to_srgb_m01() -> Double { -1.5371385 }
fn xyz_to_srgb_m02() -> Double { -0.4985314 }
fn xyz_to_srgb_m10() -> Double { -0.9692660 }
fn xyz_to_srgb_m11() -> Double {  1.8760108 }
fn xyz_to_srgb_m12() -> Double {  0.0415560 }
fn xyz_to_srgb_m20() -> Double {  0.0556434 }
fn xyz_to_srgb_m21() -> Double { -0.2040259 }
fn xyz_to_srgb_m22() -> Double {  1.0572252 }

/// Convert linear sRGB to XYZ (D65) using shared matrix constants.
pub fn srgb_to_xyz(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  let x = srgb_to_xyz_m00() * r + srgb_to_xyz_m01() * g + srgb_to_xyz_m02() * b
  let y = srgb_to_xyz_m10() * r + srgb_to_xyz_m11() * g + srgb_to_xyz_m12() * b
  let z = srgb_to_xyz_m20() * r + srgb_to_xyz_m21() * g + srgb_to_xyz_m22() * b
  (x, y, z)
}

/// Convert XYZ (D65) to linear sRGB using shared matrix constants.
pub fn xyz_to_srgb(x : Double, y : Double, z : Double) -> (Double, Double, Double) {
  let r = xyz_to_srgb_m00() * x + xyz_to_srgb_m01() * y + xyz_to_srgb_m02() * z
  let g = xyz_to_srgb_m10() * x + xyz_to_srgb_m11() * y + xyz_to_srgb_m12() * z
  let b = xyz_to_srgb_m20() * x + xyz_to_srgb_m21() * y + xyz_to_srgb_m22() * z
  (r, g, b)
}

// ---------- Display P3 <-> XYZ (D65) matrices (SMPTE RP 431-2) ----------

fn display_p3_to_xyz_m00() -> Double { 0.4865709486 }
fn display_p3_to_xyz_m01() -> Double { 0.2656676930 }
fn display_p3_to_xyz_m02() -> Double { 0.1982174952 }
fn display_p3_to_xyz_m10() -> Double { 0.2289745680 }
fn display_p3_to_xyz_m11() -> Double { 0.6913675516 }
fn display_p3_to_xyz_m12() -> Double { 0.0798577308 }
fn display_p3_to_xyz_m20() -> Double { 0.0000000000 }
fn display_p3_to_xyz_m21() -> Double { 0.0451133837 }
fn display_p3_to_xyz_m22() -> Double { 1.0419372025 }

fn xyz_to_display_p3_m00() -> Double {  2.4935489629 }
fn xyz_to_display_p3_m01() -> Double { -0.9692655616 }
fn xyz_to_display_p3_m02() -> Double { -0.5547881192 }
fn xyz_to_display_p3_m10() -> Double { -0.8074181753 }
fn xyz_to_display_p3_m11() -> Double {  1.5351469331 }
fn xyz_to_display_p3_m12() -> Double {  0.2722712049 }
fn xyz_to_display_p3_m20() -> Double {  0.0393129714 }
fn xyz_to_display_p3_m21() -> Double { -0.1115672990 }
fn xyz_to_display_p3_m22() -> Double {  0.9597769819 }

/// Convert linear Display P3 to XYZ (D65).
pub fn display_p3_to_xyz(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  let x = display_p3_to_xyz_m00() * r + display_p3_to_xyz_m01() * g + display_p3_to_xyz_m02() * b
  let y = display_p3_to_xyz_m10() * r + display_p3_to_xyz_m11() * g + display_p3_to_xyz_m12() * b
  let z = display_p3_to_xyz_m20() * r + display_p3_to_xyz_m21() * g + display_p3_to_xyz_m22() * b
  (x, y, z)
}

/// Convert XYZ (D65) to linear Display P3.
pub fn xyz_to_display_p3(x : Double, y : Double, z : Double) -> (Double, Double, Double) {
  let r = xyz_to_display_p3_m00() * x + xyz_to_display_p3_m01() * y + xyz_to_display_p3_m02() * z
  let g = xyz_to_display_p3_m10() * x + xyz_to_display_p3_m11() * y + xyz_to_display_p3_m12() * z
  let b = xyz_to_display_p3_m20() * x + xyz_to_display_p3_m21() * y + xyz_to_display_p3_m22() * z
  (r, g, b)
}

/// Apply ACES ODT for Rec.709 output to a single pixel in AP0 space.
fn apply_odt_rec709(r_ap0 : Double, g_ap0 : Double, b_ap0 : Double) -> (Double, Double, Double) {
  // Clamp to [0, 1] and apply simple gamma
  (simple_gamma(clamp01(r_ap0)), simple_gamma(clamp01(g_ap0)), simple_gamma(clamp01(b_ap0)))
}

// ---------- Full ACEScg → Rec.709 ----------

/// Convert from ACEScg to Rec.709 display output.
/// Chain: ACEScg (AP1) → RRT → AP0 (AP1→AP0 matrix) → ODT (clamp + gamma)
pub fn acescg_to_rec709(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  // Step 1: Apply RRT in ACEScg space
  let (rrt_r, rrt_g, rrt_b) = apply_rrt(r, g, b)

  // Step 2: Convert RRT output from AP1 to AP0
  let (ap0_r, ap0_g, ap0_b) = acescg_to_aces2065(rrt_r, rrt_g, rrt_b)

  // Step 3: Apply ODT (clamp + gamma)
  apply_odt_rec709(ap0_r, ap0_g, ap0_b)
}

// ---------- Inverse RRT (Newton's method) ----------

/// Numerically invert the ACES RRT curve using Newton's method.
/// Given y = rrt_curve(x), find x such that rrt_curve(x) ≈ y.
/// Converges in 4-5 iterations to ~1e-10 precision for the monotonic RRT curve.
fn inverse_rrt_curve(y : Double) -> Double {
  if y <= 0.0 { return 0.0 }

  let a = 2.51
  let b = 0.03
  let c = 2.43
  let d = 0.59
  let e = 0.14

  // Initial guess: RRT compresses highlights, so outputs above RRT(1)≈0.8038
  // imply inputs > 1. Use linear scaling for better convergence.
  let mut x = if y >= 0.8 { y * 1.25 } else { y }

  let mut i = 0
  while i < 8 {
    let ax = a * x
    let cx = c * x
    let num = x * (ax + b)        // x*(a*x+b)
    let den = x * (cx + d) + e    // x*(c*x+d)+e
    if den == 0.0 { break }

    let fx = num / den
    let residual = fx - y

    // Check convergence (tolerance = 1e-10)
    let abs_r = if residual < 0.0 { 0.0 - residual } else { residual }
    if abs_r < 0.0000000001 { break }

    // f'(x) = (N'*D - N*D') / D²
    // N'(x) = 2*a*x + b,  D'(x) = 2*c*x + d
    let n_prime = 2.0 * ax + b
    let d_prime = 2.0 * cx + d
    let fpx = (n_prime * den - num * d_prime) / (den * den)
    if fpx == 0.0 { break }

    x = x - residual / fpx
    if x < 0.0 { x = 0.0 }

    i = i + 1
  }

  x
}

// ---------- Rec.709 → ACEScg (inverse) ----------

/// Convert from Rec.709 display output back to ACEScg.
/// Chain: inverse sRGB gamma → AP0 → AP1 → inverse RRT
/// Uses Newton's method for the inverse RRT step.
/// Note: values that were clamped during the forward acescg_to_rec709
/// (AP0 values > 1.0) cannot be recovered — round-trip is lossy for those.
pub fn rec709_to_acescg(r : Double, g : Double, b : Double) -> (Double, Double, Double) {
  // Step 1: Inverse gamma
  let lin_r = simple_gamma_inverse(r)
  let lin_g = simple_gamma_inverse(g)
  let lin_b = simple_gamma_inverse(b)

  // Step 2: Convert AP0 to AP1
  let (ap1_r, ap1_g, ap1_b) = aces2065_to_acescg(lin_r, lin_g, lin_b)

  // Step 3: Apply inverse RRT
  (inverse_rrt_curve(ap1_r), inverse_rrt_curve(ap1_g), inverse_rrt_curve(ap1_b))
}

// ---------- DCE prevention ----------

fn init {
  let _ = aces2065_to_acescg(0.0, 0.0, 0.0)
  let _ = acescg_to_aces2065(0.0, 0.0, 0.0)
  let _ = acescg_to_rec709(0.18, 0.18, 0.18)
  let _ = rec709_to_acescg(0.5, 0.5, 0.5)
  let _ = inverse_rrt_curve(0.5)
}

// ---------- FFI export ----------

/// Apply ACES color space transform to interleaved RGB pixel data.
///
/// Parameters:
///   data_len   — number of f64 values (num_pixels * 3)
///   data_ptr   — interleaved RGB pixel data
///   src_tag    — source color space: 10=ACES2065-1, 11=ACEScg, 20=Rec709
///   dst_tag    — destination color space: 10=ACES2065-1, 11=ACEScg, 20=Rec709
///   out_len    — length of output buffer (must be >= data_len)
///   out_ptr    — output buffer for transformed pixel data
///
/// Returns: number of output values written, or negative error code.
///   -1 = invalid data (less than 3 values)
///   -2 = output buffer too small
///   -3 = unsupported transform (e.g., Log source without conversion)
///   -4 = unknown source or destination color space
pub fn mengxi_aces_transform(
  data_len : Int,
  data_ptr : FixedArray[Double],
  src_tag : Int,
  dst_tag : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if data_len < 3 { return -1 }
  if out_len < data_len { return -2 }

  let num_pixels = data_len / 3

  // Reject log-encoded sources (FR22 type safety)
  if is_log_space(src_tag) { return -3 }

  // Determine transform function based on src/dst
  let mut i = 0
  while i < num_pixels {
    let idx = i * 3
    if idx + 2 >= data_ptr.length() { break }

    let r = data_ptr[idx]
    let g = data_ptr[idx + 1]
    let b = data_ptr[idx + 2]

    let (or, og, ob) = apply_transform(r, g, b, src_tag, dst_tag)

    // Detect unsupported transform: apply_transform returns NaN for
    // unsupported src/dst combinations. Check first pixel to fail fast.
    if i == 0 {
      // NaN is the only value not equal to itself
      if or != or || og != og || ob != ob { return -4 }
    }

    if idx < out_ptr.length() { out_ptr[idx] = or }
    let idx1 = idx + 1
    if idx1 < out_ptr.length() { out_ptr[idx1] = og }
    let idx2 = idx + 2
    if idx2 < out_ptr.length() { out_ptr[idx2] = ob }

    i = i + 1
  }

  data_len
}

/// Internal: dispatch to the correct transform based on src/dst tags.
fn apply_transform(r : Double, g : Double, b : Double, src : Int, dst : Int) -> (Double, Double, Double) {
  // ACEScg → Rec.709
  if src == cs_acescg() && dst == cs_rec709() {
    return acescg_to_rec709(r, g, b)
  }
  // Rec.709 → ACEScg
  if src == cs_rec709() && dst == cs_acescg() {
    return rec709_to_acescg(r, g, b)
  }
  // ACES2065-1 → ACEScg
  if src == cs_aces2065_1() && dst == cs_acescg() {
    return aces2065_to_acescg(r, g, b)
  }
  // ACEScg → ACES2065-1
  if src == cs_acescg() && dst == cs_aces2065_1() {
    return acescg_to_aces2065(r, g, b)
  }
  // Same space: identity
  if src == dst { return (r, g, b) }

  // Unsupported transform — return NaN to signal error to caller
  let nan = 0.0 / 0.0
  return (nan, nan, nan)
}

// ---------- Unit tests ----------

test {
  // Identity: AP0 → AP1 → AP0 round-trip
  let r = 0.5
  let g = 0.25
  let b = 0.75
  let (cg_r, cg_g, cg_b) = aces2065_to_acescg(r, g, b)
  let (back_r, back_g, back_b) = acescg_to_aces2065(cg_r, cg_g, cg_b)
  assert_eq(approx_eq(back_r, r), true)
  assert_eq(approx_eq(back_g, g), true)
  assert_eq(approx_eq(back_b, b), true)
}

test {
  // Identity: AP1 → AP0 → AP1 round-trip
  let r = 0.1
  let g = 0.2
  let b = 0.3
  let (ap0_r, ap0_g, ap0_b) = acescg_to_aces2065(r, g, b)
  let (back_r, back_g, back_b) = aces2065_to_acescg(ap0_r, ap0_g, ap0_b)
  assert_eq(approx_eq(back_r, r), true)
  assert_eq(approx_eq(back_g, g), true)
  assert_eq(approx_eq(back_b, b), true)
}

test {
  // ACEScg(0,0,0) → Rec.709 should be (0,0,0)
  let (r, g, b) = acescg_to_rec709(0.0, 0.0, 0.0)
  inspect(r, content="0")
  inspect(g, content="0")
  inspect(b, content="0")
}

test {
  // ACEScg(1,1,1) → Rec.709 should be near 0.9 (RRT compresses highlights)
  let (r, g, b) = acescg_to_rec709(1.0, 1.0, 1.0)
  assert_eq(approx_eq(r, 0.9055), true)
  assert_eq(approx_eq(g, 0.9055), true)
  assert_eq(approx_eq(b, 0.9055), true)
}

test {
  // ACEScg(0.18,0.18,0.18) → Rec.709 should be a valid display value
  let (r, g, b) = acescg_to_rec709(0.18, 0.18, 0.18)
  // 18% grey luminance should be around 0.55
  let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b
  assert_eq(approx_eq(lum, 0.5486), true)
}

test {
  // RRT curve: RRT(0) = 0
  let r = rrt_curve(0.0)
  inspect(r, content="0")
}

test {
  // RRT curve: RRT(1) = 0.8038
  let r = rrt_curve(1.0)
  assert_eq(approx_eq(r, 0.8038), true)
}

test {
  // Matrix identity: AP0→AP1→AP0 for a larger value
  let (cg_r, cg_g, cg_b) = aces2065_to_acescg(5.0, 10.0, 2.0)
  let (back_r, back_g, back_b) = acescg_to_aces2065(cg_r, cg_g, cg_b)
  assert_eq(approx_eq(back_r, 5.0), true)
  assert_eq(approx_eq(back_g, 10.0), true)
  assert_eq(approx_eq(back_b, 2.0), true)
}

test {
  // Negative values: AP0→AP1→AP0 round-trip
  let (cg_r, cg_g, cg_b) = aces2065_to_acescg(-0.1, -0.2, -0.3)
  let (back_r, back_g, back_b) = acescg_to_aces2065(cg_r, cg_g, cg_b)
  assert_eq(approx_eq(back_r, -0.1), true)
  assert_eq(approx_eq(back_g, -0.2), true)
  assert_eq(approx_eq(back_b, -0.3), true)
}

test {
  // ACEScg(0.5, 0.5, 0.5) → Rec.709 should be around 0.80
  let (r, _g, _b) = acescg_to_rec709(0.5, 0.5, 0.5)
  assert_eq(approx_eq(r, 0.8025), true)
}

test {
  // Rec.709(0,0,0) → ACEScg should be (0,0,0)
  let (r, g, b) = rec709_to_acescg(0.0, 0.0, 0.0)
  inspect(r, content="0")
  inspect(g, content="0")
  inspect(b, content="0")
}

test {
  // Rec.709(1,1,1) → ACEScg should be approximately (1,1,1)
  let (r, g, b) = rec709_to_acescg(1.0, 1.0, 1.0)
  assert_eq(approx_eq(r, 1.0), true)
  assert_eq(approx_eq(g, 1.0), true)
  assert_eq(approx_eq(b, 1.0), true)
}

test {
  // Simple gamma: gamma(0) = 0, gamma(1) = 1
  inspect(simple_gamma(0.0), content="0")
  inspect(simple_gamma(1.0), content="1")
}

test {
  // Simple gamma inverse: inv_gamma(0) = 0, inv_gamma(1) = 1
  inspect(simple_gamma_inverse(0.0), content="0")
  inspect(simple_gamma_inverse(1.0), content="1")
}

test {
  // Simple gamma round-trip
  let original = 0.5
  let encoded = simple_gamma(original)
  let decoded = simple_gamma_inverse(encoded)
  assert_eq(approx_eq(decoded, 0.5), true)
}

test {
  // Inverse RRT: rrt_curve(0) = 0, so inverse_rrt(0) = 0
  let x = inverse_rrt_curve(0.0)
  inspect(x, content="0")
}

test {
  // Inverse RRT: inverse_rrt(rrt_curve(1.0)) ≈ 1.0
  let forward = rrt_curve(1.0)
  let back = inverse_rrt_curve(forward)
  assert_eq(approx_eq(back, 1.0), true)
}

test {
  // Inverse RRT: inverse_rrt(rrt_curve(0.18)) ≈ 0.18
  let forward = rrt_curve(0.18)
  let back = inverse_rrt_curve(forward)
  assert_eq(approx_eq(back, 0.18), true)
}

test {
  // Inverse RRT: inverse_rrt(rrt_curve(0.5)) ≈ 0.5
  let forward = rrt_curve(0.5)
  let back = inverse_rrt_curve(forward)
  assert_eq(approx_eq(back, 0.5), true)
}

test {
  // Inverse RRT: round-trip for a value > 1 (highlight)
  let forward = rrt_curve(2.0)
  let back = inverse_rrt_curve(forward)
  assert_eq(approx_eq(back, 2.0), true)
}

test {
  // Inverse RRT: round-trip for a small value (shadow)
  let forward = rrt_curve(0.01)
  let back = inverse_rrt_curve(forward)
  assert_eq(approx_eq(back, 0.01), true)
}

test {
  // Full round-trip: ACEScg → Rec.709 → ACEScg for mid-grey
  let (r709, g709, b709) = acescg_to_rec709(0.18, 0.18, 0.18)
  let (back_r, back_g, back_b) = rec709_to_acescg(r709, g709, b709)
  assert_eq(approx_eq(back_r, 0.18), true)
  assert_eq(approx_eq(back_g, 0.18), true)
  assert_eq(approx_eq(back_b, 0.18), true)
}

test {
  // Full round-trip: ACEScg → Rec.709 → ACEScg for a saturated color
  let (r709, g709, b709) = acescg_to_rec709(0.8, 0.3, 0.1)
  let (back_r, back_g, back_b) = rec709_to_acescg(r709, g709, b709)
  assert_eq(approx_eq(back_r, 0.8), true)
  assert_eq(approx_eq(back_g, 0.3), true)
  assert_eq(approx_eq(back_b, 0.1), true)
}

test {
  // Full round-trip: ACEScg → Rec.709 → ACEScg for black and white
  let (r0, g0, b0) = acescg_to_rec709(0.0, 0.0, 0.0)
  let (back0_r, back0_g, back0_b) = rec709_to_acescg(r0, g0, b0)
  assert_eq(approx_eq(back0_r, 0.0), true)
  assert_eq(approx_eq(back0_g, 0.0), true)
  assert_eq(approx_eq(back0_b, 0.0), true)
  let (r1, g1, b1) = acescg_to_rec709(1.0, 1.0, 1.0)
  let (back1_r, back1_g, back1_b) = rec709_to_acescg(r1, g1, b1)
  assert_eq(approx_eq(back1_r, 1.0), true)
  assert_eq(approx_eq(back1_g, 1.0), true)
  assert_eq(approx_eq(back1_b, 1.0), true)
}

test {
  // FFI: valid transform ACEScg → Rec.709
  let data = FixedArray::make(6, 0.0)
  data[0] = 0.18; data[1] = 0.18; data[2] = 0.18
  data[3] = 0.5;  data[4] = 0.5;  data[5] = 0.5
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_aces_transform(6, data, cs_acescg(), cs_rec709(), 6, out)
  assert_eq(result, 6)
}

test {
  // FFI: reject log-encoded source
  let data = FixedArray::make(3, 0.5)
  let out = FixedArray::make(3, 0.0)
  let result = mengxi_aces_transform(3, data, cs_acescct(), cs_rec709(), 3, out)
  assert_eq(result, -3)
}

test {
  // FFI: too few data values
  let data = FixedArray::make(2, 0.5)
  let out = FixedArray::make(2, 0.0)
  let result = mengxi_aces_transform(2, data, cs_acescg(), cs_rec709(), 2, out)
  assert_eq(result, -1)
}

test {
  // FFI: output buffer too small
  let data = FixedArray::make(6, 0.5)
  let out = FixedArray::make(3, 0.0)
  let result = mengxi_aces_transform(6, data, cs_acescg(), cs_rec709(), 3, out)
  assert_eq(result, -2)
}

test {
  // FFI: identity transform (same src and dst)
  let data = FixedArray::make(6, 0.0)
  data[0] = 0.3; data[1] = 0.6; data[2] = 0.9
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_aces_transform(6, data, cs_acescg(), cs_acescg(), 6, out)
  assert_eq(result, 6)
  inspect(out[0], content="0.3")
  inspect(out[1], content="0.6")
  inspect(out[2], content="0.9")
}

test {
  // FFI: ACES2065-1 → ACEScg round-trip
  let data = FixedArray::make(6, 0.0)
  data[0] = 0.5; data[1] = 0.25; data[2] = 0.75
  let out = FixedArray::make(6, 0.0)
  let result = mengxi_aces_transform(6, data, cs_aces2065_1(), cs_acescg(), 6, out)
  assert_eq(result, 6)
  // Now round-trip back
  let back = FixedArray::make(6, 0.0)
  let result2 = mengxi_aces_transform(6, out, cs_acescg(), cs_aces2065_1(), 6, back)
  assert_eq(result2, 6)
  assert_eq(approx_eq(back[0], 0.5), true)
  assert_eq(approx_eq(back[1], 0.25), true)
  assert_eq(approx_eq(back[2], 0.75), true)
}
