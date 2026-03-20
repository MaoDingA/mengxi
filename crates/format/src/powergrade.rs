// powergrade.rs — DaVinci Resolve PowerGrade read-only parser

use std::fmt;
use std::io;
use std::path::Path;

/// PowerGrade version for format detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerGradeVersion {
    V1,
    V2,
    V3,
}

/// Errors from PowerGrade parsing.
#[derive(Debug)]
pub enum PowerGradeError {
    UnsupportedVersion(String),
    ParseError(String),
    IoError(String),
}

impl fmt::Display for PowerGradeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PowerGradeError::UnsupportedVersion(ver) => {
                write!(f, "LUT_UNSUPPORTED_FORMAT -- unsupported PowerGrade version: {}", ver)
            }
            PowerGradeError::ParseError(msg) => {
                write!(f, "LUT_PARSE_ERROR -- PowerGrade parse error: {}", msg)
            }
            PowerGradeError::IoError(msg) => {
                write!(f, "LUT_IO_ERROR -- PowerGrade I/O error: {}", msg)
            }
        }
    }
}

impl std::error::Error for PowerGradeError {}

impl From<io::Error> for PowerGradeError {
    fn from(e: io::Error) -> Self {
        PowerGradeError::IoError(e.to_string())
    }
}

/// Extracted PowerGrade color correction parameters.
///
/// These represent the basic Resolve node color corrections
/// which can be converted to a 3D LUT representation.
#[derive(Debug, Clone, PartialEq)]
pub struct PowerGradeData {
    /// Version detected from the file.
    pub version: PowerGradeVersion,
    /// Gamma value (if present).
    pub gamma: f64,
    /// Per-channel gain (highlights): [R, G, B]
    pub gain: [f64; 3],
    /// Per-channel gamma (midtones): [R, G, B]
    pub lift: [f64; 3],
    /// Per-channel lift (shadows): [R, G, B]
    pub power: [f64; 3],
    /// Per-channel offset: [R, G, B]
    pub offset: [f64; 3],
    /// Saturation value.
    pub saturation: f64,
    /// Number of nodes in the PowerGrade.
    pub node_count: u32,
}

impl Default for PowerGradeData {
    fn default() -> Self {
        PowerGradeData {
            version: PowerGradeVersion::V2,
            gamma: 1.0,
            gain: [1.0, 1.0, 1.0],
            lift: [0.0, 0.0, 0.0],
            power: [1.0, 1.0, 1.0],
            offset: [0.0, 0.0, 0.0],
            saturation: 1.0,
            node_count: 1,
        }
    }
}

impl PowerGradeData {
    /// Convert PowerGrade parameters to a 3D LUT representation.
    ///
    /// This samples the PowerGrade color correction function across
    /// a uniform grid to produce a 3D LUT.
    pub fn to_lut_data(&self, grid_size: u32) -> crate::lut::LutData {
        let n = grid_size as usize;
        let mut values = Vec::with_capacity(n * n * n * 3);

        for b in 0..n {
            for g in 0..n {
                for r in 0..n {
                    let in_r = r as f64 / (n - 1).max(1) as f64;
                    let in_g = g as f64 / (n - 1).max(1) as f64;
                    let in_b = b as f64 / (n - 1).max(1) as f64;

                    // Apply lift (shadows)
                    let mut out_r = in_r + self.lift[0];
                    let mut out_g = in_g + self.lift[1];
                    let mut out_b = in_b + self.lift[2];

                    // Apply gain (highlights) — multiply above 0.5
                    let gain_factor_r = if out_r > 0.5 { self.gain[0] } else { 1.0 };
                    let gain_factor_g = if out_g > 0.5 { self.gain[1] } else { 1.0 };
                    let gain_factor_b = if out_b > 0.5 { self.gain[2] } else { 1.0 };
                    out_r = 0.5 + (out_r - 0.5) * gain_factor_r;
                    out_g = 0.5 + (out_g - 0.5) * gain_factor_g;
                    out_b = 0.5 + (out_b - 0.5) * gain_factor_b;

                    // Apply power (gamma)
                    out_r = out_r.powf(self.power[0]);
                    out_g = out_g.powf(self.power[1]);
                    out_b = out_b.powf(self.power[2]);

                    // Apply saturation
                    let luma = 0.2126 * out_r + 0.7152 * out_g + 0.0722 * out_b;
                    out_r = luma + self.saturation * (out_r - luma);
                    out_g = luma + self.saturation * (out_g - luma);
                    out_b = luma + self.saturation * (out_b - luma);

                    values.push(out_r);
                    values.push(out_g);
                    values.push(out_b);
                }
            }
        }

        crate::lut::LutData {
            title: None,
            grid_size,
            domain_min: [0.0, 0.0, 0.0],
            domain_max: [1.0, 1.0, 1.0],
            values,
        }
    }
}

