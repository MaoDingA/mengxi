use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use std::fmt;
use std::io::{self, Cursor, Write};
use std::path::Path;

/// DPX endianness determined by magic number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DpxEndian {
    Big,
    Little,
}

/// Summary of DPX file metadata — only the fields Mengxi needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DpxHeader {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub transfer: u8,
    pub colorimetric: u8,
    pub descriptor: u8,
    pub packing: u16,
    pub encoding: u16,
    pub data_offset: u32,
    pub endianness: DpxEndian,
}

/// Errors returned by DPX parsing.
#[derive(Debug)]
pub enum DpxError {
    InvalidMagic([u8; 4]),
    TruncatedFile(String),
    UnsupportedVariant(String),
    IoError(String),
}

impl fmt::Display for DpxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DpxError::InvalidMagic(bytes) => {
                write!(f, "Invalid DPX magic number: {:02X} {:02X} {:02X} {:02X}", bytes[0], bytes[1], bytes[2], bytes[3])
            }
            DpxError::TruncatedFile(reason) => write!(f, "Truncated DPX file: {}", reason),
            DpxError::UnsupportedVariant(reason) => write!(f, "Unsupported DPX variant: {}", reason),
            DpxError::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for DpxError {}

impl From<io::Error> for DpxError {
    fn from(e: io::Error) -> Self {
        DpxError::IoError(e.to_string())
    }
}

const DPX_HEADER_SIZE: usize = 2048;
const MAGIC_BE: u32 = 0x53445058; // "SDPX"
const MAGIC_LE: u32 = 0x58504453; // "XPDS"

/// Parse a DPX file header and extract summary metadata.
pub fn parse_dpx_header(path: &Path) -> Result<DpxHeader, DpxError> {
    let data = std::fs::read(path).map_err(|e| DpxError::IoError(e.to_string()))?;

    if data.len() < DPX_HEADER_SIZE {
        return Err(DpxError::TruncatedFile(format!(
            "file is {} bytes, need at least {}",
            data.len(),
            DPX_HEADER_SIZE
        )));
    }

    let slice = &data[..];
    let mut cursor = Cursor::new(slice);

    // Read magic number (first 4 bytes) to determine endianness
    let magic = cursor.read_u32::<BigEndian>()?;
    let endianness = match magic {
        MAGIC_BE => DpxEndian::Big,
        MAGIC_LE => DpxEndian::Little,
        other => {
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(&other.to_be_bytes());
            return Err(DpxError::InvalidMagic(bytes));
        }
    };

    // Parse image information header (starts at offset 768)
    // Skip generic file header (768 bytes) — seek to image info header
    cursor.set_position(768);

    let _image_orientation = read_u16(&mut cursor, endianness)?;
    let _number_of_elements = read_u16(&mut cursor, endianness)?;
    let pixels_per_line = read_u32(&mut cursor, endianness)?;
    let lines_per_element = read_u32(&mut cursor, endianness)?;

    // Validate dimensions
    if pixels_per_line == 0 || lines_per_element == 0 {
        return Err(DpxError::UnsupportedVariant(
            "zero dimensions".to_string(),
        ));
    }

    // Skip to first ImageElement (8 bytes into the image info header = offset 780)
    cursor.set_position(780);

    // Parse first ImageElement (72 bytes)
    let _data_sign = read_u32(&mut cursor, endianness)?;
    let _low_data = read_u32(&mut cursor, endianness)?;
    let _low_quantity = read_f32(&mut cursor, endianness)?;
    let _high_data = read_u32(&mut cursor, endianness)?;
    let _high_quantity = read_f32(&mut cursor, endianness)?;
    let descriptor = read_u8(&mut cursor)?;
    let transfer = read_u8(&mut cursor)?;
    let colorimetric = read_u8(&mut cursor)?;
    let bit_depth = read_u8(&mut cursor)?;
    let packing = read_u16(&mut cursor, endianness)?;
    let encoding = read_u16(&mut cursor, endianness)?;
    let data_offset = read_u32(&mut cursor, endianness)?;

    // Validate bit depth
    match bit_depth {
        1 | 8 | 10 | 12 | 16 | 32 | 64 => {}
        _ => {
            return Err(DpxError::UnsupportedVariant(format!(
                "unsupported bit depth: {}",
                bit_depth
            )))
        }
    }

    // Validate encoding (only support uncompressed for MVP)
    if encoding != 0 {
        return Err(DpxError::UnsupportedVariant(format!(
            "RLE encoding not supported (encoding={})",
            encoding
        )));
    }

    Ok(DpxHeader {
        width: pixels_per_line,
        height: lines_per_element,
        bit_depth,
        transfer,
        colorimetric,
        descriptor,
        packing,
        encoding,
        data_offset,
        endianness,
    })
}

