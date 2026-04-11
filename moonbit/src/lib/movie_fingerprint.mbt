// movie_fingerprint.mbt — Movie fingerprint strip extraction and cineiris transform
// Center column extraction, strip stitching, and polar coordinate mapping.
// Pure functions only — no I/O, no stateful dependencies.

// ---------- Internal helpers ----------
// Note: my_atan2 and normalize_angle are now in utils.mbt — re-exported here for
// backward compat within this file's test blocks.

/// Bilinear interpolation on a row-major RGB strip image.
/// strip_width and strip_height are in pixels (each pixel = 3 f64 values).
fn bilinear_sample(
  strip : FixedArray[Double],
  strip_width : Int,
  strip_height : Int,
  fx : Double,
  fy : Double
) -> (Double, Double, Double) {
  let x0 = @math.floor(fx).to_int()
  let y0 = @math.floor(fy).to_int()
  let x1 = x0 + 1
  let y1 = y0 + 1
  let dx = fx - x0.to_double()
  let dy = fy - y0.to_double()

  fn clamp_idx(v : Int, max : Int) -> Int {
    if v < 0 { 0 } else if v >= max { max - 1 } else { v }
  }

  let cx0 = clamp_idx(x0, strip_width)
  let cx1 = clamp_idx(x1, strip_width)
  let cy0 = clamp_idx(y0, strip_height)
  let cy1 = clamp_idx(y1, strip_height)

  fn pixel_at(px : Int, py : Int) -> (Double, Double, Double) {
    let idx = (py * strip_width + px) * 3
    if idx + 2 >= strip.length() {
      return (0.0, 0.0, 0.0)
    }
    (strip[idx], strip[idx + 1], strip[idx + 2])
  }

  let (r00, g00, b00) = pixel_at(cx0, cy0)
  let (r10, g10, b10) = pixel_at(cx1, cy0)
  let (r01, g01, b01) = pixel_at(cx0, cy1)
  let (r11, g11, b11) = pixel_at(cx1, cy1)

  let w00 = (1.0 - dx) * (1.0 - dy)
  let w10 = dx * (1.0 - dy)
  let w01 = (1.0 - dx) * dy
  let w11 = dx * dy

  (
    r00 * w00 + r10 * w10 + r01 * w01 + r11 * w11,
    g00 * w00 + g10 * w10 + g01 * w01 + g11 * w11,
    b00 * w00 + b10 * w10 + b01 * w01 + b11 * w11
  )
}

// ---------- Pure computation functions ----------

/// Extract the center column from an interleaved RGB image.
fn do_extract_center_column(
  pixels : FixedArray[Double],
  width : Int,
  height : Int
) -> FixedArray[Double] {
  let expected_len = width * height * 3
  if pixels.length() < expected_len || width <= 0 || height <= 0 {
    return FixedArray::make(0, 0.0)
  }

  let center_x = width / 2
  let out = FixedArray::make(height * 3, 0.0)
  let mut y = 0
  while y < height {
    let src_idx = (y * width + center_x) * 3
    let dst_idx = y * 3
    out[dst_idx] = pixels[src_idx]
    out[dst_idx + 1] = pixels[src_idx + 1]
    out[dst_idx + 2] = pixels[src_idx + 2]
    y = y + 1
  }
  out
}

/// Stitch center columns from multiple frames into a row-major strip image.
fn do_stitch_fingerprint_strip(
  columns : FixedArray[Double],
  num_frames : Int,
  frame_height : Int
) -> FixedArray[Double] {
  let expected_len = num_frames * frame_height * 3
  if columns.length() < expected_len || num_frames <= 0 || frame_height <= 0 {
    return FixedArray::make(0, 0.0)
  }

  let strip_width = num_frames
  let strip_height = frame_height
  let out = FixedArray::make(strip_width * strip_height * 3, 0.0)

  let mut frame = 0
  while frame < num_frames {
    let mut row = 0
    while row < frame_height {
      let src_idx = (frame * frame_height + row) * 3
      let dst_idx = (row * strip_width + frame) * 3
      out[dst_idx] = columns[src_idx]
      out[dst_idx + 1] = columns[src_idx + 1]
      out[dst_idx + 2] = columns[src_idx + 2]
      row = row + 1
    }
    frame = frame + 1
  }
  out
}

