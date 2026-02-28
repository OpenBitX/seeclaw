/// Draw annotated bounding boxes and semantic labels on a screenshot.
///
/// Each detected element gets a colour-coded rectangle and a text label
/// (e.g. "btn_1: OK") drawn directly onto the image.
use crate::errors::{SeeClawError, SeeClawResult};
use crate::perception::types::{ElementType, UIElement};

/// RGBA colour palette indexed by element type.
fn element_colour(et: &ElementType) -> [u8; 4] {
    match et {
        ElementType::Button   => [255, 68, 68, 220],   // red
        ElementType::Input    => [68, 255, 68, 220],    // green
        ElementType::Link     => [68, 68, 255, 220],    // blue
        ElementType::Icon     => [255, 170, 0, 220],    // orange
        ElementType::Checkbox => [255, 68, 255, 220],   // magenta
        ElementType::Radio    => [255, 68, 255, 220],   // magenta
        ElementType::Menu     => [0, 220, 255, 220],    // cyan
        ElementType::MenuItem => [0, 200, 220, 220],    // dark cyan
        ElementType::Select   => [170, 170, 68, 220],   // olive (scrollbar / select)
        ElementType::Text     => [170, 170, 170, 200],  // grey
        ElementType::Image    => [255, 200, 100, 220],  // light orange
        ElementType::Container=> [120, 120, 80, 180],   // dark olive
        ElementType::Unknown  => [255, 255, 255, 200],  // white
    }
}

/// Annotate `src_bytes` (JPEG/PNG) with bounding boxes for each element.
/// Returns PNG-encoded bytes of the annotated image.
///
/// On high-resolution images (width > 1600) the label font is drawn at 2×
/// scale so it remains readable when the image is shown to a VLM.
pub fn annotate_image(
    src_bytes: &[u8],
    elements: &[UIElement],
) -> SeeClawResult<Vec<u8>> {
    let img = image::load_from_memory(src_bytes)
        .map_err(|e| SeeClawError::Perception(format!("annotate load: {e}")))?;
    let mut canvas = img.to_rgba8();
    let (w, h) = canvas.dimensions();

    // Use 2× scale for labels on high-res screens (> 1600 px wide)
    let label_scale: u32 = if w > 1600 { 2 } else { 1 };
    let box_thickness: i32 = if w > 1600 { 3 } else { 2 };

    for elem in elements {
        let [x1n, y1n, x2n, y2n] = elem.bbox;
        let x1 = (x1n * w as f32).round() as i32;
        let y1 = (y1n * h as f32).round() as i32;
        let x2 = (x2n * w as f32).round() as i32;
        let y2 = (y2n * h as f32).round() as i32;

        let col = element_colour(&elem.node_type);

        // Draw bounding box
        draw_rect(&mut canvas, x1, y1, x2, y2, col, box_thickness);

        // Draw label: just the short numeric ID on the image.
        // Content and hierarchy are conveyed via the element list text.
        let label = elem.id.clone();
        let label_h_px = (5 * label_scale + 4) as i32;
        draw_label_bg(
            &mut canvas,
            x1,
            (y1 - label_h_px).max(0),
            &label,
            col,
            label_scale,
        );
    }

    // Encode as PNG
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(canvas)
        .write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Png,
        )
        .map_err(|e| SeeClawError::Perception(format!("PNG encode: {e}")))?;

    Ok(out)
}

/// Build a text listing of detected elements for the VLM prompt.
///
/// Uses containment-chain addressing: if element 12 is inside element 7
/// which is inside element 3, it shows `3>7>12`. This lets the VLM
/// precisely locate nested elements with short labels on the image.
pub fn build_element_list(elements: &[UIElement]) -> String {
    if elements.is_empty() {
        return "No UI elements detected.".to_string();
    }

    // Pre-build a map from id → element for chain lookup
    let id_map: std::collections::HashMap<&str, &UIElement> = elements
        .iter()
        .map(|e| (e.id.as_str(), e))
        .collect();

    let mut lines = vec!["Detected elements:".to_string()];
    for e in elements {
        // Build containment chain bottom-up: e.g. "3>7>12"
        let chain = build_chain(&e.id, &id_map);

        let name_part = match &e.content {
            Some(n) if !n.is_empty() => format!(" \"{}\"", n),
            _ => String::new(),
        };
        lines.push(format!(
            "  - [{}] {:?} ({:.0}%){}",
            chain,
            e.node_type,
            e.confidence * 100.0,
            name_part,
        ));
    }
    lines.join("\n")
}

/// Build a containment chain string like "3>7>12" by walking parent_id links.
fn build_chain<'a>(
    id: &'a str,
    id_map: &std::collections::HashMap<&str, &UIElement>,
) -> String {
    let mut chain = vec![id.to_string()];
    let mut current = id;
    // Walk up at most 10 levels to avoid infinite loops
    for _ in 0..10 {
        if let Some(elem) = id_map.get(current) {
            if let Some(ref pid) = elem.parent_id {
                chain.push(pid.clone());
                current = pid;
                continue;
            }
        }
        break;
    }
    chain.reverse();
    chain.join(">")
}

