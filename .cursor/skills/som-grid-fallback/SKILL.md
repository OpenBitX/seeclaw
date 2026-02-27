---
name: som-grid-fallback
description: Implement the SoM grid fallback when ONNX vision detects zero elements. Use when asked to "add grid fallback", "implement SoM", "handle no detections", "draw coordinate grid", or "build the VLM grid prompt".
argument-hint: <target-module>
---

# Skill: SoM Grid Fallback Algorithm & VLM Prompt (Plan C)

## When to Use This Skill

Activate the SoM Grid fallback when the local ONNX vision model detects **zero bounding boxes**
or confidence scores fall below the threshold. This is the last-resort element locator.

## Step 1 — Draw a Labeled Grid onto the Screenshot

> **Dependency note**: `rusttype` is unmaintained. Use `ab_glyph` instead.
> `imageproc`'s `draw_text_mut` accepts `&impl ab_glyph::Font` since imageproc 0.23+.

```toml
# Cargo.toml
[dependencies]
image = "0.25"
imageproc = "0.25"
ab_glyph = "0.2"
```

```rust
// src/vision/som_grid.rs

use ab_glyph::{FontVec, PxScale};
use image::{DynamicImage, Rgba};
use imageproc::drawing::{draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;

/// Draw an N×N grid of labeled cells onto the screenshot.
/// Returns the annotated image as PNG bytes.
pub fn draw_som_grid(img: &DynamicImage, grid_n: u32) -> Vec<u8> {
    let mut canvas = img.to_rgba8();
    let (w, h) = canvas.dimensions();
    let cell_w = w / grid_n;
    let cell_h = h / grid_n;

    let font = load_font();
    let scale = PxScale::from(14.0);

    // Column labels: A, B, C ... Z, AA, AB ...
    let col_label = |c: u32| -> String {
        if c < 26 {
            String::from(char::from(b'A' + c as u8))
        } else {
            format!("A{}", char::from(b'A' + (c - 26) as u8))
        }
    };

    let grid_color = Rgba([0u8, 220u8, 255u8, 60u8]);   // dim cyan, semi-transparent
    let text_color = Rgba([0u8, 220u8, 255u8, 220u8]);

    for row in 0..grid_n {
        for col in 0..grid_n {
            let x = (col * cell_w) as i32;
            let y = (row * cell_h) as i32;

            draw_hollow_rect_mut(&mut canvas, Rect::at(x, y).of_size(cell_w, cell_h), grid_color);

            // Label: e.g., "C4" — col letter + row number
            let label = format!("{}{}", col_label(col), row + 1);
            draw_text_mut(&mut canvas, text_color, x + 4, y + 4, scale, &font, &label);
        }
    }

    let mut buf = Vec::new();
    canvas.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
    buf
}

fn load_font() -> FontVec {
    let font_data = include_bytes!("../../assets/JetBrainsMono-Regular.ttf").to_vec();
    FontVec::try_from_vec(font_data).expect("failed to load font")
}
```

## Step 2 — Build the VLM Prompt

When sending the grid-annotated screenshot to GLM-4.6V, use this exact prompt structure:

```rust
// src/llm/prompt_builder.rs

pub fn build_som_grid_prompt(goal: &str, grid_n: u32) -> String {
    format!(
        r#"You are a GUI automation agent. The screenshot has been overlaid with a {n}×{n} coordinate grid.
Each cell is labeled with a column letter (A, B, C...) and a row number (1, 2, 3...), for example: "C4", "A1", "F7".

Your task: {goal}

Instructions:
1. Identify the UI element that matches the task description.
2. Report the SINGLE grid cell label that best covers the target element's center.
3. Reply in this exact JSON format and nothing else:
   {{"grid_cell": "C4", "reasoning": "The submit button appears to be centered in column C, row 4."}}

Do NOT include any markdown, code fences, or extra text outside the JSON."#,
        n = grid_n,
        goal = goal,
    )
}
```

## Step 3 — Map Grid Cell Back to Physical Coordinates

```rust
// src/vision/som_grid.rs (continued)

pub struct GridCoord {
    pub col: u32,  // 0-indexed
    pub row: u32,  // 0-indexed
}

/// Parse "C4" -> GridCoord { col: 2, row: 3 }
pub fn parse_grid_label(label: &str) -> Option<GridCoord> {
    let label = label.trim().to_uppercase();
    let col_str: String = label.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let row_str: String = label.chars().skip_while(|c| c.is_ascii_alphabetic()).collect();

    let col = if col_str.len() == 1 {
        (col_str.chars().next()? as u32).checked_sub(b'A' as u32)?
    } else {
        26 + (col_str.chars().nth(1)? as u32 - b'A' as u32)
    };
    let row = row_str.parse::<u32>().ok()?.checked_sub(1)?;
    Some(GridCoord { col, row })
}

/// Convert grid cell to center pixel coordinates (physical, pre-DPI)
pub fn grid_to_pixel(coord: GridCoord, img_w: u32, img_h: u32, grid_n: u32) -> (f64, f64) {
    let cell_w = img_w as f64 / grid_n as f64;
    let cell_h = img_h as f64 / grid_n as f64;
    let cx = coord.col as f64 * cell_w + cell_w / 2.0;
    let cy = coord.row as f64 * cell_h + cell_h / 2.0;
    (cx, cy)
}
```

## Important Notes

- Grid size recommendation: **8×8** for dense UIs, **12×12** for sparse desktop layouts
- Always multiply output coordinates by `window.scale_factor()` before passing to `enigo`
- Log the fallback activation: `tracing::warn!("SoM grid fallback activated — no YOLO detections");`
