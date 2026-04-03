use crate::color_distribution::{classify_color_distribution, ColorCategory, NUM_CATEGORIES};
use crate::movie_fingerprint::cineiris_transform;
use std::path::Path;

use super::cineprint::Thumbnail;
use super::font::{draw_text_scaled, draw_text_ttf, measure_text_width};
use super::cineprint::{draw_rect, draw_thumbnail_scaled};

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

pub struct PosterMetadata {
    pub title: String,
    pub colorist: String,
    pub director: String,
    pub year: String,
    pub duration_min: usize,
    pub font_path: Option<String>,
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
/// │  [thumb][thumb]         ══════════════    │  ← footer bottom
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
    let header_h: usize = 100;
    let subheader_h: usize = 45;
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
    // 1. HEADER: Title (left large) + colorist/director (right stacked)
    // ================================================================
    let fp = metadata.font_path.as_deref();
    let title_size = 52.0; // large title in pixels
    draw_text_ttf(
        &mut img, cw, ch,
        margin, margin + 8,
        &metadata.title,
        title_size, TXT_R, TXT_G, TXT_B,
        fp,
    );

    // Right side: colorist (top line) + director (bottom line)
    let right_x = cw - margin;
    let name_size = 16.0;
    // Measure and draw colorist — right-aligned
    let colorist_w = measure_text_width(&metadata.colorist, name_size, fp);
    draw_text_ttf(
        &mut img, cw, ch,
        right_x.saturating_sub(colorist_w), margin + 8,
        &metadata.colorist,
        name_size, TXT_R, TXT_G, TXT_B,
        fp,
    );
    // Director — right-aligned, below colorist
    let dir_w = measure_text_width(&metadata.director, name_size, fp);
    draw_text_ttf(
        &mut img, cw, ch,
        right_x.saturating_sub(dir_w), margin + 8 + (name_size as usize) + 6,
        &metadata.director,
        name_size, TXT_R, TXT_G, TXT_B,
        fp,
    );

    // Header separator line
    let header_line_y = margin + header_h - 3;
    draw_hline(&mut img, cw, ch, margin, cw - margin, header_line_y, LINE_R, LINE_G, LINE_B);

    // ================================================================
    // 2. SUB-HEADER: duration (left) + year (right)
    // ================================================================
    let sub_y = margin + header_h + 12;
    let dur_str = format!("{}min", metadata.duration_min);
    draw_text_ttf(&mut img, cw, ch, margin, sub_y, &dur_str, 14.0, TXT_R, TXT_G, TXT_B, fp);

    let year_str = metadata.year.clone();
    let year_w = measure_text_width(&year_str, 14.0, fp);
    draw_text_ttf(&mut img, cw, ch, right_x.saturating_sub(year_w), sub_y, &year_str, 14.0, TXT_R, TXT_G, TXT_B, fp);

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

        // White center hole (radius = 12% of iris radius)
        let hole_r = (iris_diameter as f64 * 0.12).round() as usize;
        fill_circle(&mut img, cw, ch, iris_cx as f64, iris_cy as f64, hole_r as f64, 232, 230, 225);
    }

    // ================================================================
    // 4. FOOTER TOP: studio name (left) + color palette bar (right)
    // ================================================================
    let ft_y = footer_top_y + 12;

    // Studio/creator label (left)
    draw_text_ttf(&mut img, cw, ch, margin, ft_y, "FADEVYIN.", 14.0, TXT_R, TXT_G, TXT_B, fp);

    // Color palette bar (right) — based on color distribution classification
    let palette_bar_w = 120usize;
    let palette_bar_h = 14usize;
    let palette_x = cw - margin - palette_bar_w;
    draw_color_palette_bar(
        &mut img, cw, ch,
        palette_x, ft_y,
        palette_bar_w, palette_bar_h,
        strip, strip_width, strip_height,
    );

    // Footer top separator line
    let ft_line_y = footer_top_y + footer_top_h - 4;
    draw_hline(&mut img, cw, ch, margin, cw - margin, ft_line_y, LINE_R, LINE_G, LINE_B);

    // ================================================================
    // 5. FOOTER BOTTOM: 2 mini thumbnails (left) + full strip (right)
    // ================================================================
    let fb_y = footer_bottom_y + 15;
    let fb_h = footer_bottom_h - 25; // available height for content

    // Left side: two mini squares
    let thumb_size = fb_h.min(90); // max 90px per thumbnail
    let thumb_gap = 12;

    // Mini strip thumbnail (leftmost)
    draw_mini_strip(&mut img, cw, ch, margin, fb_y, thumb_size, thumb_size, strip, strip_width, strip_height);
    draw_rect(&mut img, cw, ch, margin, fb_y, thumb_size, thumb_size, 40, 40, 40);

    // Mini CineIris thumbnail (next to it)
    let thumb2_x = margin + thumb_size + thumb_gap;
    let mini_iris_d = thumb_size;
    let mini_iris = match cineiris_transform(strip, strip_width, strip_height, mini_iris_d) {
        Ok(data) => data,
        Err(_) => Vec::new(),
    };
    if !mini_iris.is_empty() {
        for py in 0..mini_iris_d {
            for px in 0..mini_iris_d {
                let dx = px as isize - mini_iris_d as isize / 2;
                let dy = py as isize - mini_iris_d as isize / 2;
                let dist_sq = dx * dx + dy * dy;
                let r_sq = (mini_iris_d as isize / 2) * (mini_iris_d as isize / 2);
                if dist_sq > r_sq { continue; }
                let si = (py * mini_iris_d + px) * 3;
                let cpx = thumb2_x + px;
                let cpy = fb_y + py;
                if cpx < cw && cpy < ch && si + 2 < mini_iris.len() {
                    let di = (cpy * cw + cpx) * 3;
                    if di + 2 < img.len() {
                        img[di] = (mini_iris[si].clamp(0.0, 1.0) * 255.0).round() as u8;
                        img[di + 1] = (mini_iris[si + 1].clamp(0.0, 1.0) * 255.0).round() as u8;
                        img[di + 2] = (mini_iris[si + 2].clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
            }
        }
        // White hole in mini iris too
        let mini_hole = (mini_iris_d as f64 * 0.12).round() as usize;
        fill_circle(&mut img, cw, ch, (thumb2_x + mini_iris_d / 2) as f64, (fb_y + mini_iris_d / 2) as f64, mini_hole as f64, 232, 230, 225);
    }
    draw_rect(&mut img, cw, ch, thumb2_x, fb_y, thumb_size, thumb_size, 40, 40, 40);

    // Right side: full horizontal strip (scaled to fit remaining width)
    let strip_area_x = thumb2_x + thumb_size + thumb_gap + 20;
    let strip_area_w = cw - margin - strip_area_x;
    if strip_area_w > 10 {
        draw_mini_strip(&mut img, cw, ch, strip_area_x, fb_y, strip_area_w, fb_h, strip, strip_width, strip_height);
        draw_rect(&mut img, cw, ch, strip_area_x, fb_y, strip_area_w, fb_h, 40, 40, 40);
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
