use crate::color_distribution::{classify_color_distribution, ColorCategory};
use gif::{Encoder, Frame, Repeat};
use std::fs::File;
use std::path::Path;

// ---------------------------------------------------------------------------
// Segment: a contiguous run of frames sharing the same dominant color category
// ---------------------------------------------------------------------------

struct Segment {
    start: usize,
    end: usize, // exclusive
    category: ColorCategory,
}

/// Classify each frame into its dominant color category, then merge consecutive
/// same-category frames into segments.
fn build_segments(strip: &[f64], strip_width: usize, strip_height: usize) -> Vec<Segment> {
    let mut frame_cats: Vec<Option<ColorCategory>> = Vec::with_capacity(strip_width);
    let min_chroma = 0.03;

    for col in 0..strip_width {
        let mut counts = [0usize; 7];
        for row in 0..strip_height {
            let idx = (col * strip_height + row) * 3;
            if idx + 2 >= strip.len() {
                break;
            }
            let r = strip[idx];
            let g = strip[idx + 1];
            let b = strip[idx + 2];

            // Inline Oklab classification (same logic as color_distribution.rs)
            let lr = if r <= 0.04045 { r / 12.92 } else { ((r + 0.055) / 1.055).powf(2.4) };
            let lg = if g <= 0.04045 { g / 12.92 } else { ((g + 0.055) / 1.055).powf(2.4) };
            let lb = if b <= 0.04045 { b / 12.92 } else { ((b + 0.055) / 1.055).powf(2.4) };

            let lms_l = 0.4122214708 * lr + 0.5363325363 * lg + 0.0514459929 * lb;
            let lms_m = 0.2119034982 * lr + 0.6806995451 * lg + 0.1073969566 * lb;
            let lms_s = 0.0883024619 * lr + 0.2817188376 * lg + 0.6299787005 * lb;

            let lc = lms_l.max(0.0).cbrt();
            let mc = lms_m.max(0.0).cbrt();
            let sc = lms_s.max(0.0).cbrt();

            let ok_a = 1.9779984951 * lc - 2.4285922050 * mc + 0.4505937099 * sc;
            let ok_b = 0.0259040371 * lc + 0.7827717662 * mc - 0.8086757660 * sc;
            let chroma = (ok_a * ok_a + ok_b * ok_b).sqrt();

            if chroma < min_chroma {
                continue; // neutral, skip
            }

            let deg = ok_b.atan2(ok_a).to_degrees();
            let deg = if deg < 0.0 { deg + 360.0 } else { deg };

            let cat_idx = if deg >= 15.0 && deg < 45.0 && chroma < 0.15 {
                1 // Skin
            } else if deg >= 345.0 || deg < 45.0 {
                0 // Red
            } else if deg < 70.0 {
                2 // Yellow
            } else if deg < 165.0 {
                3 // Green
            } else if deg < 200.0 {
                4 // Cyan
            } else if deg < 270.0 {
                5 // Blue
            } else {
                6 // Magenta
            };
            counts[cat_idx] += 1;
        }

        // Dominant category for this frame
        let dom = counts.iter().enumerate().max_by_key(|&(_, c)| c);
        match dom {
            Some((idx, &count)) if count > 0 => {
                frame_cats.push(Some(match idx {
                    0 => ColorCategory::Red,
                    1 => ColorCategory::Skin,
                    2 => ColorCategory::Yellow,
                    3 => ColorCategory::Green,
                    4 => ColorCategory::Cyan,
                    5 => ColorCategory::Blue,
                    _ => ColorCategory::Magenta,
                }));
            }
            _ => frame_cats.push(None),
        }
    }

    // Merge consecutive same-category frames into segments
    let mut segments = Vec::new();
    let mut i = 0;
    while i < strip_width {
        match frame_cats[i] {
            None => {
                i += 1;
                continue;
            }
            Some(cat) => {
                let start = i;
                while i < strip_width && frame_cats[i] == Some(cat) {
                    i += 1;
                }
                segments.push(Segment { start, end: i, category: cat });
            }
        }
    }

    segments
}

// ---------------------------------------------------------------------------
// Main renderer — animated GIF output
// ---------------------------------------------------------------------------

