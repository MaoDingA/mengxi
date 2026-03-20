use std::path::Path;
use exr::prelude::WritableImage;

/// Summary of EXR file header metadata.
#[derive(Debug, Clone)]
pub struct ExrHeader {
    pub width: u32,
    pub height: u32,
    pub channels: Vec<String>,
    pub pixel_type: ExrPixelType,
    pub compression: ExrCompression,
}

/// EXR pixel type.
#[derive(Debug, Clone, PartialEq)]
pub enum ExrPixelType {
    Half, // 16-bit float
    Float, // 32-bit float
    Uint, // 32-bit unsigned int
}

/// EXR compression method.
#[derive(Debug, Clone, PartialEq)]
pub enum ExrCompression {
    None,
    Rle,
    Zips,
    Zip,
    Piz,
    Pxr24,
    B44,
    B44A,
}

impl ExrCompression {
    /// Display string for variant key (uppercase, e.g., "NONE", "PIZ").
    pub fn to_display(&self) -> &'static str {
        match self {
            ExrCompression::None => "NONE",
            ExrCompression::Rle => "RLE",
            ExrCompression::Zips => "ZIPS",
            ExrCompression::Zip => "ZIP",
            ExrCompression::Piz => "PIZ",
            ExrCompression::Pxr24 => "PXR24",
            ExrCompression::B44 => "B44",
            ExrCompression::B44A => "B44A",
        }
    }
}

/// EXR parsing error.
#[derive(Debug)]
pub enum ExrError {
    ParseError(String),
    NoLayers,
    IoError(String),
    UnsupportedVariant(String),
}

impl std::fmt::Display for ExrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExrError::ParseError(msg) => write!(f, "{}", msg),
            ExrError::NoLayers => write!(f, "EXR file contains no layers"),
            ExrError::IoError(msg) => write!(f, "IO error: {}", msg),
            ExrError::UnsupportedVariant(msg) => write!(f, "Unsupported EXR variant: {}", msg),
        }
    }
}

impl std::error::Error for ExrError {}

/// Parse an EXR file header, extracting metadata.
pub fn parse_exr_header(path: &Path) -> Result<ExrHeader, ExrError> {
    let image = exr::prelude::read_first_flat_layer_from_file(path)
        .map_err(|e: exr::error::Error| ExrError::ParseError(e.to_string()))?;

    let layer = &image.layer_data;

    let width = layer.size.width() as u32;
    let height = layer.size.height() as u32;

    let channels: Vec<String> = layer
        .channel_data
        .list
        .iter()
        .map(|ch| ch.name.to_string())
        .collect();

    let pixel_type = layer
        .channel_data
        .list
        .first()
        .map(|ch| sample_type_from_flat(&ch.sample_data))
        .unwrap_or(ExrPixelType::Half);

    let compression = map_compression(layer.encoding.compression);

    Ok(ExrHeader {
        width,
        height,
        channels,
        pixel_type,
        compression,
    })
}

fn sample_type_from_flat(samples: &exr::image::FlatSamples) -> ExrPixelType {
    match samples {
        exr::image::FlatSamples::F16(_) => ExrPixelType::Half,
        exr::image::FlatSamples::F32(_) => ExrPixelType::Float,
        exr::image::FlatSamples::U32(_) => ExrPixelType::Uint,
    }
}

fn map_compression(compression: exr::compression::Compression) -> ExrCompression {
    use exr::compression::Compression;
    match compression {
        Compression::Uncompressed => ExrCompression::None,
        Compression::RLE => ExrCompression::Rle,
        Compression::ZIP1 => ExrCompression::Zips,
        Compression::ZIP16 => ExrCompression::Zip,
        Compression::PIZ => ExrCompression::Piz,
        Compression::PXR24 => ExrCompression::Pxr24,
        Compression::B44 => ExrCompression::B44,
        Compression::B44A => ExrCompression::B44A,
        _ => ExrCompression::None,
    }
}

/// Convert compression to human-readable string for database storage.
pub fn compression_to_db_string(compression: &ExrCompression) -> &'static str {
    match compression {
        ExrCompression::None => "none",
        ExrCompression::Rle => "rle",
        ExrCompression::Zips => "zips",
        ExrCompression::Zip => "zip",
        ExrCompression::Piz => "piz",
        ExrCompression::Pxr24 => "pxr24",
        ExrCompression::B44 => "b44",
        ExrCompression::B44A => "b44a",
    }
}

/// Convert pixel type to display string for variant key.
pub fn pixel_type_to_string(pixel_type: &ExrPixelType) -> &'static str {
    match pixel_type {
        ExrPixelType::Half => "half-float",
        ExrPixelType::Float => "float",
        ExrPixelType::Uint => "uint",
    }
}

/// Convert pixel type to bit depth.
pub fn pixel_type_to_bit_depth(pixel_type: &ExrPixelType) -> u32 {
    match pixel_type {
        ExrPixelType::Half => 16,
        ExrPixelType::Float => 32,
        ExrPixelType::Uint => 32,
    }
}