/// Parse a DaVinci Resolve PowerGrade file.
///
/// PowerGrade files are proprietary binary format. This parser handles
/// the simplified text-based format that Resolve uses for its grade files,
/// and reports errors for unrecognized binary versions.
pub fn parse_powergrade(path: &Path) -> Result<PowerGradeData, PowerGradeError> {
    let data = std::fs::read(path).map_err(|e| PowerGradeError::IoError(e.to_string()))?;

    if data.is_empty() {
        return Err(PowerGradeError::ParseError("empty PowerGrade file".to_string()));
    }

    // Detect version from file header
    let version = detect_version(&data)?;

    match version {
        PowerGradeVersion::V1 => Err(PowerGradeError::UnsupportedVersion(
            "v1 (legacy binary format not supported)".to_string(),
        )),
        PowerGradeVersion::V2 => parse_powergrade_v2(&data),
        PowerGradeVersion::V3 => parse_powergrade_v3(&data),
    }
}

/// Detect PowerGrade version from file header.
fn detect_version(data: &[u8]) -> Result<PowerGradeVersion, PowerGradeError> {
    if data.len() < 4 {
        return Err(PowerGradeError::ParseError(
            "file too small to detect version".to_string(),
        ));
    }

    // Check for text-based markers first
    let header = std::str::from_utf8(&data[..data.len().min(256)]).unwrap_or("");

    if header.contains("Version 3") || header.contains("version 3") {
        return Ok(PowerGradeVersion::V3);
    }

    if header.contains("Version 2") || header.contains("version 2") {
        return Ok(PowerGradeVersion::V2);
    }

    if header.contains("Version 1") || header.contains("version 1") {
        return Ok(PowerGradeVersion::V1);
    }

    // Binary format: check magic bytes
    // Resolve PowerGrade files start with specific binary markers
    // For unrecognized binary formats, report as unsupported
    if data[0] == 0x00 || data[0] < 0x20 {
        return Err(PowerGradeError::UnsupportedVersion(format!(
            "unrecognized binary format (magic: {:02X} {:02X} {:02X} {:02X})",
            data[0], data[1], data[2], data[3]
        )));
    }

    // Default to V2 for text-based files without explicit version
    Ok(PowerGradeVersion::V2)
}

