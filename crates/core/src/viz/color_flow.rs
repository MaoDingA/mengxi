// crates/core/src/viz/color_flow.rs
//
// Color Flow fingerprint visualization.
// Three-layer layout: frame strip (top) -> flow arcs (middle) -> glow nodes (bottom).
// Inspired by 电影指纹 (Movie Fingerprint) WeChat public account style.

use crate::color_distribution::{classify_color_distribution, ColorCategory, NUM_CATEGORIES};
use super::font::{draw_text_scaled, draw_text_ttf, measure_text_width};
use std::path::Path;

/// Rasterize a quadratic Bezier curve onto an RGB8 image buffer with alpha blending.
fn draw_bezier_arc(
    img: &mut [u8],
    w: usize,
    h: usize,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    r: u8,
    g: u8,
    b: u8,
    alpha_base: f64,
    thickness: f64,
) {
    const STEPS: usize = 64;
    let mut prev_x = p0.0;
    let mut prev_y = p0.1;

    for i in 1..=STEPS {
        let t = i as f64 / STEPS as f64;
        let inv_t = 1.0 - t;
        let x = inv_t * inv_t * p0.0 + 2.0 * inv_t * t * p1.0 + t * t * p2.0;
        let y = inv_t * inv_t * p0.1 + 2.0 * inv_t * t * p1.1 + t * t * p2.1;

        draw_aa_line(img, w, h, prev_x, prev_y, x, y, r, g, b, alpha_base, thickness);

        prev_x = x;
        prev_y = y;
    }
}

/// Draw an anti-aliased line segment with variable thickness using pixel coverage.
fn draw_aa_line(
    img: &mut [u8],
    w: usize,
    h: usize,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    r: u8,
    g: u8,
    b: u8,
    alpha: f64,
    thickness: f64,
) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 0.001 {
        return;
    }
    let steps = ((dist * 2.0).ceil() as usize).max(2);
    let half_t = thickness / 2.0;

    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let cx = x0 + dx * t;
        let cy = y0 + dy * t;

        let radius_px = (half_t).ceil() as isize;
        for dy_off in -radius_px..=radius_px {
            for dx_off in -radius_px..=radius_px {
                let px = (cx + dx_off as f64).round() as isize;
                let py = (cy + dy_off as f64).round() as isize;
                if px < 0 || (px as usize) >= w || py < 0 || (py as usize) >= h {
                    continue;
                }
                let dd = ((dx_off as f64).powi(2) + (dy_off as f64).powi(2)).sqrt();
                if dd > half_t {
                    continue;
                }
                let edge_alpha = if half_t > 1.0 {
                    let falloff = (half_t - dd) / half_t;
                    falloff * falloff
                } else {
                    1.0
                };
                let a = alpha * edge_alpha;
                blend_pixel(img, w, px as usize, py as usize, r, g, b, a);
            }
        }
    }
}

/// Alpha-blend a pixel onto the buffer.
fn blend_pixel(img: &mut [u8], w: usize, x: usize, y: usize, r: u8, g: u8, b: u8, alpha: f64) {
    let idx = (y * w + x) * 3;
    if idx + 2 >= img.len() {
        return;
    }
    let inv_a = 1.0 - alpha;
    img[idx]     = (img[idx]     as f64 * inv_a + r as f64 * alpha).round() as u8;
    img[idx + 1] = (img[idx + 1] as f64 * inv_a + g as f64 * alpha).round() as u8;
    img[idx + 2] = (img[idx + 2] as f64 * inv_a + b as f64 * alpha).round() as u8;
}