/// Map channel names to a descriptor string (e.g., "rgb", "rgba").
pub fn channels_to_descriptor(channels: &[String]) -> String {
    let sorted: Vec<&str> = channels.iter().map(|s| s.as_str()).collect();
    if sorted == ["B", "G", "R"] {
        "rgb".to_string()
    } else if sorted == ["A", "B", "G", "R"] {
        "rgba".to_string()
    } else {
        sorted.join(",")
    }
}

/// Read pixel data from an EXR file, returning interleaved RGB f64 values normalized to [0.0, 1.0].
/// Channels are reordered to RGB order (EXR stores B, G, R alphabetical).
pub fn read_pixel_data(path: &Path) -> Result<Vec<f64>, ExrError> {
    let image = exr::prelude::read_first_flat_layer_from_file(path)
        .map_err(|e: exr::error::Error| ExrError::ParseError(e.to_string()))?;

    let layer = &image.layer_data;
    let width = layer.size.width() as usize;
    let height = layer.size.height() as usize;
    let num_pixels = width * height;

    // Collect channels in sorted order (B, G, R, A...)
    let sorted_channels: Vec<_> = {
        let mut indexed: Vec<_> = layer.channel_data.list.iter().collect();
        indexed.sort_by_key(|ch| ch.name.as_slice());
        indexed
    };

    // Find RGB channels
    let r_idx = sorted_channels.iter().position(|ch| ch.name == *"R");
    let g_idx = sorted_channels.iter().position(|ch| ch.name == *"G");
    let b_idx = sorted_channels.iter().position(|ch| ch.name == *"B");

    match (r_idx, g_idx, b_idx) {
        (Some(ri), Some(gi), Some(bi)) => {
            let r_data = &sorted_channels[ri].sample_data;
            let g_data = &sorted_channels[gi].sample_data;
            let b_data = &sorted_channels[bi].sample_data;

            let mut result = Vec::with_capacity(num_pixels * 3);
            for p in 0..num_pixels {
                match (&r_data, &g_data, &b_data) {
                    (exr::image::FlatSamples::F16(r), exr::image::FlatSamples::F16(g), exr::image::FlatSamples::F16(b)) => {
                        result.push(r[p].to_f64());
                        result.push(g[p].to_f64());
                        result.push(b[p].to_f64());
                    }
                    (exr::image::FlatSamples::F32(r), exr::image::FlatSamples::F32(g), exr::image::FlatSamples::F32(b)) => {
                        result.push(r[p] as f64);
                        result.push(g[p] as f64);
                        result.push(b[p] as f64);
                    }
                    _ => return Err(ExrError::UnsupportedVariant("non-float RGB channels".to_string())),
                }
            }
            Ok(result)
        }
        _ => Err(ExrError::NoLayers),
    }
}

/// Create a synthetic EXR file for testing.
/// Uses HALF (f16) pixel type by default with the specified compression.
pub fn create_synthetic_exr(
    path: &Path,
    width: usize,
    height: usize,
    compression: exr::image::Encoding,
) -> Result<(), Box<dyn std::error::Error>> {
    let channels = exr::image::SpecificChannels::rgb(|_pos: exr::math::Vec2<usize>| {
        (
            exr::prelude::f16::from_f32(0.5),
            exr::prelude::f16::from_f32(0.25),
            exr::prelude::f16::from_f32(0.125),
        )
    });

    let image: exr::image::Image<exr::image::Layer<exr::image::SpecificChannels<_, _>>> =
        exr::image::Image::from_encoded_channels(
            exr::math::Vec2(width, height),
            compression,
            channels,
        );

    image.write().non_parallel().to_file(path)?;
    Ok(())
}

