// color_transfer.rs — Color transfer LUT generation via FFI
//
// Generates a 3D LUT that maps source strip colors to target strip colors
// using cumulative distribution function (CDF) matching in RGB space.

use std::path::Path;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ColorTransferError {
    #[error("COLOR_TRANSFER_FFI_ERROR -- {0}")]
    FfiError(String),
    #[error("COLOR_TRANSFER_INVALID_INPUT -- {0}")]
    InvalidInput(String),
    #[error("COLOR_TRANSFER_IO_ERROR -- {0}")]
    IoError(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, ColorTransferError>;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

extern "C" {
    fn mengxi_generate_color_transfer_lut(
        src_len: i32,
        src_ptr: *const f64,
        src_w: i32,
        src_h: i32,
        tgt_len: i32,
        tgt_ptr: *const f64,
        tgt_w: i32,
        tgt_h: i32,
        grid_size: i32,
        out_len: i32,
        out_ptr: *mut f64,
    ) -> i32;
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Maximum LUT grid size to prevent excessive memory allocation.
const MAX_GRID_SIZE: usize = 129;

/// A 3D color transfer LUT (RGB → RGB mapping table).
#[derive(Debug, Clone)]
pub struct ColorTransferLut {
    /// Grid size per dimension.
    pub grid_size: usize,
    /// LUT data: grid_size³ × 3 f64, RGB→RGB mapping.
    /// For index (ri, gi, bi), output RGB is at:
    ///   offset = ((ri * grid_size + gi) * grid_size + bi) * 3
    pub data: Vec<f64>,
}

impl ColorTransferLut {
    /// Sample the LUT at a given RGB position [0, 1].
    ///
    /// Uses trilinear interpolation for smooth results.
    pub fn sample(&self, r: f64, g: f64, b: f64) -> [f64; 3] {
        let gs = self.grid_size;
        let gmax = (gs - 1) as f64;
        let rf = (r.clamp(0.0, 1.0) * gmax).min(gmax);
        let gf = (g.clamp(0.0, 1.0) * gmax).min(gmax);
        let bf = (b.clamp(0.0, 1.0) * gmax).min(gmax);

        let r0 = rf.floor() as usize;
        let g0 = gf.floor() as usize;
        let b0 = bf.floor() as usize;
        let r1 = (r0 + 1).min(gs - 1);
        let g1 = (g0 + 1).min(gs - 1);
        let b1 = (b0 + 1).min(gs - 1);

        let dr = rf - r0 as f64;
        let dg = gf - g0 as f64;
        let db = bf - b0 as f64;

        let mut result = [0.0f64; 3];
        for (ch, item) in result.iter_mut().enumerate() {
            let c000 = self.data[((r0 * gs + g0) * gs + b0) * 3 + ch];
            let c001 = self.data[((r0 * gs + g0) * gs + b1) * 3 + ch];
            let c010 = self.data[((r0 * gs + g1) * gs + b0) * 3 + ch];
            let c011 = self.data[((r0 * gs + g1) * gs + b1) * 3 + ch];
            let c100 = self.data[((r1 * gs + g0) * gs + b0) * 3 + ch];
            let c101 = self.data[((r1 * gs + g0) * gs + b1) * 3 + ch];
            let c110 = self.data[((r1 * gs + g1) * gs + b0) * 3 + ch];
            let c111 = self.data[((r1 * gs + g1) * gs + b1) * 3 + ch];

            let c00 = c000 * (1.0 - db) + c001 * db;
            let c01 = c010 * (1.0 - db) + c011 * db;
            let c10 = c100 * (1.0 - db) + c101 * db;
            let c11 = c110 * (1.0 - db) + c111 * db;

            let c0 = c00 * (1.0 - dg) + c01 * dg;
            let c1 = c10 * (1.0 - dg) + c11 * dg;

            *item = c0 * (1.0 - dr) + c1 * dr;
        }
        result
    }

    /// Export the LUT as a .cube file (Adobe Cube format).
    pub fn write_cube_file(&self, path: &Path) -> Result<()> {
        let gs = self.grid_size;
        let mut content = String::with_capacity(gs * gs * gs * 40);
        content.push_str(&format!("LUT_3D_SIZE {}\n", gs));
        content.push_str("DOMAIN_MIN 0.0 0.0 0.0\n");
        content.push_str("DOMAIN_MAX 1.0 1.0 1.0\n");
        content.push('\n');

        // .cube format: B changes fastest, then G, then R
        let mut ri = 0;
        while ri < gs {
            let mut gi = 0;
            while gi < gs {
                let mut bi = 0;
                while bi < gs {
                    let idx = ((ri * gs + gi) * gs + bi) * 3;
                    content.push_str(&format!(
                        "{:.6} {:.6} {:.6}\n",
                        self.data[idx], self.data[idx + 1], self.data[idx + 2]
                    ));
                    bi += 1;
                }
                gi += 1;
            }
            ri += 1;
        }

        std::fs::write(path, content)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Safe wrappers
// ---------------------------------------------------------------------------

/// Generate a color transfer 3D LUT via CDF matching.
///
/// # Arguments
/// * `src` — Source strip interleaved sRGB [0,1] data
/// * `src_w`, `src_h` — Source dimensions
/// * `tgt` — Target strip interleaved sRGB [0,1] data
/// * `tgt_w`, `tgt_h` — Target dimensions
/// * `grid_size` — LUT grid size (e.g. 33), 0 = default 33
pub fn generate_color_transfer_lut(
    src: &[f64],
    src_w: usize,
    src_h: usize,
    tgt: &[f64],
    tgt_w: usize,
    tgt_h: usize,
    grid_size: usize,
) -> Result<ColorTransferLut> {
    let gs = if grid_size == 0 { 33 } else { grid_size };
    if !(2..=MAX_GRID_SIZE).contains(&gs) {
        return Err(ColorTransferError::InvalidInput(format!(
            "grid_size {} out of range [2, {}]",
            gs, MAX_GRID_SIZE
        )));
    }
    if src_w == 0 || src_h == 0 || tgt_w == 0 || tgt_h == 0 {
        return Err(ColorTransferError::InvalidInput(
            "dimensions must be non-zero".to_string(),
        ));
    }

    let lut_size = gs * gs * gs * 3;
    let mut output = vec![0.0_f64; lut_size];

    let result = unsafe {
        mengxi_generate_color_transfer_lut(
            src.len() as i32,
            src.as_ptr(),
            src_w as i32,
            src_h as i32,
            tgt.len() as i32,
            tgt.as_ptr(),
            tgt_w as i32,
            tgt_h as i32,
            gs as i32,
            lut_size as i32,
            output.as_mut_ptr(),
        )
    };

    if result < 0 {
        return Err(ColorTransferError::FfiError(format!(
            "mengxi_generate_color_transfer_lut returned {}",
            result
        )));
    }

    Ok(ColorTransferLut {
        grid_size: gs,
        data: output,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_color_transfer_identity() {
        let strip = vec![0.5; 2 * 2 * 3];
        let lut = generate_color_transfer_lut(&strip, 2, 2, &strip, 2, 2, 3).unwrap();
        assert_eq!(lut.grid_size, 3);
        // All output values should be in [0, 1]
        for &v in &lut.data {
            assert!(v >= 0.0 && v <= 1.0, "value {} out of range", v);
        }
    }

    #[test]
    fn test_color_transfer_sample() {
        let strip = vec![0.5; 2 * 2 * 3];
        let lut = generate_color_transfer_lut(&strip, 2, 2, &strip, 2, 2, 3).unwrap();
        let [r, g, b] = lut.sample(0.5, 0.5, 0.5);
        assert!(r >= 0.0 && r <= 1.0);
        assert!(g >= 0.0 && g <= 1.0);
        assert!(b >= 0.0 && b <= 1.0);
    }

    #[test]
    fn test_color_transfer_bad_grid_size() {
        let strip = vec![0.5; 12];
        let result = generate_color_transfer_lut(&strip, 2, 2, &strip, 2, 2, 0);
        // grid_size 0 defaults to 33 — this should work, not error
        // Actually it defaults to 33, so it should succeed. But let's test bad sizes:
        assert!(generate_color_transfer_lut(&strip, 2, 2, &strip, 2, 2, 1).is_err());
        assert!(generate_color_transfer_lut(&strip, 2, 2, &strip, 2, 2, 200).is_err());
    }

    #[test]
    fn test_color_transfer_zero_dims() {
        let strip = vec![0.5; 12];
        let result = generate_color_transfer_lut(&strip, 0, 2, &strip, 2, 2, 3);
        assert!(result.is_err());
    }

    #[test]
    fn test_write_cube_file() {
        let dir = TempDir::new().unwrap();
        let strip = vec![0.5; 2 * 2 * 3];
        let lut = generate_color_transfer_lut(&strip, 2, 2, &strip, 2, 2, 3).unwrap();
        let path = dir.path().join("test.cube");
        lut.write_cube_file(&path).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("LUT_3D_SIZE 3"));
        assert!(content.contains("DOMAIN_MIN 0.0 0.0 0.0"));
    }
}
