// distance_metrics.mbt — Bhattacharyya distance for histogram similarity
// Implements: FR11 Bhattacharyya distance candidate ranking
// Reference: architecture-v2.md ADR-v2-3 (hybrid scoring engine)

// ============================================================
// Helper Functions
// ============================================================

fn dm_max(a : Double, b : Double) -> Double {
  if a > b { a } else { b }
}

fn dm_approx_eq(actual : Double, expected : Double) -> Bool {
  let diff = actual - expected
  let abs_diff = if diff < 0.0 { 0.0 - diff } else { diff }
  abs_diff < 0.0001
}

// ============================================================
// Bhattacharyya Coefficient (multi-channel)
// ============================================================

/// Compute Bhattacharyya similarity between two normalized histogram sets.
///
/// Parameters:
///   query_hist      — Normalized query histogram (channels * hist_len f64, interleaved)
///   candidate_hist   — Normalized candidate histogram (same layout)
///   hist_len         — Number of bins per channel (64)
///   channels         — Number of channels (3 for L, a, b)
///   out_score        — Output buffer (1 element): similarity in [0.0, 1.0]
///
/// Returns: 0 on success, negative on error
///   -1 = invalid hist_len or channels
///
/// Formula: BC(p, q) = (1/channels) * sum_c( (1/hist_len) * sum_i( sqrt(clamp(p_ci) * clamp(q_ci)) ) )
/// Clamp: max(v, 1e-10) — prevents sqrt(0) issues in MoonBit
/// Uses @math.pow(x, 0.5) instead of @math.sqrt (known broken in MoonBit static lib)
pub fn mengxi_bhattacharyya_distance(
  query_hist : FixedArray[Double],
  candidate_hist : FixedArray[Double],
  hist_len : Int,
  channels : Int,
  out_score : FixedArray[Double]
) -> Int {
  if hist_len <= 0 { return -1 }
  if channels <= 0 { return -1 }

  let mut total_bc : Double = 0.0

  // Compute per-channel BC, then average
  let mut c = 0
  while c < channels {
    let mut channel_bc : Double = 0.0
    let offset = c * hist_len

    let mut i = 0
    while i < hist_len {
      let p = query_hist[offset + i]
      let q = candidate_hist[offset + i]

      // Clamp to 1e-10 before sqrt (MoonBit math safety)
      let p_clamped = dm_max(p, 0.0000000001)
      let q_clamped = dm_max(q, 0.0000000001)

      let product = p_clamped * q_clamped
      let sq_root = @math.pow(product, 0.5)
      channel_bc = channel_bc + sq_root

      i = i + 1
    }

    // channel_bc is already the BC for this channel (histograms are normalized by caller)
    total_bc = total_bc + channel_bc

    c = c + 1
  }

  // Average across channels
  let score = total_bc / (channels.to_double())
  out_score[0] = score

  0
}

// ============================================================
// DCE Prevention
// ============================================================

fn init {
  // DCE prevention: reference mengxi_bhattacharyya_distance to prevent dead code elimination
  let out = FixedArray::make(1, 0.0)
  let q = FixedArray::make(192, 0.0)  // 3 channels * 64 bins
  let c = FixedArray::make(192, 0.0)
  let _ = mengxi_bhattacharyya_distance(q, c, 64, 3, out)
}

// ============================================================
// Tests
// ============================================================

test {
  // mengxi_bhattacharyya_distance: identical histograms -> BC = 1.0
  let out = FixedArray::make(1, 0.0)
  let q = FixedArray::make(2, 0.0)
  let c = FixedArray::make(2, 0.0)
  // Uniform: [0.5, 0.5] for 2 bins, 1 channel
  q[0] = 0.5; q[1] = 0.5
  c[0] = 0.5; c[1] = 0.5
  let result = mengxi_bhattacharyya_distance(q, c, 2, 1, out)
  assert_eq(result, 0)
  assert_eq(dm_approx_eq(out[0], 1.0), true)
}

test {
  // mengxi_bhattacharyya_distance: orthogonal distributions -> BC ~ 0
  let out = FixedArray::make(1, 0.0)
  let q = FixedArray::make(2, 0.0)
  let c = FixedArray::make(2, 0.0)
  // [1.0, 0.0] vs [0.0, 1.0] — no overlap
  q[0] = 1.0; q[1] = 0.0
  c[0] = 0.0; c[1] = 1.0
  let result = mengxi_bhattacharyya_distance(q, c, 2, 1, out)
  assert_eq(result, 0)
  // With clamping: sqrt(1e-10 * 1) + sqrt(1 * 1e-10) = 2e-5, / 2 = 1e-5
  assert_eq(out[0] < 0.001, true)
}