/// Create a synthetic EXR file with RGBA channels for testing.
pub fn create_synthetic_rgba_exr(
    path: &Path,
    width: usize,
    height: usize,
    compression: exr::image::Encoding,
) -> Result<(), Box<dyn std::error::Error>> {
    let channels = exr::image::SpecificChannels::rgba(|_pos: exr::math::Vec2<usize>| {
        (
            exr::prelude::f16::from_f32(0.5),
            exr::prelude::f16::from_f32(0.25),
            exr::prelude::f16::from_f32(0.125),
            exr::prelude::f16::from_f32(1.0),
        )
    });

    let image: exr::image::Image<exr::image::Layer<exr::image::SpecificChannels<_, _>>> =
        exr::image::Image::from_encoded_channels(
            exr::math::Vec2(width, height),
            compression,
            channels,
        );

    image.write().non_parallel().to_file(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_test_exr(
        width: usize,
        height: usize,
        compression: exr::image::Encoding,
    ) -> (TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.exr");
        create_synthetic_exr(&path, width, height, compression).unwrap();
        (dir, path)
    }

    #[test]
    fn test_parse_half_float_uncompressed() {
        let (_dir, path) = make_test_exr(1920, 1080, exr::image::Encoding::UNCOMPRESSED);
        let header = parse_exr_header(&path).unwrap();

        assert_eq!(header.width, 1920);
        assert_eq!(header.height, 1080);
        assert_eq!(header.pixel_type, ExrPixelType::Half);
        assert_eq!(header.compression, ExrCompression::None);
        assert_eq!(header.channels, vec!["B", "G", "R"]); // sorted alphabetically
    }

    #[test]
    fn test_parse_half_float_rle() {
        let (_dir, path) = make_test_exr(640, 480, exr::image::Encoding::FAST_LOSSLESS);
        let header = parse_exr_header(&path).unwrap();

        assert_eq!(header.width, 640);
        assert_eq!(header.height, 480);
        assert_eq!(header.pixel_type, ExrPixelType::Half);
        assert_eq!(header.compression, ExrCompression::Rle);
    }

    #[test]
    fn test_parse_half_float_piz() {
        let (_dir, path) = make_test_exr(256, 256, exr::image::Encoding::SMALL_FAST_LOSSLESS);
        let header = parse_exr_header(&path).unwrap();

        assert_eq!(header.width, 256);
        assert_eq!(header.height, 256);
        assert_eq!(header.pixel_type, ExrPixelType::Half);
        assert_eq!(header.compression, ExrCompression::Piz);
    }

    #[test]
    fn test_parse_half_float_zip16() {
        let (_dir, path) = make_test_exr(128, 64, exr::image::Encoding::SMALL_LOSSLESS);
        let header = parse_exr_header(&path).unwrap();

        assert_eq!(header.pixel_type, ExrPixelType::Half);
        assert_eq!(header.compression, ExrCompression::Zip);
    }

    #[test]
    fn test_parse_rgba_channels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rgba.exr");
        create_synthetic_rgba_exr(&path, 100, 100, exr::image::Encoding::UNCOMPRESSED).unwrap();
        let header = parse_exr_header(&path).unwrap();

        assert_eq!(header.channels, vec!["A", "B", "G", "R"]); // sorted alphabetically
        assert_eq!(channels_to_descriptor(&header.channels), "rgba");
    }

    #[test]
    fn test_channels_to_descriptor() {
        assert_eq!(
            channels_to_descriptor(&["B".to_string(), "G".to_string(), "R".to_string()]),
            "rgb"
        );
        assert_eq!(
            channels_to_descriptor(&["A".to_string(), "B".to_string(), "G".to_string(), "R".to_string()]),
            "rgba"
        );
        assert_eq!(
            channels_to_descriptor(&["Y".to_string()]),
            "Y"
        );
    }

    #[test]
    fn test_compression_to_db_string() {
        assert_eq!(compression_to_db_string(&ExrCompression::None), "none");
        assert_eq!(compression_to_db_string(&ExrCompression::Rle), "rle");
        assert_eq!(compression_to_db_string(&ExrCompression::Zips), "zips");
        assert_eq!(compression_to_db_string(&ExrCompression::Zip), "zip");
        assert_eq!(compression_to_db_string(&ExrCompression::Piz), "piz");
        assert_eq!(compression_to_db_string(&ExrCompression::Pxr24), "pxr24");
        assert_eq!(compression_to_db_string(&ExrCompression::B44), "b44");
        assert_eq!(compression_to_db_string(&ExrCompression::B44A), "b44a");
    }

    #[test]
    fn test_pixel_type_helpers() {
        assert_eq!(pixel_type_to_string(&ExrPixelType::Half), "half-float");
        assert_eq!(pixel_type_to_string(&ExrPixelType::Float), "float");
        assert_eq!(pixel_type_to_string(&ExrPixelType::Uint), "uint");

        assert_eq!(pixel_type_to_bit_depth(&ExrPixelType::Half), 16);
        assert_eq!(pixel_type_to_bit_depth(&ExrPixelType::Float), 32);
        assert_eq!(pixel_type_to_bit_depth(&ExrPixelType::Uint), 32);
    }

    #[test]
    fn test_corrupt_truncated_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.exr");
        // Write 50 bytes of garbage — too small for any valid EXR header
        std::fs::write(&path, vec![0u8; 50]).unwrap();

        let result = parse_exr_header(&path);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(!msg.is_empty());
    }

    #[test]
    fn test_corrupt_garbage_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("garbage.exr");
        // Write 2048 bytes of non-EXR data
        std::fs::write(&path, vec![0xAB; 2048]).unwrap();

        let result = parse_exr_header(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_roundtrip_uncompressed() {
        let (_dir, path) = make_test_exr(64, 32, exr::image::Encoding::UNCOMPRESSED);
        let header = parse_exr_header(&path).unwrap();

        assert_eq!(header.width, 64);
        assert_eq!(header.height, 32);
        assert_eq!(header.compression, ExrCompression::None);
        assert_eq!(pixel_type_to_bit_depth(&header.pixel_type), 16);
        assert_eq!(channels_to_descriptor(&header.channels), "rgb");
    }
}