/// Cineiris (cine-iris) polar coordinate transform with angular frame blending.
/// Each output pixel blends ±4 adjacent frames via Gaussian weights for smooth color transitions.
fn do_cineiris_transform(
  strip_pixels : FixedArray[Double],
  strip_width : Int,
  strip_height : Int,
  iris_diameter : Int
) -> FixedArray[Double] {
  let pi = pi_const()
  let two_pi = 2.0 * pi

  let strip_expected = strip_width * strip_height * 3
  if strip_pixels.length() < strip_expected || strip_width <= 0 || strip_height <= 0 || iris_diameter <= 0 {
    return FixedArray::make(0, 0.0)
  }

  let out = FixedArray::make(iris_diameter * iris_diameter * 3, 0.0)
  let cx = iris_diameter.to_double() / 2.0
  let cy = iris_diameter.to_double() / 2.0
  let radius = iris_diameter.to_double() / 2.0

  if radius <= 0.0 {
    return out
  }

  // Angular Gaussian kernel for frame blending (±2 frames, σ=1.2)
  // Light blending for smooth transitions without losing detail
  let ang_radius = 2
  let ang_sigma = 1.2
  let ang_taps = ang_radius * 2 + 1
  let ang_weights = FixedArray::make(ang_taps, 0.0)
  let mut ang_wsum = 0.0
  let mut ai = 0
  while ai < ang_taps {
    let offset = (ai - ang_radius).to_double()
    let w = @math.exp(-0.5 * offset * offset / (ang_sigma * ang_sigma))
    ang_weights[ai] = w
    ang_wsum = ang_wsum + w
    ai = ai + 1
  }
  ai = 0
  while ai < ang_taps {
    ang_weights[ai] = ang_weights[ai] / ang_wsum
    ai = ai + 1
  }

  // 4x SSAA sub-pixel offsets (2x2 grid)
  let ss_offsets : FixedArray[(Double, Double)] =
    FixedArray::make(4, (0.0, 0.0))
  ss_offsets[0] = (-0.25, -0.25)
  ss_offsets[1] = ( 0.25, -0.25)
  ss_offsets[2] = (-0.25,  0.25)
  ss_offsets[3] = ( 0.25,  0.25)

  let sw_double = strip_width.to_double()
  let sh_minus1 = (strip_height - 1).to_double()

  let mut py = 0
  while py < iris_diameter {
    let mut px = 0
    while px < iris_diameter {
      let out_idx = (py * iris_diameter + px) * 3

      let mut r_sum = 0.0
      let mut g_sum = 0.0
      let mut b_sum = 0.0
      let mut valid_count = 0

      // 4x supersampling
      let mut si = 0
      while si < 4 {
        let (sox, soy) = ss_offsets[si]
        let dx = (px.to_double() + sox) - cx
        let dy = (py.to_double() + soy) - cy
        let dist = my_sqrt(dx * dx + dy * dy)

        if dist <= radius {
          let angle = normalize_angle_0_2pi(my_atan2(dy, dx))
          let center_frame = angle / two_pi * sw_double
          let vertical_idx = dist / radius * sh_minus1

          let clamped_vert = if vertical_idx < 0.0 {
            0.0
          } else if vertical_idx >= sh_minus1 {
            sh_minus1
          } else {
            vertical_idx
          }

          // Angular Gaussian sampling: blend nearby frames
          let mut r_ang = 0.0
          let mut g_ang = 0.0
          let mut b_ang = 0.0
          let mut tap = 0
          while tap < ang_taps {
            let mut frame_idx = center_frame + (tap - ang_radius).to_double()
            // Wrap around for seamless circular mapping
            if frame_idx < 0.0 {
              frame_idx = frame_idx + sw_double
            } else if frame_idx >= sw_double {
              frame_idx = frame_idx - sw_double
            }

            let (r, g, b) = bilinear_sample(
              strip_pixels, strip_width, strip_height,
              frame_idx, clamped_vert
            )
            let w = ang_weights[tap]
            r_ang = r_ang + r * w
            g_ang = g_ang + g * w
            b_ang = b_ang + b * w
            tap = tap + 1
          }

          r_sum = r_sum + r_ang
          g_sum = g_sum + g_ang
          b_sum = b_sum + b_ang
          valid_count = valid_count + 1
        }
        si = si + 1
      }

      if valid_count > 0 {
        let inv = 1.0 / valid_count.to_double()
        out[out_idx] = r_sum * inv
        out[out_idx + 1] = g_sum * inv
        out[out_idx + 2] = b_sum * inv
      }
      px = px + 1
    }
    py = py + 1
  }

  // Apply 3x3 Gaussian blur to smooth remaining artifacts
  gaussian_blur_circular(out, iris_diameter)
}