/// Render an animated color distribution network as a breathing GIF.
///
/// Layout (top to bottom):
/// 1. Fingerprint strip (each frame = 1 vertical line)
/// 2. Curved connecting lines grouped by segment → category node
/// 3. Glowing/pulsating category nodes with labels
pub fn render_color_distribution_png(
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let dist = classify_color_distribution(strip, strip_width, strip_height, 0.03);
    let segments = build_segments(strip, strip_width, strip_height);

    // Layout constants
    let node_radius_base: f64 = 24.0;
    let node_spacing: u32 = 110;
    let strip_display_h: u32 = 80;
    let gap: u32 = 120; // space between strip and nodes for lines to breathe
    let top_margin: u32 = 10;
    let bottom_margin: u32 = 50;
    let label_gap: u32 = 30;

    let total_w: u32 = node_spacing * 7 + 20;
    let total_h: u32 =
        top_margin + strip_display_h + gap + (node_radius_base as u32 * 3) + label_gap + bottom_margin;

    let categories = ColorCategory::all();

    // Precompute node positions
    let nodes_start_x = (total_w - node_spacing * 6) / 2;
    let node_y = top_margin + strip_display_h + gap + node_radius_base as u32;
    let strip_bottom_y = top_margin + strip_display_h;
    let strip_x_off = ((total_w - strip_width as u32) / 2) as i32;

    // Animation parameters
    const NUM_FRAMES: u32 = 48;
    const BREATH_PERIOD: f64 = std::f64::consts::TAU / 16.0; // ~3 full breaths over 48 frames

    // Create GIF encoder
    let file = File::create(output_path)?;
    let mut encoder = Encoder::new(file, total_w as u16, total_h as u16, &[])?;
    encoder.set_repeat(Repeat::Infinite)?;

    for frame_idx in 0..NUM_FRAMES {
        let t = frame_idx as f64 * BREATH_PERIOD;
        let breath = (t).sin(); // -1 .. 1
        let breath_norm = (breath + 1.0) / 2.0; // 0 .. 1

        let mut img = vec![0u8; (total_w * total_h * 3) as usize];

        // === 1. Draw fingerprint strip (each frame = 1px wide vertical bar) ===
        for col in 0..strip_width {
            for row in 0..strip_display_h as usize {
                let src_row = (row as f64 / strip_display_h as f64 * strip_height as f64) as usize;
                let src_idx = (col * strip_height + src_row.min(strip_height - 1)) * 3;
                if src_idx + 2 >= strip.len() {
                    break;
                }
                let px = strip_x_off + col as i32;
                let py = top_margin as i32 + row as i32;
                if px < 0 || px >= total_w as i32 || py < 0 || py >= total_h as i32 {
                    continue;
                }
                let dst = ((py as u32 * total_w + px as u32) * 3) as usize;
                img[dst] = (strip[src_idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                img[dst + 1] = (strip[src_idx + 1].clamp(0.0, 1.0) * 255.0).round() as u8;
                img[dst + 2] = (strip[src_idx + 2].clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }

        // === 2. Draw segment-based connecting curves ===
        for seg in &segments {
            let cat_idx = seg.category as usize;
            let fraction = dist.categories[cat_idx][0];
            if fraction < 0.005 {
                continue;
            }

            let cx = (nodes_start_x + cat_idx as u32 * node_spacing) as f64;
            let cy = node_y as f64;

            // Segment center x on strip
            let seg_center_x = strip_x_off as f64
                + (seg.start as f64 + (seg.end - seg.start) as f64 / 2.0);
            let seg_left_x = strip_x_off as f64 + seg.start as f64;
            let seg_right_x = strip_x_off as f64 + seg.end as f64;
            let sy = strip_bottom_y as f64;

            let (dr, dg, db) = seg.category.display_rgb();

            // Line alpha breathes slightly out of phase
            let line_alpha = 0.55 + 0.25 * (t + cat_idx as f64 * 0.5).sin();

            // Draw two boundary curves that "envelope" the segment area
            // Left edge curve: from (seg_left_x, strip_bottom) to node top
            draw_curve(
                &mut img, total_w, total_h,
                seg_left_x, sy,
                cx, cy - node_radius_base,
                dr, dg, db, line_alpha,
            );
            // Right edge curve
            draw_curve(
                &mut img, total_w, total_h,
                seg_right_x, sy,
                cx, cy - node_radius_base,
                dr, dg, db, line_alpha,
            );
            // Center curve (slightly brighter)
            draw_curve(
                &mut img, total_w, total_h,
                seg_center_x, sy,
                cx, cy - node_radius_base,
                dr, dg, db, line_alpha * 1.3,
            );
        }

        // === 3. Draw glowing/breathing category nodes ===
        for (cat_idx, cat) in categories.iter().enumerate() {
            let cx = (nodes_start_x + cat_idx as u32 * node_spacing) as f64;
            let cy = node_y as f64;
            let fraction = dist.categories[cat_idx][0];
            let (dr, dg, db) = cat.display_rgb();

            // Pulsating base size
            let pulse = 1.0 + 0.18 * breath;
            let node_r = node_radius_base * pulse * (0.6 + fraction * 1.4).min(2.0);

            // Outer glow layers (multiple passes with decreasing alpha)
            for glow_layer in (0..4).rev() {
                let glow_r = node_r + glow_layer as f64 * 12.0;
                let glow_alpha = (0.08 - glow_layer as f64 * 0.018) * (0.7 + 0.3 * breath_norm);
                draw_filled_circle(&mut img, total_w, total_h, cx, cy, glow_r, dr, dg, db, glow_alpha);
            }

            // Core filled circle
            draw_filled_circle(&mut img, total_w, total_h, cx, cy, node_r, dr, dg, db, 0.85);

            // Bright center highlight
            let highlight_r = node_r * 0.35;
            draw_filled_circle(&mut img, total_w, total_h, cx, cy, highlight_r, 255, 255, 255, 0.25 * breath_norm);

            // Label below node
            let label = format!("{} {:.1}%", cat.name(), fraction * 100.0);
            let label_x = cx as i32 - (label.len() as i32 * 3);
            let label_y = cy as i32 + node_r as i32 + 14;
            super::font::draw_text_simple(
                &mut img,
                total_w as usize,
                total_h as usize,
                label_x as usize,
                label_y as usize,
                &label,
            );
        }

        // Encode this frame into GIF (~33ms per frame ≈ 30fps)
        let mut frame = Frame::from_rgb(total_w as u16, total_h as u16, &img);
        frame.delay = 4; // 4/100s = 40ms per frame
        frame.dispose = gif::DisposalMethod::Any;
        encoder.write_frame(&frame)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Drawing primitives
// ---------------------------------------------------------------------------

fn blend_pixel(dst: &mut [u8], r: u8, g: u8, b: u8, alpha: f64) {
    let a = alpha.clamp(0.0, 1.0);
    dst[0] = (dst[0] as f64 * (1.0 - a) + r as f64 * a).round() as u8;
    dst[1] = (dst[1] as f64 * (1.0 - a) + g as f64 * a).round() as u8;
    dst[2] = (dst[2] as f64 * (1.0 - a) + b as f64 * a).round() as u8;
}

/// Draw a quadratic Bezier curve from (x0,y0) to (x1,y1) with control point
/// pushed outward to create a natural arc.
fn draw_curve(
    img: &mut [u8],
    w: u32,
    h: u32,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    r: u8,
    g: u8,
    b: u8,
    alpha: f64,
) {
    // Control point: push midpoint downward to create arc
    let mx = (x0 + x1) / 2.0;
    let my = (y0 + y1) / 2.0 + (y1 - y0).abs() * 0.35;

    let steps = 24;
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        // Quadratic bezier: B(t) = (1-t)²P0 + 2(1-t)tC + t²P1
        let inv_t = 1.0 - t;
        let px = inv_t * inv_t * x0 + 2.0 * inv_t * t * mx + t * t * x1;
        let py = inv_t * inv_t * y0 + 2.0 * inv_t * t * my + t * t * y1;

        let ix = px.round() as i32;
        let iy = py.round() as i32;
        if ix >= 0 && ix < w as i32 && iy >= 0 && iy < h as i32 {
            let idx = ((iy as u32 * w + ix as u32) * 3) as usize;
            blend_pixel(&mut img[idx..idx + 3], r, g, b, alpha * 0.7);
        }
    }
}

/// Draw a filled anti-aliased circle with additive blending.
fn draw_filled_circle(
    img: &mut [u8],
    w: u32,
    h: u32,
    cx: f64,
    cy: f64,
    radius: f64,
    r: u8,
    g: u8,
    b: u8,
    alpha: f64,
) {
    let ir = radius.round() as i32;
    let icx = cx.round() as i32;
    let icy = cy.round() as i32;

    for dy in -ir..=ir {
        for dx in -ir..=ir {
            let dist = ((dx as f64).powi(2) + (dy as f64).powi(2)).sqrt();
            if dist <= radius {
                let px = icx + dx;
                let py = icy + dy;
                if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                    let idx = ((py as u32 * w + px as u32) * 3) as usize;
                    // Edge softening: fade at boundary
                    let edge = (radius - dist) / radius.max(1.0);
                    let a = alpha * edge.min(1.0);
                    blend_pixel(&mut img[idx..idx + 3], r, g, b, a);
                }
            }
        }
    }
}
