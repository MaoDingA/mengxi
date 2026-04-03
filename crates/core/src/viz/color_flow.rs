// crates/core/src/viz/color_flow.rs
//
// Color Flow fingerprint visualization.
// Three-layer layout: frame strip (top) -> flow arcs (middle) -> glow nodes (bottom).
// Inspired by 电影指纹 (Movie Fingerprint) WeChat public account style.

use crate::color_distribution::{classify_color_distribution, ColorCategory, NUM_CATEGORIES};
use super::font::{draw_text_ttf, measure_text_width};
use std::path::Path;

/// Rasterize a quadratic Bezier curve as a true single-pixel hairline.
///
/// No disc rendering — each sample sets exactly one pixel with very low alpha.
/// The reference image uses this style: hundreds of ultra-faint strands that
/// only become visible where they converge at the nodes.
fn draw_bezier_hairline(
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
) {
    const STEPS: usize = 256; // very fine sampling for smooth curves
    for i in 0..=STEPS {
        let t = i as f64 / STEPS as f64;
        let inv_t = 1.0 - t;
        let x = inv_t * inv_t * p0.0 + 2.0 * inv_t * t * p1.0 + t * t * p2.0;
        let y = inv_t * inv_t * p0.1 + 2.0 * inv_t * t * p1.1 + t * t * p2.1;
        let px = x.round() as isize;
        let py = y.round() as isize;
        if px >= 0 && (px as usize) < w && py >= 0 && (py as usize) < h {
            blend_pixel(img, w, px as usize, py as usize, r, g, b, alpha_base);
        }
    }
}