/// Convert transfer characteristic code to human-readable string.
pub fn transfer_to_string(transfer: u8) -> &'static str {
    match transfer {
        0 => "user_defined",
        1 => "printing_density",
        2 => "linear",
        3 => "logarithmic",
        4 => "unspecified_video",
        5 => "smpte_274m",
        6 => "bt709",
        7 => "bt601_bg",
        8 => "bt601_m",
        9 => "ntsc_composite",
        10 => "pal_composite",
        _ => "unknown",
    }
}

/// Convert colorimetric code to human-readable string.
pub fn colorimetric_to_string(colorimetric: u8) -> &'static str {
    match colorimetric {
        0 => "user_defined",
        1 => "printing_density",
        4 => "unspecified_video",
        5 => "smpte_274m",
        6 => "bt709",
        7 => "bt601_bg",
        8 => "bt601_m",
        9 => "ntsc_composite",
        10 => "pal_composite",
        _ => "unknown",
    }
}

/// Convert descriptor code to human-readable string.
pub fn descriptor_to_string(descriptor: u8) -> &'static str {
    match descriptor {
        0 => "user_defined",
        1 => "red",
        2 => "green",
        3 => "blue",
        4 => "alpha",
        6 => "luminance",
        7 => "chrominance",
        8 => "depth",
        9 => "composite_video",
        50 => "rgb",
        51 => "rgba",
        52 => "abgr",
        100 => "cbYcrY_422",
        101 => "cbYaCrYa_4224",
        102 => "cbYcr_444",
        103 => "cbYcrA_4444",
        _ => "unknown",
    }
}

fn read_u16(cursor: &mut Cursor<&[u8]>, endian: DpxEndian) -> io::Result<u16> {
    match endian {
        DpxEndian::Big => cursor.read_u16::<BigEndian>(),
        DpxEndian::Little => cursor.read_u16::<LittleEndian>(),
    }
}

fn read_u32(cursor: &mut Cursor<&[u8]>, endian: DpxEndian) -> io::Result<u32> {
    match endian {
        DpxEndian::Big => cursor.read_u32::<BigEndian>(),
        DpxEndian::Little => cursor.read_u32::<LittleEndian>(),
    }
}

fn read_f32(cursor: &mut Cursor<&[u8]>, endian: DpxEndian) -> io::Result<f32> {
    match endian {
        DpxEndian::Big => cursor.read_f32::<BigEndian>(),
        DpxEndian::Little => cursor.read_f32::<LittleEndian>(),
    }
}

fn read_u8(cursor: &mut Cursor<&[u8]>) -> io::Result<u8> {
    cursor.read_u8()
}

fn write_u16(cursor: &mut Cursor<&mut [u8]>, val: u16, endian: DpxEndian) -> io::Result<()> {
    match endian {
        DpxEndian::Big => cursor.write_u16::<BigEndian>(val),
        DpxEndian::Little => cursor.write_u16::<LittleEndian>(val),
    }
}

fn write_u32(cursor: &mut Cursor<&mut [u8]>, val: u32, endian: DpxEndian) -> io::Result<()> {
    match endian {
        DpxEndian::Big => cursor.write_u32::<BigEndian>(val),
        DpxEndian::Little => cursor.write_u32::<LittleEndian>(val),
    }
}

fn write_f32(cursor: &mut Cursor<&mut [u8]>, val: f32, endian: DpxEndian) -> io::Result<()> {
    match endian {
        DpxEndian::Big => cursor.write_f32::<BigEndian>(val),
        DpxEndian::Little => cursor.write_f32::<LittleEndian>(val),
    }
}