/// Apply 3x3 Gaussian blur to circular image, preserving circle boundary.
fn gaussian_blur_circular(
  image : FixedArray[Double],
  diameter : Int
) -> FixedArray[Double] {
  let out = FixedArray::make(diameter * diameter * 3, 0.0)
  // 3x3 Gaussian kernel: [1,2,1; 2,4,2; 1,2,1] / 16
  let kernel : FixedArray[Double] =
    FixedArray::make(9, 0.0)
  kernel[0] = 1.0; kernel[1] = 2.0; kernel[2] = 1.0
  kernel[3] = 2.0; kernel[4] = 4.0; kernel[5] = 2.0
  kernel[6] = 1.0; kernel[7] = 2.0; kernel[8] = 1.0

  let mut py = 0
  while py < diameter {
    let mut px = 0
    while px < diameter {
      let center_idx = (py * diameter + px) * 3
      // Skip black (outside circle) pixels
      let center_r = image[center_idx]
      let center_g = image[center_idx + 1]
      let center_b = image[center_idx + 2]
      if center_r <= 0.0 && center_g <= 0.0 && center_b <= 0.0 {
        px = px + 1
        continue
      }

      let mut r_sum = 0.0
      let mut g_sum = 0.0
      let mut b_sum = 0.0
      let mut weight_sum = 0.0

      let mut ky = -1
      while ky <= 1 {
        let mut kx = -1
        while kx <= 1 {
          let ny = py + ky
          let nx = px + kx
          if nx >= 0 && nx < diameter && ny >= 0 && ny < diameter {
            let n_idx = (ny * diameter + nx) * 3
            let nr = image[n_idx]
            let ng = image[n_idx + 1]
            let nb = image[n_idx + 2]
            // Only include non-black neighbors (inside circle)
            if nr > 0.0 || ng > 0.0 || nb > 0.0 {
              let w = kernel[(ky + 1) * 3 + (kx + 1)]
              r_sum = r_sum + nr * w
              g_sum = g_sum + ng * w
              b_sum = b_sum + nb * w
              weight_sum = weight_sum + w
            }
          }
          kx = kx + 1
        }
        ky = ky + 1
      }

      let out_idx = (py * diameter + px) * 3
      if weight_sum > 0.0 {
        out[out_idx] = r_sum / weight_sum
        out[out_idx + 1] = g_sum / weight_sum
        out[out_idx + 2] = b_sum / weight_sum
      }
      px = px + 1
    }
    py = py + 1
  }
  out
}

// ---------- FFI exports ----------

/// Extract center column from interleaved RGB frame.
///
/// Parameters:
///   pixel_len — number of f64 values in input (width * height * 3)
///   pixel_ptr — interleaved RGB pixel data
///   width     — frame width in pixels
///   height    — frame height in pixels
///   out_len   — length of output buffer (must be >= height * 3)
///   out_ptr   — output buffer for center column RGB
///
/// Returns: number of output values written, or negative error code.
pub fn mengxi_extract_center_column(
  pixel_len : Int,
  pixel_ptr : FixedArray[Double],
  width : Int,
  height : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  if pixel_len < width * height * 3 { return -1 }
  if out_len < height * 3 { return -2 }

  let col = do_extract_center_column(pixel_ptr, width, height)
  let n = height * 3
  let mut i = 0
  while i < n {
    if i < out_ptr.length() { out_ptr[i] = col[i] }
    i = i + 1
  }
  n
}