test {
  // mengxi_bhattacharyya_distance: all zeros -> no crash (clamped)
  let out = FixedArray::make(1, 0.0)
  let q = FixedArray::make(4, 0.0)
  let c = FixedArray::make(4, 0.0)
  let result = mengxi_bhattacharyya_distance(q, c, 4, 1, out)
  assert_eq(result, 0)
  // All clamped to 1e-10: BC = sqrt(1e-10 * 1e-10) * 4 / 4 = 1e-10
  assert_eq(out[0] < 0.001, true)
}

test {
  // mengxi_bhattacharyya_distance: single bin, identical -> 1.0
  let out = FixedArray::make(1, 0.0)
  let q = FixedArray::make(1, 0.0)
  let c = FixedArray::make(1, 0.0)
  q[0] = 1.0
  c[0] = 1.0
  let result = mengxi_bhattacharyya_distance(q, c, 1, 1, out)
  assert_eq(result, 0)
  assert_eq(dm_approx_eq(out[0], 1.0), true)
}

test {
  // mengxi_bhattacharyya_distance: multi-channel (3 channels, 2 bins each)
  let out = FixedArray::make(1, 0.0)
  let q = FixedArray::make(6, 0.0)
  let c = FixedArray::make(6, 0.0)
  // Channel L: [0.5, 0.5] vs [0.5, 0.5] -> BC_L = 1.0
  // Channel a: [1.0, 0.0] vs [0.5, 0.5] -> BC_a < 1.0
  // Channel b: [0.5, 0.5] vs [0.5, 0.5] -> BC_b = 1.0
  q[0] = 0.5; q[1] = 0.5  // L
  q[2] = 1.0; q[3] = 0.0  // a
  q[4] = 0.5; q[5] = 0.5  // b
  c[0] = 0.5; c[1] = 0.5  // L
  c[2] = 0.5; c[3] = 0.5  // a
  c[4] = 0.5; c[5] = 0.5  // b
  let result = mengxi_bhattacharyya_distance(q, c, 2, 3, out)
  assert_eq(result, 0)
  // BC_L = 1.0, BC_a ≈ 0.707, BC_b = 1.0
  // avg ≈ (1.0 + 0.707 + 1.0) / 3 ≈ 0.902
  assert_eq(out[0] > 0.85, true)
  assert_eq(out[0] < 0.95, true)
}

test {
  // mengxi_bhattacharyya_distance: NaN safety — uniform distribution
  let out = FixedArray::make(1, 0.0)
  let q = FixedArray::make(4, 0.0)
  let c = FixedArray::make(4, 0.0)
  q[0] = 0.25; q[1] = 0.25; q[2] = 0.25; q[3] = 0.25
  c[0] = 0.25; c[1] = 0.25; c[2] = 0.25; c[3] = 0.25
  let result = mengxi_bhattacharyya_distance(q, c, 4, 1, out)
  assert_eq(result, 0)
  assert_eq(dm_approx_eq(out[0], 1.0), true)
}

test {
  // mengxi_bhattacharyya_distance: invalid parameters
  let out = FixedArray::make(1, 0.0)
  let q = FixedArray::make(4, 0.0)
  let c = FixedArray::make(4, 0.0)
  assert_eq(mengxi_bhattacharyya_distance(q, c, 0, 1, out), -1)
  assert_eq(mengxi_bhattacharyya_distance(q, c, 4, 0, out), -1)
}

test {
  // mengxi_bhattacharyya_distance: 64 bins x 3 channels (real-world size)
  let size = 192
  let out = FixedArray::make(1, 0.0)
  let q = FixedArray::make(size, 0.0)
  let c = FixedArray::make(size, 0.0)
  let bin_val = 1.0 / 64.0
  let mut i = 0
  while i < size {
    q[i] = bin_val
    c[i] = bin_val
    i = i + 1
  }
  let result = mengxi_bhattacharyya_distance(q, c, 64, 3, out)
  assert_eq(result, 0)
  assert_eq(dm_approx_eq(out[0], 1.0), true)
}
