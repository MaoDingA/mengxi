use std::path::Path;

use crate::vectorscope::{VectorscopeDensity, VectorscopeError};

/// Render a vectorscope density grid as a circular PNG heatmap.
///
/// Creates a square image with a polar density visualization:
/// - Black background
/// - Circular heatmap with density → color mapping (black → deep blue → purple → orange → yellow → white)
/// - Thin hue-colored ring at the edge as a legend
pub fn render_vectorscope_png(
    density: &VectorscopeDensity,
    size: u32,
    path: &Path,
) -> std::result::Result<(), VectorscopeError> {
    if size == 0 {
        return Err(VectorscopeError::InvalidInput("size must be > 0".to_string()));
    }

    let angle_bins = density.angle_bins;
    let radius_bins = density.radius_bins;
    let grid = &density.grid;

    let center = size as f64 / 2.0;
    let max_radius = center - 4.0; // leave room for hue ring

    // Create image buffer
    let mut img = vec![0u8; (size * size * 3) as usize];

    // Colormap: density value → RGB
    // Uses a "inferno" style: black → indigo → purple → red → orange → yellow → white
    let density_to_color = |v: f64| -> [u8; 3] {
        if v <= 0.0 {
            [0, 0, 0]
        } else if v <= 0.15 {
            let t = v / 0.15;
            lerp_color(0.0, 0.0, 0.0, 20.0 / 255.0, 0.0, 80.0 / 255.0, t)
        } else if v <= 0.35 {
            let t = (v - 0.15) / 0.20;
            lerp_color(20.0 / 255.0, 0.0, 80.0 / 255.0, 120.0 / 255.0, 0.0, 160.0 / 255.0, t)
        } else if v <= 0.55 {
            let t = (v - 0.35) / 0.20;
            lerp_color(120.0 / 255.0, 0.0, 160.0 / 255.0, 220.0 / 255.0, 40.0 / 255.0, 0.0, t)
        } else if v <= 0.75 {
            let t = (v - 0.55) / 0.20;
            lerp_color(220.0 / 255.0, 40.0 / 255.0, 0.0, 255.0 / 255.0, 180.0 / 255.0, 0.0, t)
        } else if v <= 0.9 {
            let t = (v - 0.75) / 0.15;
            lerp_color(255.0 / 255.0, 180.0 / 255.0, 0.0, 255.0 / 255.0, 255.0 / 255.0, 40.0 / 255.0, t)
        } else {
            let t = (v - 0.9) / 0.1;
            let t = t.min(1.0);
            lerp_color(255.0 / 255.0, 255.0 / 255.0, 40.0 / 255.0, 1.0, 1.0, 1.0, t)
        }
    };

    // Render density grid as circular heatmap
    for py in 0..size {
        for px in 0..size {
            let dx = px as f64 - center;
            let dy = py as f64 - center;
            let r = (dx * dx + dy * dy).sqrt();
            let pixel_idx = (py * size + px) as usize * 3;

            // Hue ring at the edge (2px wide band)
            let ring_inner = max_radius + 2.0;
            let ring_outer = max_radius + 8.0;
            if r >= ring_inner && r <= ring_outer {
                let angle = dy.atan2(dx); // -pi to pi
                let hue_deg = (angle.to_degrees() + 360.0) % 360.0;
                let (rr, gg, bb) = hue_to_rgb(hue_deg);
                img[pixel_idx] = (rr * 255.0).round() as u8;
                img[pixel_idx + 1] = (gg * 255.0).round() as u8;
                img[pixel_idx + 2] = (bb * 255.0).round() as u8;
                continue;
            }

            if r > max_radius {
                continue; // black background outside circle
            }

            // Map pixel to polar grid
            let angle = dy.atan2(dx);
            let hue_deg = (angle.to_degrees() + 360.0) % 360.0;
            let angle_bin = (hue_deg / 360.0 * angle_bins as f64) as usize;
            let angle_bin = angle_bin.min(angle_bins - 1);

            let radius_frac = r / max_radius;
            let radius_bin = (radius_frac * radius_bins as f64) as usize;
            let radius_bin = radius_bin.min(radius_bins - 1);

            let cell = angle_bin * radius_bins + radius_bin;
            let density_val = grid[cell];

            let [cr, cg, cb] = density_to_color(density_val);
            img[pixel_idx] = cr;
            img[pixel_idx + 1] = cg;
            img[pixel_idx + 2] = cb;
        }
    }

    image::save_buffer(path, &img, size, size, image::ExtendedColorType::Rgb8)
        .map_err(|e| VectorscopeError::FfiError(format!("failed to save PNG: {}", e)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn lerp_color(r0: f64, g0: f64, b0: f64, r1: f64, g1: f64, b1: f64, t: f64) -> [u8; 3] {
    let r = r0 + (r1 - r0) * t;
    let g = g0 + (g1 - g0) * t;
    let b = b0 + (b1 - b0) * t;
    [
        (r * 255.0).round().clamp(0.0, 255.0) as u8,
        (g * 255.0).round().clamp(0.0, 255.0) as u8,
        (b * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

/// Convert hue angle (0-360) to RGB using pure spectral colors.
fn hue_to_rgb(hue_deg: f64) -> (f64, f64, f64) {
    let h = hue_deg / 60.0;
    let sector = h.floor() as i32 % 6;
    let f = h - h.floor();
    match sector {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    }
}