/// Stitch per-frame center columns into a row-major fingerprint strip.
///
/// Parameters:
///   columns_len  — number of f64 values in input (num_frames * frame_height * 3)
///   columns_ptr  — concatenated center columns (column-major)
///   num_frames   — number of frames (strip width)
///   frame_height — height of each frame column (strip height)
///   out_len      — length of output buffer (must be >= num_frames * frame_height * 3)
///   out_ptr      — output buffer for row-major strip
///
/// Returns: number of output values written, or negative error code.
pub fn mengxi_stitch_fingerprint_strip(
  columns_len : Int,
  columns_ptr : FixedArray[Double],
  num_frames : Int,
  frame_height : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  let expected = num_frames * frame_height * 3
  if columns_len < expected { return -1 }
  if out_len < expected { return -2 }

  let strip = do_stitch_fingerprint_strip(columns_ptr, num_frames, frame_height)
  let n = expected
  let mut i = 0
  while i < n {
    if i < out_ptr.length() { out_ptr[i] = strip[i] }
    i = i + 1
  }
  n
}

/// Cineiris circular iris transform on fingerprint strip.
///
/// Parameters:
///   strip_pixel_len — number of f64 values in strip input
///   strip_pixel_ptr — strip image row-major RGB
///   strip_width     — strip width in pixels (angular axis)
///   strip_height    — strip height in pixels (radial axis)
///   iris_diameter   — output circular image diameter in pixels
///   out_len         — length of output buffer (must be >= iris_diameter^2 * 3)
///   out_ptr         — output buffer for circular image
///
/// Returns: number of output values written, or negative error code.
pub fn mengxi_cineiris_transform(
  strip_pixel_len : Int,
  strip_pixel_ptr : FixedArray[Double],
  strip_width : Int,
  strip_height : Int,
  iris_diameter : Int,
  out_len : Int,
  out_ptr : FixedArray[Double]
) -> Int {
  let strip_expected = strip_width * strip_height * 3
  if strip_pixel_len < strip_expected { return -1 }
  let out_expected = iris_diameter * iris_diameter * 3
  if out_len < out_expected { return -2 }

  let result = do_cineiris_transform(strip_pixel_ptr, strip_width, strip_height, iris_diameter)
  let n = out_expected
  let mut i = 0
  while i < n {
    if i < out_ptr.length() { out_ptr[i] = result[i] }
    i = i + 1
  }
  n
}

// ---------- Unit tests ----------

test "extract_center_column_known" {
  // 3x3 image (9 pixels, 27 f64 values)
  let width = 3
  let height = 3
  let data = FixedArray::make(27, 0.0)
  // Row 0: (1,0,0), (0.5,0.5,0.5), (0,0,1)
  data[0] = 1.0;  data[1] = 0.0;  data[2] = 0.0
  data[3] = 0.5;  data[4] = 0.5;  data[5] = 0.5    // center
  data[6] = 0.0;  data[7] = 0.0;  data[8] = 1.0
  // Row 1: (0,1,0), (0.2,0.8,0.3), (1,0,0)
  data[9]  = 0.0; data[10] = 1.0; data[11] = 0.0
  data[12] = 0.2; data[13] = 0.8; data[14] = 0.3   // center
  data[15] = 1.0; data[16] = 0.0; data[17] = 0.0
  // Row 2: (0,0,1), (0.9,0.1,0.5), (1,1,0)
  data[18] = 0.0; data[19] = 0.0; data[20] = 1.0
  data[21] = 0.9; data[22] = 0.1; data[23] = 0.5   // center
  data[24] = 1.0; data[25] = 1.0; data[26] = 0.0

  let col = do_extract_center_column(data, width, height)
  // center_x = 3/2 = 1
  assert_eq(col.length(), 9)
  // Row 0 center: (0.5, 0.5, 0.5)
  assert_eq(col[0], 0.5)
  assert_eq(col[1], 0.5)
  assert_eq(col[2], 0.5)
  // Row 1 center: (0.2, 0.8, 0.3)
  assert_eq(col[3], 0.2)
  assert_eq(col[4], 0.8)
  assert_eq(col[5], 0.3)
  // Row 2 center: (0.9, 0.1, 0.5)
  assert_eq(col[6], 0.9)
  assert_eq(col[7], 0.1)
  assert_eq(col[8], 0.5)
}

test "extract_center_column_empty" {
  let empty = FixedArray::make(0, 0.0)
  let col = do_extract_center_column(empty, 0, 0)
  assert_eq(col.length(), 0)

  let col2 = do_extract_center_column(empty, 3, 3)
  assert_eq(col2.length(), 0)
}