/// Parse text-based PowerGrade V2 format.
fn parse_powergrade_v2(data: &[u8]) -> Result<PowerGradeData, PowerGradeError> {
    let content = std::str::from_utf8(data)
        .map_err(|e| PowerGradeError::ParseError(format!("invalid UTF-8: {}", e)))?;

    let mut pg = PowerGradeData::default();
    pg.version = PowerGradeVersion::V2;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("gamma") {
            if let Ok(v) = rest.trim().parse::<f64>() {
                pg.gamma = v;
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("gain") {
            let parts: Vec<f64> = parse_floats(rest);
            if parts.len() == 3 {
                pg.gain = [parts[0], parts[1], parts[2]];
            } else if parts.len() == 1 {
                pg.gain = [parts[0], parts[0], parts[0]];
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("lift") {
            let parts: Vec<f64> = parse_floats(rest);
            if parts.len() == 3 {
                pg.lift = [parts[0], parts[1], parts[2]];
            } else if parts.len() == 1 {
                pg.lift = [parts[0], parts[0], parts[0]];
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("power") {
            let parts: Vec<f64> = parse_floats(rest);
            if parts.len() == 3 {
                pg.power = [parts[0], parts[1], parts[2]];
            } else if parts.len() == 1 {
                pg.power = [parts[0], parts[0], parts[0]];
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("offset") {
            let parts: Vec<f64> = parse_floats(rest);
            if parts.len() == 3 {
                pg.offset = [parts[0], parts[1], parts[2]];
            } else if parts.len() == 1 {
                pg.offset = [parts[0], parts[0], parts[0]];
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("saturation") {
            if let Ok(v) = rest.trim().parse::<f64>() {
                pg.saturation = v;
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("nodes") {
            if let Ok(v) = rest.trim().parse::<u32>() {
                pg.node_count = v;
            }
            continue;
        }
    }

    Ok(pg)
}

/// Parse text-based PowerGrade V3 format.
fn parse_powergrade_v3(data: &[u8]) -> Result<PowerGradeData, PowerGradeError> {
    // V3 uses the same text-based key=value format as V2
    // but with additional V3-specific parameters
    let mut pg = parse_powergrade_v2(data)?;
    pg.version = PowerGradeVersion::V3;
    Ok(pg)
}

/// Parse space-separated floats from a string.
fn parse_floats(s: &str) -> Vec<f64> {
    s.split_whitespace()
        .filter_map(|t| t.parse::<f64>().ok())
        .collect()
}

/// Create synthetic PowerGrade test data.
pub fn create_synthetic_powergrade_v2() -> String {
    String::from(
        "# DaVinci Resolve PowerGrade\n\
         Version 2\n\
         gamma 1.0\n\
         gain 1.2 0.9 1.1\n\
         lift 0.01 -0.02 0.03\n\
         power 1.1 0.95 1.05\n\
         offset 0.0 0.0 0.0\n\
         saturation 0.85\n\
         nodes 1\n",
    )
}

/// Create synthetic PowerGrade V3 test data.
pub fn create_synthetic_powergrade_v3() -> String {
    String::from(
        "# DaVinci Resolve PowerGrade V3\n\
         Version 3\n\
         gamma 1.0\n\
         gain 1.0 1.0 1.0\n\
         lift 0.0 0.0 0.0\n\
         power 1.0 1.0 1.0\n\
         offset 0.0 0.0 0.0\n\
         saturation 1.0\n\
         nodes 3\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn write_temp_file(content: &str, suffix: &str) -> NamedTempFile {
        use std::io::Write;
        let mut tmp = NamedTempFile::with_suffix(suffix).unwrap();
        write!(tmp, "{}", content).unwrap();
        tmp.flush().unwrap();
        tmp
    }

    #[test]
    fn test_parse_powergrade_v2() {
        let content = create_synthetic_powergrade_v2();
        let tmp = write_temp_file(&content, ".drp");
        let pg = parse_powergrade(tmp.path()).unwrap();
        assert_eq!(pg.version, PowerGradeVersion::V2);
        assert!((pg.gamma - 1.0).abs() < 1e-10);
        assert_eq!(pg.gain, [1.2, 0.9, 1.1]);
        assert_eq!(pg.lift, [0.01, -0.02, 0.03]);
        for i in 0..3 {
            assert!((pg.power[i] - [1.1, 0.95, 1.05][i]).abs() < 1e-10);
        }
        assert_eq!(pg.saturation, 0.85);
        assert_eq!(pg.node_count, 1);
    }

    #[test]
    fn test_parse_powergrade_v3() {
        let content = create_synthetic_powergrade_v3();
        let tmp = write_temp_file(&content, ".drp");
        let pg = parse_powergrade(tmp.path()).unwrap();
        assert_eq!(pg.version, PowerGradeVersion::V3);
        assert_eq!(pg.node_count, 3);
    }

    #[test]
    fn test_parse_powergrade_v1_unsupported() {
        let content = "# PowerGrade V1\nVersion 1\ngamma 1.0\n";
        let tmp = write_temp_file(content, ".drp");
        let err = parse_powergrade(tmp.path()).unwrap_err();
        assert!(format!("{}", err).contains("LUT_UNSUPPORTED_FORMAT"));
        assert!(format!("{}", err).contains("v1"));
    }

    #[test]
    fn test_parse_powergrade_empty_file() {
        let tmp = write_temp_file("", ".drp");
        let err = parse_powergrade(tmp.path()).unwrap_err();
        assert!(format!("{}", err).contains("LUT_PARSE_ERROR"));
    }

    #[test]
    fn test_parse_powergrade_identity_to_lut() {
        let pg = PowerGradeData::default();
        let lut = pg.to_lut_data(3);
        let identity = crate::lut::LutData::identity(3);
        for i in 0..lut.values.len() {
            assert!(
                (lut.values[i] - identity.values[i]).abs() < 1e-6,
                "identity PG mismatch at {}",
                i
            );
        }
    }

    #[test]
    fn test_powergrade_to_lut_custom() {
        let mut pg = PowerGradeData::default();
        pg.gain = [0.5, 0.5, 0.5];
        pg.saturation = 0.0;
        let lut = pg.to_lut_data(3);
        // With saturation=0, all channels should be equal at each point
        for chunk in lut.values.chunks(3) {
            assert!(
                (chunk[0] - chunk[1]).abs() < 1e-6,
                "sat=0 R vs G"
            );
            assert!(
                (chunk[1] - chunk[2]).abs() < 1e-6,
                "sat=0 G vs B"
            );
        }
    }

    #[test]
    fn test_powergrade_default() {
        let pg = PowerGradeData::default();
        assert_eq!(pg.version, PowerGradeVersion::V2);
        assert_eq!(pg.gain, [1.0, 1.0, 1.0]);
        assert_eq!(pg.lift, [0.0, 0.0, 0.0]);
        assert_eq!(pg.saturation, 1.0);
    }

    #[test]
    fn test_powergrade_error_display() {
        let err = PowerGradeError::UnsupportedVersion("v99".to_string());
        assert!(format!("{}", err).contains("LUT_UNSUPPORTED_FORMAT"));
        assert!(format!("{}", err).contains("v99"));
    }

    #[test]
    fn test_detect_version_text_v2() {
        let data = b"# Version 2\ngamma 1.0\n";
        assert_eq!(detect_version(data).unwrap(), PowerGradeVersion::V2);
    }

    #[test]
    fn test_detect_version_text_v3() {
        let data = b"# Version 3\ngamma 1.0\n";
        assert_eq!(detect_version(data).unwrap(), PowerGradeVersion::V3);
    }

    #[test]
    fn test_detect_version_binary_unsupported() {
        let data = [0x00, 0x01, 0x02, 0x03];
        let err = detect_version(&data).unwrap_err();
        assert!(format!("{}", err).contains("unrecognized binary format"));
    }
}
