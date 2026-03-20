// types.mbt — Color space constants for FFI boundary (FR22)
// Color space is represented as Int at the FFI boundary:
// 0 = Linear, 1 = Log, 2 = Video

pub fn cs_linear() -> Int { 0 }
pub fn cs_log() -> Int { 1 }
pub fn cs_video() -> Int { 2 }

fn init {
  let _ = cs_linear()
  let _ = cs_log()
  let _ = cs_video()
}

test {
  assert_eq(cs_linear(), 0)
  assert_eq(cs_log(), 1)
  assert_eq(cs_video(), 2)
}
