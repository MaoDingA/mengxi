// lut.rs — Multi-format LUT file parser and serializer
// Supports: .cube, .3dl, .look, .csp, ASC-CDL

use std::fmt;
use std::io;
use std::path::Path;

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

/// Internal 3D LUT representation — format-agnostic.
///
/// Values are stored in red-fastest order:
/// index = r + grid_size * g + grid_size^2 * b
#[derive(Debug, Clone, PartialEq)]
pub struct LutData {
    pub title: Option<String>,
    pub grid_size: u32,
    pub domain_min: [f64; 3],
    pub domain_max: [f64; 3],
    /// grid_size^3 * 3 floats (interleaved RGB, red-fastest).
    pub values: Vec<f64>,
}

impl LutData {
    /// Create a new identity LUT of the given grid size.
    pub fn identity(grid_size: u32) -> Self {
        let n = grid_size as usize;
        let mut values = Vec::with_capacity(n * n * n * 3);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    values.push(r as f64 / (n - 1).max(1) as f64);
                    values.push(g as f64 / (n - 1).max(1) as f64);
                    values.push(b as f64 / (n - 1).max(1) as f64);
                }
            }
        }
        LutData {
            title: None,
            grid_size,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
            values,
        }
    }

    /// Validate the LUT data: check grid_size, value count, and domain range.
    pub fn validate(&self) -> Result<(), LutError> {
        if self.grid_size < 2 {
            return Err(LutError::InvalidGridSize(self.grid_size));
        }
        if self.grid_size > 256 {
            return Err(LutError::InvalidGridSize(self.grid_size));
        }
        let expected = self.grid_size as usize * self.grid_size as usize * self.grid_size as usize * 3;
        if self.values.len() != expected {
            return Err(LutError::InvalidValueCount {
                expected,
                actual: self.values.len(),
            });
        }
        for i in 0..3 {
            if self.domain_min[i] >= self.domain_max[i] {
                return Err(LutError::InvalidDomainRange);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Format enum
// ---------------------------------------------------------------------------

/// Supported LUT file formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LutFormat {
    Cube,
    ThreeDL,
    Look,
    Csp,
    AscCdl,
}

impl LutFormat {
    /// Detect format from file extension.
    pub fn from_extension(ext: &str) -> Result<Self, LutError> {
        match ext.to_lowercase().as_str() {
            "cube" => Ok(LutFormat::Cube),
            "3dl" => Ok(LutFormat::ThreeDL),
            "look" => Ok(LutFormat::Look),
            "csp" => Ok(LutFormat::Csp),
            "cdl" => Ok(LutFormat::AscCdl),
            _ => Err(LutError::UnsupportedFormat(ext.to_string())),
        }
    }

    /// File extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            LutFormat::Cube => "cube",
            LutFormat::ThreeDL => "3dl",
            LutFormat::Look => "look",
            LutFormat::Csp => "csp",
            LutFormat::AscCdl => "cdl",
        }
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors from LUT parsing and serialization.
#[derive(Debug)]
pub enum LutError {
    ParseError(String),
    UnsupportedFormat(String),
    InvalidGridSize(u32),
    InvalidDomainRange,
    InvalidValueCount { expected: usize, actual: usize },
    WriteError(String),
    UnsupportedPowerGradeVersion(String),
    IoError(String),
}

impl fmt::Display for LutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LutError::ParseError(msg) => write!(f, "LUT_PARSE_ERROR -- {}", msg),
            LutError::UnsupportedFormat(fmt) => {
                write!(f, "LUT_UNSUPPORTED_FORMAT -- unknown format: {}", fmt)
            }
            LutError::InvalidGridSize(n) => {
                write!(f, "LUT_INVALID_CUBE -- invalid grid size: {}", n)
            }
            LutError::InvalidDomainRange => write!(f, "LUT_PARSE_ERROR -- invalid domain range"),
            LutError::InvalidValueCount { expected, actual } => {
                write!(
                    f,
                    "LUT_PARSE_ERROR -- expected {} values, got {}",
                    expected, actual
                )
            }
            LutError::WriteError(msg) => write!(f, "LUT_WRITE_ERROR -- {}", msg),
            LutError::UnsupportedPowerGradeVersion(ver) => {
                write!(f, "LUT_UNSUPPORTED_FORMAT -- unsupported PowerGrade version: {}", ver)
            }
            LutError::IoError(msg) => write!(f, "LUT_IO_ERROR -- {}", msg),
        }
    }
}

impl std::error::Error for LutError {}