/// Create a synthetic DPX file for testing. Writes a valid 2048-byte header
/// with the specified parameters followed by pixel data (all zeros for simplicity).
pub fn create_synthetic_dpx(
    path: &Path,
    width: u32,
    height: u32,
    bit_depth: u8,
    transfer: u8,
    endian: DpxEndian,
) -> io::Result<()> {
    let num_pixels = (width as usize) * (height as usize);
    let pixel_bytes = match bit_depth {
        8 => num_pixels * 3,
        10 => num_pixels * 4, // 3x10bit packed in 32-bit words
        16 => num_pixels * 6, // 3x16bit
        _ => num_pixels * 4,
    };
    let file_size = (DPX_HEADER_SIZE + pixel_bytes) as u32;
    let mut buf = vec![0u8; DPX_HEADER_SIZE + pixel_bytes];
    let mut cursor = Cursor::new(buf.as_mut_slice());

    // Generic File Header (768 bytes)
    // Magic number is always written as raw bytes (not endian-swapped)
    match endian {
        DpxEndian::Big => cursor.write_all(b"SDPX")?,
        DpxEndian::Little => cursor.write_all(b"XPDS")?,
    }
    write_u32(&mut cursor, DPX_HEADER_SIZE as u32, endian)?; // imageOffset
    cursor.write_all(b"V2.0\0\0\0\0")?; // version
    write_u32(&mut cursor, file_size, endian)?; // fileSize
    write_u32(&mut cursor, 1, endian)?; // dittoKey (1 = new)
    write_u32(&mut cursor, 1664, endian)?; // genericHeaderSize
    write_u32(&mut cursor, 384, endian)?; // industryHeaderSize
    write_u32(&mut cursor, 0, endian)?; // user data size

    // Skip to Image Information Header (offset 768)
    cursor.set_position(768);

    write_u16(&mut cursor, 0, endian)?; // imageOrientation
    write_u16(&mut cursor, 1, endian)?; // numberOfElements
    write_u32(&mut cursor, width, endian)?; // pixelsPerLine
    write_u32(&mut cursor, height, endian)?; // linesPerElement

    // Skip to first ImageElement (offset 780)
    cursor.set_position(780);

    write_u32(&mut cursor, 0, endian)?; // dataSign (unsigned)
    write_u32(&mut cursor, 0, endian)?; // lowData
    write_f32(&mut cursor, 0.0, endian)?; // lowQuantity

    let high_data = match bit_depth {
        8 => 255u32,
        10 => 1023,
        12 => 4095,
        16 => 65535,
        _ => 1023,
    };
    write_u32(&mut cursor, high_data, endian)?; // highData
    write_f32(&mut cursor, 1.0, endian)?; // highQuantity

    cursor.write_u8(50)?; // descriptor (RGB)
    cursor.write_u8(transfer)?; // transfer
    cursor.write_u8(if transfer == 1 || transfer == 3 { 1 } else { 6 })?; // colorimetric
    cursor.write_u8(bit_depth)?; // bitDepth
    write_u16(&mut cursor, 0, endian)?; // packing
    write_u16(&mut cursor, 0, endian)?; // encoding
    write_u32(&mut cursor, DPX_HEADER_SIZE as u32, endian)?; // dataOffset

    std::fs::write(path, &buf)?;
    Ok(())
}

/// Read pixel data from a DPX file, returning interleaved RGB f64 values normalized to [0.0, 1.0].
/// Supports 8-bit, 10-bit (packed, method A/B), and 16-bit depths.
pub fn read_pixel_data(path: &Path) -> Result<Vec<f64>, DpxError> {
    let data = std::fs::read(path).map_err(|e| DpxError::IoError(e.to_string()))?;
    let header = parse_dpx_header(path)?;

    let offset = header.data_offset as usize;
    if offset >= data.len() {
        return Err(DpxError::TruncatedFile("data offset beyond file size".to_string()));
    }

    let pixel_data = &data[offset..];
    let num_pixels = (header.width as usize) * (header.height as usize);

    match header.bit_depth {
        8 => read_pixels_8bit(pixel_data, num_pixels),
        10 => read_pixels_10bit(pixel_data, num_pixels),
        16 => read_pixels_16bit(pixel_data, num_pixels, header.endianness),
        other => Err(DpxError::UnsupportedVariant(format!(
            "pixel reading not supported for {}-bit depth", other
        ))),
    }
}

