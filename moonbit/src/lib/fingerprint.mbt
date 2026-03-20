// fingerprint.mbt — Color fingerprint extraction (FR2)
// Histogram bins, color space distribution, keyframe characteristics.
// Pure functions only — no I/O, no stateful dependencies.

// ---------- Constants ----------

fn bins_per_channel() -> Int { 64 }

fn output_size() -> Int { 64 * 3 + 2 }

// ---------- Internal helpers ----------

fn clamp01(v : Double) -> Double {
  if v < 0.0 { 0.0 }
  else if v > 1.0 { 1.0 }
  else { v }
}

fn bin_index(v : Double, num_bins : Int) -> Int {
  let clamped = clamp01(v)
  let float_idx = @math.floor(clamped * (num_bins.to_double()))
  let idx = float_idx.to_int()
  if idx >= num_bins { num_bins - 1 } else { idx }
}

fn compute_channel_histogram(
  pixel_data : FixedArray[Double],
  num_pixels : Int,
  channel_offset : Int,
  num_bins : Int
) -> FixedArray[Double] {
  let histogram = FixedArray::make(num_bins, 0.0)
  let mut i = 0
  while i < num_pixels {
    let idx = i * 3 + channel_offset
    if idx < pixel_data.length() {
      let value = pixel_data[idx]
      let bin = bin_index(value, num_bins)
      histogram[bin] = histogram[bin] + 1.0
    }
    i = i + 1
  }
  if num_pixels > 0 {
    let mut j = 0
    let norm = num_pixels.to_double()
    while j < num_bins {
      histogram[j] = histogram[j] / norm
      j = j + 1
    }
  }
  histogram
}