/// Draw a glowing circle (radial gradient fade).
fn draw_glow_circle(
    img: &mut [u8],
    w: usize,
    h: usize,
    cx: f64,
    cy: f64,
    outer_radius: f64,
    inner_radius: f64,
    r: u8,
    g: u8,
    b: u8,
) {
    let ir_outer = outer_radius.ceil() as i32;
    for dy in -ir_outer..=ir_outer {
        for dx in -ir_outer..=ir_outer {
            let dist = ((dx as f64).powi(2) + (dy as f64).powi(2)).sqrt();
            if dist > outer_radius {
                continue;
            }
            let px = (cx + dx as f64).round() as isize;
            let py = (cy + dy as f64).round() as isize;
            if px < 0 || (px as usize) >= w || py < 0 || (py as usize) >= h {
                continue;
            }

            let intensity = if dist <= inner_radius {
                1.0
            } else {
                let t = (dist - inner_radius) / (outer_radius - inner_radius);
                1.0 - t * t
            };

            blend_pixel(img, w, px as usize, py as usize, r, g, b, intensity);
        }
    }
}

/// Fill a solid circle.
fn fill_solid_circle(img: &mut [u8], w: usize, h: usize, cx: f64, cy: f64, radius: f64, r: u8, g: u8, b: u8) {
    let ir = radius.round() as i32;
    let icx = cx.round() as i32;
    let icy = cy.round() as i32;
    for dy in -ir..=ir {
        for dx in -ir..=ir {
            let dist = ((dx as f64).powi(2) + (dy as f64).powi(2)).sqrt();
            if dist <= radius {
                let px = icx + dx;
                let py = icy + dy;
                if px >= 0 && (px as usize) < w && py >= 0 && (py as usize) < h {
                    let idx = (py as usize * w + px as usize) * 3;
                    if idx + 2 < img.len() {
                        img[idx] = r;
                        img[idx + 1] = g;
                        img[idx + 2] = b;
                    }
                }
            }
        }
    }
}

