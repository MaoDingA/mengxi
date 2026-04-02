use std::path::Path;

/// A thumbnail image in f64 RGB format.
pub struct Thumbnail {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<f64>, // interleaved sRGB [0,1]
    pub frame_index: usize, // which frame this thumbnail came from (for time alignment)
}

/// Render a CinePrint timeline poster as PNG (portrait orientation).
///
/// Layout:
/// ```text
/// +---+---------------------------+---+
/// |          "CinePrints"              |
/// +---+---------------------------+---+
/// | T |                           | T |
/// | H |    central vertical strip | H |
/// | U |    (narrow, top to bottom)| U |
/// | M |\ /                        | M |
/// | B | x  connecting lines       | B |
/// | N |/ \                        | N |
/// |   |                           |   |
/// +---+---------------------------+---+
/// |     "Movie Fingerprint"            |
/// +---+---------------------------+---+
/// ```
pub fn render_cineprint_png(
    strip: &[f64],
    strip_width: usize,  // = number of frames
    strip_height: usize, // = frame height (rows per frame)
    thumbnails: &[Thumbnail],
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // --- Layout constants ---
    let padding: usize = 30;
    let title_h: usize = 50;
    let footer_h: usize = 50;
    let target_strip_visual_w: usize = if !thumbnails.is_empty() { thumbnails[0].height } else { 400 }; // match frame height for detail
    let poster_h: usize = 3600; // tall poster for high resolution

    // Scale factors for strip → poster
    let scale_x = target_strip_visual_w as f64 / strip_height as f64;
    let strip_visual_h = poster_h;
    let scale_y = strip_visual_h as f64 / strip_width as f64;

    // Thumbnail sizing — use actual thumbnail dimensions
    let thumb_display_w: usize = if !thumbnails.is_empty() { thumbnails[0].width } else { 160 };
    let thumb_display_h: usize = if !thumbnails.is_empty() { thumbnails[0].height } else { 90 };
    let thumb_slot_w: usize = thumb_display_w + padding;

    // Total poster dimensions (portrait)
    let total_width = thumb_slot_w + padding + target_strip_visual_w + padding + thumb_slot_w + padding * 2;
    let total_height = title_h + strip_visual_h + footer_h + padding * 2;

    let mut img = vec![0u8; total_width * total_height * 3]; // black background

    // --- Draw title (centered) ---
    let title_text = "CinePrints";
    let title_w = title_text.chars().count() * 5;
    super::font::draw_text_simple(
        &mut img, total_width, total_height,
        total_width.saturating_sub(title_w) / 2, padding / 2 + 4,
        title_text,
    );

    // --- Draw rotated strip in center (vertical, top to bottom) ---
    // Strip rotation: original strip is [width x height] (horizontal).
    // After 90° CW rotation: visual_x comes from original row (y),
    // visual_y comes from original column (x) reversed so time flows top→bottom.
    let strip_x0 = thumb_slot_w + padding * 2;
    let strip_y0 = title_h + padding;

    for vy in 0..strip_visual_h {
        for vx in 0..target_strip_visual_w {
            // Bilinear interpolation for smoother scaling
            let src_col_f = vy as f64 / scale_y;
            let src_row_f = vx as f64 / scale_x;
            let c0 = (src_col_f.floor() as usize).min(strip_width - 1);
            let c1 = (c0 + 1).min(strip_width - 1);
            let r0 = (src_row_f.floor() as usize).min(strip_height - 1);
            let r1 = (r0 + 1).min(strip_height - 1);
            let fc = src_col_f - c0 as f64;
            let fr = src_row_f - r0 as f64;

            let px = strip_x0 + vx;
            let py = strip_y0 + vy;
            if px < total_width && py < total_height {
                let dst = (py * total_width + px) * 3;
                if dst + 2 < img.len() {
                    // Sample 4 corners and interpolate
                    let i00 = (r0 * strip_width + c0) * 3;
                    let i10 = (r0 * strip_width + c1) * 3;
                    let i01 = (r1 * strip_width + c0) * 3;
                    let i11 = (r1 * strip_width + c1) * 3;
                    for ch in 0..3 {
                        let v00 = strip.get(i00 + ch).copied().unwrap_or(0.0);
                        let v10 = strip.get(i10 + ch).copied().unwrap_or(0.0);
                        let v01 = strip.get(i01 + ch).copied().unwrap_or(0.0);
                        let v11 = strip.get(i11 + ch).copied().unwrap_or(0.0);
                        let v = v00 * (1.0 - fr) * (1.0 - fc)
                              + v10 * (1.0 - fr) * fc
                              + v01 * fr * (1.0 - fc)
                              + v11 * fr * fc;
                        img[dst + ch] = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
            }
        }
    }

    // --- Draw thumbnails with connecting lines ---
    let n_thumbs = thumbnails.len();
    if n_thumbs > 0 {
        let thumb_h = thumb_display_h;

        for (i, thumb) in thumbnails.iter().enumerate() {
            // Position: even indices → left, odd → right
            let is_left = i % 2 == 0;

            // Vertical position: map actual frame_index to strip position
            let frame_fraction = thumb.frame_index as f64 / strip_width as f64;
            let strip_y_pos = strip_y0 + (frame_fraction * strip_visual_h as f64).round() as usize;

            // Thumbnail top-left
            let thumb_x0 = if is_left {
                padding
            } else {
                strip_x0 + target_strip_visual_w + padding
            };
            let thumb_y0 = strip_y_pos.saturating_sub(thumb_h / 2);

            // Draw connecting line from thumbnail edge to strip edge
            let line_y = strip_y_pos;
            let line_x_start = if is_left {
                thumb_x0 + thumb_display_w
            } else {
                strip_x0 + target_strip_visual_w
            };
            let line_x_end = if is_left {
                strip_x0
            } else {
                thumb_x0
            };
            draw_line(&mut img, total_width, total_height, line_x_start, line_y, line_x_end, line_y, 60, 60, 60);

            // Draw thumbnail (scaled to thumb_display_w × thumb_h)
            draw_thumbnail_scaled(
                &mut img, total_width, total_height,
                thumb_x0, thumb_y0,
                thumb_display_w, thumb_h,
                thumb,
            );

            // Draw border around thumbnail
            draw_rect(&mut img, total_width, total_height, thumb_x0, thumb_y0, thumb_display_w, thumb_h, 50, 50, 50);
        }
    }

    // --- Draw footer (centered) ---
    let footer_text = "Movie Fingerprint";
    let footer_w = footer_text.chars().count() * 5;
    super::font::draw_text_simple(
        &mut img, total_width, total_height,
        total_width.saturating_sub(footer_w) / 2,
        title_h + strip_visual_h + padding + 4,
        footer_text,
    );

    // Save PNG
    image::save_buffer(
        output_path,
        &img,
        total_width as u32,
        total_height as u32,
        image::ExtendedColorType::Rgb8,
    )?;

    Ok(())
}

/// Draw a thumbnail scaled to fit the given display dimensions.
pub(crate) fn draw_thumbnail_scaled(
    img: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    x0: usize,
    y0: usize,
    display_w: usize,
    display_h: usize,
    thumb: &Thumbnail,
) {
    let tw = thumb.width;
    let th = thumb.height;

    for dy in 0..display_h {
        for dx in 0..display_w {
            // Nearest-neighbor sampling
            let sx = (dx as f64 * tw as f64 / display_w as f64) as usize;
            let sy = (dy as f64 * th as f64 / display_h as f64) as usize;
            let sx = sx.min(tw - 1);
            let sy = sy.min(th - 1);
            let src_idx = (sy * tw + sx) * 3;

            let px = x0 + dx;
            let py = y0 + dy;
            if px < canvas_w && py < canvas_h {
                let dst = (py * canvas_w + px) * 3;
                if src_idx + 2 < thumb.pixels.len() && dst + 2 < img.len() {
                    img[dst] = (thumb.pixels[src_idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                    img[dst + 1] = (thumb.pixels[src_idx + 1].clamp(0.0, 1.0) * 255.0).round() as u8;
                    img[dst + 2] = (thumb.pixels[src_idx + 2].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
    }
}

/// Draw a 1px rectangle outline.
pub(crate) fn draw_rect(
    img: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,
    r: u8,
    g: u8,
    b: u8,
) {
    // Top and bottom edges
    for dx in 0..w {
        for &dy in &[0, h] {
            let px = x0 + dx;
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
    // Left and right edges
    for dy in 0..h {
        for &dx in &[0, w] {
            let px = x0 + dx;
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
}

/// Draw a line using Bresenham's algorithm.
pub(crate) fn draw_line(
    img: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    r: u8,
    g: u8,
    b: u8,
) {
    let mut x = x0 as i32;
    let mut y = y0 as i32;
    let x1 = x1 as i32;
    let y1 = y1 as i32;

    let dx = (x1 - x).abs();
    let dy = -(y1 - y).abs();
    let sx: i32 = if x < x1 { 1 } else { -1 };
    let sy: i32 = if y < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        if x >= 0 && (x as usize) < canvas_w && y >= 0 && (y as usize) < canvas_h {
            let idx = (y as usize * canvas_w + x as usize) * 3;
            if idx + 2 < img.len() {
                img[idx] = r;
                img[idx + 1] = g;
                img[idx + 2] = b;
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}
