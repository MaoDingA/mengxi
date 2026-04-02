use crate::color_distribution::{classify_color_distribution, ColorCategory};
use std::path::Path;

/// Render a color distribution network visualization as PNG.
///
/// Layout (top to bottom):
/// 1. Fingerprint strip image
/// 2. Connecting lines from strip to category node
/// 3. 7 color category circles with labels
pub fn render_color_distribution_png(
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let dist = classify_color_distribution(strip, strip_width, strip_height, 0.03);

    // Layout constants
    let node_radius: u32 = 30;
    let node_spacing: u32 = 100;
    let strip_display_height: u32 = 60;
    let gap: u32 = 40;
    let top_margin: u32 = 30;
    let bottom_margin: u32 = 60;
    let label_height: u32 = 5;

    let total_width: u32 = node_spacing * 7 + 40;
    let total_height: u32 = top_margin + strip_display_height + gap + node_radius * 2 + label_height + bottom_margin;

    let mut img = vec![0u8; (total_width * total_height * 3) as usize];

    // --- Draw fingerprint strip ---
    let strip_x_offset = ((total_width - strip_width as u32) / 2) as i32;
    for col in 0..strip_width {
        for row in 0..strip_display_height as usize {
            let src_row = (row as f64 / strip_display_height as f64 * strip_height as f64) as usize;
            let src_idx = (col * strip_height + src_row.min(strip_height - 1)) * 3;
            if src_idx + 2 >= strip.len() {
                break;
            }
            let px = strip_x_offset + col as i32;
            let py = top_margin as i32 + row as i32;
            if px < 0 || px >= total_width as i32 || py < 0 || py >= total_height as i32 {
                continue;
            }
            let dst = ((py as u32 * total_width + px as u32) * 3) as usize;
            img[dst] = (strip[src_idx].clamp(0.0, 1.0) * 255.0).round() as u8;
            img[dst + 1] = (strip[src_idx + 1].clamp(0.0, 1.0) * 255.0).round() as u8;
            img[dst + 2] = (strip[src_idx + 2].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }

    // --- Draw connecting lines and category nodes ---
    let nodes_y = top_margin + strip_display_height + gap + node_radius;
    let nodes_start_x = (total_width - node_spacing * 6) / 2;

    for (cat_idx, cat) in ColorCategory::all().iter().enumerate() {
        let cat = *cat;
        let i = cat_idx as usize;
        let cx = nodes_start_x + cat_idx as u32 * node_spacing;
        let cy = nodes_y;
        let fraction = dist.categories[i][0];
        let (display_r, display_g, display_b) = cat.display_rgb();

        // Draw connecting lines from strip to node
        if fraction > 0.005 {
            let line_alpha = (fraction * 3.0).min(1.0);
            let strip_bottom = top_margin + strip_display_height;
            let num_lines = (fraction * 30.0).ceil() as u32;
            for li in 0..num_lines {
                let t = if num_lines > 1 {
                    li as f64 / (num_lines - 1) as f64
                } else {
                    0.5
                };
                let sx = strip_x_offset + (t * strip_width as f64) as i32;
                let sy = strip_bottom as i32;
                let ex = cx as i32;
                let ey = (cy - node_radius) as i32;
                draw_line_aa(
                    &mut img, total_width, total_height,
                    sx, sy, ex, ey,
                    display_r, display_g, display_b, line_alpha,
                );
            }
        }
        // Draw filled circle for category node
        let node_size = (node_radius as f64 * (0.5 + fraction * 1.5)).min(node_radius as f64 * 1.5) as u32;
        for dy in 0..=node_size * 2 {
            for dx in 0..=node_size * 2 {
                let dist_from_center = ((dx as f64 - node_size as f64).powi(2)
                    + (dy as f64 - node_size as f64).powi(2))
                .sqrt();
                if dist_from_center <= node_size as f64 {
                    let px = cx as i32 - node_size as i32 + dx as i32;
                    let py = cy as i32 - node_size as i32 + dy as i32;
                    if px >= 0 && px < total_width as i32 && py >= 0 && py < total_height as i32 {
                        let dst = ((py as u32 * total_width + px as u32) * 3) as usize;
                        let edge_dist = node_size as f64 - dist_from_center;
                        let alpha = (edge_dist / 2.0).min(1.0);
                        blend_pixel(&mut img[dst..dst + 3], display_r, display_g, display_b, alpha);
                    }
                }
            }
        }
        // Draw label below node
        let label = format!("{} {:.0}%", cat.name(), fraction * 100.0);
        let label_x = cx as i32 - (label.len() as i32 * 3);
        let label_y = cy + node_radius + 8;
        super::font::draw_text_simple(
            &mut img,
            total_width as usize,
            total_height as usize,
            label_x as usize,
            label_y as usize,
            &label,
        );
    }

    // Save as PNG
    image::save_buffer(
        output_path,
        &img,
        total_width,
        total_height,
        image::ExtendedColorType::Rgb8,
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Drawing helpers (minimal, no external deps)
// ---------------------------------------------------------------------------

fn blend_pixel(dst: &mut [u8], r: u8, g: u8, b: u8, alpha: f64) {
    let a = alpha.clamp(0.0, 1.0);
    dst[0] = (dst[0] as f64 * (1.0 - a) + r as f64 * a).round() as u8;
    dst[1] = (dst[1] as f64 * (1.0 - a) + g as f64 * a).round() as u8;
    dst[2] = (dst[2] as f64 * (1.0 - a) + b as f64 * a).round() as u8;
}

/// Bresenham-style anti-aliased line
fn draw_line_aa(
    img: &mut [u8],
    w: u32,
    h: u32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    r: u8,
    g: u8,
    b: u8,
    alpha: f64,
) {
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let steps = dx.max(dy).max(1);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let px = (x0 as f64 + (x1 - x0) as f64 * t).round() as i32;
        let py = (y0 as f64 + (y1 - y0) as f64 * t).round() as i32;
        if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
            let idx = ((py as u32 * w + px as u32) * 3) as usize;
            blend_pixel(&mut img[idx..idx + 3], r, g, b, alpha * 0.6);
        }
    }
}
