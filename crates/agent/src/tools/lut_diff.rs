// tools/lut_diff.rs — LUT diff comparison and ASCII curve rendering tools

use std::path::Path;

use async_trait::async_trait;
use mengxi_format::lut::{self, LutData};
use serde_json::{json, Value};

use crate::tool::{Tool, ToolError, ToolResult};

// ---------------------------------------------------------------------------
// DiffLutTool — Compare two LUT files
// ---------------------------------------------------------------------------

pub struct DiffLutTool;

#[async_trait]
impl Tool for DiffLutTool {
    fn name(&self) -> &str {
        "diff_lut"
    }

    fn description(&self) -> &str {
        "Compare two LUT files and report per-channel differences (mean delta, max delta, \
         changed entry count). Also shows per-region breakdown (shadows/midtones/highlights). \
         Use this to understand the magnitude and location of differences between two LUTs."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path_a": { "type": "string", "description": "Path to the first LUT file" },
                "path_b": { "type": "string", "description": "Path to the second LUT file" }
            },
            "required": ["path_a", "path_b"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let path_a = params
            .get("path_a")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: path_a".into()))?;
        let path_b = params
            .get("path_b")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: path_b".into()))?;

        let lut_a = lut::parse_lut(Path::new(path_a))
            .map_err(|e| ToolError::ExecutionError(format!("LUT_PARSE_ERROR (path_a) -- {}", e)))?;
        let lut_b = lut::parse_lut(Path::new(path_b))
            .map_err(|e| ToolError::ExecutionError(format!("LUT_PARSE_ERROR (path_b) -- {}", e)))?;

        let diff = lut_a
            .diff(&lut_b)
            .map_err(|e| ToolError::ExecutionError(format!("LUT_DIFF_ERROR -- {}", e)))?;

        let channel_names = ["R", "G", "B"];
        let mut report = format!(
            "LUT Diff: {} vs {}\nGrid sizes: {} vs {}\nTotal points: {}\n\nPer-channel:\n",
            path_a, path_b, lut_a.grid_size, lut_b.grid_size, diff.total_points
        );

        for (i, name) in channel_names.iter().enumerate() {
            let ch = &diff.channels[i];
            report.push_str(&format!(
                "  {}: mean_delta={:.6}, max_delta={:.6}, changed={}/{}\n",
                name, ch.mean_delta, ch.max_delta, ch.changed_count, diff.total_points
            ));
        }

        // Per-region breakdown
        report.push_str("\nPer-region:\n");
        let regions = [
            ("Shadows (lum <= 0.25)", 0.0f64, 0.25f64),
            ("Midtones (0.25 < lum <= 0.75)", 0.25f64, 0.75f64),
            ("Highlights (lum > 0.75)", 0.75f64, 1.0f64),
        ];

        for (label, lo, hi) in &regions {
            let mut region_deltas = [0.0f64; 3];
            let mut region_counts = [0usize; 3];
            let mut region_entries = 0usize;

            let total = lut_a.grid_size as usize;
            for i in 0..total * total * total {
                let idx = i * 3;
                let ra = lut_a.values[idx];
                let ga = lut_a.values[idx + 1];
                let ba = lut_a.values[idx + 2];
                let lum = 0.2126 * ra + 0.7152 * ga + 0.0722 * ba;

                if (*lo == 0.0 || lum > *lo) && lum <= *hi {
                    region_entries += 1;
                    for ch in 0..3 {
                        let delta = (lut_a.values[idx + ch] - lut_b.values[idx + ch]).abs();
                        if delta > 1e-6 {
                            region_deltas[ch] += delta;
                            region_counts[ch] += 1;
                        }
                    }
                }
            }

            report.push_str(&format!(
                "  {} ({} entries): R={:.4}({}), G={:.4}({}), B={:.4}({})\n",
                label,
                region_entries,
                if region_counts[0] > 0 {
                    region_deltas[0] / region_counts[0] as f64
                } else {
                    0.0
                },
                region_counts[0],
                if region_counts[1] > 0 {
                    region_deltas[1] / region_counts[1] as f64
                } else {
                    0.0
                },
                region_counts[1],
                if region_counts[2] > 0 {
                    region_deltas[2] / region_counts[2] as f64
                } else {
                    0.0
                },
                region_counts[2],
            ));
        }

        Ok(ToolResult::ok(report))
    }
}

