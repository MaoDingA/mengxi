// movie_fingerprint.rs — Movie fingerprint generation via FFI
//
// Generates visual fingerprints from video frame sequences by extracting
// center columns, stitching them into a strip, and optionally applying
// a CineIris transform. Reads PPM frames, outputs PNG fingerprints.

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors from movie fingerprint operations.
#[derive(Debug, thiserror::Error)]
pub enum MovieFingerprintError {
    /// MoonBit FFI call failed.
    #[error("MOVIE_FINGERPRINT_FFI_ERROR -- {0}")]
    FfiError(String),
    /// I/O error reading frames or writing output.
    #[error("MOVIE_FINGERPRINT_IO_ERROR -- {0}")]
    IoError(#[from] std::io::Error),
    /// Invalid input parameters.
    #[error("MOVIE_FINGERPRINT_INVALID_INPUT -- {0}")]
    InvalidInput(String),
    /// PPM file parsing error.
    #[error("MOVIE_FINGERPRINT_PPM_PARSE_ERROR -- {0}")]
    PpmParseError(String),
    /// Visualization rendering error.
    #[error("MOVIE_FINGERPRINT_VIZ_ERROR -- {0}")]
    VizError(String),
}

type Result<T> = std::result::Result<T, MovieFingerprintError>;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

extern "C" {
    fn mengxi_extract_center_column(
        pixel_len: i32,
        pixels: *const f64,
        width: i32,
        height: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;

    fn mengxi_stitch_fingerprint_strip(
        columns_len: i32,
        columns: *const f64,
        num_frames: i32,
        frame_height: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;

    fn mengxi_cineiris_transform(
        strip_pixel_len: i32,
        strip_pixels: *const f64,
        strip_width: i32,
        strip_height: i32,
        iris_diameter: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

/// Extract the center column of pixels from an interleaved RGB image.
///
/// # Arguments
/// * `pixels` — Interleaved RGB values [R0,G0,B0, R1,G1,B1, ...]
/// * `width` — Image width in pixels
/// * `height` — Image height in pixels
///
/// # Returns
/// Center column as interleaved RGB values (height * 3 elements).
pub fn extract_center_column(
    pixels: &[f64],
    width: usize,
    height: usize,
) -> Result<Vec<f64>> {
    if width == 0 || height == 0 {
        return Err(MovieFingerprintError::InvalidInput(
            "width and height must be non-zero".to_string(),
        ));
    }
    let expected_input = width * height * 3;
    if pixels.len() != expected_input {
        return Err(MovieFingerprintError::InvalidInput(format!(
            "pixel data length {} does not match width {} * height {} * 3 = {}",
            pixels.len(),
            width,
            height,
            expected_input
        )));
    }

    let out_size = height * 3;
    let mut output = vec![0.0_f64; out_size];

    let result = unsafe {
        mengxi_extract_center_column(
            pixels.len() as i32,
            pixels.as_ptr(),
            width as i32,
            height as i32,
            out_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(MovieFingerprintError::FfiError(format!(
            "extract_center_column returned error code {}",
            result
        )));
    }

    Ok(output)
}

/// Extract a representative color column by averaging each row across the full width.
///
/// Instead of sampling a single pixel at the center (which is noisy and jarring),
/// this computes the **mean color of each horizontal row** across all pixels.
/// The result captures the frame's true color distribution (sky → ground gradient)
/// while eliminating single-pixel noise, producing a naturally smooth strip.
fn extract_center_column_rust(pixels: &[f64], width: usize, height: usize) -> Result<Vec<f64>> {
    if width == 0 || height == 0 {
        return Err(MovieFingerprintError::InvalidInput(
            "width and height must be non-zero".to_string(),
        ));
    }
    let inv_w = 1.0 / width as f64;
    let mut col = Vec::with_capacity(height * 3);
    for y in 0..height {
        let mut sum_r = 0.0f64;
        let mut sum_g = 0.0f64;
        let mut sum_b = 0.0f64;
        for x in 0..width {
            let src_idx = (y * width + x) * 3;
            sum_r += pixels[src_idx];
            sum_g += pixels[src_idx + 1];
            sum_b += pixels[src_idx + 2];
        }
        col.push(sum_r * inv_w);
        col.push(sum_g * inv_w);
        col.push(sum_b * inv_w);
    }
    Ok(col)
}

/// Maximum allowed CineIris diameter to prevent excessive memory allocation.
const MAX_CINEIRIS_DIAMETER: usize = 4096;

/// Stitch center columns from multiple frames into a single strip image.
///
/// # Arguments
/// * `columns` — Concatenated center columns, each `frame_height * 3` elements
/// * `num_frames` — Number of frames (columns)
/// * `frame_height` — Height of each frame in pixels
///
/// # Returns
/// Strip image as interleaved RGB values (num_frames * frame_height * 3 elements).
pub fn stitch_fingerprint_strip(
    columns: &[f64],
    num_frames: usize,
    frame_height: usize,
) -> Result<Vec<f64>> {
    if num_frames == 0 {
        return Err(MovieFingerprintError::InvalidInput(
            "num_frames must be non-zero".to_string(),
        ));
    }
    if frame_height == 0 {
        return Err(MovieFingerprintError::InvalidInput(
            "frame_height must be non-zero".to_string(),
        ));
    }
    let expected_input = num_frames * frame_height * 3;
    if columns.len() != expected_input {
        return Err(MovieFingerprintError::InvalidInput(format!(
            "columns length {} does not match num_frames {} * frame_height {} * 3 = {}",
            columns.len(),
            num_frames,
            frame_height,
            expected_input
        )));
    }

    let out_size = num_frames * frame_height * 3;
    let mut output = vec![0.0_f64; out_size];

    let result = unsafe {
        mengxi_stitch_fingerprint_strip(
            columns.len() as i32,
            columns.as_ptr(),
            num_frames as i32,
            frame_height as i32,
            out_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(MovieFingerprintError::FfiError(format!(
            "stitch_fingerprint_strip returned error code {}",
            result
        )));
    }

    Ok(output)
}

/// Apply CineIris transform to a fingerprint strip.
///
/// # Arguments
/// * `strip` — Interleaved RGB strip data
/// * `width` — Strip width in pixels (number of frames)
/// * `height` — Strip height in pixels
/// * `diameter` — Iris diameter for the transform
///
/// # Returns
/// Transformed strip as interleaved RGB values.
pub fn cineiris_transform(
    strip: &[f64],
    width: usize,
    height: usize,
    diameter: usize,
) -> Result<Vec<f64>> {
    if width == 0 || height == 0 {
        return Err(MovieFingerprintError::InvalidInput(
            "width and height must be non-zero".to_string(),
        ));
    }
    if diameter == 0 {
        return Err(MovieFingerprintError::InvalidInput(
            "diameter must be non-zero".to_string(),
        ));
    }
    let expected_input = width * height * 3;
    if strip.len() != expected_input {
        return Err(MovieFingerprintError::InvalidInput(format!(
            "strip length {} does not match width {} * height {} * 3 = {}",
            strip.len(),
            width,
            height,
            expected_input
        )));
    }

    let out_size = diameter * diameter * 3;
    let mut output = vec![0.0_f64; out_size];

    let result = unsafe {
        mengxi_cineiris_transform(
            strip.len() as i32,
            strip.as_ptr(),
            width as i32,
            height as i32,
            diameter as i32,
            out_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(MovieFingerprintError::FfiError(format!(
            "cineiris_transform returned error code {}",
            result
        )));
    }

    Ok(output)
}

// ---------------------------------------------------------------------------
// PPM reader
// ---------------------------------------------------------------------------

/// Read a PPM P6 binary file and convert to interleaved RGB f64 [0.0, 1.0].
///
/// Returns (width, height, pixel_data).
fn read_ppm_as_rgb64(path: &Path) -> Result<(usize, usize, Vec<f64>)> {
    let data = std::fs::read(path).map_err(MovieFingerprintError::IoError)?;
    let mut pos = 0;

    // Helper to read next non-comment token
    let next_token = |data: &[u8], pos: &mut usize| -> Option<String> {
        let mut token = String::new();
        while *pos < data.len() {
            let b = data[*pos];
            if b == b'#' {
                // Skip comment until end of line
                while *pos < data.len() && data[*pos] != b'\n' {
                    *pos += 1;
                }
                if *pos < data.len() {
                    *pos += 1; // skip newline
                }
                continue;
            }
            if b.is_ascii_whitespace() {
                if !token.is_empty() {
                    return Some(token);
                }
                *pos += 1;
                continue;
            }
            token.push(b as char);
            *pos += 1;
        }
        if token.is_empty() {
            None
        } else {
            Some(token)
        }
    };

    // Read magic number
    let magic = next_token(&data, &mut pos).ok_or_else(|| {
        MovieFingerprintError::PpmParseError("missing magic number".to_string())
    })?;
    if magic != "P6" {
        return Err(MovieFingerprintError::PpmParseError(format!(
            "expected P6 format, got '{}'",
            magic
        )));
    }

    // Read width
    let width_str = next_token(&data, &mut pos).ok_or_else(|| {
        MovieFingerprintError::PpmParseError("missing width".to_string())
    })?;
    let width: usize = width_str.parse().map_err(|_| {
        MovieFingerprintError::PpmParseError(format!("invalid width: '{}'", width_str))
    })?;

    // Read height
    let height_str = next_token(&data, &mut pos).ok_or_else(|| {
        MovieFingerprintError::PpmParseError("missing height".to_string())
    })?;
    let height: usize = height_str.parse().map_err(|_| {
        MovieFingerprintError::PpmParseError(format!("invalid height: '{}'", height_str))
    })?;

    // Read maxval
    let maxval_str = next_token(&data, &mut pos).ok_or_else(|| {
        MovieFingerprintError::PpmParseError("missing maxval".to_string())
    })?;
    let maxval: usize = maxval_str.parse().map_err(|_| {
        MovieFingerprintError::PpmParseError(format!("invalid maxval: '{}'", maxval_str))
    })?;
    if maxval == 0 || maxval > 65535 {
        return Err(MovieFingerprintError::PpmParseError(format!(
            "maxval must be 1-65535, got {}",
            maxval
        )));
    }

    // After maxval token, exactly one whitespace character before binary data
    if pos >= data.len() {
        return Err(MovieFingerprintError::PpmParseError(
            "unexpected end after maxval".to_string(),
        ));
    }
    pos += 1; // skip single whitespace separator

    let bytes_per_sample = if maxval > 255 { 2 } else { 1 };
    let pixel_count = width * height;
    let expected_bytes = pixel_count * 3 * bytes_per_sample;

    if data.len() - pos < expected_bytes {
        return Err(MovieFingerprintError::PpmParseError(format!(
            "insufficient pixel data: expected {} bytes, got {}",
            expected_bytes,
            data.len() - pos
        )));
    }

    let maxval_f = maxval as f64;
    let mut pixels = Vec::with_capacity(pixel_count * 3);

    if bytes_per_sample == 1 {
        for i in 0..pixel_count * 3 {
            let val = data[pos + i] as f64 / maxval_f;
            pixels.push(val);
        }
    } else {
        for i in 0..pixel_count * 3 {
            let offset = pos + i * 2;
            let val = ((data[offset] as u16) << 8 | data[offset + 1] as u16) as f64 / maxval_f;
            pixels.push(val);
        }
    }

    Ok((width, height, pixels))
}

// ---------------------------------------------------------------------------
// PNG output
// ---------------------------------------------------------------------------

/// Save f64 RGB data [0.0, 1.0] as an RGB8 PNG file.
///
/// Values are clamped to [0.0, 1.0] and converted to [0, 255].
pub fn save_fingerprint_png(
    data: &[f64],
    width: usize,
    height: usize,
    path: &Path,
) -> Result<()> {
    let expected_len = width * height * 3;
    if data.len() != expected_len {
        return Err(MovieFingerprintError::InvalidInput(format!(
            "data length {} does not match width {} * height {} * 3 = {}",
            data.len(),
            width,
            height,
            expected_len
        )));
    }

    let u8_data: Vec<u8> = data
        .iter()
        .map(|&v| {
            let clamped = v.clamp(0.0, 1.0);
            (clamped * 255.0).round() as u8
        })
        .collect();

    image::save_buffer(
        path,
        &u8_data,
        width as u32,
        height as u32,
        image::ExtendedColorType::Rgb8,
    )
    .map_err(|e| MovieFingerprintError::IoError(std::io::Error::other(
        format!("failed to save PNG: {}", e),
    )))?;

    Ok(())
}

/// Read a fingerprint strip PNG and convert to interleaved f64 [0.0, 1.0].
///
/// Returns `(width, height, pixel_data)`.
pub fn read_strip_png(path: &Path) -> Result<(usize, usize, Vec<f64>)> {
    let data = std::fs::read(path).map_err(MovieFingerprintError::IoError)?;
    let img = image::load_from_memory(&data)
        .map_err(|e| MovieFingerprintError::IoError(std::io::Error::other(
            format!("failed to decode image: {}", e),
        )))?;
    let rgb = img.to_rgb8();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    let pixels: Vec<f64> = rgb.iter().map(|&v| v as f64 / 255.0).collect();
    Ok((w, h, pixels))
}

// ---------------------------------------------------------------------------
// Pipeline types
// ---------------------------------------------------------------------------

/// Which fingerprint outputs to generate.
#[derive(Debug, Clone)]
pub enum FingerprintMode {
    /// Standard strip fingerprint only.
    Strip,
    /// CineIris transform only with the given iris diameter.
    CineIris { diameter: usize },
    /// Both strip and CineIris outputs.
    Both { diameter: usize },
    /// CinePrint timeline poster (vertical strip with frame thumbnails).
    CinePrint {
        thumbnails: usize,
        /// Watermark image path (None = no watermark)
        watermark_path: Option<String>,
        /// Watermark position: "left", "center", or "right"
        watermark_position: String,
        /// Whether to show EP episode label
        show_ep_label: bool,
    },
}

impl FingerprintMode {
    /// Return the CineIris diameter if applicable.
    pub fn diameter(&self) -> Option<usize> {
        match self {
            FingerprintMode::Strip
            | FingerprintMode::CinePrint { .. }
            | _ => None,
            FingerprintMode::CineIris { diameter } | FingerprintMode::Both { diameter } => {
                Some(*diameter)
            }
        }
    }
}

/// Output paths and metadata from fingerprint generation.
#[derive(Debug, Clone)]
pub struct FingerprintOutput {
    /// Path to the strip PNG, if generated.
    pub strip_path: Option<PathBuf>,
    /// Path to the CineIris PNG, if generated.
    pub cineiris_path: Option<PathBuf>,
    /// Path to the CinePrint PNG, if generated.
    pub cineprint_path: Option<PathBuf>,
    /// Number of frames processed.
    pub frame_count: usize,
}

// ---------------------------------------------------------------------------
// Pipeline function
// ---------------------------------------------------------------------------

/// Generate movie fingerprint(s) from a sequence of PPM frame files.
///
/// # Arguments
/// * `frame_paths` — Ordered list of PPM P6 frame file paths.
/// * `output_dir` — Directory where output PNGs are written.
/// * `mode` — Which fingerprint outputs to generate.
///
/// # Pipeline
/// 1. Read each PPM frame and extract the center column (pure Rust).
/// 2. Stitch all center columns into a strip image via FFI.
/// 3. Optionally apply CineIris transform via FFI.
/// 4. Save PNG outputs to `output_dir`.
pub fn generate_fingerprint(
    frame_paths: &[PathBuf],
    output_dir: &Path,
    mode: &FingerprintMode,
    video_name: Option<&str>,
) -> Result<FingerprintOutput> {
    if frame_paths.is_empty() {
        return Err(MovieFingerprintError::InvalidInput(
            "no frame paths provided".to_string(),
        ));
    }

    // Validate diameter early to prevent excessive memory allocation
    if let Some(d) = mode.diameter() {
        if d == 0 {
            return Err(MovieFingerprintError::InvalidInput(
                "diameter must be non-zero".to_string(),
            ));
        }
        if d > MAX_CINEIRIS_DIAMETER {
            return Err(MovieFingerprintError::InvalidInput(format!(
                "diameter {} exceeds maximum {} (would allocate ~{:.1} GB)",
                d,
                MAX_CINEIRIS_DIAMETER,
                (d * d * 3 * 8) as f64 / 1e9
            )));
        }
    }

    std::fs::create_dir_all(output_dir)?;

    let mut center_columns: Vec<f64> = Vec::new();
    let mut frame_width: usize = 0;
    let mut frame_height: usize = 0;
    let mut frame_count: usize = 0;

    // For CinePrint mode: collect thumbnails and optional settings
    let n_thumbs = match mode {
        FingerprintMode::CinePrint { thumbnails, .. } => *thumbnails,
        _ => 0,
    };
    let thumb_interval = if n_thumbs > 0 {
        (frame_paths.len() / n_thumbs).max(1)
    } else {
        1
    };
    let mut thumbnails: Vec<crate::viz::cineprint::Thumbnail> = Vec::new();

    let mut skipped = 0usize;
    for (idx, frame_path) in frame_paths.iter().enumerate() {
        let (w, h, pixels) = match read_ppm_as_rgb64(frame_path) {
            Ok(v) => v,
            Err(e) => {
                eprintln!(
                    "Warning: skipping corrupt frame {} ({:?}): {}",
                    idx + 1,
                    frame_path.file_name(),
                    e
                );
                skipped += 1;
                continue;
            }
        };
        // Pure Rust center column extraction — avoids per-frame FFI overhead
        let col = extract_center_column_rust(&pixels, w, h)?;

        if frame_count == 0 {
            frame_width = w;
            frame_height = h;
        } else {
            if w != frame_width {
                return Err(MovieFingerprintError::InvalidInput(format!(
                    "frame {} has width {} but expected {} (all frames must have same dimensions)",
                    frame_count, w, frame_width
                )));
            }
            if h != frame_height {
                return Err(MovieFingerprintError::InvalidInput(format!(
                    "frame {} has height {} but expected {} (all frames must have same dimensions)",
                    frame_count, h, frame_height
                )));
            }
        }

        // Collect thumbnail if this is a selected frame
        if n_thumbs > 0 && thumb_interval > 0 && idx % thumb_interval == 0 && thumbnails.len() < n_thumbs {
            // Half-resolution thumbnails (960x540 for 1920x1080 source)
            let thumb_w = w / 2;
            let thumb_h = h / 2;
            let mut thumb_pixels = Vec::with_capacity(thumb_w * thumb_h * 3);
            for ty in 0..thumb_h {
                for tx in 0..thumb_w {
                    // Box filter: average 2x2 block
                    let sx = tx * 2;
                    let sy = ty * 2;
                    let src_idx00 = (sy * w + sx) * 3;
                    let src_idx10 = (sy * w + sx + 1) * 3;
                    let src_idx01 = ((sy + 1) * w + sx) * 3;
                    let src_idx11 = ((sy + 1) * w + sx + 1) * 3;
                    for ch in 0..3 {
                        let v = (pixels[src_idx00 + ch]
                               + pixels[src_idx10 + ch]
                               + pixels[src_idx01 + ch]
                               + pixels[src_idx11 + ch]) / 4.0;
                        thumb_pixels.push(v);
                    }
                }
            }
            thumbnails.push(crate::viz::cineprint::Thumbnail {
                width: thumb_w,
                height: thumb_h,
                pixels: thumb_pixels,
                frame_index: idx,
            });
        }

        center_columns.extend_from_slice(&col);
        frame_count += 1;
    }

    // Stitch center columns into strip
    let strip_data = stitch_fingerprint_strip(&center_columns, frame_count, frame_height)?;
    // Strip dimensions: width = frame_count, height = frame_height
    let strip_width = frame_count;

    let mut output = FingerprintOutput {
        strip_path: None,
        cineiris_path: None,
        cineprint_path: None,
        frame_count,
    };

    // Row-averaged strip data is already naturally smooth — no post-processing needed.
    // CineIris uses the same smooth data.

    // Build base filename from video name (fallback: "fingerprint")
    let video_base = video_name.unwrap_or("fingerprint");

    // Generate outputs based on mode
    match mode {
        FingerprintMode::Strip => {
            let path = output_dir.join(format!("{}_strip.png", video_base));
            save_fingerprint_png(&strip_data, strip_width, frame_height, &path)?;
            output.strip_path = Some(path);
        }
        FingerprintMode::CineIris { diameter } => {
            let transformed = cineiris_transform(&strip_data, strip_width, frame_height, *diameter)?;
            let path = output_dir.join(format!("{}_cineiris.png", video_base));
            save_fingerprint_png(&transformed, *diameter, *diameter, &path)?;
            output.cineiris_path = Some(path);
        }
        FingerprintMode::Both { diameter } => {
            let strip_path = output_dir.join(format!("{}_strip.png", video_base));
            save_fingerprint_png(&strip_data, strip_width, frame_height, &strip_path)?;
            output.strip_path = Some(strip_path);

            let transformed = cineiris_transform(&strip_data, strip_width, frame_height, *diameter)?;
            let cineiris_path = output_dir.join(format!("{}_cineiris.png", video_base));
            save_fingerprint_png(&transformed, *diameter, *diameter, &cineiris_path)?;
            output.cineiris_path = Some(cineiris_path);
        }
        FingerprintMode::CinePrint { thumbnails: _, watermark_path, watermark_position, show_ep_label } => {
            let path = output_dir.join(format!("{}_cineprint.png", video_base));
            let wm_path_ref: Option<&Path> = watermark_path.as_ref().map(|s| s.as_ref());
            crate::viz::cineprint::render_cineprint_png(
                &strip_data, strip_width, frame_height, &thumbnails, &path,
                video_name,
                wm_path_ref,
                &watermark_position,
                *show_ep_label,
            ).map_err(|e| MovieFingerprintError::VizError(e.to_string()))?;
            output.cineprint_path = Some(path);
        }
    }

    Ok(output)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Create a minimal valid P6 PPM file in the temp directory.
    fn write_test_ppm(dir: &Path, name: &str, width: usize, height: usize, rgb: &[u8]) -> PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "P6\n{} {}\n255\n", width, height).unwrap();
        f.write_all(rgb).unwrap();
        path
    }

    #[test]
    fn test_read_ppm_valid() {
        let dir = TempDir::new().unwrap();
        // 2x2 image: all pixels (R=255, G=128, B=0)
        let rgb: Vec<u8> = vec![255, 128, 0, 255, 128, 0, 255, 128, 0, 255, 128, 0];
        let path = write_test_ppm(dir.path(), "test.ppm", 2, 2, &rgb);

        let (w, h, pixels) = read_ppm_as_rgb64(&path).unwrap();
        assert_eq!(w, 2);
        assert_eq!(h, 2);
        assert_eq!(pixels.len(), 12);
        assert!((pixels[0] - 1.0).abs() < 1e-6, "R channel of first pixel");
        assert!((pixels[1] - (128.0 / 255.0)).abs() < 1e-6, "G channel of first pixel");
        assert!((pixels[2] - 0.0).abs() < 1e-6, "B channel of first pixel");
    }

    #[test]
    fn test_read_ppm_with_comments() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("comment.ppm");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            "P6\n# this is a comment\n2 1\n# another comment\n255\n"
        )
        .unwrap();
        f.write_all(&[255, 0, 0, 0, 255, 0]).unwrap();

        let (w, h, pixels) = read_ppm_as_rgb64(&path).unwrap();
        assert_eq!(w, 2);
        assert_eq!(h, 1);
        assert_eq!(pixels.len(), 6);
        assert!((pixels[0] - 1.0).abs() < 1e-6);
        assert!((pixels[4] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_read_ppm_wrong_magic() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.ppm");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(f, "P3\n2 2\n255\n").unwrap();

        let result = read_ppm_as_rgb64(&path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("MOVIE_FINGERPRINT_PPM_PARSE_ERROR"), "error: {}", err);
        assert!(err.contains("expected P6"), "error: {}", err);
    }

    #[test]
    fn test_ffi_extract_center_column() {
        // 3x3 image: center column is x=1
        // Row 0: (0,1,2) (3,4,5) (6,7,8)     → center: (3,4,5)
        // Row 1: (9,10,11) (12,13,14) (15,16,17) → center: (12,13,14)
        // Row 2: (18,19,20) (21,22,23) (24,25,26) → center: (21,22,23)
        let pixels: Vec<f64> = (0..27).map(|v| v as f64 / 26.0).collect();
        let result = extract_center_column(&pixels, 3, 3).unwrap();

        assert_eq!(result.len(), 9); // 3 rows * 3 channels
        // Center column values at normalized scale
        assert!((result[0] - 3.0 / 26.0).abs() < 1e-10);
        assert!((result[1] - 4.0 / 26.0).abs() < 1e-10);
        assert!((result[2] - 5.0 / 26.0).abs() < 1e-10);
        assert!((result[3] - 12.0 / 26.0).abs() < 1e-10);
        assert!((result[6] - 21.0 / 26.0).abs() < 1e-10);
    }

    #[test]
    fn test_ffi_extract_center_column_zero_dims() {
        let data = vec![0.0; 9];
        let result = extract_center_column(&data, 0, 3);
        assert!(result.is_err());
        let result = extract_center_column(&data, 3, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_ffi_extract_center_column_wrong_length() {
        let data = vec![0.0; 6]; // should be 3*3*3=27 for 3x3
        let result = extract_center_column(&data, 3, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_stitch_fingerprint_strip_zero_frames() {
        let data = vec![0.0; 9];
        let result = stitch_fingerprint_strip(&data, 0, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_cineiris_transform_zero_diameter() {
        let data = vec![0.5; 27]; // 3x3 image
        let result = cineiris_transform(&data, 3, 3, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_save_png() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test_out.png");

        // 2x2 image, all white
        let data = vec![1.0; 12];
        save_fingerprint_png(&data, 2, 2, &path).unwrap();

        assert!(path.exists(), "PNG file should exist");
        assert!(path.metadata().unwrap().len() > 0, "PNG file should not be empty");
    }

    #[test]
    fn test_save_png_clamping() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("clamp.png");

        // Values outside [0,1] should be clamped
        let data = vec![-0.5, 0.5, 1.5, 0.0, 2.0, -1.0];
        save_fingerprint_png(&data, 1, 2, &path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_save_png_wrong_length() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.png");
        let data = vec![0.5; 6]; // only 2 pixels, but claiming 2x2=4
        let result = save_fingerprint_png(&data, 2, 2, &path);
        assert!(result.is_err());
    }

    #[test]
    fn test_error_display() {
        let err = MovieFingerprintError::FfiError("code -1".to_string());
        assert!(err.to_string().contains("MOVIE_FINGERPRINT_FFI_ERROR"));

        let err = MovieFingerprintError::InvalidInput("bad params".to_string());
        assert!(err.to_string().contains("MOVIE_FINGERPRINT_INVALID_INPUT"));

        let err = MovieFingerprintError::PpmParseError("bad header".to_string());
        assert!(err.to_string().contains("MOVIE_FINGERPRINT_PPM_PARSE_ERROR"));
    }

    #[test]
    fn test_generate_fingerprint_empty_paths() {
        let dir = TempDir::new().unwrap();
        let result = generate_fingerprint(&[], dir.path(), &FingerprintMode::Strip, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no frame paths"));
    }

    #[test]
    fn test_generate_fingerprint_creates_output_dir() {
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("a").join("b").join("c");

        // Create a single 2x2 frame
        let rgb: Vec<u8> = vec![128; 12]; // 2x2 * 3 channels
        let frame_path = write_test_ppm(dir.path(), "frame.ppm", 2, 2, &rgb);

        let result = generate_fingerprint(
            &[frame_path],
            &nested,
            &FingerprintMode::Strip,
            Some("test_video"),
        );
        assert!(result.is_ok(), "generate_fingerprint should succeed: {:?}", result);
        let output = result.unwrap();
        assert_eq!(output.frame_count, 1);
        assert!(output.strip_path.is_some());
        assert!(output.strip_path.unwrap().exists());
    }
}
