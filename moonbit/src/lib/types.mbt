// types.mbt — Color space constants for FFI boundary (FR22)
// Color space is represented as Int at the FFI boundary.
//
// Encoding type (low range 0-9):
//   0 = Linear, 1 = Log, 2 = Video
//
// ACES color space (high range 10+):
//   10 = ACES2065-1 (AP0), 11 = ACEScg (AP1), 12 = ACEScct, 20 = Rec709

// ---------- Encoding type constants ----------

pub fn cs_linear() -> Int { 0 }
pub fn cs_log() -> Int { 1 }
pub fn cs_video() -> Int { 2 }

// ---------- ACES color space constants ----------

pub fn cs_aces2065_1() -> Int { 10 }
pub fn cs_acescg() -> Int { 11 }
pub fn cs_acescct() -> Int { 12 }
pub fn cs_rec709() -> Int { 20 }

// ---------- Helpers ----------

/// Check if a color space tag represents a log-encoded space.
pub fn is_log_space(tag : Int) -> Bool {
  tag == cs_acescct() || tag == cs_log()
}

// ---------- DCE prevention ----------

fn init {
  let _ = cs_linear()
  let _ = cs_log()
  let _ = cs_video()
  let _ = cs_aces2065_1()
  let _ = cs_acescg()
  let _ = cs_acescct()
  let _ = cs_rec709()
  let _ = is_log_space(0)
}

// ---------- Unit tests ----------

test {
  assert_eq(cs_linear(), 0)
  assert_eq(cs_log(), 1)
  assert_eq(cs_video(), 2)
}

test {
  assert_eq(cs_aces2065_1(), 10)
  assert_eq(cs_acescg(), 11)
  assert_eq(cs_acescct(), 12)
  assert_eq(cs_rec709(), 20)
}

test {
  assert_eq(is_log_space(cs_acescct()), true)
  assert_eq(is_log_space(cs_log()), true)
  assert_eq(is_log_space(cs_linear()), false)
  assert_eq(is_log_space(cs_video()), false)
  assert_eq(is_log_space(cs_acescg()), false)
  assert_eq(is_log_space(cs_aces2065_1()), false)
  assert_eq(is_log_space(cs_rec709()), false)
}