// ---------------------------------------------------------------------------
// RenderLutCurvesTool — ASCII curve rendering
// ---------------------------------------------------------------------------

pub struct RenderLutCurvesTool;

#[async_trait]
impl Tool for RenderLutCurvesTool {
    fn name(&self) -> &str {
        "render_lut_curves"
    }

    fn description(&self) -> &str {
        "Render ASCII transfer curves for a LUT file. Shows R, G, B channel curves \
         by extracting the diagonal (R=G=B input → R/G/B output) and plotting in a 40x12 \
         character grid. Useful for visualizing the overall color transform shape."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the LUT file" },
                "channel": {
                    "type": "string",
                    "enum": ["r", "g", "b", "all"],
                    "description": "Channel to display (default: all)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult, ToolError> {
        let path = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("Missing required parameter: path".into()))?;
        let channel = params
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let lut_data = lut::parse_lut(Path::new(path))
            .map_err(|e| ToolError::ExecutionError(format!("LUT_PARSE_ERROR -- {}", e)))?;

        let curves = extract_diagonal_curves(&lut_data);
        let width = 40usize;
        let height = 12usize;

        let show_r = channel == "all" || channel == "r";
        let show_g = channel == "all" || channel == "g";
        let show_b = channel == "all" || channel == "b";

        let mut output = format!(
            "LUT Curves: {} (grid_size: {})\n\n",
            path, lut_data.grid_size
        );

        if show_r {
            output.push_str("R channel:\n");
            output.push_str(&render_ascii_curve(&curves[0], width, height, 'R'));
            output.push('\n');
        }
        if show_g {
            output.push_str("G channel:\n");
            output.push_str(&render_ascii_curve(&curves[1], width, height, 'G'));
            output.push('\n');
        }
        if show_b {
            output.push_str("B channel:\n");
            output.push_str(&render_ascii_curve(&curves[2], width, height, 'B'));
            output.push('\n');
        }

        // Combined view
        if channel == "all" {
            output.push_str("Combined (R=*, G=+, B=~):\n");
            output.push_str(&render_combined_curve(&curves, width, height));
        }

        Ok(ToolResult::ok(output))
    }
}

// ---------------------------------------------------------------------------
// Curve extraction and rendering
// ---------------------------------------------------------------------------

/// Extract the diagonal curves from a LUT (R=G=B input → R/G/B output).
///
/// For a LUT with grid_size N, this extracts N points along the diagonal
/// where input_r = input_g = input_b.
fn extract_diagonal_curves(lut: &LutData) -> [Vec<f64>; 3] {
    let n = lut.grid_size as usize;
    let mut curves: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];

    for i in 0..n {
        // Index in red-fastest order: r + g*n + b*n*n
        // On diagonal: r = g = b = i
        let idx = (i + i * n + i * n * n) * 3;
        curves[0].push(lut.values[idx]);
        curves[1].push(lut.values[idx + 1]);
        curves[2].push(lut.values[idx + 2]);
    }

    curves
}

/// Render a single channel curve as ASCII art.
#[allow(clippy::needless_range_loop)]
fn render_ascii_curve(values: &[f64], width: usize, height: usize, label: char) -> String {
    let mut grid = vec![vec![' '; width]; height];

    // Axis labels
    for row in grid.iter_mut().take(height) {
        row[0] = '│';
    }
    for (col, cell) in grid[height - 1].iter_mut().enumerate().take(width) {
        *cell = if col == 0 { '└' } else { '─' };
    }

    // Plot the curve
    for col in 0..width {
        let idx = (col as f64 / (width - 1).max(1) as f64 * (values.len() - 1) as f64).round() as usize;
        let idx = idx.min(values.len() - 1);
        let val = values[idx].clamp(0.0, 1.0);
        let row = ((1.0 - val) * (height - 2) as f64).round() as usize;
        let row = row.min(height - 2);
        grid[row][col] = label;
    }

    let mut s = String::new();
    for (i, row) in grid.iter().enumerate() {
        let y_label = if i == 0 {
            "1.0"
        } else if i == height - 2 {
            "   "
        } else if i == height - 1 {
            "0.0"
        } else {
            "   "
        };
        s.push_str(&format!("{} {}\n", y_label, row.iter().collect::<String>()));
    }

    s
}

