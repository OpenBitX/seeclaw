/// SoM (Set-of-Mark) grid overlay utility.
///
/// Draws a labeled N×N grid onto a screenshot so that a VLM can identify
/// elements by their grid-cell label (e.g. "C4").
///
/// Grid labeling convention:
///   - Columns: A, B, C … Z, AA, AB … (left → right)
///   - Rows:    1, 2, 3 … N           (top  → bottom)
use crate::errors::{SeeClawError, SeeClawResult};

// ── Minimal 5×5 bitmap font ───────────────────────────────────────────────────
// Each glyph: 5 rows, each row is a u8 where bit4=leftmost pixel, bit0=rightmost.
// Index 0–9 = digits '0'–'9', index 10-35 = letters 'A'–'Z'.
const FONT_5X5: [[u8; 5]; 36] = [
    // digits 0-9
    [0b01110, 0b10001, 0b10001, 0b10001, 0b01110], // 0
    [0b00100, 0b01100, 0b00100, 0b00100, 0b01110], // 1
    [0b01110, 0b10001, 0b00110, 0b01000, 0b11111], // 2
    [0b11110, 0b00001, 0b00110, 0b00001, 0b11110], // 3
    [0b00110, 0b01010, 0b10010, 0b11111, 0b00010], // 4
    [0b11111, 0b10000, 0b11110, 0b00001, 0b11110], // 5
    [0b01110, 0b10000, 0b11110, 0b10001, 0b01110], // 6
    [0b11111, 0b00001, 0b00010, 0b00100, 0b00100], // 7
    [0b01110, 0b10001, 0b01110, 0b10001, 0b01110], // 8
    [0b01110, 0b10001, 0b01111, 0b00001, 0b01110], // 9
    // letters A-Z (only A-L used for 12-col grid, rest are placeholders)
    [0b01110, 0b10001, 0b11111, 0b10001, 0b10001], // A
    [0b11110, 0b10001, 0b11110, 0b10001, 0b11110], // B
    [0b01110, 0b10000, 0b10000, 0b10000, 0b01110], // C
    [0b11100, 0b10010, 0b10001, 0b10010, 0b11100], // D
    [0b11111, 0b10000, 0b11110, 0b10000, 0b11111], // E
    [0b11111, 0b10000, 0b11110, 0b10000, 0b10000], // F
    [0b01110, 0b10000, 0b10011, 0b10001, 0b01110], // G
    [0b10001, 0b10001, 0b11111, 0b10001, 0b10001], // H
    [0b01110, 0b00100, 0b00100, 0b00100, 0b01110], // I
    [0b00111, 0b00010, 0b00010, 0b10010, 0b01100], // J
    [0b10001, 0b10010, 0b11100, 0b10010, 0b10001], // K
    [0b10000, 0b10000, 0b10000, 0b10000, 0b11111], // L
    [0b10001, 0b11011, 0b10101, 0b10001, 0b10001], // M
    [0b10001, 0b11001, 0b10101, 0b10011, 0b10001], // N
    [0b01110, 0b10001, 0b10001, 0b10001, 0b01110], // O
    [0b11110, 0b10001, 0b11110, 0b10000, 0b10000], // P
    [0b01110, 0b10001, 0b10101, 0b10010, 0b01101], // Q
    [0b11110, 0b10001, 0b11110, 0b10010, 0b10001], // R
    [0b01111, 0b10000, 0b01110, 0b00001, 0b11110], // S
    [0b11111, 0b00100, 0b00100, 0b00100, 0b00100], // T
    [0b10001, 0b10001, 0b10001, 0b10001, 0b01110], // U
    [0b10001, 0b10001, 0b10001, 0b01010, 0b00100], // V
    [0b10001, 0b10001, 0b10101, 0b11011, 0b10001], // W
    [0b10001, 0b01010, 0b00100, 0b01010, 0b10001], // X
    [0b10001, 0b01010, 0b00100, 0b00100, 0b00100], // Y
    [0b11111, 0b00010, 0b00100, 0b01000, 0b11111], // Z
];

fn char_to_glyph(c: char) -> Option<&'static [u8; 5]> {
    let idx = match c {
        '0'..='9' => (c as u8 - b'0') as usize,
        'A'..='Z' => 10 + (c as u8 - b'A') as usize,
        _ => return None,
    };
    FONT_5X5.get(idx)
}