impl From<io::Error> for LutError {
    fn from(e: io::Error) -> Self {
        LutError::IoError(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// .cube format — parse
// ---------------------------------------------------------------------------

/// Parse a .cube LUT file.
pub fn parse_cube(path: &Path) -> Result<LutData, LutError> {
    let data = std::fs::read_to_string(path).map_err(|e| LutError::IoError(e.to_string()))?;
    parse_cube_from_str(&data)
}

/// Parse .cube LUT from a string.
fn parse_cube_from_str(content: &str) -> Result<LutData, LutError> {
    let mut title: Option<String> = None;
    let mut grid_size: Option<u32> = None;
    let mut domain_min: Option<[f64; 3]> = None;
    let mut domain_max: Option<[f64; 3]> = None;
    let mut values: Vec<f64> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse keywords
        if trimmed.starts_with("TITLE") {
            // TITLE "..."
            if let Some(start) = trimmed.find('"') {
                if let Some(end) = trimmed[start + 1..].find('"') {
                    title = Some(trimmed[start + 1..start + 1 + end].to_string());
                }
            }
            continue;
        }

        if trimmed.starts_with("LUT_1D_SIZE") {
            // Skip — we only support 3D LUTs
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("LUT_3D_SIZE") {
            let n: u32 = rest.trim().parse().map_err(|_| {
                LutError::ParseError(format!("invalid LUT_3D_SIZE: {}", rest.trim()))
            })?;
            if n < 2 || n > 256 {
                return Err(LutError::InvalidGridSize(n));
            }
            grid_size = Some(n);
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("DOMAIN_MIN") {
            let parts: Vec<f64> = rest
                .split_whitespace()
                .map(|s| s.parse::<f64>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| LutError::ParseError(format!("invalid DOMAIN_MIN: {}", e)))?;
            if parts.len() != 3 {
                return Err(LutError::ParseError("DOMAIN_MIN requires 3 values".to_string()));
            }
            domain_min = Some([parts[0], parts[1], parts[2]]);
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("DOMAIN_MAX") {
            let parts: Vec<f64> = rest
                .split_whitespace()
                .map(|s| s.parse::<f64>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| LutError::ParseError(format!("invalid DOMAIN_MAX: {}", e)))?;
            if parts.len() != 3 {
                return Err(LutError::ParseError("DOMAIN_MAX requires 3 values".to_string()));
            }
            domain_max = Some([parts[0], parts[1], parts[2]]);
            continue;
        }

        // Otherwise it's a data line — try to parse 3 floats
        // Detect transition from keywords to data: first token is a number
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 3 {
            let parsed: Result<Vec<f64>, _> = parts.iter().map(|s| s.parse::<f64>()).collect();
            if let Ok(floats) = parsed {
                values.extend_from_slice(&floats[..3]);
            }
        }
    }

    let grid_size = grid_size.ok_or_else(|| {
        LutError::ParseError("LUT_3D_SIZE not found in .cube file".to_string())
    })?;

    let expected = grid_size as usize * grid_size as usize * grid_size as usize * 3;
    if values.len() != expected {
        return Err(LutError::InvalidValueCount {
            expected,
            actual: values.len(),
        });
    }

    let domain_min = domain_min.unwrap_or([0.0, 0.0, 0.0]);
    let domain_max = domain_max.unwrap_or([1.0, 1.0, 1.0]);

    if domain_min[0] >= domain_max[0] || domain_min[1] >= domain_max[1] || domain_min[2] >= domain_max[2]
    {
        return Err(LutError::InvalidDomainRange);
    }

    Ok(LutData {
        title,
        grid_size,
        domain_min,
        domain_max,
        values,
    })
}

// ---------------------------------------------------------------------------
// .cube format — serialize
// ---------------------------------------------------------------------------

/// Serialize LUT data to .cube format string.
pub fn serialize_cube(data: &LutData) -> Result<String, LutError> {
    data.validate()?;

    let mut out = String::new();

    if let Some(ref t) = data.title {
        out.push_str(&format!("TITLE \"{}\"\n", t));
    }

    out.push_str(&format!(
        "DOMAIN_MIN {:.6} {:.6} {:.6}\n",
        data.domain_min[0], data.domain_min[1], data.domain_min[2]
    ));
    out.push_str(&format!(
        "DOMAIN_MAX {:.6} {:.6} {:.6}\n",
        data.domain_max[0], data.domain_max[1], data.domain_max[2]
    ));
    out.push_str(&format!("LUT_3D_SIZE {}\n", data.grid_size));

    for chunk in data.values.chunks(3) {
        out.push_str(&format!(
            "{:.6} {:.6} {:.6}\n",
            chunk[0], chunk[1], chunk[2]
        ));
    }

    Ok(out)
}

/// Write LUT data to a .cube file.
pub fn write_cube(data: &LutData, path: &Path) -> Result<(), LutError> {
    let content = serialize_cube(data)?;
    std::fs::write(path, content).map_err(|e| LutError::WriteError(e.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// .3dl format — parse
// ---------------------------------------------------------------------------

/// Parse a .3dl LUT file.
pub fn parse_3dl(path: &Path) -> Result<LutData, LutError> {
    let content = std::fs::read_to_string(path).map_err(|e| LutError::IoError(e.to_string()))?;
    parse_3dl_from_str(&content)
}

fn parse_3dl_from_str(content: &str) -> Result<LutData, LutError> {
    let mut shaper: Option<Vec<i32>> = None;
    let mut lut_values: Vec<i32> = Vec::new();
    let mut grid_size: Option<u32> = None;
    let mut output_bit_depth: Option<u32> = None;

    // Known keywords to skip
    let keywords = [
        "3DMESH", "Mesh", "LUT8", "LUT10", "LUT12", "LUT16", "gamma", "LUT12_16",
    ];

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('<') {
            continue;
        }

        if keywords.iter().any(|kw| trimmed.starts_with(kw)) {
            // Check for "Mesh N M" to get input/output bit depths
            if let Some(rest) = trimmed.strip_prefix("Mesh") {
                let parts: Vec<&str> = rest.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let (Ok(in_bits), Ok(out_bits)) = (
                        parts[0].parse::<u32>(),
                        parts[1].parse::<u32>(),
                    ) {
                        grid_size = Some((1 << in_bits) + 1);
                        output_bit_depth = Some(out_bits);
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("gamma") {
                // gamma value — skip
                let _ = rest.trim().parse::<f64>();
            }
            continue;
        }

        // Try to parse as integers
        let nums: Result<Vec<i32>, _> = trimmed
            .split_whitespace()
            .map(|s| s.parse::<i32>())
            .collect();

        if let Ok(nums) = nums {
            if nums.len() > 3 && shaper.is_none() {
                // This is the shaper LUT line (more than 3 values)
                shaper = Some(nums);
            } else if nums.len() == 3 {
                lut_values.push(nums[0]);
                lut_values.push(nums[1]);
                lut_values.push(nums[2]);
            }
        }
    }

    if lut_values.is_empty() {
        return Err(LutError::ParseError("no 3D LUT data found in .3dl file".to_string()));
    }

    // Determine grid size if not from Mesh keyword
    let grid_size = match grid_size {
        Some(n) => n,
        None => {
            let total_entries = lut_values.len() / 3;
            let cube_root = (total_entries as f64).cbrt().round() as u32;
            if cube_root * cube_root * cube_root != total_entries as u32 {
                return Err(LutError::ParseError(format!(
                    "cannot determine grid size from .3dl data: {} entries",
                    total_entries
                )));
            }
            cube_root
        }
    };

    // Infer bit depth from max value if not from Mesh keyword
    let max_val = *lut_values.iter().max().unwrap_or(&0) as u32;
    let max_code = match output_bit_depth {
        Some(bits) => (1u32 << bits) - 1,
        None => infer_3dl_bit_depth(max_val),
    };

    // Convert integer values to floats (in 0.0-1.0 range)
    let values: Vec<f64> = lut_values
        .iter()
        .map(|&v| v as f64 / max_code as f64)
        .collect();

    // .3dl stores in blue-fastest order — convert to red-fastest
    let values = convert_blue_fastest_to_red_fastest(&values, grid_size);

    Ok(LutData {
        title: None,
        grid_size,
        domain_min: [0.0, 0.0, 0.0],
        domain_max: [1.0, 1.0, 1.0],
        values,
    })
}

/// Infer bit depth from the maximum value in a .3dl file.
fn infer_3dl_bit_depth(max_val: u32) -> u32 {
    if max_val <= 511 {
        255 // 8-bit
    } else if max_val <= 2047 {
        1023 // 10-bit
    } else if max_val <= 8191 {
        4095 // 12-bit
    } else {
        65535 // 16-bit
    }
}

/// Convert data ordering from blue-fastest (.3dl) to red-fastest (.cube internal).
fn convert_blue_fastest_to_red_fastest(values: &[f64], grid_size: u32) -> Vec<f64> {
    let n = grid_size as usize;
    let mut out = vec![0.0f64; values.len()];
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let blue_fast_idx = (b * n * n + g * n + r) * 3;
                let red_fast_idx = (r + g * n + b * n * n) * 3;
                out[red_fast_idx] = values[blue_fast_idx];
                out[red_fast_idx + 1] = values[blue_fast_idx + 1];
                out[red_fast_idx + 2] = values[blue_fast_idx + 2];
            }
        }
    }
    out
}

/// Convert data ordering from red-fastest (internal) to blue-fastest (.3dl).
fn convert_red_fastest_to_blue_fastest(values: &[f64], grid_size: u32) -> Vec<f64> {
    let n = grid_size as usize;
    let mut out = vec![0.0f64; values.len()];
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let red_fast_idx = (r + g * n + b * n * n) * 3;
                let blue_fast_idx = (b * n * n + g * n + r) * 3;
                out[blue_fast_idx] = values[red_fast_idx];
                out[blue_fast_idx + 1] = values[red_fast_idx + 1];
                out[blue_fast_idx + 2] = values[red_fast_idx + 2];
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// .3dl format — serialize
// ---------------------------------------------------------------------------

/// Serialize LUT data to .3dl format string (12-bit output).
pub fn serialize_3dl(data: &LutData, bit_depth: u32) -> Result<String, LutError> {
    data.validate()?;

    let max_code = (1u32 << bit_depth) - 1;
    let shaper_count = data.grid_size;
    let mut shaper: Vec<i32> = Vec::with_capacity(shaper_count as usize);
    for i in 0..shaper_count {
        shaper.push((i as f64 / (shaper_count - 1).max(1) as f64 * max_code as f64).round() as i32);
    }

    // Convert from red-fastest to blue-fastest
    let blue_fast = convert_red_fastest_to_blue_fastest(&data.values, data.grid_size);

    let mut out = String::new();

    // Shaper LUT line
    let shaper_strs: Vec<String> = shaper.iter().map(|v| v.to_string()).collect();
    out.push_str(&shaper_strs.join(" "));
    out.push('\n');

    // 3D LUT data
    for chunk in blue_fast.chunks(3) {
        let r = (chunk[0].clamp(0.0, 1.0) * max_code as f64).round() as i32;
        let g = (chunk[1].clamp(0.0, 1.0) * max_code as f64).round() as i32;
        let b = (chunk[2].clamp(0.0, 1.0) * max_code as f64).round() as i32;
        out.push_str(&format!("{} {} {}\n", r, g, b));
    }

    Ok(out)
}

/// Write LUT data to a .3dl file.
pub fn write_3dl(data: &LutData, path: &Path, bit_depth: u32) -> Result<(), LutError> {
    let content = serialize_3dl(data, bit_depth)?;
    std::fs::write(path, content).map_err(|e| LutError::WriteError(e.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// .look format — parse (simplified IRIDAS/Lustre XML)
// ---------------------------------------------------------------------------

/// Parse a .look LUT file (simplified IRIDAS/Lustre XML format).
pub fn parse_look(path: &Path) -> Result<LutData, LutError> {
    let content = std::fs::read_to_string(path).map_err(|e| LutError::IoError(e.to_string()))?;
    parse_look_from_str(&content)
}

fn parse_look_from_str(content: &str) -> Result<LutData, LutError> {
    let mut title: Option<String> = None;
    let mut grid_size: Option<u32> = None;
    let mut values: Vec<f64> = Vec::new();

    // Simple XML parsing — no xml crate dependency needed
    // Look for <Title>...</Title>, <LUT3D ...> blocks
    if let Some(start) = content.find("<Title>") {
        if let Some(end) = content[start..].find("</Title>") {
            title = Some(content[start + 7..start + end].to_string());
        }
    }

    // Find LUT3D element — extract size attribute
    if let Some(start) = content.find("<LUT3D") {
        let tag_end = content[start..].find('>').unwrap_or(content.len() - start);
        let tag = &content[start..start + tag_end];
        if let Some(size_start) = tag.find("size=") {
            let rest = &tag[size_start + 5..];
            // Extract quoted or unquoted value
            let num_str = if rest.starts_with('"') {
                let end = rest[1..].find('"').unwrap_or(rest.len() - 1);
                &rest[1..1 + end]
            } else {
                let end = rest.find(|c: char| c.is_whitespace() || c == '>')
                    .unwrap_or(rest.len());
                &rest[..end]
            };
            grid_size = Some(
                num_str
                    .parse::<u32>()
                    .map_err(|_| LutError::ParseError(format!("invalid LUT3D size: {}", num_str)))?,
            );
        }
    }

    // Extract float values from between <LUT3D> and </LUT3D>
    if let Some(start) = content.find("<LUT3D") {
        if let Some(data_start) = content[start..].find('>') {
            let data_section = &content[start + data_start + 1..];
            if let Some(data_end) = data_section.find("</LUT3D>") {
                let data = &data_section[..data_end];
                for token in data.split_whitespace() {
                    if let Ok(v) = token.parse::<f64>() {
                        values.push(v);
                    }
                }
            }
        }
    }

    let grid_size = grid_size.unwrap_or_else(|| {
        // Infer from value count
        let entries = values.len() / 3;
        let root = (entries as f64).cbrt().round() as u32;
        root.max(2)
    });

    let expected = grid_size as usize * grid_size as usize * grid_size as usize * 3;
    if values.len() != expected {
        return Err(LutError::InvalidValueCount {
            expected,
            actual: values.len(),
        });
    }

    Ok(LutData {
        title,
        grid_size,
        domain_min: [0.0, 0.0, 0.0],
        domain_max: [1.0, 1.0, 1.0],
        values,
    })
}

// ---------------------------------------------------------------------------
// .look format — serialize
// ---------------------------------------------------------------------------

/// Serialize LUT data to .look XML format string.
pub fn serialize_look(data: &LutData) -> Result<String, LutError> {
    data.validate()?;

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<Look>\n");

    if let Some(ref t) = data.title {
        out.push_str(&format!("  <Title>{}</Title>\n", escape_xml(t)));
    }

    out.push_str(&format!("  <LUT3D size=\"{}\">\n", data.grid_size));
    for chunk in data.values.chunks(3) {
        out.push_str(&format!("    {:.6} {:.6} {:.6}\n", chunk[0], chunk[1], chunk[2]));
    }
    out.push_str("  </LUT3D>\n");
    out.push_str("</Look>\n");

    Ok(out)
}

/// Write LUT data to a .look file.
pub fn write_look(data: &LutData, path: &Path) -> Result<(), LutError> {
    let content = serialize_look(data)?;
    std::fs::write(path, content).map_err(|e| LutError::WriteError(e.to_string()))?;
    Ok(())
}

/// Escape XML special characters.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Unescape XML special characters.
#[allow(dead_code)]
fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

// ---------------------------------------------------------------------------
// .csp format — parse (CineSpace / Rising Sun Research)
// ---------------------------------------------------------------------------

/// Parse a .csp LUT file (CineSpace format).
pub fn parse_csp(path: &Path) -> Result<LutData, LutError> {
    let content = std::fs::read_to_string(path).map_err(|e| LutError::IoError(e.to_string()))?;
    parse_csp_from_str(&content)
}

fn parse_csp_from_str(content: &str) -> Result<LutData, LutError> {
    let mut lines = content.lines();

    // Line 1: magic header "CSPLUTV100"
    let magic = lines
        .next()
        .ok_or_else(|| LutError::ParseError("empty .csp file".to_string()))?
        .trim();
    if !magic.eq_ignore_ascii_case("CSPLUTV100") {
        return Err(LutError::ParseError(format!(
            "invalid .csp magic header: expected CSPLUTV100, got {}",
            magic
        )));
    }

    // Line 2: "3D" or "1D" — we only support 3D
    let lut_type = lines
        .next()
        .ok_or_else(|| LutError::ParseError("missing LUT type in .csp file".to_string()))?
        .trim();
    if lut_type != "3D" {
        return Err(LutError::ParseError(format!(
            "unsupported .csp LUT type: {} (only 3D supported)",
            lut_type
        )));
    }

    // Skip metadata block if present, also skip empty lines
    let remaining: Vec<String> = lines
        .filter(|l| !l.trim().is_empty())
        .map(String::from)
        .collect();
    let mut idx = 0;
    while idx < remaining.len() {
        let trimmed = remaining[idx].trim();
        if trimmed.eq_ignore_ascii_case("BEGIN METADATA") {
            idx += 1;
            while idx < remaining.len() {
                if remaining[idx].trim().eq_ignore_ascii_case("END METADATA") {
                    idx += 1;
                    break;
                }
                idx += 1;
            }
            continue;
        }
        break;
    }

    // Read 3 per-channel pre-LUTs (skip them — we store only the 3D LUT)
    for _ch in 0..3 {
        if idx >= remaining.len() {
            return Err(LutError::ParseError(
                "incomplete pre-LUT data in .csp file".to_string(),
            ));
        }
        let count: usize = remaining[idx]
            .trim()
            .parse()
            .map_err(|_| LutError::ParseError("invalid pre-LUT count".to_string()))?;
        idx += 1;
        // Skip input values — may span multiple lines
        let mut collected = 0;
        while collected < count && idx < remaining.len() {
            let tokens: Vec<&str> = remaining[idx].trim().split_whitespace().collect();
            collected += tokens.len();
            idx += 1;
        }
        if collected < count {
            return Err(LutError::ParseError(
                "incomplete pre-LUT input data in .csp file".to_string(),
            ));
        }
        // Skip output values — may span multiple lines
        collected = 0;
        while collected < count && idx < remaining.len() {
            let tokens: Vec<&str> = remaining[idx].trim().split_whitespace().collect();
            collected += tokens.len();
            idx += 1;
        }
        if collected < count {
            return Err(LutError::ParseError(
                "incomplete pre-LUT output data in .csp file".to_string(),
            ));
        }
    }

    // Read 3D LUT dimensions: "r_size g_size b_size"
    if idx >= remaining.len() {
        return Err(LutError::ParseError(
            "missing 3D LUT dimensions in .csp file".to_string(),
        ));
    }
    let dims: Vec<u32> = remaining[idx]
        .split_whitespace()
        .map(|s| s.parse::<u32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| LutError::ParseError("invalid 3D LUT dimensions".to_string()))?;
    idx += 1;

    if dims.len() != 3 || dims[0] != dims[1] || dims[1] != dims[2] {
        return Err(LutError::ParseError(
            "only uniform cube sizes supported in .csp".to_string(),
        ));
    }

    let grid_size = dims[0];
    let expected = grid_size as usize * grid_size as usize * grid_size as usize * 3;
    let mut values: Vec<f64> = Vec::with_capacity(expected);

    // Read RGB float data lines
    while idx < remaining.len() && values.len() < expected {
        let line = remaining[idx].trim();
        idx += 1;
        if line.is_empty() {
            continue;
        }
        for token in line.split_whitespace() {
            if let Ok(v) = token.parse::<f64>() {
                values.push(v);
            }
        }
    }

    if values.len() != expected {
        return Err(LutError::InvalidValueCount {
            expected,
            actual: values.len(),
        });
    }

    Ok(LutData {
        title: None,
        grid_size,
        domain_min: [0.0, 0.0, 0.0],
        domain_max: [1.0, 1.0, 1.0],
        values,
    })
}

// ---------------------------------------------------------------------------
// .csp format — serialize
// ---------------------------------------------------------------------------

/// Serialize LUT data to .csp format string.
pub fn serialize_csp(data: &LutData) -> Result<String, LutError> {
    data.validate()?;

    let mut out = String::from("CSPLUTV100\n3D\n\n");

    // Write identity pre-LUTs (3 channels)
    let pre_lut_size = data.grid_size.min(65536);
    for _ in 0..3 {
        out.push_str(&format!("{}\n", pre_lut_size));
        // Input values (linear ramp)
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for i in 0..pre_lut_size {
            let v = i as f64 / (pre_lut_size - 1).max(1) as f64;
            inputs.push(format!("{:.6}", v));
            outputs.push(format!("{:.6}", v));
        }
        out.push_str(&inputs.join(" "));
        out.push('\n');
        out.push_str(&outputs.join(" "));
        out.push('\n');
    }

    // Write 3D LUT dimensions
    out.push_str(&format!(
        "{} {} {}\n",
        data.grid_size, data.grid_size, data.grid_size
    ));

    // Write RGB values
    for chunk in data.values.chunks(3) {
        out.push_str(&format!("{:.6} {:.6} {:.6}\n", chunk[0], chunk[1], chunk[2]));
    }

    Ok(out)
}

/// Write LUT data to a .csp file.
pub fn write_csp(data: &LutData, path: &Path) -> Result<(), LutError> {
    let content = serialize_csp(data)?;
    std::fs::write(path, content).map_err(|e| LutError::WriteError(e.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// ASC-CDL format — parse
// ---------------------------------------------------------------------------

/// ASC-CDL color correction parameters (10 float values).
#[derive(Debug, Clone, PartialEq)]
pub struct CdlParams {
    pub slope: [f64; 3],
    pub offset: [f64; 3],
    pub power: [f64; 3],
    pub saturation: f64,
}

impl Default for CdlParams {
    fn default() -> Self {
        CdlParams {
            slope: [1.0, 1.0, 1.0],
            offset: [0.0, 0.0, 0.0],
            power: [1.0, 1.0, 1.0],
            saturation: 1.0,
        }
    }
}

impl CdlParams {
    /// Apply CDL to a single RGB triplet.
    pub fn apply(&self, r: f64, g: f64, b: f64) -> (f64, f64, f64) {
        let r = (r * self.slope[0] + self.offset[0]).max(0.0).powf(self.power[0]);
        let g = (g * self.slope[1] + self.offset[1]).max(0.0).powf(self.power[1]);
        let b = (b * self.slope[2] + self.offset[2]).max(0.0).powf(self.power[2]);
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let r = luma + self.saturation * (r - luma);
        let g = luma + self.saturation * (g - luma);
        let b = luma + self.saturation * (b - luma);
        (r, g, b)
    }

    /// Convert CDL params to a 3D LUT by sampling across the grid.
    pub fn to_lut_data(&self, grid_size: u32) -> LutData {
        let n = grid_size as usize;
        let mut values = Vec::with_capacity(n * n * n * 3);
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let in_r = r as f64 / (n - 1).max(1) as f64;
                    let in_g = g as f64 / (n - 1).max(1) as f64;
                    let in_b = b as f64 / (n - 1).max(1) as f64;
                    let (out_r, out_g, out_b) = self.apply(in_r, in_g, in_b);
                    values.push(out_r);
                    values.push(out_g);
                    values.push(out_b);
                }
            }
        }
        LutData {
            title: None,
            grid_size,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
            values,
        }
    }
}

/// Parse an ASC-CDL XML file.
pub fn parse_cdl(path: &Path) -> Result<CdlParams, LutError> {
    let content = std::fs::read_to_string(path).map_err(|e| LutError::IoError(e.to_string()))?;
    parse_cdl_from_str(&content)
}

fn parse_cdl_from_str(content: &str) -> Result<CdlParams, LutError> {
    let mut params = CdlParams::default();

    // Parse <SOPNode> — contains <Slope>, <Offset>, <Power>
    if let Some(sop_start) = content.find("<SOPNode>") {
        let sop_end = content[sop_start..]
            .find("</SOPNode>")
            .map(|e| sop_start + e + 10)
            .unwrap_or(content.len());
        let sop = &content[sop_start..sop_end];

        params.slope = parse_cdl_xyz(sop, "Slope")?;
        params.offset = parse_cdl_xyz(sop, "Offset")?;
        params.power = parse_cdl_xyz(sop, "Power")?;
    }

    // Parse <SatNode> — contains <Saturation>
    if let Some(sat_start) = content.find("<SatNode>") {
        let sat_end = content[sat_start..]
            .find("</SatNode>")
            .map(|e| sat_start + e + 9)
            .unwrap_or(content.len());
        let sat = &content[sat_start..sat_end];

        if let Some(val_start) = sat.find("<Saturation>") {
            let val_section = &sat[val_start..];
            if let Some(val_end) = val_section.find("</Saturation>") {
                params.saturation = val_section[12..val_end]
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| {
                        LutError::ParseError("invalid <Saturation> value in ASC-CDL".to_string())
                    })?;
            }
        }
    }

    Ok(params)
}

/// Parse a 3-value element like <Slope>1.0 1.0 1.0</Slope> within a section.
fn parse_cdl_xyz(section: &str, tag: &str) -> Result<[f64; 3], LutError> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    let start = section
        .find(&open_tag)
        .ok_or_else(|| {
            LutError::ParseError(format!("<{}> not found in ASC-CDL", tag))
        })? + open_tag.len();
    let end = section[start..]
        .find(&close_tag)
        .ok_or_else(|| {
            LutError::ParseError(format!("</{}> not found in ASC-CDL", tag))
        })?;

    let values: Vec<f64> = section[start..start + end]
        .split_whitespace()
        .map(|s| s.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| LutError::ParseError(format!("invalid {} values in ASC-CDL", tag)))?;

    if values.len() != 3 {
        return Err(LutError::ParseError(format!(
            "{} requires exactly 3 values",
            tag
        )));
    }

    Ok([values[0], values[1], values[2]])
}

// ---------------------------------------------------------------------------
// ASC-CDL format — serialize
// ---------------------------------------------------------------------------

/// Serialize CDL params to ASC-CDL XML format string.
pub fn serialize_cdl(params: &CdlParams) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<ColorDecision>\n");
    out.push_str("  <ColorCorrection id=\"mengxi-cdl\">\n");
    out.push_str("    <SOPNode>\n");
    out.push_str(&format!(
        "      <Slope>{:.6} {:.6} {:.6}</Slope>\n",
        params.slope[0], params.slope[1], params.slope[2]
    ));
    out.push_str(&format!(
        "      <Offset>{:.6} {:.6} {:.6}</Offset>\n",
        params.offset[0], params.offset[1], params.offset[2]
    ));
    out.push_str(&format!(
        "      <Power>{:.6} {:.6} {:.6}</Power>\n",
        params.power[0], params.power[1], params.power[2]
    ));
    out.push_str("    </SOPNode>\n");
    out.push_str("    <SatNode>\n");
    out.push_str(&format!(
        "      <Saturation>{:.6}</Saturation>\n",
        params.saturation
    ));
    out.push_str("    </SatNode>\n");
    out.push_str("  </ColorCorrection>\n");
    out.push_str("</ColorDecision>\n");
    out
}

/// Write CDL params to an ASC-CDL file.
pub fn write_cdl(params: &CdlParams, path: &Path) -> Result<(), LutError> {
    let content = serialize_cdl(params);
    std::fs::write(path, content).map_err(|e| LutError::WriteError(e.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Unified API
// ---------------------------------------------------------------------------

/// Parse a LUT file, auto-detecting format from file extension.
pub fn parse_lut(path: &Path) -> Result<LutData, LutError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| LutError::UnsupportedFormat("no file extension".to_string()))?;

    let format = LutFormat::from_extension(ext)?;
    parse_lut_with_format(path, format)
}

/// Parse a LUT file with explicit format.
pub fn parse_lut_with_format(path: &Path, format: LutFormat) -> Result<LutData, LutError> {
    match format {
        LutFormat::Cube => parse_cube(path),
        LutFormat::ThreeDL => parse_3dl(path),
        LutFormat::Look => parse_look(path),
        LutFormat::Csp => parse_csp(path),
        LutFormat::AscCdl => {
            let params = parse_cdl(path)?;
            Ok(params.to_lut_data(33)) // Default 33x33x33 grid
        }
    }
}

/// Parse a LUT from bytes with explicit format.
pub fn parse_lut_from_bytes(data: &[u8], format: LutFormat) -> Result<LutData, LutError> {
    let content = std::str::from_utf8(data)
        .map_err(|e| LutError::ParseError(format!("invalid UTF-8: {}", e)))?;
    match format {
        LutFormat::Cube => parse_cube_from_str(content),
        LutFormat::ThreeDL => parse_3dl_from_str(content),
        LutFormat::Look => parse_look_from_str(content),
        LutFormat::Csp => parse_csp_from_str(content),
        LutFormat::AscCdl => {
            let params = parse_cdl_from_str(content)?;
            Ok(params.to_lut_data(33))
        }
    }
}

/// Serialize a LUT to bytes with explicit format.
pub fn serialize_lut_to_bytes(data: &LutData, format: LutFormat) -> Result<Vec<u8>, LutError> {
    let content = match format {
        LutFormat::Cube => serialize_cube(data)?,
        LutFormat::ThreeDL => serialize_3dl(data, 12)?, // Default 12-bit
        LutFormat::Look => serialize_look(data)?,
        LutFormat::Csp => serialize_csp(data)?,
        LutFormat::AscCdl => {
            return Err(LutError::UnsupportedFormat(
                "ASC-CDL is parametric; use CdlParams directly".to_string(),
            ))
        }
    };
    Ok(content.into_bytes())
}

/// Serialize a LUT to a file, auto-detecting format from file extension.
pub fn serialize_lut(data: &LutData, path: &Path) -> Result<(), LutError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .ok_or_else(|| LutError::UnsupportedFormat("no file extension".to_string()))?;

    let format = LutFormat::from_extension(ext)?;
    match format {
        LutFormat::Cube => write_cube(data, path),
        LutFormat::ThreeDL => write_3dl(data, path, 12),
        LutFormat::Look => write_look(data, path),
        LutFormat::Csp => write_csp(data, path),
        LutFormat::AscCdl => Err(LutError::UnsupportedFormat(
            "ASC-CDL is parametric; use write_cdl() with CdlParams".to_string(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Synthetic test data generators
// ---------------------------------------------------------------------------

/// Create a synthetic .cube file string for testing.
pub fn create_synthetic_cube(grid_size: u32, title: Option<&str>) -> String {
    let lut = LutData {
        title: title.map(|s| s.to_string()),
        grid_size,
        domain_min: [0.0, 0.0, 0.0],
        domain_max: [1.0, 1.0, 1.0],
        values: {
            let n = grid_size as usize;
            let mut v = Vec::with_capacity(n * n * n * 3);
            for b in 0..n {
                for g in 0..n {
                    for r in 0..n {
                        v.push(r as f64 / (n - 1).max(1) as f64);
                        v.push(g as f64 / (n - 1).max(1) as f64);
                        v.push(b as f64 / (n - 1).max(1) as f64);
                    }
                }
            }
            v
        },
    };
    serialize_cube(&lut).unwrap()
}

/// Create a synthetic .3dl file string for testing.
pub fn create_synthetic_3dl(grid_size: u32) -> String {
    let lut = LutData::identity(grid_size);
    serialize_3dl(&lut, 12).unwrap()
}

/// Create a synthetic .look file string for testing.
pub fn create_synthetic_look(grid_size: u32, title: Option<&str>) -> String {
    let mut lut = LutData::identity(grid_size);
    lut.title = title.map(|s| s.to_string());
    serialize_look(&lut).unwrap()
}

/// Create a synthetic .csp file string for testing.
pub fn create_synthetic_csp(grid_size: u32) -> String {
    let lut = LutData::identity(grid_size);
    serialize_csp(&lut).unwrap()
}

/// Create a synthetic ASC-CDL file string for testing.
pub fn create_synthetic_cdl(params: &CdlParams) -> String {
    serialize_cdl(params)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use tempfile::NamedTempFile;

    // -- Task 1: Data model tests --

    #[test]
    fn test_lut_data_identity() {
        let lut = LutData::identity(2);
        assert_eq!(lut.grid_size, 2);
        assert_eq!(lut.values.len(), 24); // 2^3 * 3
        // First entry: r=0, g=0, b=0
        assert!((lut.values[0]).abs() < 1e-10);
        assert!((lut.values[1]).abs() < 1e-10);
        assert!((lut.values[2]).abs() < 1e-10);
        // Last entry: r=1, g=1, b=1
        assert!((lut.values[21] - 1.0).abs() < 1e-10);
        assert!((lut.values[22] - 1.0).abs() < 1e-10);
        assert!((lut.values[23] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_lut_data_validate_ok() {
        let lut = LutData::identity(17);
        assert!(lut.validate().is_ok());
    }

    #[test]
    fn test_lut_data_validate_invalid_grid_size_zero() {
        let mut lut = LutData::identity(17);
        lut.grid_size = 0;
        assert!(matches!(lut.validate(), Err(LutError::InvalidGridSize(0))));
    }

    #[test]
    fn test_lut_data_validate_invalid_grid_size_too_large() {
        let mut lut = LutData::identity(17);
        lut.grid_size = 300;
        assert!(matches!(lut.validate(), Err(LutError::InvalidGridSize(300))));
    }

    #[test]
    fn test_lut_data_validate_invalid_value_count() {
        let mut lut = LutData::identity(17);
        lut.values.truncate(100);
        assert!(matches!(lut.validate(), Err(LutError::InvalidValueCount { .. })));
    }

    #[test]
    fn test_lut_data_validate_invalid_domain_range() {
        let mut lut = LutData::identity(17);
        lut.domain_min = [1.0, 0.0, 0.0];
        lut.domain_max = [0.5, 1.0, 1.0];
        assert!(matches!(lut.validate(), Err(LutError::InvalidDomainRange)));
    }

    #[test]
    fn test_lut_format_from_extension() {
        assert_eq!(LutFormat::from_extension("cube").unwrap(), LutFormat::Cube);
        assert_eq!(LutFormat::from_extension("3DL").unwrap(), LutFormat::ThreeDL);
        assert_eq!(LutFormat::from_extension("Look").unwrap(), LutFormat::Look);
        assert_eq!(LutFormat::from_extension("CSP").unwrap(), LutFormat::Csp);
        assert_eq!(LutFormat::from_extension("cdl").unwrap(), LutFormat::AscCdl);
        assert!(LutFormat::from_extension("xyz").is_err());
    }

    #[test]
    fn test_lut_error_display() {
        let err = LutError::ParseError("test error".to_string());
        assert!(format!("{}", err).contains("LUT_PARSE_ERROR"));

        let err = LutError::UnsupportedFormat("xyz".to_string());
        assert!(format!("{}", err).contains("LUT_UNSUPPORTED_FORMAT"));

        let err = LutError::InvalidGridSize(999);
        assert!(format!("{}", err).contains("LUT_INVALID_CUBE"));

        let err = LutError::WriteError("disk full".to_string());
        assert!(format!("{}", err).contains("LUT_WRITE_ERROR"));
    }

    // -- Task 2: .cube parser tests --

    #[test]
    fn test_parse_cube_valid() {
        let content = create_synthetic_cube(2, Some("Test LUT"));
        let tmp = write_temp_file(&content, "test.cube");
        let lut = parse_cube(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 2);
        assert_eq!(lut.title, Some("Test LUT".to_string()));
        assert_eq!(lut.values.len(), 24);
        assert_eq!(lut.domain_min, [0.0, 0.0, 0.0]);
        assert_eq!(lut.domain_max, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn test_parse_cube_no_title() {
        let content = "LUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 0.0 0.0\n0.0 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n";
        let tmp = write_temp_file(content, "test.cube");
        let lut = parse_cube(tmp.path()).unwrap();
        assert_eq!(lut.title, None);
        assert_eq!(lut.grid_size, 2);
    }

    #[test]
    fn test_parse_cube_with_comments() {
        let content = "# This is a comment\n# Another comment\nLUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 0.0 0.0\n0.0 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n";
        let tmp = write_temp_file(content, "test.cube");
        let lut = parse_cube(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 2);
    }

    #[test]
    fn test_parse_cube_custom_domain() {
        let content = "DOMAIN_MIN -0.5 -0.5 -0.5\nDOMAIN_MAX 2.0 2.0 2.0\nLUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 0.0 0.0\n0.0 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n";
        let tmp = write_temp_file(content, "test.cube");
        let lut = parse_cube(tmp.path()).unwrap();
        assert_eq!(lut.domain_min, [-0.5, -0.5, -0.5]);
        assert_eq!(lut.domain_max, [2.0, 2.0, 2.0]);
    }

    #[test]
    fn test_parse_cube_skip_1d_size() {
        let content = "LUT_1D_SIZE 256\nLUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 0.0 0.0\n0.0 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n";
        let tmp = write_temp_file(content, "test.cube");
        let lut = parse_cube(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 2);
    }

    #[test]
    fn test_parse_cube_missing_size() {
        let content = "0.0 0.0 0.0\n1.0 0.0 0.0\n";
        let tmp = write_temp_file(content, "test.cube");
        let err = parse_cube(tmp.path()).unwrap_err();
        assert!(format!("{}", err).contains("LUT_3D_SIZE not found"));
    }

    #[test]
    fn test_parse_cube_invalid_size() {
        let content = "LUT_3D_SIZE 0\n";
        let tmp = write_temp_file(content, "test.cube");
        let err = parse_cube(tmp.path()).unwrap_err();
        assert!(matches!(err, LutError::InvalidGridSize(0)));
    }

    #[test]
    fn test_parse_cube_size_too_large() {
        let content = "LUT_3D_SIZE 300\n";
        let tmp = write_temp_file(content, "test.cube");
        let err = parse_cube(tmp.path()).unwrap_err();
        assert!(matches!(err, LutError::InvalidGridSize(300)));
    }

    #[test]
    fn test_parse_cube_wrong_value_count() {
        let content = "LUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 0.0 0.0\n";
        let tmp = write_temp_file(content, "test.cube");
        let err = parse_cube(tmp.path()).unwrap_err();
        assert!(matches!(err, LutError::InvalidValueCount { .. }));
    }

    #[test]
    fn test_parse_cube_invalid_domain() {
        let content = "DOMAIN_MIN 1.0 1.0 1.0\nDOMAIN_MAX 0.0 0.0 0.0\nLUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 0.0 0.0\n0.0 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n";
        let tmp = write_temp_file(content, "test.cube");
        let err = parse_cube(tmp.path()).unwrap_err();
        assert!(matches!(err, LutError::InvalidDomainRange));
    }

    // -- Task 3: .cube serializer + round-trip tests --

    #[test]
    fn test_serialize_cube() {
        let lut = LutData::identity(2);
        let s = serialize_cube(&lut).unwrap();
        assert!(s.contains("LUT_3D_SIZE 2"));
        assert!(s.contains("DOMAIN_MIN"));
        assert!(s.contains("DOMAIN_MAX"));
    }

    #[test]
    fn test_serialize_cube_with_title() {
        let mut lut = LutData::identity(2);
        lut.title = Some("My LUT".to_string());
        let s = serialize_cube(&lut).unwrap();
        assert!(s.contains("TITLE \"My LUT\""));
    }

    #[test]
    fn test_cube_roundtrip_2() {
        let original = create_synthetic_cube(2, Some("Round-trip Test"));
        let tmp = write_temp_file(&original, "test.cube");
        let lut = parse_cube(tmp.path()).unwrap();
        let reser = serialize_cube(&lut).unwrap();

        let lut2 = parse_cube_from_str(&reser).unwrap();
        assert_eq!(lut.title, lut2.title);
        assert_eq!(lut.grid_size, lut2.grid_size);
        assert_eq!(lut.values.len(), lut2.values.len());
        for i in 0..lut.values.len() {
            assert!(
                (lut.values[i] - lut2.values[i]).abs() < 1e-6,
                "value mismatch at index {}: {} vs {}",
                i,
                lut.values[i],
                lut2.values[i]
            );
        }
    }

    #[test]
    fn test_cube_roundtrip_17() {
        let content = create_synthetic_cube(17, None);
        let tmp = write_temp_file(&content, "test.cube");
        let lut = parse_cube(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 17);
        assert_eq!(lut.values.len(), 17 * 17 * 17 * 3);

        let reser = serialize_cube(&lut).unwrap();
        let lut2 = parse_cube_from_str(&reser).unwrap();
        for i in 0..lut.values.len() {
            assert!((lut.values[i] - lut2.values[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_cube_roundtrip_33() {
        let content = create_synthetic_cube(33, Some("33-grid LUT"));
        let tmp = write_temp_file(&content, "test.cube");
        let lut = parse_cube(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 33);

        let reser = serialize_cube(&lut).unwrap();
        let lut2 = parse_cube_from_str(&reser).unwrap();
        assert_eq!(lut.values.len(), lut2.values.len());
        for i in 0..lut.values.len() {
            assert!((lut.values[i] - lut2.values[i]).abs() < 1e-6);
        }
    }

    // -- Task 4: .3dl tests --

    #[test]
    fn test_parse_3dl_basic() {
        let content = create_synthetic_3dl(2);
        let tmp = write_temp_file(&content, "test.3dl");
        let lut = parse_3dl(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 2);
        assert_eq!(lut.values.len(), 24);
    }

    #[test]
    fn test_3dl_roundtrip_17() {
        let content = create_synthetic_3dl(17);
        let tmp = write_temp_file(&content, "test.3dl");
        let lut = parse_3dl(tmp.path()).unwrap();

        let reser = serialize_3dl(&lut, 12).unwrap();
        let lut2 = parse_3dl_from_str(&reser).unwrap();
        assert_eq!(lut.grid_size, lut2.grid_size);
        for i in 0..lut.values.len() {
            assert!((lut.values[i] - lut2.values[i]).abs() < 0.002, // 12-bit quantization
                "3dl roundtrip mismatch at {}: {} vs {}", i, lut.values[i], lut2.values[i]);
        }
    }

    #[test]
    #[test]
    fn test_3dl_mesh_keyword() {
        // Mesh 2 12 → grid_size = (1<<2)+1 = 5
        let mut content = String::from("Mesh 2 12\n");
        content.push_str("0 256 512 768 1023\n");
        let n = 5usize;
        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let rv = (r as f64 / (n - 1) as f64 * 1023.0).round() as i32;
                    let gv = (g as f64 / (n - 1) as f64 * 1023.0).round() as i32;
                    let bv = (b as f64 / (n - 1) as f64 * 1023.0).round() as i32;
                    content.push_str(&format!("{} {} {}\n", rv, gv, bv));
                }
            }
        }
        let tmp = write_temp_file(&content, "test.3dl");
        let lut = parse_3dl(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 5);
        assert_eq!(lut.values.len(), 5 * 5 * 5 * 3);
    }

    #[test]
    fn test_cube_to_3dl_cross_format() {
        let cube_content = create_synthetic_cube(2, None);
        let tmp = write_temp_file(&cube_content, "test.cube");
        let lut = parse_cube(tmp.path()).unwrap();

        let cdl_str = serialize_3dl(&lut, 12).unwrap();
        let lut2 = parse_3dl_from_str(&cdl_str).unwrap();
        assert_eq!(lut.grid_size, lut2.grid_size);
        // Allow 12-bit quantization error
        for i in 0..lut.values.len() {
            assert!(
                (lut.values[i] - lut2.values[i]).abs() < 0.002,
                "cross-format mismatch at {}: {} vs {}",
                i,
                lut.values[i],
                lut2.values[i]
            );
        }
    }

    // -- Task 5: .look tests --

    #[test]
    fn test_parse_look_basic() {
        let content = create_synthetic_look(2, Some("Test Look"));
        let tmp = write_temp_file(&content, "test.look");
        let lut = parse_look(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 2);
        assert_eq!(lut.title, Some("Test Look".to_string()));
        assert_eq!(lut.values.len(), 24);
    }

    #[test]
    fn test_look_roundtrip() {
        let content = create_synthetic_look(17, Some("17 Look"));
        let tmp = write_temp_file(&content, "test.look");
        let lut = parse_look(tmp.path()).unwrap();

        let reser = serialize_look(&lut).unwrap();
        let lut2 = parse_look_from_str(&reser).unwrap();
        assert_eq!(lut.grid_size, lut2.grid_size);
        for i in 0..lut.values.len() {
            assert!((lut.values[i] - lut2.values[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_look_xml_escape() {
        let mut lut = LutData::identity(2);
        lut.title = Some("Test <>&\"'".to_string());
        let s = serialize_look(&lut).unwrap();
        assert!(s.contains("Test &lt;&gt;&amp;&quot;&apos;"));
    }

    // -- Task 6: .csp tests --

    #[test]
    fn test_parse_csp_basic() {
        let content = create_synthetic_csp(2);
        let tmp = write_temp_file(&content, "test.csp");
        let lut = parse_csp(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 2);
        assert_eq!(lut.values.len(), 24);
    }

    #[test]
    fn test_csp_roundtrip() {
        let content = create_synthetic_csp(3);
        let tmp = write_temp_file(&content, "test.csp");
        let lut = parse_csp(tmp.path()).unwrap();

        let reser = serialize_csp(&lut).unwrap();
        let lut2 = parse_csp_from_str(&reser).unwrap();
        assert_eq!(lut.grid_size, lut2.grid_size);
        for i in 0..lut.values.len() {
            assert!((lut.values[i] - lut2.values[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_parse_csp_invalid_magic() {
        let content = "INVALID_HEADER\n3D\n";
        let tmp = write_temp_file(content, "test.csp");
        let err = parse_csp(tmp.path()).unwrap_err();
        assert!(format!("{}", err).contains("LUT_PARSE_ERROR"));
    }

    #[test]
    fn test_parse_csp_1d_rejected() {
        let content = "CSPLUTV100\n1D\n2\n0.0 1.0\n0.0 1.0\n";
        let tmp = write_temp_file(content, "test.csp");
        let err = parse_csp(tmp.path()).unwrap_err();
        assert!(format!("{}", err).contains("only 3D supported"));
    }

    // -- Task 7: ASC-CDL tests --

    #[test]
    fn test_cdl_default_is_identity() {
        let cdl = CdlParams::default();
        let lut = cdl.to_lut_data(3);
        let identity = LutData::identity(3);
        for i in 0..lut.values.len() {
            assert!(
                (lut.values[i] - identity.values[i]).abs() < 1e-6,
                "CDL identity mismatch at {}",
                i
            );
        }
    }

    #[test]
    fn test_cdl_apply_slope() {
        let mut cdl = CdlParams::default();
        cdl.slope = [0.5, 0.5, 0.5];
        let (r, g, b) = cdl.apply(1.0, 1.0, 1.0);
        assert!((r - 0.5).abs() < 1e-10);
        assert!((g - 0.5).abs() < 1e-10);
        assert!((b - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_cdl_apply_offset() {
        let mut cdl = CdlParams::default();
        cdl.offset = [0.1, 0.2, 0.3];
        let (r, g, b) = cdl.apply(0.5, 0.5, 0.5);
        assert!((r - 0.6).abs() < 1e-10);
        assert!((g - 0.7).abs() < 1e-10);
        assert!((b - 0.8).abs() < 1e-10);
    }

    #[test]
    fn test_cdl_apply_power() {
        let mut cdl = CdlParams::default();
        cdl.power = [2.0, 2.0, 2.0];
        let (r, _, _) = cdl.apply(0.5, 0.5, 0.5);
        assert!((r - 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_cdl_apply_saturation() {
        let mut cdl = CdlParams::default();
        cdl.saturation = 0.0;
        let (r, g, b) = cdl.apply(0.5, 0.5, 0.5);
        // With saturation=0, output should be luma only (all equal)
        assert!((r - g).abs() < 1e-10);
        assert!((g - b).abs() < 1e-10);
    }

    #[test]
    fn test_cdl_roundtrip() {
        let params = CdlParams {
            slope: [1.2, 0.9, 1.1],
            offset: [0.01, -0.02, 0.03],
            power: [1.1, 0.95, 1.05],
            saturation: 0.85,
        };
        let xml = create_synthetic_cdl(&params);
        let parsed = parse_cdl_from_str(&xml).unwrap();
        assert_eq!(parsed.slope, params.slope);
        assert_eq!(parsed.offset, params.offset);
        for i in 0..3 {
            assert!((parsed.power[i] - params.power[i]).abs() < 1e-6);
        }
        assert!((parsed.saturation - params.saturation).abs() < 1e-6);
    }

    #[test]
    fn test_cdl_serialization_format() {
        let params = CdlParams::default();
        let xml = serialize_cdl(&params);
        assert!(xml.contains("<ColorDecision>"));
        assert!(xml.contains("<SOPNode>"));
        assert!(xml.contains("<Slope>"));
        assert!(xml.contains("<Offset>"));
        assert!(xml.contains("<Power>"));
        assert!(xml.contains("<SatNode>"));
        assert!(xml.contains("<Saturation>"));
    }

    // -- Task 9: Unified API tests --

    #[test]
    fn test_parse_lut_auto_detect_cube() {
        let content = create_synthetic_cube(2, None);
        let tmp = write_temp_file(&content, "auto.cube");
        let lut = parse_lut(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 2);
    }

    #[test]
    fn test_parse_lut_auto_detect_3dl() {
        let content = create_synthetic_3dl(2);
        let tmp = write_temp_file(&content, "auto.3dl");
        let lut = parse_lut(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 2);
    }

    #[test]
    fn test_parse_lut_auto_detect_csp() {
        let content = create_synthetic_csp(2);
        let tmp = write_temp_file(&content, "auto.csp");
        let lut = parse_lut(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 2);
    }

    #[test]
    fn test_parse_lut_auto_detect_no_ext() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("");
        std::fs::write(&path, "test").unwrap();
        let err = parse_lut(&path).unwrap_err();
        assert!(format!("{}", err).contains("LUT_UNSUPPORTED_FORMAT"));
    }

    #[test]
    fn test_parse_lut_from_bytes() {
        let content = create_synthetic_cube(3, None);
        let lut = parse_lut_from_bytes(content.as_bytes(), LutFormat::Cube).unwrap();
        assert_eq!(lut.grid_size, 3);
    }

    #[test]
    fn test_serialize_lut_to_bytes() {
        let lut = LutData::identity(2);
        let bytes = serialize_lut_to_bytes(&lut, LutFormat::Cube).unwrap();
        assert!(!bytes.is_empty());
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(s.contains("LUT_3D_SIZE 2"));
    }

    #[test]
    fn test_serialize_lut_auto_ext() {
        let lut = LutData::identity(2);
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().with_extension("cube");
        serialize_lut(&lut, &path).unwrap();
        let lut2 = parse_cube(&path).unwrap();
        assert_eq!(lut.grid_size, lut2.grid_size);
    }

    // -- Task 10: Integration / error handling tests --

    #[test]
    fn test_empty_file_cube() {
        let tmp = write_temp_file("", "test.cube");
        let err = parse_cube(tmp.path()).unwrap_err();
        assert!(format!("{}", err).contains("LUT_PARSE_ERROR"));
    }

    #[test]
    fn test_corrupt_file_cube() {
        let tmp = write_temp_file("garbage data not a lut", "test.cube");
        let err = parse_cube(tmp.path()).unwrap_err();
        assert!(format!("{}", err).contains("LUT_PARSE_ERROR"));
    }

    #[test]
    fn test_nonexistent_file() {
        let err = parse_cube(Path::new("/nonexistent/path/lut.cube")).unwrap_err();
        assert!(format!("{}", err).contains("LUT_IO_ERROR"));
    }

    #[test]
    fn test_large_grid_65_roundtrip() {
        let content = create_synthetic_cube(65, None);
        let tmp = write_temp_file(&content, "test.cube");
        let lut = parse_cube(tmp.path()).unwrap();
        assert_eq!(lut.grid_size, 65);
        assert_eq!(lut.values.len(), 65 * 65 * 65 * 3);

        let reser = serialize_cube(&lut).unwrap();
        let lut2 = parse_cube_from_str(&reser).unwrap();
        for i in 0..lut.values.len() {
            assert!((lut.values[i] - lut2.values[i]).abs() < 1e-6);
        }
    }

    #[test]
    fn test_all_formats_roundtrip_17() {
        // .cube
        let cube_s = create_synthetic_cube(17, Some("Multi-format Test"));
        let lut_cube = parse_cube_from_str(&cube_s).unwrap();

        // .look
        let look_s = serialize_look(&lut_cube).unwrap();
        let lut_look = parse_look_from_str(&look_s).unwrap();

        // .csp
        let csp_s = serialize_csp(&lut_look).unwrap();
        let lut_csp = parse_csp_from_str(&csp_s).unwrap();

        // .3dl (with quantization)
        let tdl_s = serialize_3dl(&lut_csp, 12).unwrap();
        let lut_3dl = parse_3dl_from_str(&tdl_s).unwrap();

        // Back to .cube
        let final_s = serialize_cube(&lut_3dl).unwrap();
        let lut_final = parse_cube_from_str(&final_s).unwrap();

        assert_eq!(lut_final.grid_size, 17);
        // 12-bit quantization at .3dl step causes small errors
        for i in 0..lut_final.values.len() {
            assert!(
                (lut_cube.values[i] - lut_final.values[i]).abs() < 0.003,
                "multi-format roundtrip mismatch at {}: {} vs {}",
                i,
                lut_cube.values[i],
                lut_final.values[i]
            );
        }
    }

    // -- Helpers --

    fn write_temp_file(content: &str, suffix: &str) -> NamedTempFile {
        let mut tmp = NamedTempFile::with_suffix(suffix).unwrap();
        tmp.write_all(content.as_bytes()).unwrap();
        tmp.flush().unwrap();
        tmp
    }
}
