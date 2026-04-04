use crate::color_distribution::{classify_color_distribution, ColorCategory, NUM_CATEGORIES};
use crate::color_scatter::extract_frame_scatter;
use crate::movie_fingerprint::cineiris_transform;
use std::path::Path;

use super::cineprint::Thumbnail;
use super::font::{draw_text_scaled, draw_text_ttf, measure_text_width};
use super::cineprint::{draw_rect, draw_thumbnail_scaled};

/// Embedded watermark image (天工异彩 logo).
const WATERMARK_PNG: &[u8] = include_bytes!("../../assets/watermark.png");

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

pub struct PosterMetadata {
    pub title: String,
    pub project_type: String,   // e.g., "电影", "电视剧", "纪录片"
    pub colorist: String,
    pub team: String,           // comma-separated team member names
    pub director: String,
    pub year: String,
    pub duration_min: usize,
    pub font_path: Option<String>,
    pub watermark: bool,        // show embedded logo watermark
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Render a movie fingerprint poster as PNG.
///
/// Layout inspired by movie fingerprint posters:
/// ```
/// ┌──────────────────────────────────────────┐
/// │  TITLE            Colorist     Director   │  ← header
/// │  ──────────────────────────────────────  │
/// │  duration                            year │  ← sub-header
/// │                                          │
/// │           ○ CineIris Circle ○             │  ← main visual
/// │                                          │
/// │                                          │
/// │  STUDIO.                        ■■■■■■   │  ← footer top
/// │  ──────────────────────────────────────  │
/// │  [ dot cloud ]          ══════════════    │  ← footer bottom
/// └──────────────────────────────────────────┘
/// ```
pub fn render_poster_png(
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
    _thumbnails: &[Thumbnail],
    metadata: &PosterMetadata,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // --- Canvas dimensions ---
    let cw: usize = 1200; // canvas width
    let ch: usize = 1800; // canvas height
    let margin: usize = 40;

    // --- Colors ---
    const BG_R: u8 = 232; // #E8E6E1 warm off-white
    const BG_G: u8 = 230;
    const BG_B: u8 = 225;
    const TXT_R: u8 = 30;  // near-black text
    const TXT_G: u8 = 30;
    const TXT_B: u8 = 30;
    const LINE_R: u8 = 30; // separator lines
    const LINE_G: u8 = 30;
    const LINE_B: u8 = 30;

    // --- Region heights ---
    let header_h: usize = 120;
    let subheader_h: usize = 40;
    let footer_top_h: usize = 55;
    let footer_bottom_h: usize = 220;
    let main_top = margin + header_h + subheader_h;
    let main_bottom = ch - margin - footer_top_h - footer_bottom_h;
    let main_h = main_bottom - main_top;
    let footer_top_y = main_bottom;
    let footer_bottom_y = footer_top_y + footer_top_h;

    // --- Allocate buffer ---
    let mut img = vec![BG_R; cw * ch * 3];
    for i in (0..img.len()).step_by(3) {
        img[i] = BG_R;
        img[i + 1] = BG_G;
        img[i + 2] = BG_B;
    }

    // ================================================================
    // 1. HEADER: Two-row layout straddling separator line
    //   Above line: [watermark] Project name (left)   Episode (right)
    //   Below line: Type · Date (left)                 Duration (right)
    // ================================================================
    let fp = metadata.font_path.as_deref();

    // --- Split title into name + episode (last word = episode) ---
    let (proj_name, episode) = if let Some(last_space) = metadata.title.rfind(' ') {
        (&metadata.title[..last_space], &metadata.title[last_space + 1..])
    } else {
        (metadata.title.as_str(), "")
    };

    // Separator line at vertical center of header area
    let header_line_y = margin + header_h / 2;
    let text_gap = 6; // pixels from text baseline to the line

    // --- Draw watermark logo: left of separator line, vertically centered in header ---
    let wm_size = 70; // watermark display size (square-ish)
    let show_watermark = metadata.watermark;
    if show_watermark {
        let wm_y = margin + (header_h - wm_size) / 2; // center in header area
        draw_watermark(&mut img, cw, ch, margin, wm_y, wm_size);
    }
    let line_left = if show_watermark { margin + wm_size + 6 } else { margin }; // separator starts after watermark (tight gap)

    // --- Row 1 above line: Project name (left of line) + Episode (right of line) ---
    let name_size = 36.0;
    let row1_baseline = header_line_y - text_gap;
    let title_x = line_left;
    draw_text_ttf(
        &mut img, cw, ch,
        title_x, row1_baseline - (name_size as usize),
        proj_name,
        name_size, TXT_R, TXT_G, TXT_B,
        fp,
    );

    let right_x = cw - margin;
    if !episode.is_empty() {
        let ep_w = measure_text_width(episode, name_size, fp);
        draw_text_ttf(&mut img, cw, ch,
            right_x.saturating_sub(ep_w), row1_baseline - (name_size as usize),
            episode, name_size, TXT_R, TXT_G, TXT_B, fp);
    }

    // Separator line: shortened on left to leave room for watermark
    draw_hline(&mut img, cw, ch, line_left, cw - margin, header_line_y, LINE_R, LINE_G, LINE_B);

    // --- Row 2 below line: Type+date (left) + Duration (right), same size, flush to line ---
    let sub_size = 15.0;
    let row2_baseline = header_line_y + text_gap; // baseline just below the line
    let type_label = if !metadata.project_type.is_empty() && !metadata.year.is_empty() {
        format!("{} · {}", metadata.project_type, metadata.year)
    } else if !metadata.project_type.is_empty() {
        metadata.project_type.clone()
    } else {
        metadata.year.clone()
    };
    draw_text_ttf(&mut img, cw, ch, title_x, row2_baseline, &type_label, sub_size, TXT_R, TXT_G, TXT_B, fp);

    let dur_str = format!("{}min", metadata.duration_min);
    let dur_w = measure_text_width(&dur_str, sub_size, fp);
    draw_text_ttf(&mut img, cw, ch, right_x.saturating_sub(dur_w), row2_baseline, &dur_str, sub_size, TXT_R, TXT_G, TXT_B, fp);

    // ================================================================
    // 3. MAIN AREA: CineIris circle (centered)
    // ================================================================
    let iris_diameter = ((cw as f64 * 0.78).min(main_h as f64 * 0.88)) as usize;
    // Make even for symmetry
    let iris_diameter = iris_diameter / 2 * 2;
    let iris_cx = cw / 2;
    let iris_cy = main_top + main_h / 2;
    let iris_r = iris_diameter / 2;

    // Generate CineIris data
    let cineiris_data = match cineiris_transform(strip, strip_width, strip_height, iris_diameter) {
        Ok(data) => data,
        Err(_) => {
            // Fallback: just skip the circle if transform fails
            Vec::new()
        }
    };

    if !cineiris_data.is_empty() {
        // Draw CineIris circle onto canvas (centered)
        let id = iris_diameter;
        for py in 0..id {
            for px in 0..id {
                // Only draw pixels within the circle radius from center
                let dx = px as isize - iris_r as isize;
                let dy = py as isize - iris_r as isize;
                let dist_sq = dx * dx + dy * dy;
                let r_sq = (iris_r * iris_r) as i64;
                if dist_sq as i64 > r_sq {
                    continue;
                }

                let src_idx = (py * id + px) * 3;
                let canvas_px = (iris_cx as isize + dx) as usize;
                let canvas_py = (iris_cy as isize + dy) as usize;
                if canvas_px < cw && canvas_py < ch {
                    let dst = (canvas_py * cw + canvas_px) * 3;
                    if src_idx + 2 < cineiris_data.len() && dst + 2 < img.len() {
                        img[dst] = (cineiris_data[src_idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                        img[dst + 1] = (cineiris_data[src_idx + 1].clamp(0.0, 1.0) * 255.0).round() as u8;
                        img[dst + 2] = (cineiris_data[src_idx + 2].clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
            }
        }

        // Center hole: fill with polar dot cloud instead of solid white
        let hole_r = (iris_diameter as f64 * 0.12).round() as usize;
        // Dark background for contrast inside the hole
        fill_circle(&mut img, cw, ch, iris_cx as f64, iris_cy as f64, hole_r as f64, 42, 42, 42);
        // Draw dot cloud inside the hole
        draw_dot_cloud_in_circle(
            &mut img, cw, ch,
            iris_cx, iris_cy,
            hole_r,
            strip, strip_width, strip_height,
        );
    }

    // ================================================================
    // 4. FOOTER TOP: studio name (left) + color palette bar (right)
    // ================================================================
    let ft_y = footer_top_y + 12;

    // Footer top separator line
    let ft_line_y = footer_top_y + footer_top_h - 4;
    draw_hline(&mut img, cw, ch, margin, cw - margin, ft_line_y, LINE_R, LINE_G, LINE_B);

    // ================================================================
    // 5. FOOTER BOTTOM: full-width strip (top) + team name (left) + dot cloud (right)
    // ================================================================
    let fb_top_y = footer_bottom_y + 12;
    let fb_h_strip = footer_bottom_h - 75;   // upper area for full-width strip
    let fb_lower_y = fb_top_y + fb_h_strip + 8;
    let fb_h_lower = footer_bottom_h - fb_h_strip - 25;  // lower area for team + dot cloud

    // Row 1: Full-width color strip
    let strip_area_w = cw - margin * 2;
    if strip_area_w > 10 {
        draw_mini_strip(&mut img, cw, ch, margin, fb_top_y, strip_area_w, fb_h_strip, strip, strip_width, strip_height);
        draw_rect(&mut img, cw, ch, margin, fb_top_y, strip_area_w, fb_h_strip, 40, 40, 40);
    }

    // Row 2 left: Colorist (left-aligned)
    if !metadata.colorist.is_empty() {
        draw_text_ttf(&mut img, cw, ch, margin, fb_lower_y + 4,
            &format!("调光指导：{}", metadata.colorist), 14.0, TXT_R, TXT_G, TXT_B, fp);
    }

    // Row 2 right: Assistant team (right-aligned)
    if !metadata.team.is_empty() {
        let asst_w = measure_text_width(&metadata.team, 13.0, fp);
        draw_text_ttf(&mut img, cw, ch, cw - margin - asst_w, fb_lower_y + 4,
            &metadata.team, 13.0, TXT_R, TXT_G, TXT_B, fp);
    }

    // Save PNG
    image::save_buffer(
        output_path,
        &img,
        cw as u32,
        ch as u32,
        image::ExtendedColorType::Rgb8,
    )?;

    Ok(())
}

/// Draw a polar dot cloud inside a circular boundary (used for CineIris center hole).
fn draw_dot_cloud_in_circle(
    img: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    cx: usize,       // center x of the circle
    cy: usize,       // center y of the circle
    radius: usize,   // radius of the circle (dot cloud is clipped to this)
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
) {
    let scatter = match extract_frame_scatter(strip, strip_width, strip_height) {
        Ok(s) => s,
        Err(_) => return,
    };
    let points = &scatter.points;
    if points.is_empty() {
        return;
    }

    let outer_r = radius as f64;
    let sample_y = strip_height / 2;
    let n = points.len().min(strip_width);

    // Compute polar coords + sRGB colors
    let mut max_chroma = 0.0_f64;
    let mut thetas = Vec::with_capacity(n);
    let mut chromas = Vec::with_capacity(n);
    let mut colors_r = Vec::with_capacity(n);
    let mut colors_g = Vec::with_capacity(n);
    let mut colors_b = Vec::with_capacity(n);

    for col in 0..n {
        let pt = &points[col];
        let c = (pt.a * pt.a + pt.b * pt.b).sqrt();
        if c > max_chroma { max_chroma = c; }
        thetas.push(pt.b.atan2(pt.a));
        chromas.push(c);
        let src_idx = (sample_y * strip_width + col) * 3;
        if src_idx + 2 < strip.len() {
            colors_r.push((strip[src_idx].clamp(0.0, 1.0) * 255.0).round() as u8);
            colors_g.push((strip[src_idx + 1].clamp(0.0, 1.0) * 255.0).round() as u8);
            colors_b.push((strip[src_idx + 2].clamp(0.0, 1.0) * 255.0).round() as u8);
        } else {
            colors_r.push(128); colors_g.push(128); colors_b.push(128);
        }
    }

    const MAX_DOTS: usize = 120;  // fewer dots for small center area
    let stride = if n <= MAX_DOTS { 1 } else { n / MAX_DOTS };
    let effective_max = max_chroma.max(0.005);
    let chroma_scale = outer_r * 0.85 / effective_max;

    for i in (0..n).step_by(stride.max(1)) {
        let r = chromas[i] * chroma_scale;
        let effective_r = if max_chroma < 0.01 {
            (points[i].l * 0.3 + 0.1) * outer_r * 0.6
        } else { r };

        let dx = effective_r * thetas[i].cos();
        let dy = effective_r * thetas[i].sin();

        let jx = (((i as i32).wrapping_mul(13297) & 0xFFFF) as f64 / 65535.0) * 2.5 - 1.25;
        let jy = (((i as i32).wrapping_mul(38429) & 0xFFFF) as f64 / 65535.0) * 2.5 - 1.25;

        let px = (cx as f64 + dx + jx).round() as i32;
        let py = (cy as f64 + dy + jy).round() as i32;

        // Only draw if within the circle boundary
        let dist_sq = ((px - cx as i32) as f64).powi(2) + ((py - cy as i32) as f64).powi(2);
        if dist_sq > (outer_r - 1.0).powi(2) { continue; }

        if px >= 0 && (px as usize) < canvas_w && py >= 0 && (py as usize) < canvas_h {
            let idx = (py as usize * canvas_w + px as usize) * 3;
            if idx + 2 < img.len() {
                img[idx] = colors_r[i];
                img[idx + 1] = colors_g[i];
                img[idx + 2] = colors_b[i];
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Drawing primitives
// ---------------------------------------------------------------------------

/// Draw a horizontal line.
fn draw_hline(img: &mut [u8], w: usize, h: usize, x0: usize, x1: usize, y: usize, r: u8, g: u8, b: u8) {
    for x in x0..x1.min(w) {
        if y < h {
            let idx = (y * w + x) * 3;
            if idx + 2 < img.len() {
                img[idx] = r;
                img[idx + 1] = g;
                img[idx + 2] = b;
            }
        }
    }
}

/// Draw the embedded watermark (天工异彩 logo) at the given position with specified size.
fn draw_watermark(img: &mut [u8], canvas_w: usize, canvas_h: usize, x0: usize, y0: usize, size: usize) {
    let decoded = image::load_from_memory_with_format(WATERMARK_PNG, image::ImageFormat::Png);
    let wm = match decoded {
        Ok(img) => img.to_rgba8(),
        Err(_) => return,
    };
    let (wm_w, wm_h) = (wm.width() as usize, wm.height() as usize);

    // Scale to fit within `size` while preserving aspect ratio
    let scale = size as f64 / wm_w.max(wm_h) as f64;
    let dw = (wm_w as f64 * scale).round() as usize;
    let dh = (wm_h as f64 * scale).round() as usize;

    let wm_raw = wm.as_raw(); // flat RGBA bytes

    for dy in 0..dh {
        for dx in 0..dw {
            // Map display coords → watermark coords
            let sx = (dx as f64 * wm_w as f64 / dw as f64) as usize;
            let sy = (dy as f64 * wm_h as f64 / dh as f64) as usize;
            let sx = sx.min(wm_w - 1);
            let sy = sy.min(wm_h - 1);

            let px = x0 + dx;
            let py = y0 + dy;
            if px < canvas_w && py < canvas_h {
                let src_idx = (sy * wm_w + sx) * 4; // RGBA
                let alpha = wm_raw[src_idx + 3] as f64 / 255.0;
                if alpha < 0.02 {
                    continue; // skip fully transparent pixels
                }

                let dst = (py * canvas_w + px) * 3;
                if dst + 2 < img.len() {
                    let inv_a = 1.0 - alpha;
                    img[dst]     = (img[dst]     as f64 * inv_a + wm_raw[src_idx] as f64 * alpha).round() as u8;
                    img[dst + 1] = (img[dst + 1] as f64 * inv_a + wm_raw[src_idx + 1] as f64 * alpha).round() as u8;
                    img[dst + 2] = (img[dst + 2] as f64 * inv_a + wm_raw[src_idx + 2] as f64 * alpha).round() as u8;
                }
            }
        }
    }
}

/// Fill a solid circle.
fn fill_circle(img: &mut [u8], w: usize, h: usize, cx: f64, cy: f64, radius: f64, r: u8, g: u8, b: u8) {
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

/// Draw a mini version of the strip (scaled down).
fn draw_mini_strip(
    img: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    x0: usize,
    y0: usize,
    display_w: usize,
    display_h: usize,
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
) {
    for dy in 0..display_h {
        for dx in 0..display_w {
            // Map display coords → strip coords (bilinear-ish nearest neighbor)
            let sx = (dx as f64 * strip_width as f64 / display_w as f64) as usize;
            let sy = (dy as f64 * strip_height as f64 / display_h as f64) as usize;
            let sx = sx.min(strip_width - 1);
            let sy = sy.min(strip_height - 1);
            let src_idx = (sy * strip_width + sx) * 3;

            let px = x0 + dx;
            let py = y0 + dy;
            if px < canvas_w && py < canvas_h {
                let dst = (py * canvas_w + px) * 3;
                if src_idx + 2 < strip.len() && dst + 2 < img.len() {
                    img[dst] = (strip[src_idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                    img[dst + 1] = (strip[src_idx + 1].clamp(0.0, 1.0) * 255.0).round() as u8;
                    img[dst + 2] = (strip[src_idx + 2].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
    }
}

/// Draw a color palette bar showing dominant colors by category fraction.
fn draw_color_palette_bar(
    img: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    x0: usize,
    y0: usize,
    bar_w: usize,
    bar_h: usize,
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
) {
    let dist = classify_color_distribution(strip, strip_width, strip_height, 0.03);

    let mut x_off = 0usize;
    for (cat_idx, cat) in ColorCategory::all().iter().enumerate() {
        let fraction = dist.categories[cat_idx][0];
        if fraction < 0.005 {
            continue;
        }
        let seg_w = ((fraction * bar_w as f64).round() as usize).max(1);
        let (r, g, b) = cat.display_rgb();
        for dy in 0..bar_h {
            for dx in 0..seg_w {
                let px = x0 + x_off + dx;
                let py = y0 + dy;
                if px < canvas_w && py < canvas_h {
                    let idx = (py * canvas_w + px) * 3;
                    if idx + 2 < img.len() {
                        img[idx] = r;
                        img[idx + 1] = g;
                        img[idx + 2] = b;
                    }
                }
            }
        }
        x_off += seg_w;
    }
}

/// Draw a polar/radial dot cloud visualization of frame colors.
///
/// Each frame becomes one colored dot arranged in polar coordinates:
///   - Angle (theta) = hue from Oklab atan2(b, a)
///   - Radius (r)    = chroma sqrt(a^2 + b^2)
///   - Dot color     = original sRGB color of that frame (center-row sample)
///   - Background    = dark filled circle for contrast
fn draw_polar_dot_cloud(
    img: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    x0: usize,
    y0: usize,
    size: usize,
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
) {
    // --- Extract per-frame Oklab scatter via FFI ---
    let scatter = match extract_frame_scatter(strip, strip_width, strip_height) {
        Ok(s) => s,
        Err(_) => return,
    };
    let points = &scatter.points;
    if points.is_empty() {
        return;
    }

    let cx = x0 + size / 2;
    let cy = y0 + size / 2;
    let outer_r = (size / 2) as f64;

    // --- Sample original sRGB colors from strip center row (matches extract_frame_scatter) ---
    let sample_y = strip_height / 2;
    let n = points.len().min(strip_width);

    // --- Compute polar coordinates and find max chroma ---
    let mut max_chroma = 0.0_f64;
    let mut thetas = Vec::with_capacity(n);
    let mut chromas = Vec::with_capacity(n);
    let mut colors_r = Vec::with_capacity(n);
    let mut colors_g = Vec::with_capacity(n);
    let mut colors_b = Vec::with_capacity(n);

    for col in 0..n {
        let pt = &points[col];
        let c = (pt.a * pt.a + pt.b * pt.b).sqrt();
        if c > max_chroma {
            max_chroma = c;
        }
        thetas.push(pt.b.atan2(pt.a));
        chromas.push(c);

        let src_idx = (sample_y * strip_width + col) * 3;
        if src_idx + 2 < strip.len() {
            colors_r.push((strip[src_idx].clamp(0.0, 1.0) * 255.0).round() as u8);
            colors_g.push((strip[src_idx + 1].clamp(0.0, 1.0) * 255.0).round() as u8);
            colors_b.push((strip[src_idx + 2].clamp(0.0, 1.0) * 255.0).round() as u8);
        } else {
            colors_r.push(128);
            colors_g.push(128);
            colors_b.push(128);
        }
    }

    // --- Draw dark background circle for contrast against off-white poster bg ---
    fill_circle(img, canvas_w, canvas_h, cx as f64, cy as f64, outer_r, 42, 42, 42);

    // --- Determine scale and stride ---
    const MAX_DOTS: usize = 300;
    let stride = if n <= MAX_DOTS { 1 } else { n / MAX_DOTS };

    // Chroma-to-radius mapping: max chroma -> 85% of circle radius
    let effective_max = max_chroma.max(0.005);
    let chroma_scale = outer_r * 0.85 / effective_max;

    // --- Draw dots ---
    for i in (0..n).step_by(stride.max(1)) {
        let r = chromas[i] * chroma_scale;

        // For monochrome movies, use lightness-based radial scattering
        let effective_r = if max_chroma < 0.01 {
            let l_val = points[i].l;
            (l_val * 0.3 + 0.1) * outer_r * 0.6
        } else {
            r
        };

        let dx = effective_r * thetas[i].cos();
        let dy = effective_r * thetas[i].sin();

        // Deterministic jitter based on frame index (avoids mechanical ring patterns)
        let jx = (((i as i32).wrapping_mul(13297) & 0xFFFF) as f64 / 65535.0) * 3.0 - 1.5;
        let jy = (((i as i32).wrapping_mul(38429) & 0xFFFF) as f64 / 65535.0) * 3.0 - 1.5;

        let px = (cx as f64 + dx + jx).round() as i32;
        let py = (cy as f64 + dy + jy).round() as i32;

        if px >= 0 && (px as usize) < canvas_w && py >= 0 && (py as usize) < canvas_h {
            let idx = (py as usize * canvas_w + px as usize) * 3;
            if idx + 2 < img.len() {
                img[idx] = colors_r[i];
                img[idx + 1] = colors_g[i];
                img[idx + 2] = colors_b[i];
            }
        }
    }
}
