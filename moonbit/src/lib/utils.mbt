// utils.mbt -- Shared utility functions for all lib modules
// All functions are pub (same-package visibility).
// These eliminate duplication across 17 .mbt files.

/// Clamp value to [0.0, 1.0].
pub fn clamp01(v : Double) -> Double {
  if v < 0.0 { 0.0 }
  else if v > 1.0 { 1.0 }
  else { v }
}

/// Approximate equality for Double values (within 1e-4 tolerance).
/// Standard tolerance for color science comparisons where 0.01% precision suffices.
pub fn approx_eq(actual : Double, expected : Double) -> Bool {
  let diff = actual - expected
  let abs_diff = if diff < 0.0 { 0.0 - diff } else { diff }
  abs_diff < 0.0001
}

/// Approximate equality with custom tolerance.
pub fn approx_eq_tol(actual : Double, expected : Double, tol : Double) -> Bool {
  let diff = actual - expected
  let abs_diff = if diff < 0.0 { 0.0 - diff } else { diff }
  abs_diff < tol
}

/// Newton's method square root (safe for all non-negative inputs).
/// Used when @math.sqrt is unavailable or unreliable.
pub fn my_sqrt(x : Double) -> Double {
  if x <= 0.0 { return 0.0 }
  if x == 1.0 { return 1.0 }
  let mut z = x
  let mut i = 0
  while i < 20 {
    z = (z + x / z) / 2.0
    i = i + 1
  }
  z
}

/// Pi constant (MoonBit may not expose this directly in all build modes).
pub fn pi_const() -> Double { 3.14159265358979323846 }

/// Hue angle from Y,X coordinates (atan2 wrapper).
/// Returns angle in radians, range [-pi, pi].
pub fn hue_angle(y : Double, x : Double) -> Double {
  @math.atan2(y, x)
}

// DCE prevention
fn init {
  let _ = clamp01(0.5)
  let _ = approx_eq(0.0, 0.0)
  let _ = approx_eq_tol(0.0, 0.0, 0.001)
  let _ = my_sqrt(4.0)
  let _ = pi_const()
  let _ = hue_angle(1.0, 0.0)
}

// Unit tests
test {
  assert_eq(clamp01(-0.5), 0.0)
  assert_eq(clamp01(0.0), 0.0)
  assert_eq(clamp01(0.5), 0.5)
  assert_eq(clamp01(1.0), 1.0)
  assert_eq(clamp01(1.5), 1.0)
}

test {
  assert_eq(approx_eq(0.0, 0.0), true)
  assert_eq(approx_eq(1.0, 1.00001), true)
  assert_eq(approx_eq(1.0, 1.001), false)
  assert_eq(approx_eq_tol(1.0, 1.001, 0.01), true)
  assert_eq(approx_eq_tol(1.0, 1.001, 0.0001), false)
}

test {
  assert_eq(my_sqrt(0.0), 0.0)
  assert_eq(my_sqrt(1.0), 1.0)
  assert_eq(my_sqrt(4.0), 2.0)
  assert_eq(approx_eq(my_sqrt(2.0), 1.41421356), true)
}

test {
  assert_eq(pi_const() > 3.14, true)
  assert_eq(pi_const() < 3.15, true)
}

test {
  // hue_angle(0, 1) = 0 (pointing right along +X)
  assert_eq(approx_eq(hue_angle(0.0, 1.0), 0.0), true)
  // hue_angle(1, 0) = pi/2 (pointing up along +Y)
  assert_eq(approx_eq(hue_angle(1.0, 0.0), pi_const() / 2.0), true)
}
