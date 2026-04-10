//! Traits abstracting Format crate I/O operations needed by Core.
//! Implemented by CLI layer (or test mocks).

use std::path::Path;

// ---------------------------------------------------------------------------
// Data types — mirror of mengxi_format::lut
// ---------------------------------------------------------------------------

/// Mirror of `mengxi_format::lut::LutData` (grid_size, cube data).
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
    pub fn validate(&self) -> Result<(), LutIoError> {
        if self.grid_size < 2 {
            return Err(LutIoError::Format(format!(
                "invalid grid size: {}",
                self.grid_size
            )));
        }
        if self.grid_size > 256 {
            return Err(LutIoError::Format(format!(
                "invalid grid size: {}",
                self.grid_size
            )));
        }
        let expected = self.grid_size as usize * self.grid_size as usize * self.grid_size as usize * 3;
        if self.values.len() != expected {
            return Err(LutIoError::Format(format!(
                "expected {} values, got {}",
                expected,
                self.values.len()
            )));
        }
        for i in 0..3 {
            if self.domain_min[i] >= self.domain_max[i] {
                return Err(LutIoError::Format("invalid domain range".to_string()));
            }
        }
        Ok(())
    }

    /// Compare this LUT against another, returning per-channel diff statistics.
    ///
    /// Both LUTs must have the same `grid_size`. Returns an error if grid sizes
    /// differ or if either LUT has an invalid value count.
    pub fn diff(&self, other: &LutData) -> Result<LutDiffResult, LutIoError> {
        if self.grid_size != other.grid_size {
            return Err(LutIoError::Format(format!(
                "grid sizes differ: {} vs {}",
                self.grid_size, other.grid_size
            )));
        }
        let expected =
            self.grid_size as usize * self.grid_size as usize * self.grid_size as usize * 3;
        if self.values.len() != expected {
            return Err(LutIoError::Format(format!(
                "expected {} values, got {}",
                expected,
                self.values.len()
            )));
        }
        if other.values.len() != expected {
            return Err(LutIoError::Format(format!(
                "expected {} values, got {}",
                expected,
                other.values.len()
            )));
        }

        let total_points = self.grid_size as usize * self.grid_size as usize * self.grid_size as usize;
        let epsilon = 1e-6_f64;
        let mut channels = [
            ChannelDiff {
                mean_delta: 0.0,
                max_delta: 0.0,
                changed_count: 0,
            },
            ChannelDiff {
                mean_delta: 0.0,
                max_delta: 0.0,
                changed_count: 0,
            },
            ChannelDiff {
                mean_delta: 0.0,
                max_delta: 0.0,
                changed_count: 0,
            },
        ];

        for i in 0..total_points {
            for (ch, channel) in channels.iter_mut().enumerate() {
                let idx = i * 3 + ch;
                let delta = (self.values[idx] - other.values[idx]).abs();
                channel.mean_delta += delta;
                if delta > channel.max_delta {
                    channel.max_delta = delta;
                }
                if delta > epsilon {
                    channel.changed_count += 1;
                }
            }
        }

        for channel in &mut channels {
            channel.mean_delta /= total_points as f64;
        }

        Ok(LutDiffResult {
            channels,
            total_points,
        })
    }
}

/// Per-channel diff statistics for a single color channel (R, G, or B).
#[derive(Debug, Clone)]
pub struct ChannelDiff {
    pub mean_delta: f64,
    pub max_delta: f64,
    pub changed_count: usize,
}

/// Result of comparing two LUT files.
#[derive(Debug, Clone)]
pub struct LutDiffResult {
    /// Per-channel statistics (index 0=R, 1=G, 2=B).
    pub channels: [ChannelDiff; 3],
    /// Total number of RGB triplets compared.
    pub total_points: usize,
}

/// Mirror of `mengxi_format::lut::LutFormat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LutFormat {
    Cube,
    ThreeDL,
    Look,
    Csp,
    Cdl,
}

impl LutFormat {
    pub fn from_extension(ext: &str) -> Result<Self, LutIoError> {
        match ext.to_lowercase().as_str() {
            "cube" => Ok(LutFormat::Cube),
            "3dl" => Ok(LutFormat::ThreeDL),
            "look" => Ok(LutFormat::Look),
            "csp" => Ok(LutFormat::Csp),
            "cdl" => Ok(LutFormat::Cdl),
            other => Err(LutIoError::Format(format!("unknown LUT format: {}", other))),
        }
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error type for LUT I/O operations.
#[derive(Debug, thiserror::Error)]
pub enum LutIoError {
    #[error("LUT parse error: {0}")]
    Parse(String),
    #[error("LUT serialize error: {0}")]
    Serialize(String),
    #[error("LUT format error: {0}")]
    Format(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// LUT serialization/deserialization abstraction.
///
/// Core calls this trait instead of touching `mengxi_format` directly.
/// The CLI layer (or tests) provides the implementation.
pub trait LutIo {
    /// Parse a LUT file into `LutData`.
    fn parse_lut(&self, path: &Path) -> Result<LutData, LutIoError>;

    /// Serialize `LutData` to a file.
    fn serialize_lut(&self, data: &LutData, path: &Path) -> Result<(), LutIoError>;
}