/// Draw a single glyph at pixel position (px, py) with the given pixel scale.
/// Foreground: bright yellow (255, 220, 0); background: semi-opaque dark box.
fn draw_glyph(canvas: &mut image::RgbaImage, c: char, px: u32, py: u32, scale: u32) {
    let Some(glyph) = char_to_glyph(c) else { return };
    let (w, h) = canvas.dimensions();
    let char_w = 5 * scale;
    let char_h = 5 * scale;

    // Dark background padding = 1px
    let bg_x = px.saturating_sub(1);
    let bg_y = py.saturating_sub(1);
    let bg_w = (char_w + 2).min(w.saturating_sub(bg_x));
    let bg_h = (char_h + 2).min(h.saturating_sub(bg_y));
    for dy in 0..bg_h {
        for dx in 0..bg_w {
            let x = bg_x + dx;
            let y = bg_y + dy;
            if x < w && y < h {
                let p = canvas.get_pixel_mut(x, y);
                p[0] = (p[0] as f32 * 0.25) as u8;
                p[1] = (p[1] as f32 * 0.25) as u8;
                p[2] = (p[2] as f32 * 0.25) as u8;
                p[3] = 255;
            }
        }
    }

    // Foreground pixels
    for (row, &bits) in glyph.iter().enumerate() {
        for col in 0..5u32 {
            if (bits >> (4 - col)) & 1 == 0 {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    let x = px + col * scale + sx;
                    let y = py + row as u32 * scale + sy;
                    if x < w && y < h {
                        let p = canvas.get_pixel_mut(x, y);
                        p[0] = 255;
                        p[1] = 220;
                        p[2] = 0;
                        p[3] = 255;
                    }
                }
            }
        }
    }
}

/// Draw a multi-character label string starting at (px, py).
fn draw_label_str(canvas: &mut image::RgbaImage, label: &str, px: u32, py: u32, scale: u32) {
    let char_step = 5 * scale + 1; // 1px gap between chars
    for (i, c) in label.chars().enumerate() {
        draw_glyph(canvas, c, px + i as u32 * char_step, py, scale);
    }
}

// ── Label helpers ────────────────────────────────────────────────────────────

/// Convert 0-indexed column number to its letter label.
/// 0→A, 1→B, …, 25→Z, 26→AA, 27→AB, …
pub fn col_label(col: u32) -> String {
    if col < 26 {
        String::from(char::from(b'A' + col as u8))
    } else {
        format!("A{}", char::from(b'A' + (col - 26) as u8))
    }
}

/// Full label for a grid cell: col=2, row=3 → "C4".
pub fn cell_label(col: u32, row: u32) -> String {
    format!("{}{}", col_label(col), row + 1)
}

// ── Grid drawing ──────────────────────────────────────────────────────────────

/// Overlay an N×N labeled grid on `src_bytes` (JPEG or PNG input).
///
/// **Every cell gets its unique label drawn inside the cell** at the top-left
/// corner (e.g. "A1", "C4", "L12").  The VLM simply reads the visible text —
/// no counting, no mental arithmetic.  Returns PNG-encoded bytes.
pub fn draw_som_grid(src_bytes: &[u8], grid_n: u32) -> SeeClawResult<Vec<u8>> {
    let img = image::load_from_memory(src_bytes)
        .map_err(|e| SeeClawError::Perception(format!("load image: {e}")))?;
    let mut canvas = img.to_rgba8();
    let (w, h) = canvas.dimensions();

    let grid_n = grid_n.max(1);
    let cell_w = (w / grid_n).max(1);
    let cell_h = (h / grid_n).max(1);

    // ── Cyan semi-transparent grid lines (2 px wide) ──────────────────────
    let (lr, lg, lb, la) = (0u8, 200u8, 255u8, 130u8);
    for col in 1..grid_n {
        let x = col * cell_w;
        if x >= w { break; }
        for y in 0..h {
            blend_pixel(canvas.get_pixel_mut(x, y), lr, lg, lb, la);
            if x + 1 < w { blend_pixel(canvas.get_pixel_mut(x + 1, y), lr, lg, lb, la); }
        }
    }
    for row in 1..grid_n {
        let y = row * cell_h;
        if y >= h { break; }
        for x in 0..w {
            blend_pixel(canvas.get_pixel_mut(x, y), lr, lg, lb, la);
            if y + 1 < h { blend_pixel(canvas.get_pixel_mut(x, y + 1), lr, lg, lb, la); }
        }
    }

    // ── Full cell label drawn INSIDE every cell ───────────────────────────
    // scale=2 when cell width ≥ 80 px → 10×10 px per glyph, clearly readable.
    let scale: u32 = if cell_w >= 80 { 2 } else { 1 };
    let pad = 4u32; // px offset from the top-left corner of each cell

    for row in 0..grid_n {
        for col in 0..grid_n {
            let label = cell_label(col, row); // e.g. "A1", "D7", "L12"
            let lx = col * cell_w + pad;
            let ly = row * cell_h + pad;
            if lx < w && ly < h {
                draw_label_str(&mut canvas, &label, lx, ly, scale);
            }
        }
    }

    // ── Encode result as PNG ──────────────────────────────────────────────
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(canvas)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| SeeClawError::Perception(format!("PNG encode: {e}")))?;

    Ok(out)
}