fn read_pixels_8bit(data: &[u8], num_pixels: usize) -> Result<Vec<f64>, DpxError> {
    let mut result = Vec::with_capacity(num_pixels * 3);
    let components = 3;
    let needed = num_pixels * components;
    if data.len() < needed {
        return Err(DpxError::TruncatedFile(format!(
            "need {} bytes, have {}", needed, data.len()
        )));
    }
    for &b in &data[..needed] {
        result.push(b as f64 / 255.0);
    }
    Ok(result)
}

fn read_pixels_10bit(data: &[u8], num_pixels: usize) -> Result<Vec<f64>, DpxError> {
    // Each RGB pixel = 3x10bit + 2bit padding = 1 32-bit word
    let needed_words = num_pixels;
    if data.len() < needed_words * 4 {
        return Err(DpxError::TruncatedFile(format!(
            "need {} bytes for {} 10-bit pixels, have {}",
            needed_words * 4, num_pixels, data.len()
        )));
    }
    let max_val = (1u32 << 10) - 1; // 1023
    let mut result = Vec::with_capacity(num_pixels * 3);
    for i in 0..num_pixels {
        let word = u32::from_le_bytes([data[i*4], data[i*4+1], data[i*4+2], data[i*4+3]]);
        // Method A (fill from LSB): [pad(2)][C3(10)][C2(10)][C1(10)]
        let c1 = ((word >> 20) & max_val) as f64 / max_val as f64;
        let c2 = ((word >> 10) & max_val) as f64 / max_val as f64;
        let c3 = (word & max_val) as f64 / max_val as f64;
        result.push(c1);
        result.push(c2);
        result.push(c3);
    }
    Ok(result)
}