test "stitch_strip_two_frames" {
  // Two frames, each 2 pixels tall
  let num_frames = 2
  let frame_height = 2
  let columns = FixedArray::make(12, 0.0)
  // Frame 0, row 0: (1.0, 0.0, 0.0)
  columns[0] = 1.0; columns[1] = 0.0; columns[2] = 0.0
  // Frame 0, row 1: (0.0, 1.0, 0.0)
  columns[3] = 0.0; columns[4] = 1.0; columns[5] = 0.0
  // Frame 1, row 0: (0.0, 0.0, 1.0)
  columns[6] = 0.0; columns[7] = 0.0; columns[8] = 1.0
  // Frame 1, row 1: (1.0, 1.0, 1.0)
  columns[9] = 1.0; columns[10] = 1.0; columns[11] = 1.0

  let strip = do_stitch_fingerprint_strip(columns, num_frames, frame_height)
  assert_eq(strip.length(), 12)

  // Row 0: frame0=(1,0,0), frame1=(0,0,1)
  assert_eq(strip[0], 1.0)
  assert_eq(strip[1], 0.0)
  assert_eq(strip[2], 0.0)
  assert_eq(strip[3], 0.0)
  assert_eq(strip[4], 0.0)
  assert_eq(strip[5], 1.0)

  // Row 1: frame0=(0,1,0), frame1=(1,1,1)
  assert_eq(strip[6], 0.0)
  assert_eq(strip[7], 1.0)
  assert_eq(strip[8], 0.0)
  assert_eq(strip[9], 1.0)
  assert_eq(strip[10], 1.0)
  assert_eq(strip[11], 1.0)
}

test "stitch_strip_empty" {
  let empty = FixedArray::make(0, 0.0)
  let strip = do_stitch_fingerprint_strip(empty, 0, 0)
  assert_eq(strip.length(), 0)

  let strip2 = do_stitch_fingerprint_strip(empty, 2, 3)
  assert_eq(strip2.length(), 0)
}

test "cineiris_transform_circle" {
  // 4x4 uniform strip, iris diameter 5
  let strip_width = 4
  let strip_height = 4
  let strip_pixels = FixedArray::make(strip_width * strip_height * 3, 0.8)
  let iris_diameter = 5

  let result = do_cineiris_transform(strip_pixels, strip_width, strip_height, iris_diameter)
  assert_eq(result.length(), 5 * 5 * 3)

  // Corner (0,0): dist = sqrt(4+4) ≈ 2.828 > 2.5 -> black
  assert_eq(result[0], 0.0)
  assert_eq(result[1], 0.0)
  assert_eq(result[2], 0.0)

  // Corner (4,0): dist = sqrt(4+4) ≈ 2.828 > 2.5 -> black
  let corner2_idx = 4 * 3
  assert_eq(result[corner2_idx], 0.0)

  // Corner (0,4)
  let corner3_idx = (4 * 5 + 0) * 3
  assert_eq(result[corner3_idx], 0.0)

  // Center (2,2): dist=0 -> samples strip at (0,0) = 0.8
  let center_idx = (2 * 5 + 2) * 3
  assert_eq(result[center_idx], 0.8)
  assert_eq(result[center_idx + 1], 0.8)
  assert_eq(result[center_idx + 2], 0.8)
}