/// Render all three channels overlaid on one grid.
#[allow(clippy::needless_range_loop)]
fn render_combined_curve(curves: &[Vec<f64>; 3], width: usize, height: usize) -> String {
    let mut grid = vec![vec![b' '; width]; height];
    let chars = [b'*', b'+', b'~']; // R, G, B

    for (ch, curve) in curves.iter().enumerate() {
        for col in 0..width {
            let idx = (col as f64 / (width - 1).max(1) as f64 * (curve.len() - 1) as f64).round() as usize;
            let idx = idx.min(curve.len() - 1);
            let val = curve[idx].clamp(0.0, 1.0);
            let row = ((1.0 - val) * (height - 2) as f64).round() as usize;
            let row = row.min(height - 2);
            grid[row][col] = chars[ch];
        }
    }

    let mut s = String::new();
    for (i, row) in grid.iter().enumerate() {
        let y_label = if i == 0 {
            "1.0"
        } else if i == height - 1 {
            "0.0"
        } else {
            "   "
        };
        s.push_str(&format!(
            "{} {}\n",
            y_label,
            row.iter().map(|&c| c as char).collect::<String>()
        ));
    }

    s
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_extract_diagonal_identity() {
        let lut = LutData::identity(9);
        let curves = extract_diagonal_curves(&lut);
        // Identity: output should equal input
        for ch in 0..3 {
            for (i, &v) in curves[ch].iter().enumerate() {
                let expected = i as f64 / 8.0;
                assert!((v - expected).abs() < 1e-10, "ch={} i={} expected={} got={}", ch, i, expected, v);
            }
        }
    }

    #[test]
    fn test_render_ascii_curve_output() {
        let values: Vec<f64> = (0..=10).map(|i| i as f64 / 10.0).collect();
        let output = render_ascii_curve(&values, 20, 8, 'R');
        assert!(output.contains('R'));
        assert!(output.contains("1.0"));
        assert!(output.contains("0.0"));
    }

    #[test]
    fn test_render_combined() {
        let curves = [
            (0..=10).map(|i| i as f64 / 10.0).collect(),
            (0..=10).map(|i| (i as f64 / 10.0).powf(0.5)).collect(),
            (0..=10).map(|i| (i as f64 / 10.0).powf(2.0)).collect(),
        ];
        let output = render_combined_curve(&curves, 20, 8);
        assert!(output.contains('*'));
        assert!(output.contains('+'));
        assert!(output.contains('~'));
    }

    #[tokio::test]
    async fn test_diff_lut_tool_identity_vs_modified() {
        let tool = DiffLutTool;

        // Create two temp LUT files
        let dir = std::env::temp_dir().join("mengxi_test_lut_diff");
        std::fs::create_dir_all(&dir).unwrap();

        let identity = LutData::identity(5);
        let mut modified = identity.clone();
        // Push shadows warmer
        for (i, v) in modified.values.iter_mut().enumerate() {
            let ch = i % 3;
            if *v < 0.5 {
                if ch == 0 { *v += 0.1; } // add red
            }
        }

        let path_a = dir.join("identity.cube");
        let path_b = dir.join("modified.cube");
        lut::serialize_lut(&identity, &path_a).unwrap();
        lut::serialize_lut(&modified, &path_b).unwrap();

        let result = tool
            .execute(json!({
                "path_a": path_a.to_str().unwrap(),
                "path_b": path_b.to_str().unwrap()
            }))
            .await;

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(!tool_result.is_error);
        assert!(tool_result.content.contains("Per-channel"));
        assert!(tool_result.content.contains("Per-region"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_render_curves_tool() {
        let tool = RenderLutCurvesTool;

        let dir = std::env::temp_dir().join("mengxi_test_lut_curves");
        std::fs::create_dir_all(&dir).unwrap();
        let lut = LutData::identity(17);
        let path = dir.join("test.cube");
        lut::serialize_lut(&lut, &path).unwrap();

        let result = tool
            .execute(json!({"path": path.to_str().unwrap()}))
            .await;

        assert!(result.is_ok());
        let tool_result = result.unwrap();
        assert!(!tool_result.is_error);
        assert!(tool_result.content.contains("R channel"));
        assert!(tool_result.content.contains("Combined"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