/// Draw the top frame color strip (downsampled vertical bars).
fn draw_frame_strip(
    img: &mut [u8],
    canvas_w: usize,
    x0: usize,
    y0: usize,
    display_w: usize,
    display_h: usize,
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
) {
    for dx in 0..display_w {
        let sx = (dx as f64 * strip_width as f64 / display_w as f64) as usize;
        let sx = sx.min(strip_width - 1);
        for dy in 0..display_h {
            let sy = (dy as f64 * strip_height as f64 / display_h as f64) as usize;
            let sy = sy.min(strip_height - 1);
            let src_idx = (sy * strip_width + sx) * 3;
            let px = x0 + dx;
            let py = y0 + dy;
            if px < canvas_w && src_idx + 2 < strip.len() {
                let dst = (py * canvas_w + px) * 3;
                if dst + 2 < img.len() {
                    img[dst]     = (strip[src_idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                    img[dst + 1] = (strip[src_idx + 1].clamp(0.0, 1.0) * 255.0).round() as u8;
                    img[dst + 2] = (strip[src_idx + 2].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Per-frame classification data
// ---------------------------------------------------------------------------

struct FrameClassification {
    primary_category: usize,
    primary_strength: f64,
    avg_r: f64,
    avg_g: f64,
    avg_b: f64,
}

/// Classify each frame column into its dominant color category.
fn classify_per_frame(
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
    min_chroma: f64,
) -> Vec<FrameClassification> {
    let mut frames = Vec::with_capacity(strip_width);
    for col in 0..strip_width {
        let mut counts = [0usize; NUM_CATEGORIES];
        let mut sum_r = 0.0f64;
        let mut sum_g = 0.0f64;
        let mut sum_b = 0.0f64;
        let mut total = 0usize;

        for row in 0..strip_height {
            let idx = (col * strip_height + row) * 3;
            if idx + 2 >= strip.len() { break; }
            let r = strip[idx];
            let g = strip[idx + 1];
            let bv = strip[idx + 2];

            sum_r += r;
            sum_g += g;
            sum_b += bv;
            total += 1;

            let (l_val, a_val, b_val) = crate::color_distribution::srgb_to_oklab_pixel(r, g, bv);
            if let Some(cat) = crate::color_distribution::classify_pixel(l_val, a_val, b_val, min_chroma) {
                counts[cat as usize] += 1;
            }
        }

        let inv_total = if total > 0 { 1.0 / total as f64 } else { 0.0 };
        let (mut best_cat, mut best_count) = (0usize, 0usize);
        for (i, &c) in counts.iter().enumerate() {
            if c > best_count {
                best_count = c;
                best_cat = i;
            }
        }

        frames.push(FrameClassification {
            primary_category: best_cat,
            primary_strength: if total > 0 { best_count as f64 * inv_total } else { 0.0 },
            avg_r: sum_r * inv_total,
            avg_g: sum_g * inv_total,
            avg_b: sum_b * inv_total,
        });
    }
    frames
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

pub fn render_color_flow_png(
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // --- Canvas dimensions ---
    let cw: usize = 1200;
    let ch: usize = 1800;
    let margin: usize = 40;

    // --- Colors ---
    const BG_R: u8 = 10;
    const BG_G: u8 = 10;
    const BG_B: u8 = 10;

    // --- Region heights (compressed: strip compact, arcs fill, nodes roomy) ---
    let strip_area_h: usize = (ch as f64 * 0.07).round() as usize;   // ~126px — compact
    let arcs_area_h: usize = (ch as f64 * 0.63).round() as usize;   // ~1134px — main visual
    let nodes_area_h: usize = ch - margin - strip_area_h - arcs_area_h - margin; // remainder

    let strip_top = margin;
    let strip_bottom = strip_top + strip_area_h;
    let arcs_top = strip_bottom;           // no gap between strip and arcs
    let arcs_bottom = arcs_top + arcs_area_h;
    let nodes_top = arcs_bottom;            // no gap between arcs and nodes
    let node_y = nodes_top as f64 + nodes_area_h as f64 * 0.12;     // nodes higher up
    let nodes_label_y = nodes_top as f64 + nodes_area_h as f64 * 0.38; // labels close under nodes
    let watermark_y = ch - margin - 20;

    // --- Allocate black background ---
    let mut img = vec![BG_R; cw * ch * 3];
    for i in (0..img.len()).step_by(3) {
        img[i] = BG_R;
        img[i + 1] = BG_G;
        img[i + 2] = BG_B;
    }

    // LAYER 1: Frame color strip (top)
    let strip_display_w = cw - 2 * margin;
    draw_frame_strip(&mut img, cw, margin, strip_top, strip_display_w, strip_area_h, strip, strip_width, strip_height);

    // LAYER 2: Flow arcs (middle) — dense hairline waterfall
    let frames = classify_per_frame(strip, strip_width, strip_height, 0.03);

    let categories = ColorCategory::all();
    let n_nodes = categories.len();
    let node_spacing = strip_display_w / (n_nodes - 1).max(1);
    let node_x_positions: Vec<usize> = (0..n_nodes)
        .map(|i| margin + i * node_spacing)
        .collect();

    let arcs_mid_y = arcs_top as f64 + arcs_area_h as f64 * 0.22;

    // === Dense sampling: draw one ultra-thin hairline per sample point ===
    // This creates the "fiber optic / light ray waterfall" effect of the reference.
    // Target: ~400-800 lines for a typical movie (one every 3-7 frames).
    let target_line_count = 600usize;
    let sample_step = if frames.len() > target_line_count {
        (frames.len() / target_line_count).max(1)
    } else {
        1
    };

    use std::hash::{Hasher, DefaultHasher};
    for (sample_idx, frame_idx) in (0..frames.len()).step_by(sample_step).enumerate() {
        let fc = &frames[frame_idx];

        // Map this frame to X position on the strip
        let fx = margin as f64 + (frame_idx as f64 / strip_width as f64) * strip_display_w as f64;
        let fy = strip_bottom as f64;

        let target_node_x = node_x_positions[fc.primary_category] as f64;

        // Control point — variable arch height for organic variation
        let mid_x = (fx + target_node_x) / 0.5; // wider spread for more dramatic arcs
        let h_dist = (target_node_x - fx).abs();

        // Pseudo-random seed for per-line variation
        let mut hasher = DefaultHasher::new();
        hasher.write_usize(sample_idx * 31 + frame_idx);
        let seed = hasher.finish() as f64 / u64::MAX as f64;

        // Arch height varies: some lines fly high, some stay low
        let pull_base = h_dist * (0.30 + seed * 0.55); // 0.30~0.85 range
        let ctrl_y_actual = arcs_mid_y - pull_base - seed * arcs_area_h as f64 * 0.06;

        // Line properties based on classification strength and seed
        let strength = fc.primary_strength;
        let alpha = 0.04 + strength * 0.12 + seed * 0.05;   // 0.04~0.21
        let thickness = 0.15 + strength * 0.25 + seed * 0.12; // 0.15~0.52

        // Line color: blend between white and the frame's average color
        // Stronger classification → more saturated color, weaker → more white/gray
        let color_tint = (strength * 0.6 + seed * 0.25).min(0.75);
        let base_gray = 190.0 + seed * 40.0; // 190~230 range (light gray base)
        let lr = (base_gray * (1.0 - color_tint) + fc.avg_r * 255.0 * color_tint).round() as u8;
        let lg = (base_gray * (1.0 - color_tint) + fc.avg_g * 255.0 * color_tint).round() as u8;
        let lb = (base_gray * (1.0 - color_tint) + fc.avg_b * 255.0 * color_tint).round() as u8;

        // Primary hairline
        draw_bezier_arc(
            &mut img, cw, ch,
            (fx, fy),
            (mid_x, ctrl_y_actual),
            (target_node_x, node_y),
            lr, lg, lb,
            alpha,
            thickness,
        );

        // Occasional secondary faint line for depth (every 4th sample, long segments only)
        if sample_idx % 4 == 0 && strength > 0.3 {
            let ctrl_offset_x = (seed - 0.5) * 30.0;
            let ctrl_y_2nd = ctrl_y_actual - seed * arcs_area_h as f64 * 0.04;
            draw_bezier_arc(
                &mut img, cw, ch,
                (fx, fy),
                (mid_x + ctrl_offset_x, ctrl_y_2nd),
                (target_node_x, node_y),
                175, 180, 188,       // cool faint gray-blue
                0.02 + strength * 0.03,
                0.10 + seed * 0.08,
            );
        }
    }

    // LAYER 3: Glow nodes + labels (bottom) — size proportional to fraction
    let overall_dist = classify_color_distribution(strip, strip_width, strip_height, 0.03);

    // Base radius for a dominant color (~50%+), scaled by sqrt(fraction)
    let max_glow_radius = (cw as f64 * 0.065).round() as f64;  // ~78px for dominant

    for (i, cat) in categories.iter().enumerate() {
        let nx = node_x_positions[i] as f64;
        let ny = node_y;
        let (cr, cg, cb) = cat.display_rgb();
        let fraction = overall_dist.categories[i][0];

        // Size scales with sqrt(fraction): 62% → large, 0.2% → tiny dot
        let size_scale = if fraction > 0.001 { fraction.sqrt() } else { fraction * 10.0 };
        let glow_outer_r = (max_glow_radius * size_scale).max(6.0);
        let glow_inner_r = glow_outer_r * 0.12;

        draw_glow_circle(&mut img, cw, ch, nx, ny, glow_outer_r, glow_inner_r, cr, cg, cb);

        // Solid core — always visible even for small fractions
        let core_r = (glow_inner_r * 0.5).max(2.0);
        fill_solid_circle(&mut img, cw, ch, nx, ny, core_r, cr, cg, cb);

        let name = cat.name();
        let font_size = 14.0;
        let name_w = measure_text_width(name, font_size, None);
        draw_text_ttf(&mut img, cw, ch,
            (nx - name_w as f64 / 2.0).round() as usize,
            nodes_label_y.round() as usize,
            name, font_size, 200, 200, 200, None);

        let pct_str = format!("{:.1}%", fraction * 100.0);
        let pct_size = 11.0;
        let pct_w = measure_text_width(&pct_str, pct_size, None);
        draw_text_ttf(&mut img, cw, ch,
            (nx - pct_w as f64 / 2.0).round() as usize,
            (nodes_label_y + 18.0).round() as usize,
            &pct_str, pct_size, 140, 140, 140, None);
    }

    // Watermark
    let watermark = "\u{516C}\u{4F17}\u{53F7}\u{B7}\u{7535}\u{5F71}\u{6307}\u{7EB9}";
    let wm_size = 16.0;
    let wm_w = measure_text_width(watermark, wm_size, None);
    draw_text_ttf(&mut img, cw, ch,
        (cw - wm_w) / 2,
        watermark_y,
        watermark, wm_size, 90, 90, 90, None);

    image::save_buffer(
        output_path,
        &img,
        cw as u32,
        ch as u32,
        image::ExtendedColorType::Rgb8,
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_color_flow_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("colorflow_test.png");

        let width = 100usize;
        let height = 50usize;
        let mut strip = Vec::with_capacity(width * height * 3);
        for col in 0..width {
            for _row in 0..height {
                if col < width / 3 {
                    strip.push(0.9); strip.push(0.1); strip.push(0.1);
                } else if col < 2 * width / 3 {
                    strip.push(0.1); strip.push(0.9); strip.push(0.1);
                } else {
                    strip.push(0.1); strip.push(0.1); strip.push(0.9);
                }
            }
        }

        let result = render_color_flow_png(&strip, width, height, &path);
        assert!(result.is_ok(), "render should succeed: {:?}", result.err());
        assert!(path.exists());
        assert!(path.metadata().unwrap().len() > 1000);
    }

    #[test]
    fn test_render_color_flow_single_frame() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("colorflow_single.png");
        let strip: Vec<f64> = (0..90).map(|i| if i % 3 == 1 { 0.8 } else { 0.1 }).collect();

        let result = render_color_flow_png(&strip, 1, 30, &path);
        assert!(result.is_ok(), "single frame should work: {:?}", result.err());
        assert!(path.exists());
    }

    #[test]
    fn test_classify_per_frame_basic() {
        let width = 6;
        let height = 4;
        let mut strip = Vec::new();
        for col in 0..width {
            for _row in 0..height {
                if col < 2 {
                    strip.extend_from_slice(&[0.9, 0.1, 0.1]);
                } else if col < 4 {
                    strip.extend_from_slice(&[0.1, 0.1, 0.9]);
                } else {
                    strip.extend_from_slice(&[0.1, 0.9, 0.1]);
                }
            }
        }

        let frames = classify_per_frame(&strip, width, height, 0.03);
        assert_eq!(frames.len(), 6);
        assert_eq!(frames[0].primary_category, ColorCategory::Red as usize);
        assert_eq!(frames[2].primary_category, ColorCategory::Blue as usize);
        assert_eq!(frames[4].primary_category, ColorCategory::Green as usize);
    }

    #[test]
    fn test_blend_pixel_no_overflow() {
        let mut img = vec![200u8; 3 * 10 * 10];
        blend_pixel(&mut img, 10, 5, 5, 255, 0, 0, 0.5);
        let idx = (5 * 10 + 5) * 3;
        assert!(img[idx] > 200);
        assert!(img[idx] < 255);
        assert!(img[idx + 1] < 200);
    }

    #[test]
    fn test_draw_glow_circle_in_bounds() {
        let mut img = vec![0u8; 3 * 100 * 100];
        draw_glow_circle(&mut img, 100, 100, 50.0, 50.0, 20.0, 5.0, 255, 100, 50);
        let idx = (50 * 100 + 50) * 3;
        assert!(img[idx] > 0 || img[idx + 1] > 0 || img[idx + 2] > 0);
    }
}
