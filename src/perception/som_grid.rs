/// SoM (Set-of-Mark) grid overlay utility.
///
/// Draws a labeled N×N grid onto a screenshot so that a VLM can identify
/// elements by their grid-cell label (e.g. "C4").
///
/// Grid labeling convention:
///   - Columns: A, B, C … Z, AA, AB … (left → right)
///   - Rows:    1, 2, 3 … N           (top  → bottom)
use crate::errors::{SeeClawError, SeeClawResult};

// ── Column label helpers ──────────────────────────────────────────────────────

/// Convert 0-indexed column number to its label (0=A, 1=B … 25=Z, 26=AA …).
pub fn col_label(col: u32) -> String {
    if col < 26 {
        String::from(char::from(b'A' + col as u8))
    } else {
        format!("A{}", char::from(b'A' + (col - 26) as u8))
    }
}

// ── Grid drawing ──────────────────────────────────────────────────────────────

/// Draw an N×N semi-transparent cyan grid over a PNG image.
/// Returns the annotated PNG bytes.
/// No font dependency — labels are NOT rendered; the VLM prompt describes the scheme.
pub fn draw_som_grid(png_bytes: &[u8], grid_n: u32) -> SeeClawResult<Vec<u8>> {
    let img = image::load_from_memory(png_bytes)
        .map_err(|e| SeeClawError::Perception(format!("load image: {e}")))?;
    let mut canvas = img.to_rgba8();
    let (w, h) = canvas.dimensions();

    let cell_w = (w / grid_n).max(1);
    let cell_h = (h / grid_n).max(1);

    // Cyan semi-transparent line: rgba(0, 220, 255, 110)
    let line_r = 0u8;
    let line_g = 220u8;
    let line_b = 255u8;
    let line_a = 110u8; // ~43% opacity

    // Draw vertical grid lines
    for col in 1..grid_n {
        let x = col * cell_w;
        if x >= w {
            break;
        }
        for y in 0..h {
            blend_pixel(canvas.get_pixel_mut(x, y), line_r, line_g, line_b, line_a);
            // Make lines 2 px wide for visibility
            if x + 1 < w {
                blend_pixel(canvas.get_pixel_mut(x + 1, y), line_r, line_g, line_b, line_a);
            }
        }
    }

    // Draw horizontal grid lines
    for row in 1..grid_n {
        let y = row * cell_h;
        if y >= h {
            break;
        }
        for x in 0..w {
            blend_pixel(canvas.get_pixel_mut(x, y), line_r, line_g, line_b, line_a);
            if y + 1 < h {
                blend_pixel(canvas.get_pixel_mut(x, y + 1), line_r, line_g, line_b, line_a);
            }
        }
    }

    // Draw a small colored dot at each cell origin corner for easier VLM counting
    for row in 0..grid_n {
        for col in 0..grid_n {
            let cx = col * cell_w + 6;
            let cy = row * cell_h + 6;
            // 4×4 bright cyan square
            for dy in 0..4u32 {
                for dx in 0..4u32 {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px < w && py < h {
                        let p = canvas.get_pixel_mut(px, py);
                        p[0] = 0;
                        p[1] = 220;
                        p[2] = 255;
                        p[3] = 220;
                    }
                }
            }
        }
    }

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
    // preserve original alpha
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

/// Build the VLM grid prompt describing the coordinate scheme.
pub fn build_grid_prompt(goal: &str, grid_n: u32) -> String {
    let last_col = col_label(grid_n - 1);
    format!(
        "The screenshot has a {n}×{n} coordinate grid overlay (cyan lines).\n\
         Columns are labeled A–{last} (left → right), rows are 1–{n} (top → bottom).\n\
         Example cell labels: A1 (top-left), {last}{n} (bottom-right).\n\n\
         Task: {goal}\n\n\
         Identify the UI element matching the task. Reply ONLY with a tool call using \
         the matching grid cell label as `element_id` (e.g. \"C4\").",
        n = grid_n,
        last = last_col,
        goal = goal,
    )
}