fn read_pixels_16bit(data: &[u8], num_pixels: usize, endian: DpxEndian) -> Result<Vec<f64>, DpxError> {
    let components = 3;
    let needed = num_pixels * components * 2;
    if data.len() < needed {
        return Err(DpxError::TruncatedFile(format!(
            "need {} bytes for {} 16-bit pixels, have {}", needed, num_pixels, data.len()
        )));
    }
    let max_val = 65535.0;
    let mut result = Vec::with_capacity(num_pixels * 3);
    for i in 0..num_pixels {
        let base = i * components * 2;
        for c in 0..components {
            let val = match endian {
                DpxEndian::Big => u16::from_be_bytes([data[base + c*2], data[base + c*2 + 1]]),
                DpxEndian::Little => u16::from_le_bytes([data[base + c*2], data[base + c*2 + 1]]),
            };
            result.push(val as f64 / max_val);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_file(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        (dir, path)
    }

    #[test]
    fn test_parse_10bit_linear_big_endian() {
        let (_dir, path) = make_test_file("test_10bit_linear.dpx");
        create_synthetic_dpx(&path, 1920, 1080, 10, 2, DpxEndian::Big).unwrap();

        let header = parse_dpx_header(&path).unwrap();
        assert_eq!(header.width, 1920);
        assert_eq!(header.height, 1080);
        assert_eq!(header.bit_depth, 10);
        assert_eq!(header.transfer, 2);
        assert_eq!(header.endianness, DpxEndian::Big);
        assert_eq!(header.descriptor, 50); // RGB
        assert_eq!(header.encoding, 0);    // uncompressed
    }

    #[test]
    fn test_parse_16bit_big_endian() {
        let (_dir, path) = make_test_file("test_16bit.dpx");
        create_synthetic_dpx(&path, 4096, 2160, 16, 2, DpxEndian::Big).unwrap();

        let header = parse_dpx_header(&path).unwrap();
        assert_eq!(header.width, 4096);
        assert_eq!(header.height, 2160);
        assert_eq!(header.bit_depth, 16);
        assert_eq!(header.transfer, 2);
    }

    #[test]
    fn test_parse_8bit_big_endian() {
        let (_dir, path) = make_test_file("test_8bit.dpx");
        create_synthetic_dpx(&path, 1280, 720, 8, 6, DpxEndian::Big).unwrap();

        let header = parse_dpx_header(&path).unwrap();
        assert_eq!(header.bit_depth, 8);
        assert_eq!(header.transfer, 6); // BT.709
    }

    #[test]
    fn test_parse_12bit_log() {
        let (_dir, path) = make_test_file("test_12bit_log.dpx");
        create_synthetic_dpx(&path, 2048, 1080, 12, 1, DpxEndian::Big).unwrap();

        let header = parse_dpx_header(&path).unwrap();
        assert_eq!(header.bit_depth, 12);
        assert_eq!(header.transfer, 1); // Printing density
    }

    #[test]
    fn test_little_endian_xpds() {
        let (_dir, path) = make_test_file("test_le.dpx");
        create_synthetic_dpx(&path, 1920, 1080, 10, 2, DpxEndian::Little).unwrap();

        let header = parse_dpx_header(&path).unwrap();
        assert_eq!(header.endianness, DpxEndian::Little);
        assert_eq!(header.width, 1920);
        assert_eq!(header.height, 1080);
        assert_eq!(header.bit_depth, 10);
    }

    #[test]
    fn test_invalid_magic_number() {
        let (_dir, path) = make_test_file("test_invalid.dpx");
        // Write a 2048-byte file with invalid magic
        let mut data = vec![0u8; 2048];
        data[0] = b'G'; data[1] = b'A'; data[2] = b'R'; data[3] = b'B';
        std::fs::write(&path, &data).unwrap();

        let result = parse_dpx_header(&path);
        assert!(result.is_err());
        match result.unwrap_err() {
            DpxError::InvalidMagic(bytes) => {
                assert_eq!(&bytes, b"GARB");
            }
            other => panic!("Expected InvalidMagic, got: {:?}", other),
        }
    }

    #[test]
    fn test_truncated_file() {
        let (_dir, path) = make_test_file("test_truncated.dpx");
        let mut data = vec![0u8; 100];
        data[0] = 0x53; data[1] = 0x44; data[2] = 0x50; data[3] = 0x58;
        std::fs::write(&path, &data).unwrap();

        let result = parse_dpx_header(&path);
        assert!(result.is_err());
        match result.unwrap_err() {
            DpxError::TruncatedFile(msg) => {
                assert!(msg.contains("100 bytes"));
            }
            other => panic!("Expected TruncatedFile, got: {:?}", other),
        }
    }

    #[test]
    fn test_nonexistent_file() {
        let result = parse_dpx_header(Path::new("/nonexistent/file.dpx"));
        assert!(result.is_err());
        match result.unwrap_err() {
            DpxError::IoError(msg) => assert!(msg.contains("No such file")),
            other => panic!("Expected IoError, got: {:?}", other),
        }
    }

    #[test]
    fn test_transfer_to_string() {
        assert_eq!(transfer_to_string(0), "user_defined");
        assert_eq!(transfer_to_string(1), "printing_density");
        assert_eq!(transfer_to_string(2), "linear");
        assert_eq!(transfer_to_string(3), "logarithmic");
        assert_eq!(transfer_to_string(6), "bt709");
        assert_eq!(transfer_to_string(7), "bt601_bg");
        assert_eq!(transfer_to_string(8), "bt601_m");
        assert_eq!(transfer_to_string(255), "unknown");
    }

    #[test]
    fn test_descriptor_to_string() {
        assert_eq!(descriptor_to_string(50), "rgb");
        assert_eq!(descriptor_to_string(51), "rgba");
        assert_eq!(descriptor_to_string(6), "luminance");
        assert_eq!(descriptor_to_string(100), "cbYcrY_422");
        assert_eq!(descriptor_to_string(255), "unknown");
    }

    #[test]
    fn test_synthetic_file_roundtrip() {
        let (_dir, path) = make_test_file("roundtrip.dpx");
        create_synthetic_dpx(&path, 3840, 2160, 10, 3, DpxEndian::Big).unwrap();

        let header = parse_dpx_header(&path).unwrap();
        assert_eq!(header, DpxHeader {
            width: 3840,
            height: 2160,
            bit_depth: 10,
            transfer: 3,
            colorimetric: 1,
            descriptor: 50,
            packing: 0,
            encoding: 0,
            data_offset: DPX_HEADER_SIZE as u32,
            endianness: DpxEndian::Big,
        });
    }
}
