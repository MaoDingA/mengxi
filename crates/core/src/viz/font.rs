/// Very simple bitmap text (3x5 pixel font for ASCII).
///
/// Draws text as light-gray pixels (200, 200, 200) into an RGB8 image buffer.
/// Each character is 3 pixels wide with 1 pixel spacing (4px stride).
pub(crate) fn draw_text_simple(
    img: &mut [u8],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    text: &str,
) {
    draw_text(img, w, h, x0, y0, text, 1, 200, 200, 200);
}

/// Draw bitmap text with scaling and custom color.
///
/// Each pixel of the 3x5 font is rendered as a `scale`×`scale` block.
/// Character stride becomes `3 * scale + scale` (4*scale total per char).
pub(crate) fn draw_text_scaled(
    img: &mut [u8],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    text: &str,
    scale: usize,
    r: u8,
    g: u8,
    b: u8,
) {
    draw_text(img, w, h, x0, y0, text, scale, r, g, b);
}

// ---- Internal: shared font rendering with optional scaling ----

fn draw_text(
    img: &mut [u8],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    text: &str,
    scale: usize,
    cr: u8,
    cg: u8,
    cb: u8,
) {
    const FONT: [(char, [u8; 5]); 66] = [
        // Uppercase A-Z
        ('A', [0b111, 0b101, 0b111, 0b101, 0b101]),
        ('B', [0b110, 0b101, 0b110, 0b101, 0b110]),
        ('C', [0b111, 0b100, 0b100, 0b100, 0b111]),
        ('D', [0b110, 0b101, 0b101, 0b101, 0b110]),
        ('E', [0b111, 0b100, 0b111, 0b100, 0b111]),
        ('F', [0b111, 0b100, 0b111, 0b100, 0b100]),
        ('G', [0b111, 0b100, 0b101, 0b101, 0b111]),
        ('H', [0b101, 0b101, 0b111, 0b101, 0b101]),
        ('I', [0b111, 0b010, 0b010, 0b010, 0b111]),
        ('J', [0b001, 0b001, 0b001, 0b101, 0b111]),
        ('K', [0b101, 0b110, 0b100, 0b110, 0b101]),
        ('L', [0b100, 0b100, 0b100, 0b100, 0b111]),
        ('M', [0b101, 0b111, 0b101, 0b101, 0b101]),
        ('N', [0b101, 0b111, 0b111, 0b101, 0b101]),
        ('O', [0b111, 0b101, 0b101, 0b101, 0b111]),
        ('P', [0b111, 0b101, 0b111, 0b100, 0b100]),
        ('Q', [0b111, 0b101, 0b101, 0b111, 0b001]),
        ('R', [0b111, 0b101, 0b111, 0b110, 0b101]),
        ('S', [0b111, 0b100, 0b111, 0b001, 0b111]),
        ('T', [0b111, 0b010, 0b010, 0b010, 0b010]),
        ('U', [0b101, 0b101, 0b101, 0b101, 0b111]),
        ('V', [0b101, 0b101, 0b010, 0b101, 0b101]),
        ('W', [0b101, 0b101, 0b111, 0b111, 0b101]),
        ('X', [0b101, 0b111, 0b000, 0b111, 0b101]),
        ('Y', [0b101, 0b101, 0b010, 0b010, 0b010]),
        ('Z', [0b111, 0b001, 0b010, 0b100, 0b111]),
        // Lowercase a-z
        ('a', [0b000, 0b010, 0b101, 0b111, 0b101]),
        ('b', [0b100, 0b100, 0b110, 0b101, 0b110]),
        ('c', [0b000, 0b010, 0b100, 0b100, 0b010]),
        ('d', [0b001, 0b001, 0b011, 0b101, 0b011]),
        ('e', [0b000, 0b010, 0b101, 0b110, 0b101]),
        ('f', [0b010, 0b101, 0b110, 0b100, 0b100]),
        ('g', [0b000, 0b111, 0b101, 0b011, 0b111]),
        ('h', [0b100, 0b100, 0b110, 0b101, 0b101]),
        ('i', [0b000, 0b010, 0b110, 0b010, 0b010]),
        ('j', [0b000, 0b001, 0b011, 0b001, 0b110]),
        ('k', [0b100, 0b100, 0b110, 0b101, 0b101]),
        ('l', [0b000, 0b110, 0b010, 0b010, 0b111]),
        ('m', [0b000, 0b000, 0b101, 0b111, 0b101]),
        ('n', [0b000, 0b000, 0b110, 0b101, 0b101]),
        ('o', [0b000, 0b000, 0b010, 0b101, 0b010]),
        ('p', [0b000, 0b111, 0b101, 0b111, 0b100]),
        ('q', [0b000, 0b011, 0b101, 0b111, 0b001]),
        ('r', [0b000, 0b110, 0b101, 0b110, 0b100]),
        ('s', [0b000, 0b010, 0b100, 0b001, 0b010]),
        ('t', [0b000, 0b000, 0b110, 0b010, 0b010]),
        ('u', [0b000, 0b000, 0b101, 0b101, 0b111]),
        ('v', [0b000, 0b000, 0b101, 0b101, 0b010]),
        ('w', [0b000, 0b000, 0b101, 0b111, 0b101]),
        ('x', [0b000, 0b000, 0b101, 0b010, 0b101]),
        ('y', [0b000, 0b101, 0b101, 0b011, 0b101]),
        ('z', [0b000, 0b000, 0b111, 0b010, 0b111]),
        // Digits 0-9
        ('0', [0b111, 0b101, 0b101, 0b101, 0b111]),
        ('1', [0b010, 0b110, 0b010, 0b010, 0b111]),
        ('2', [0b111, 0b001, 0b111, 0b100, 0b111]),
        ('3', [0b111, 0b001, 0b111, 0b001, 0b111]),
        ('4', [0b101, 0b101, 0b111, 0b001, 0b001]),
        ('5', [0b111, 0b100, 0b111, 0b001, 0b111]),
        ('6', [0b111, 0b100, 0b111, 0b101, 0b111]),
        ('7', [0b111, 0b001, 0b010, 0b010, 0b010]),
        ('8', [0b111, 0b101, 0b111, 0b101, 0b111]),
        ('9', [0b111, 0b101, 0b111, 0b001, 0b111]),
        // Symbols
        ('.', [0b000, 0b000, 0b000, 0b000, 0b010]),
        ('%', [0b100, 0b001, 0b010, 0b100, 0b001]),
        (' ', [0b000, 0b000, 0b000, 0b000, 0b000]),
        ('-', [0b000, 0b000, 0b111, 0b000, 0b000]),
    ];

    let char_w = 3 * scale;
    let stride = char_w + scale; // 4 * scale

    let mut cx = x0;
    for ch in text.chars().map(|c| c.to_ascii_uppercase()) {
        if let Some(entry) = FONT.iter().find(|(c, _)| *c == ch) {
            for (row, &bits) in entry.1.iter().enumerate() {
                for col in 0..3usize {
                    if bits & (1 << (2 - col)) != 0 {
                        // Draw a scale×scale block for each font pixel
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let px = cx + col * scale + sx;
                                let py = y0 + row * scale + sy;
                                if px < w && py < h {
                                    let idx = (py * w + px) * 3;
                                    if idx + 2 < img.len() {
                                        img[idx] = cr;
                                        img[idx + 1] = cg;
                                        img[idx + 2] = cb;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        cx += stride;
    }
}

// ---------------------------------------------------------------------------
// TTF-based text rendering (supports CJK via ab_glyph)
// ---------------------------------------------------------------------------

use ab_glyph::{Font, PxScale, ScaleFont};

/// Render text using a system TTF font. Supports full Unicode including CJK.
///
/// `font_size_px`: desired height of capital letters in pixels.
/// Falls back gracefully to the bitmap ASCII font if no TTF font is found.
pub(crate) fn draw_text_ttf(
    img: &mut [u8],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    text: &str,
    font_size_px: f32,
    r: u8,
    g: u8,
    b: u8,
) {
    let font = load_system_font();
    let Some(font) = font else {
        let scale = ((font_size_px / 5.0).round() as usize).max(1);
        draw_text(img, w, h, x0, y0, text, scale, r, g, b);
        return;
    };

    let scale = PxScale::from(font_size_px);
    let scaled_font = font.as_scaled(scale);
    let mut cursor_x = x0 as f32;
    let baseline_y = y0 as f32 + font_size_px;

    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        let glyph = gid.with_scale_and_position(scale, ab_glyph::point(cursor_x, baseline_y));

        if let Some(outlined) = scaled_font.outline_glyph(glyph) {
            let bb = outlined.px_bounds();
            outlined.draw(|x, y, v| {
                let px = bb.min.x as i32 + x as i32;
                let py = bb.min.y as i32 + y as i32;
                if px >= 0 && (px as usize) < w && py >= 0 && (py as usize) < h {
                    let idx = ((py as usize) * w + (px as usize)) * 3;
                    if idx + 2 < img.len() {
                        let alpha = v;
                        let inv_a = 1.0 - alpha;
                        img[idx]     = (img[idx]     as f64 * inv_a + r as f64 * alpha).round() as u8;
                        img[idx + 1] = (img[idx + 1] as f64 * inv_a + g as f64 * alpha).round() as u8;
                        img[idx + 2] = (img[idx + 2] as f64 * inv_a + b as f64 * alpha).round() as u8;
                    }
                }
            });
        }

        cursor_x += scaled_font.h_advance(gid);
    }
}

/// Try to load a system font that supports CJK characters.
/// Prefers PingFang SC (macOS), then STHeiti, then any available font.
fn load_system_font() -> Option<ab_glyph::FontArc> {
    // Candidate font paths (platform-specific)
    #[cfg(target_os = "macos")]
    let candidates = [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
    ];

    #[cfg(not(target_os = "macos"))]
    let candidates: [&str; 0] = [];

    for path in candidates.iter() {
        if let Ok(data) = std::fs::read(path) {
            if let Ok(font) = ab_glyph::FontArc::try_from_vec(data) {
                return Some(font);
            }
        }
    }

    None
}

/// Measure the pixel width of a string rendered with the system TTF font.
/// Falls back to bitmap font width estimation if no TTF available.
pub(crate) fn measure_text_width(text: &str, font_size_px: f32) -> usize {
    let font = match load_system_font() {
        Some(f) => f,
        None => {
            // Fallback: estimate from bitmap font (4px per char at scale 1)
            let scale = ((font_size_px / 5.0).round() as usize).max(1);
            return text.chars().count() * 4 * scale;
        }
    };

    let scale = PxScale::from(font_size_px);
    let scaled = font.as_scaled(scale);

    let mut total: f32 = 0.0;
    for ch in text.chars() {
        let gid = font.glyph_id(ch);
        total += scaled.h_advance(gid);
    }
    total.round() as usize
}