fn blend_pixel(pixel: &mut image::Rgba<u8>, r: u8, g: u8, b: u8, a: u8) {
    let alpha = a as f32 / 255.0;
    pixel[0] = (pixel[0] as f32 * (1.0 - alpha) + r as f32 * alpha).round() as u8;
    pixel[1] = (pixel[1] as f32 * (1.0 - alpha) + g as f32 * alpha).round() as u8;
    pixel[2] = (pixel[2] as f32 * (1.0 - alpha) + b as f32 * alpha).round() as u8;
    // alpha channel intentionally preserved
}

// ── Grid coordinate parsing ───────────────────────────────────────────────────

/// Parse a grid cell label like "C4" into (col_0indexed, row_0indexed).
/// Returns `None` if the label cannot be parsed.
pub fn parse_grid_label(label: &str) -> Option<(u32, u32)> {
    let label = label.trim().to_uppercase();
    let col_str: String = label.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let row_str: String = label.chars().skip_while(|c| c.is_ascii_alphabetic()).collect();

    if col_str.is_empty() || row_str.is_empty() {
        return None;
    }

    let col = if col_str.len() == 1 {
        (col_str.chars().next()? as u32).checked_sub(b'A' as u32)?
    } else {
        // Two-letter: AA=26, AB=27 ...
        26 + (col_str.chars().nth(1)? as u32).checked_sub(b'A' as u32)?
    };

    let row = row_str.parse::<u32>().ok()?.checked_sub(1)?;

    Some((col, row))
}

/// Convert a (col, row) grid cell to its center in **physical** pixel coordinates.
/// `img_w/h` should be the physical dimensions of the captured image.
pub fn grid_cell_to_physical(col: u32, row: u32, img_w: u32, img_h: u32, grid_n: u32) -> (i32, i32) {
    let cell_w = img_w as f64 / grid_n as f64;
    let cell_h = img_h as f64 / grid_n as f64;
    let cx = (col as f64 * cell_w + cell_w / 2.0).round() as i32;
    let cy = (row as f64 * cell_h + cell_h / 2.0).round() as i32;
    (cx, cy)
}

/// VLM prompt that explains how to read the labeled grid.
/// Since every cell has its label printed inside it, the model just reads the text.
pub fn build_grid_prompt(goal: &str, grid_n: u32) -> String {
    let last_col = col_label(grid_n - 1);
    format!(
        "The screenshot has a {n}x{n} grid overlay. \
         Every cell has its unique label printed inside it in the top-left corner \
         (e.g. A1=top-left cell, {last}{n}=bottom-right cell). \
         Columns go left to right (A to {last}), rows go top to bottom (1 to {n}).\n\n\
         Task: {goal}\n\n\
         Find the cell whose label is printed on or nearest the target UI element. \
         Reply ONLY with JSON: {{\"cell\": \"D7\", \"found\": true, \"description\": \"<what you see>\"}}",
        n = grid_n,
        last = last_col,
        goal = goal,
    )
}
