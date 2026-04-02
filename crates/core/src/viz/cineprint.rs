use std::path::Path;

/// A thumbnail image in f64 RGB format.
pub struct Thumbnail {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<f64>, // interleaved sRGB [0,1]
}

/// Render a CinePrint timeline poster as PNG.
///
/// Layout (portrait):
/// ```text
/// +---+---------------------------+---+
/// |          "CinePrints"              |
/// +---+---------------------------+---+
/// | T |                           | T |
/// | H |    central vertical strip    | H |
/// | U |    (rotated 90° CW)         | U |
/// | M |                           | M |
/// | B |                           | B |
/// |   "Movie Fingerprint"             |
/// +-----------------------------------+
/// ```
pub fn render_cineprint_png(
    strip: &[f64],
    strip_width: usize,
    strip_height: usize,
    thumbnails: &[Thumbnail],
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // --- Layout constants ---
    let padding: usize = 20;
    let title_h: usize = 30;
    let footer_h: usize = 30;

    // Rotated strip dimensions: swap width/height
    let rotated_w = strip_height; // was height, now width
    let rotated_h = strip_width.min(800); // cap poster height

    // Scale factor for strip → poster
    let scale_y = rotated_h as f64 / strip_width as f64;

    let thumb_slot_w: usize = if thumbnails.is_empty() { 0 } else { 160 + padding };

    let total_width = thumb_slot_w + rotated_w + thumb_slot_w + padding * 2;
    let total_height = title_h + rotated_h + footer_h + padding * 2;

    let mut img = vec![0u8; total_width * total_height * 3];

    // --- Draw title (centered) ---
    let title_text = "CinePrints";
    let title_w = title_text.chars().count() * 4;
    super::font::draw_text_simple(
        &mut img, total_width, total_height,
        total_width.saturating_sub(title_w) / 2, padding,
        title_text,
    );

    // --- Draw rotated strip in center ---
    let strip_x = thumb_slot_w + padding;
    let strip_y = title_h + padding;

    for y in 0..rotated_h {
        for x in 0..rotated_w {
            let src_col = (y as f64 / scale_y) as usize;
            let src_row = x;
            let src_col = src_col.min(strip_width - 1);
            let src_row = src_row.min(strip_height - 1);
            let src_idx = (src_col * strip_height + src_row) * 3;

            let px = strip_x + x;
            let py = strip_y + y;
            if px < total_width && py < total_height {
                let dst = (py * total_width + px) * 3;
                if src_idx + 2 < strip.len() && dst + 2 < img.len() {
                    img[dst] = (strip[src_idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                    img[dst + 1] = (strip[src_idx + 1].clamp(0.0, 1.0) * 255.0).round() as u8;
                    img[dst + 2] = (strip[src_idx + 2].clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            }
        }
    }

    // --- Draw thumbnails ---
    let n_thumbs = thumbnails.len();
    if n_thumbs > 0 {
        let left_x = padding;
        let right_x = strip_x + rotated_w + padding;
        let spacing = rotated_h as f64 / (n_thumbs as f64 + 1.0);

        for i in 0..n_thumbs {
            let thumb_y = strip_y + ((i as f64 + 0.5) * spacing).round() as usize;
            let is_left = i % 2 == 0;
            let base_x = if is_left { left_x } else { right_x };
            draw_thumbnail(
                &mut img, total_width, total_height,
                base_x, thumb_y,
                &thumbnails[i],
            );
        }
    }

    // --- Draw footer (centered) ---
    let footer_text = "Movie Fingerprint";
    let footer_w = footer_text.chars().count() * 4;
    super::font::draw_text_simple(
        &mut img, total_width, total_height,
        total_width.saturating_sub(footer_w) / 2,
        title_h + rotated_h + padding,
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

/// Draw a thumbnail image at the given position.
fn draw_thumbnail(
    img: &mut [u8],
    canvas_w: usize,
    canvas_h: usize,
    x0: usize,
    y0: usize,
    thumb: &Thumbnail,
) {
    let tw = thumb.width;
    let th = thumb.height;

    // Draw border
    for row in 0..(th + 1) {
        for col in 0..(tw + 1) {
            let is_border = row == 0 || row == th || col == 0 || col == tw;
            if is_border {
                let px = x0 + col;
                let py = y0 + row;
                if px < canvas_w && py < canvas_h {
                    let dst = (py * canvas_w + px) * 3;
                    if dst + 2 < img.len() {
                        img[dst] = 60;
                        img[dst + 1] = 60;
                        img[dst + 2] = 60;
                    }
                }
            }
        }
    }

    // Draw thumbnail pixels
    for row in 0..th {
        for col in 0..tw {
            let px = x0 + col;
            let py = y0 + row;
            if px < canvas_w && py < canvas_h {
                let src_idx = (row * tw + col) * 3;
                if src_idx + 2 < thumb.pixels.len() {
                    let dst = (py * canvas_w + px) * 3;
                    if dst + 2 < img.len() {
                        img[dst] = (thumb.pixels[src_idx].clamp(0.0, 1.0) * 255.0).round() as u8;
                        img[dst + 1] = (thumb.pixels[src_idx + 1].clamp(0.0, 1.0) * 255.0).round() as u8;
                        img[dst + 2] = (thumb.pixels[src_idx + 2].clamp(0.0, 1.0) * 255.0).round() as u8;
                    }
                }
            }
        }
    }
}