// ── Drawing primitives ──────────────────────────────────────────────────────

fn draw_rect(
    canvas: &mut image::RgbaImage,
    x1: i32, y1: i32, x2: i32, y2: i32,
    col: [u8; 4],
    thickness: i32,
) {
    let (w, h) = canvas.dimensions();
    let (iw, ih) = (w as i32, h as i32);

    // Top & bottom edges
    for t in 0..thickness {
        let ty = y1 + t;
        let by = y2 - t;
        for x in x1..=x2 {
            if x >= 0 && x < iw {
                if ty >= 0 && ty < ih { set_pixel(canvas, x as u32, ty as u32, col); }
                if by >= 0 && by < ih { set_pixel(canvas, x as u32, by as u32, col); }
            }
        }
    }
    // Left & right edges
    for t in 0..thickness {
        let lx = x1 + t;
        let rx = x2 - t;
        for y in y1..=y2 {
            if y >= 0 && y < ih {
                if lx >= 0 && lx < iw { set_pixel(canvas, lx as u32, y as u32, col); }
                if rx >= 0 && rx < iw { set_pixel(canvas, rx as u32, y as u32, col); }
            }
        }
    }
}

fn draw_label_bg(
    canvas: &mut image::RgbaImage,
    x: i32, y: i32,
    text: &str,
    col: [u8; 4],
    scale: u32,
) {
    let (w, h) = canvas.dimensions();
    let char_w = 5 * scale + 1; // glyph width + 1px gap
    let char_h = 5 * scale;     // glyph height
    let pad = 2 * scale;
    let label_w = text.len() as u32 * char_w + pad * 2;
    let label_h = char_h + pad * 2;

    // Dark background
    for dy in 0..label_h {
        for dx in 0..label_w {
            let px = x as u32 + dx;
            let py = y as u32 + dy;
            if px < w && py < h {
                let p = canvas.get_pixel_mut(px, py);
                p[0] = (p[0] as f32 * 0.2) as u8;
                p[1] = (p[1] as f32 * 0.2) as u8;
                p[2] = (p[2] as f32 * 0.2) as u8;
                p[3] = 255;
            }
        }
    }

    // Draw text using the SoM grid font (reuse the 5x5 bitmap glyphs)
    let text_x = x as u32 + pad;
    let text_y = y as u32 + pad;
    let step = 5 * scale + 1;

    for (i, c) in text.to_uppercase().chars().enumerate() {
        let gx = text_x + i as u32 * step;
        if gx + 5 * scale >= w { break; }
        draw_mini_glyph(canvas, c, gx, text_y, col, scale);
    }
}

/// Minimal 5×5 font renderer (same glyphs as som_grid.rs).
/// Supports `scale` for multi-pixel rendering on high-DPI screens.
fn draw_mini_glyph(canvas: &mut image::RgbaImage, c: char, px: u32, py: u32, col: [u8; 4], scale: u32) {
    let glyph = match c {
        '0'..='9' => MINI_FONT[(c as u8 - b'0') as usize],
        'A'..='Z' => MINI_FONT[10 + (c as u8 - b'A') as usize],
        ':' => [0b00000, 0b00100, 0b00000, 0b00100, 0b00000],
        '_' => [0b00000, 0b00000, 0b00000, 0b00000, 0b11111],
        ' ' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
        _   => return,
    };
    let (w, h) = canvas.dimensions();
    for (row, &bits) in glyph.iter().enumerate() {
        for bit in 0..5u32 {
            if (bits >> (4 - bit)) & 1 == 0 { continue; }
            for sy in 0..scale {
                for sx in 0..scale {
                    let x = px + bit * scale + sx;
                    let y = py + row as u32 * scale + sy;
                    if x < w && y < h {
                        set_pixel(canvas, x, y, col);
                    }
                }
            }
        }
    }
}

fn set_pixel(canvas: &mut image::RgbaImage, x: u32, y: u32, col: [u8; 4]) {
    let p = canvas.get_pixel_mut(x, y);
    let a = col[3] as f32 / 255.0;
    p[0] = (p[0] as f32 * (1.0 - a) + col[0] as f32 * a).round() as u8;
    p[1] = (p[1] as f32 * (1.0 - a) + col[1] as f32 * a).round() as u8;
    p[2] = (p[2] as f32 * (1.0 - a) + col[2] as f32 * a).round() as u8;
    p[3] = 255;
}

/// Same 5×5 bitmap font as in som_grid.rs (digits 0-9, letters A-Z).
const MINI_FONT: [[u8; 5]; 36] = [
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