fn luminance(r : Double, g : Double, b : Double) -> Double {
  0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn my_sqrt(x : Double) -> Double {
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

fn compute_mean_stddev(values : FixedArray[Double], len : Int) -> (Double, Double) {
  if len == 0 {
    return (0.0, 0.0)
  }
  let mut sum = 0.0
  let mut i = 0
  while i < len {
    sum = sum + values[i]
    i = i + 1
  }
  let mean = sum / (len.to_double())
  let mut sq_sum = 0.0
  i = 0
  while i < len {
    let diff = values[i] - mean
    sq_sum = sq_sum + diff * diff
    i = i + 1
  }
  let variance = sq_sum / (len.to_double())
  let stddev = my_sqrt(variance)
  (mean, stddev)
}

// ---------- FFI export ----------

/// Compute color fingerprint from interleaved RGB pixel data.
///
/// Parameters:
///   data_len     — number of f64 values (num_pixels * 3)
///   data_ptr     — interleaved RGB pixel data, normalized to [0.0, 1.0]
///   color_tag    — color space tag: 0=linear, 1=log, 2=video
///   out_len      — length of output buffer (must be >= output_size() = 194)
///   out_ptr      — output buffer for histogram bins + statistics
///
/// Output layout (194 f64 values):
///   [0..64)   — Red histogram bins
///   [64..128) — Green histogram bins
///   [128..192)— Blue histogram bins
///   [192]     — Luminance mean
///   [193]     — Luminance stddev
///
/// Returns: number of output values written, or negative error code.
pub fn mengxi_compute_fingerprint(
  data_len : Int,
  data_ptr : FixedArray[Double],
  color_tag : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  let nb = bins_per_channel()
  let os = output_size()

  if data_len < 3 { return -1 }
  if out_len < os { return -2 }

  let num_pixels = data_len / 3

  let hist_r = compute_channel_histogram(data_ptr, num_pixels, 0, nb)
  let hist_g = compute_channel_histogram(data_ptr, num_pixels, 1, nb)
  let hist_b = compute_channel_histogram(data_ptr, num_pixels, 2, nb)

  let lum_array = FixedArray::make(num_pixels, 0.0)
  let mut i = 0
  while i < num_pixels {
    let idx = i * 3
    if idx + 2 < data_ptr.length() {
      lum_array[i] = luminance(data_ptr[idx], data_ptr[idx + 1], data_ptr[idx + 2])
    }
    i = i + 1
  }
  let (mean, stddev) = compute_mean_stddev(lum_array, num_pixels)

  i = 0
  while i < nb {
    if i < out_ptr.length() { out_ptr[i] = hist_r[i] }
    i = i + 1
  }
  i = 0
  while i < nb {
    let out_idx = nb + i
    if out_idx < out_ptr.length() { out_ptr[out_idx] = hist_g[i] }
    i = i + 1
  }
  i = 0
  while i < nb {
    let out_idx = nb * 2 + i
    if out_idx < out_ptr.length() { out_ptr[out_idx] = hist_b[i] }
    i = i + 1
  }
  let mean_idx = nb * 3
  if mean_idx < out_ptr.length() { out_ptr[mean_idx] = mean }
  let std_idx = nb * 3 + 1
  if std_idx < out_ptr.length() { out_ptr[std_idx] = stddev }

  os
}

fn init {
  let dummy_data = FixedArray::make(6, 0.5)
  let dummy_out = FixedArray::make(output_size(), 0.0)
  let _ = mengxi_compute_fingerprint(6, dummy_data, 0, output_size(), dummy_out)
}

// ---------- Unit tests ----------

test {
  assert_eq(clamp01(-0.5), 0.0)
  assert_eq(clamp01(0.5), 0.5)
  assert_eq(clamp01(1.5), 1.0)
  assert_eq(clamp01(0.0), 0.0)
  assert_eq(clamp01(1.0), 1.0)
}

test {
  assert_eq(bin_index(0.0, 4), 0)
  assert_eq(bin_index(0.25, 4), 1)
  assert_eq(bin_index(0.5, 4), 2)
  assert_eq(bin_index(0.75, 4), 3)
  assert_eq(bin_index(1.0, 4), 3)
  assert_eq(bin_index(1.5, 4), 3)
  assert_eq(bin_index(-0.1, 4), 0)
}

test {
  let l1 = luminance(1.0, 1.0, 1.0)
  assert_eq(l1, 1.0)
  let l2 = luminance(0.0, 0.0, 0.0)
  assert_eq(l2, 0.0)
  let l3 = luminance(0.5, 0.5, 0.5)
  assert_eq(l3, 0.5)
}

test {
  let arr = FixedArray::make(4, 0.0)
  arr[0] = 2.0
  arr[1] = 4.0
  arr[2] = 4.0
  arr[3] = 4.0
  let (mean, _stddev) = compute_mean_stddev(arr, 4)
  assert_eq(mean, 3.5)
}

test {
  let arr = FixedArray::make(0, 0.0)
  let (mean, stddev) = compute_mean_stddev(arr, 0)
  assert_eq(mean, 0.0)
  assert_eq(stddev, 0.0)
}

test {
  let data = FixedArray::make(12, 1.0)
  let hist = compute_channel_histogram(data, 4, 0, 8)
  assert_eq(hist[7], 1.0)
  assert_eq(hist[0], 0.0)
}

test {
  let data = FixedArray::make(12, 0.0)
  let hist = compute_channel_histogram(data, 4, 1, 8)
  assert_eq(hist[0], 1.0)
  assert_eq(hist[7], 0.0)
}

test {
  let data = FixedArray::make(6, 0.0)
  data[0] = 1.0; data[1] = 0.0; data[2] = 0.0
  data[3] = 0.0; data[4] = 1.0; data[5] = 0.0
  let out = FixedArray::make(output_size(), 0.0)
  let result = mengxi_compute_fingerprint(6, data, 0, output_size(), out)
  assert_eq(result, output_size())
  let lum_mean = out[bins_per_channel() * 3]
  inspect(lum_mean, content="0.4639")
}

test {
  let data = FixedArray::make(2, 0.5)
  let out = FixedArray::make(output_size(), 0.0)
  let result = mengxi_compute_fingerprint(2, data, 0, output_size(), out)
  assert_eq(result, -1)
}

test {
  let data = FixedArray::make(6, 0.5)
  let out = FixedArray::make(10, 0.0)
  let result = mengxi_compute_fingerprint(6, data, 0, 10, out)
  assert_eq(result, -2)
}