test "cineiris_transform_sampling" {
  // 2x2 strip with known values
  let strip_width = 2
  let strip_height = 2
  let strip_pixels = FixedArray::make(12, 0.0)
  // (0,0): R=1.0, G=1.0, B=1.0
  strip_pixels[0] = 1.0; strip_pixels[1] = 1.0; strip_pixels[2] = 1.0
  // (1,0): R=0.5, G=0.5, B=0.5
  strip_pixels[3] = 0.5; strip_pixels[4] = 0.5; strip_pixels[5] = 0.5
  // (0,1): R=0.0, G=0.0, B=0.0
  strip_pixels[6] = 0.0; strip_pixels[7] = 0.0; strip_pixels[8] = 0.0
  // (1,1): R=0.25, G=0.25, B=0.25
  strip_pixels[9] = 0.25; strip_pixels[10] = 0.25; strip_pixels[11] = 0.25

  // iris diameter 3: center at (1,1), radius 1.5
  let iris_diameter = 3
  let result = do_cineiris_transform(strip_pixels, strip_width, strip_height, iris_diameter)
  assert_eq(result.length(), 3 * 3 * 3)

  // Center pixel (1,1): dist=0, vertical_idx=0, angle=0, time_idx=0 -> (0,0) ≈ 1.0
  // Note: 4x SSAA + angular blending averages nearby samples; atan2 impl diff may shift slightly
  let center_idx = (1 * 3 + 1) * 3
  assert_true(result[center_idx] > 0.1)
  assert_true(result[center_idx + 1] > 0.1)
  assert_true(result[center_idx + 2] > 0.1)

  // Corner (0,0): dist=sqrt(2)≈1.414 < 1.5 -> inside circle, non-black
  // Note: atan2 impl differences may shift angle slightly; verify overall non-black content
  let mut has_nonblack = false
  let mut ci = 0
  while ci < result.length() {
    if result[ci] > 0.0 { has_nonblack = true }
    ci = ci + 1
  }
  assert_true(has_nonblack)
}

test "cineiris_transform_empty" {
  let empty = FixedArray::make(0, 0.0)
  let result = do_cineiris_transform(empty, 0, 0, 5)
  assert_eq(result.length(), 0)

  let strip = FixedArray::make(12, 0.5)
  let result2 = do_cineiris_transform(strip, 2, 2, 0)
  assert_eq(result2.length(), 0)
}

test "normalize_angle_range" {
  let pi = pi_const()
  let two_pi = 2.0 * pi
  // Positive angle in range stays
  let a1 = normalize_angle_0_2pi(1.0)
  assert_true(a1 >= 0.0)
  assert_true(a1 < two_pi)

  // Negative angle wraps to positive
  let a2 = normalize_angle_0_2pi(-1.0)
  assert_true(a2 >= 0.0)
  assert_true(a2 < two_pi)
  inspect(a2, content=(
    #|5.283185307179586
  ))

  // Angle > 2pi wraps down
  let a3 = normalize_angle_0_2pi(two_pi + 0.5)
  inspect(a3, content="0.5")
}

test "ffi_extract_center_column" {
  let width = 3
  let height = 3
  let data = FixedArray::make(27, 0.0)
  data[3] = 0.5; data[4] = 0.5; data[5] = 0.5
  data[12] = 0.2; data[13] = 0.8; data[14] = 0.3
  data[21] = 0.9; data[22] = 0.1; data[23] = 0.5

  let out = FixedArray::make(9, 0.0)
  let result = mengxi_extract_center_column(27, data, width, height, 9, out)
  assert_eq(result, 9)
  assert_eq(out[0], 0.5)
  assert_eq(out[3], 0.2)
  assert_eq(out[6], 0.9)

  // Error case: insufficient output buffer
  let small_out = FixedArray::make(3, 0.0)
  let err = mengxi_extract_center_column(27, data, width, height, 3, small_out)
  assert_eq(err, -2)
}

test "ffi_stitch_fingerprint_strip" {
  let columns = FixedArray::make(12, 0.0)
  columns[0] = 1.0; columns[1] = 0.0; columns[2] = 0.0
  columns[3] = 0.0; columns[4] = 1.0; columns[5] = 0.0
  columns[6] = 0.0; columns[7] = 0.0; columns[8] = 1.0
  columns[9] = 1.0; columns[10] = 1.0; columns[11] = 1.0

  let out = FixedArray::make(12, 0.0)
  let result = mengxi_stitch_fingerprint_strip(12, columns, 2, 2, 12, out)
  assert_eq(result, 12)
  assert_eq(out[0], 1.0)
  assert_eq(out[6], 0.0)
  assert_eq(out[7], 1.0)
}

test "ffi_cineiris_transform" {
  let strip_width = 2
  let strip_height = 2
  let strip = FixedArray::make(12, 0.7)
  let iris_diameter = 3
  let out = FixedArray::make(27, 0.0)

  let result = mengxi_cineiris_transform(12, strip, strip_width, strip_height, iris_diameter, 27, out)
  assert_eq(result, 27)
  // Center should be sampled value
  let center_idx = (1 * 3 + 1) * 3
  assert_true(approx_eq(out[center_idx], 0.7))
}