/// Alpha-blend a pixel onto the buffer.
fn blend_pixel(img: &mut [u8], w: usize, x: usize, y: usize, r: u8, g: u8, b: u8, alpha: f64) {
    let idx = (y * w + x) * 3;
    if idx + 2 >= img.len() { return; }
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
            if dist > outer_radius { continue; }
            let px = (cx + dx as f64).round() as isize;
            let py = (cy + dy as f64).round() as isize;
            if px < 0 || (px as usize) >= w || py < 0 || (py as usize) >= h { continue; }

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
                        img[idx] = r; img[idx + 1] = g; img[idx + 2] = b;
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
    #[allow(dead_code)] avg_r: f64,
    #[allow(dead_code)] avg_g: f64,
    #[allow(dead_code)] avg_b: f64,
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

            sum_r += r; sum_g += g; sum_b += bv;
            total += 1;

            let (l_val, a_val, b_val) = crate::color_distribution::srgb_to_oklab_pixel(r, g, bv);
            if let Some(cat) = crate::color_distribution::classify_pixel(l_val, a_val, b_val, min_chroma) {
                counts[cat as usize] += 1;
            }
        }

        let inv_total = if total > 0 { 1.0 / total as f64 } else { 0.0 };
        let (mut best_cat, mut best_count) = (0usize, 0usize);
        for (i, &c) in counts.iter().enumerate() {
            if c > best_count { best_count = c; best_cat = i; }
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

/// A contiguous segment of frames sharing the same dominant category.
struct ColorSegment {
    category: usize,
    start_frame: usize,   // frame index in the strip
    end_frame: usize,     // exclusive
    center_x: f64,        // x position of segment center on the strip
    width_x: f64,         // visual width of segment on the strip
    strength: f64,        // average classification strength
}

/// Merge consecutive frames into color segments.
fn build_segments(frames: &[FrameClassification], strip_display_w: f64, strip_width: usize) -> Vec<ColorSegment> {
    let mut segments = Vec::new();
    if frames.is_empty() { return segments; }

    let mut seg_start = 0usize;
    let mut seg_cat = frames[0].primary_category;
    let mut seg_strength_sum = frames[0].primary_strength;
    let mut seg_count = 1usize;

    for i in 1..frames.len() {
        if frames[i].primary_category == seg_cat {
            seg_strength_sum += frames[i].primary_strength;
            seg_count += 1;
        } else {
            // Flush current segment
            let fx0 = seg_start as f64 / strip_width as f64 * strip_display_w;
            let fx1 = i as f64 / strip_width as f64 * strip_display_w;
            segments.push(ColorSegment {
                category: seg_cat,
                start_frame: seg_start,
                end_frame: i,
                center_x: (fx0 + fx1) / 2.0,
                width_x: fx1 - fx0,
                strength: seg_strength_sum / seg_count as f64,
            });
            // Start new segment
            seg_start = i;
            seg_cat = frames[i].primary_category;
            seg_strength_sum = frames[i].primary_strength;
            seg_count = 1;
        }
    }

    // Flush last segment
    let fx0 = seg_start as f64 / strip_width as f64 * strip_display_w;
    let fx1 = frames.len() as f64 / strip_width as f64 * strip_display_w;
    segments.push(ColorSegment {
        category: seg_cat,
        start_frame: seg_start,
        end_frame: frames.len(),
        center_x: (fx0 + fx1) / 2.0,
        width_x: fx1 - fx0,
        strength: seg_strength_sum / seg_count as f64,
    });

    segments
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
    // --- Canvas dimensions — compact ---
    let cw: usize = 1100;
    let ch: usize = 1500;
    let margin: usize = 35;

    // --- Colors ---
    const BG_R: u8 = 6;
    const BG_G: u8 = 6;
    const BG_B: u8 = 6;

    // --- Region heights — compact layout matching reference ---
    // Reference proportions: strip ~20%, arcs ~48%, nodes ~28%, margin ~4%
    let strip_area_h: usize = (ch as f64 * 0.20).round() as usize;   // ~300px
    let arcs_area_h: usize = (ch as f64 * 0.48).round() as usize;   // ~720px
    let nodes_area_h: usize = ch - margin - strip_area_h - arcs_area_h - margin; // remainder ~296px

    let strip_top = margin;
    let strip_bottom = strip_top + strip_area_h;
    let arcs_top = strip_bottom;           // no gap
    let arcs_bottom = arcs_top + arcs_area_h;
    let nodes_top = arcs_bottom;

    // Arcs start a few pixels below the strip to avoid overlap
    let arc_start_y = strip_bottom as f64 + 6.0;

    // Node positions
    let node_y = nodes_top as f64 + nodes_area_h as f64 * 0.16;
    let nodes_label_y = nodes_top as f64 + nodes_area_h as f64 * 0.46;

    // --- Allocate dark background ---
    let mut img = vec![BG_R; cw * ch * 3];
    for i in (0..img.len()).step_by(3) {
        img[i] = BG_R; img[i + 1] = BG_G; img[i + 2] = BG_B;
    }

    // LAYER 1: Frame color strip (top)
    let strip_display_w = cw - 2 * margin;
    draw_frame_strip(&mut img, cw, margin, strip_top, strip_display_w, strip_area_h, strip, strip_width, strip_height);

    // LAYER 2: Flow arcs — SEGMENT-BASED, not per-frame
    let frames = classify_per_frame(strip, strip_width, strip_height, 0.03);
    let categories = ColorCategory::all();
    let n_nodes = categories.len();
    let node_spacing = strip_display_w / (n_nodes - 1).max(1);
    let node_x_positions: Vec<usize> = (0..n_nodes)
        .map(|i| margin + i * node_spacing)
        .collect();

    // Build segments from consecutive same-color frame groups
    let segments = build_segments(&frames, strip_display_w as f64, strip_width);

    use std::hash::{Hasher, DefaultHasher};
    for (si, seg) in segments.iter().enumerate() {
        let target_x = node_x_positions[seg.category] as f64;
        let fx = margin as f64 + seg.center_x;

        // Seed for variation within segment
        let mut hasher = DefaultHasher::new();
        hasher.write_usize(si * 31 + seg.start_frame);
        let seed = hasher.finish() as f64 / u64::MAX as f64;

        let h_dist = (target_x - fx).abs();

        // Number of lines per segment: 1-3 based on segment width
        // Wide segments get more lines for richer detail
        let n_lines = if seg.width_x > strip_display_w as f64 * 0.08 {
            3
        } else if seg.width_x > strip_display_w as f64 * 0.03 {
            2
        } else {
            1
        };

        for li in 0..n_lines {
            // Sub-seed per line
            let mut h2 = DefaultHasher::new();
            h2.write_usize(si * 31 + li * 7);
            let line_seed = h2.finish() as f64 / u64::MAX as f64;

            // Offset start X slightly for multi-line segments
            let line_fx = fx + (li as f64 - (n_lines - 1) as f64 / 2.0) * (seg.width_x / (n_lines + 1) as f64);

            // Control point: dramatic sweeping curve — bow outward then converge
            // Pull is large for distant targets, creating elegant arcs
            let pull = h_dist * (0.50 + line_seed * 0.60); // 0.50~1.10
            let mid_x = line_fx + (target_x - line_fx) * (0.35 + line_seed * 0.30); // widely varied control X
            let ctrl_y = arcs_top as f64 - pull - line_seed * arcs_area_h as f64 * 0.08;

            // Line color: desaturated warm/cool gray based on target category
            // Reference shows subtle tinting — like aged paper or faded silk
            let (tint_r, tint_g, tint_b) = categories[seg.category].display_rgb();
            let gray_base = 155.0;
            let color_mix = 0.40; // 40% color — visible tint but muted
            let brightness_var = line_seed * 25.0;
            let line_r = (gray_base * (1.0 - color_mix) + tint_r as f64 * color_mix + brightness_var).round() as u8;
            let line_g = (gray_base * (1.0 - color_mix) + tint_g as f64 * color_mix + brightness_var).round() as u8;
            let line_b = (gray_base * (1.0 - color_mix) + tint_b as f64 * color_mix + brightness_var).round() as u8;

            // Very low alpha — these are delicate strands, only visible en masse near nodes
            let alpha = 0.025 + seg.strength * 0.025 + line_seed * 0.020; // 0.025~0.07

            draw_bezier_hairline(&mut img, cw, ch,
                (line_fx, arc_start_y),
                (mid_x, ctrl_y),
                (target_x, node_y),
                line_r, line_g, line_b, alpha);

            // For wide segments: add one slightly brighter companion line
            if n_lines >= 2 && li == 0 {
                let ctrl_y2 = ctrl_y - line_seed * arcs_area_h as f64 * 0.05;
                let br = (gray_base * 0.6 + tint_r as f64 * 0.4 + 30.0).round() as u8;
                let bg_ = (gray_base * 0.6 + tint_g as f64 * 0.4 + 30.0).round() as u8;
                let bb = (gray_base * 0.6 + tint_b as f64 * 0.4 + 30.0).round() as u8;
                draw_bezier_hairline(&mut img, cw, ch,
                    (line_fx, arc_start_y),
                    (mid_x + (line_seed - 0.5) * 12.0, ctrl_y2),
                    (target_x, node_y),
                    br, bg_, bb, 0.035 + line_seed * 0.025);
            }
        }
    }

    // LAYER 3: Glow nodes + labels
    let overall_dist = classify_color_distribution(strip, strip_width, strip_height, 0.03);

    // Max glow radius for dominant color
    let max_glow_radius = (cw as f64 * 0.095).round() as f64; // ~104px for dominant

    for (i, cat) in categories.iter().enumerate() {
        let nx = node_x_positions[i] as f64;
        let ny = node_y;
        let (cr, cg, cb) = cat.display_rgb();
        let fraction = overall_dist.categories[i][0];

        // Dramatic size scaling: pow(0.5)
        let size_scale = if fraction > 0.001 {
            fraction.powf(0.50)
        } else {
            fraction * 25.0
        };
        let glow_outer_r = (max_glow_radius * size_scale).max(2.5);
        let glow_inner_r = glow_outer_r * 0.05;

        draw_glow_circle(&mut img, cw, ch, nx, ny, glow_outer_r, glow_inner_r, cr, cg, cb);

        // Solid core
        let core_r = (glow_inner_r * 0.55).max(1.5);
        fill_solid_circle(&mut img, cw, ch, nx, ny, core_r, cr, cg, cb);

        // Labels
        let name = cat.name();
        let font_size = 15.0;
        let name_w = measure_text_width(name, font_size, None);
        draw_text_ttf(&mut img, cw, ch,
            (nx - name_w as f64 / 2.0).round() as usize,
            nodes_label_y.round() as usize,
            name, font_size, 230, 230, 230, None);

        let pct_str = format!("{:.1}%", fraction * 100.0);
        let pct_size = 12.0;
        let pct_w = measure_text_width(&pct_str, pct_size, None);
        draw_text_ttf(&mut img, cw, ch,
            (nx - pct_w as f64 / 2.0).round() as usize,
            (nodes_label_y + 19.0).round() as usize,
            &pct_str, pct_size, 145, 145, 145, None);
    }

    // NO watermark — removed per user request

    image::save_buffer(output_path, &img, cw as u32, ch as u32, image::ExtendedColorType::Rgb8)?;

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

    #[test]
    fn test_build_segments_basic() {
        let frames = vec![
            FrameClassification { primary_category: 0, primary_strength: 0.8, avg_r: 0.9, avg_g: 0.1, avg_b: 0.1 },
            FrameClassification { primary_category: 0, primary_strength: 0.7, avg_r: 0.9, avg_g: 0.1, avg_b: 0.1 },
            FrameClassification { primary_category: 2, primary_strength: 0.9, avg_r: 0.1, avg_g: 0.9, avg_b: 0.1 },
            FrameClassification { primary_category: 2, primary_strength: 0.85, avg_r: 0.1, avg_g: 0.9, avg_b: 0.1 },
            FrameClassification { primary_category: 2, primary_strength: 0.88, avg_r: 0.1, avg_g: 0.9, avg_b: 0.1 },
        ];
        let segments = build_segments(&frames, 500.0, 5);
        assert_eq!(segments.len(), 2); // red segment + green segment
        assert_eq!(segments[0].category, 0);
        assert_eq!(segments[0].start_frame, 0);
        assert_eq!(segments[0].end_frame, 2);
        assert_eq!(segments[1].category, 2);
        assert_eq!(segments[1].start_frame, 2);
        assert_eq!(segments[1].end_frame, 5);
    }
}
