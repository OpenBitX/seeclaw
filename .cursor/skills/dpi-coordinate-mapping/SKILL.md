---
name: dpi-coordinate-mapping
description: Convert normalized UIElement bounding box coordinates to physical screen pixels, handling multi-monitor DPI scaling and scale factors in Tauri. Use when asked to "map coordinates", "handle DPI scaling", "compute click position", "fix coordinate offset", or "implement the executor coordinate logic".
argument-hint: <target-module>
---

# Skill: DPI Scaling & Coordinate Reverse Mapping (Executor Core)

## Overview

This is the most error-prone part of SeeClaw. A `UIElement` stores normalized
coordinates `[xmin, ymin, xmax, ymax]` in range 0.0–1.0, relative to the
**logical** resolution of the captured screenshot. Before passing to `enigo`,
these must be converted to **physical pixel** coordinates on the correct monitor.

Three factors must all be handled:
1. **Screenshot resolution** — the logical size of the captured image
2. **DPI scale factor** — Windows display scaling (e.g. 150% = 1.5)
3. **Monitor offset** — multi-monitor setups with non-zero origin

## Step 1 — Capture Screenshot with Metadata

Always capture metadata alongside the image so the Executor can reverse-map later.

```rust
// src/perception/capture.rs

use xcap::Monitor;

#[derive(Debug, Clone)]
pub struct ScreenshotMeta {
    pub logical_width: u32,
    pub logical_height: u32,
    pub scale_factor: f64,      // e.g. 1.5 for 150% DPI
    pub monitor_x: i32,         // monitor origin in virtual screen coords
    pub monitor_y: i32,
}

pub async fn capture_primary_screen() -> crate::errors::SeeClawResult<(Vec<u8>, ScreenshotMeta)> {
    let monitors = Monitor::all().map_err(|e| crate::errors::SeeClawError::CaptureError(e.to_string()))?;
    let primary = monitors
        .into_iter()
        .find(|m| m.is_primary())
        .ok_or_else(|| crate::errors::SeeClawError::CaptureError("no primary monitor".into()))?;

    let image = primary.capture_image()
        .map_err(|e| crate::errors::SeeClawError::CaptureError(e.to_string()))?;

    let meta = ScreenshotMeta {
        logical_width: primary.width(),
        logical_height: primary.height(),
        scale_factor: primary.scale_factor() as f64,
        monitor_x: primary.x(),
        monitor_y: primary.y(),
    };

    // Encode as PNG bytes for LLM
    let mut buf = Vec::new();
    let dyn_img = image::DynamicImage::ImageRgba8(
        image::RgbaImage::from_raw(image.width(), image.height(), image.into_raw())
            .expect("invalid image dimensions"),
    );
    dyn_img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .map_err(|e| crate::errors::SeeClawError::CaptureError(e.to_string()))?;

    Ok((buf, meta))
}
```

## Step 2 — Normalize Coordinate Mapping Function

```rust
// src/executor/coords.rs

use crate::perception::capture::ScreenshotMeta;

/// Result: physical pixel position ready for enigo
#[derive(Debug, Clone, Copy)]
pub struct PhysicalPoint {
    pub x: i32,
    pub y: i32,
}

/// Convert a normalized UIElement bbox center to physical screen coordinates.
///
/// # Arguments
/// - `bbox`: [xmin, ymin, xmax, ymax] all in range 0.0–1.0 (normalized to screenshot)
/// - `meta`: screenshot metadata captured at perception time
///
/// # Returns
/// Physical pixel coordinates in the OS virtual screen space (multi-monitor safe)
pub fn bbox_to_physical(bbox: &[f32; 4], meta: &ScreenshotMeta) -> PhysicalPoint {
    // Step 1: Find center of bbox in normalized space
    let cx_norm = (bbox[0] + bbox[2]) / 2.0;
    let cy_norm = (bbox[1] + bbox[3]) / 2.0;

    // Step 2: Scale to logical pixels (screenshot resolution)
    let cx_logical = cx_norm as f64 * meta.logical_width as f64;
    let cy_logical = cy_norm as f64 * meta.logical_height as f64;

    // Step 3: Apply DPI scale factor to get physical pixels
    let cx_physical = cx_logical * meta.scale_factor;
    let cy_physical = cy_logical * meta.scale_factor;

    // Step 4: Add monitor origin offset (critical for multi-monitor setups)
    let x = meta.monitor_x + cx_physical.round() as i32;
    let y = meta.monitor_y + cy_physical.round() as i32;

    PhysicalPoint { x, y }
}

/// Convert a SoM grid cell label (e.g. "C4") to physical coordinates.
pub fn grid_cell_to_physical(label: &str, meta: &ScreenshotMeta, grid_n: u32) -> Option<PhysicalPoint> {
    let coord = crate::vision::som_grid::parse_grid_label(label)?;
    let (cx_logical, cy_logical) = crate::vision::som_grid::grid_to_pixel(
        coord,
        meta.logical_width,
        meta.logical_height,
        grid_n,
    );
    let x = meta.monitor_x + (cx_logical * meta.scale_factor).round() as i32;
    let y = meta.monitor_y + (cy_logical * meta.scale_factor).round() as i32;
    Some(PhysicalPoint { x, y })
}
```

## Step 3 — Executor: Execute a Click Action

```rust
// src/executor/mod.rs

use enigo::{Enigo, Mouse, Button, Coordinate, Settings};
use crate::executor::coords::{bbox_to_physical, PhysicalPoint};
use crate::agent_engine::action::AgentAction;
use crate::perception::PerceptionContext;
use crate::errors::SeeClawError;

pub struct Executor {
    enigo: Enigo,
}

impl Executor {
    pub fn new() -> Result<Self, SeeClawError> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| SeeClawError::ExecutorError(e.to_string()))?;
        Ok(Self { enigo })
    }

    pub fn execute_click(&mut self, element_id: &str, ctx: &PerceptionContext) -> Result<(), SeeClawError> {
        let element = ctx.elements.iter()
            .find(|el| el.id == element_id)
            .ok_or_else(|| SeeClawError::ExecutorError(format!("element id {element_id} not found")))?;

        let point = bbox_to_physical(&element.bbox, &ctx.meta);

        tracing::info!("clicking element {} at physical ({}, {})", element_id, point.x, point.y);

        self.enigo.move_mouse(point.x, point.y, Coordinate::Abs)
            .map_err(|e| SeeClawError::ExecutorError(e.to_string()))?;

        // Small delay to simulate human-like movement completion
        std::thread::sleep(std::time::Duration::from_millis(80));

        self.enigo.button(Button::Left, enigo::Direction::Click)
            .map_err(|e| SeeClawError::ExecutorError(e.to_string()))?;

        Ok(())
    }
}
```

## Step 4 — Update PerceptionContext to Include Meta

Add `meta` field to the existing `PerceptionContext` struct in `arch/types.rs`:

```rust
// src/perception/types.rs

use crate::perception::capture::ScreenshotMeta;

pub struct PerceptionContext {
    pub image_base64: Option<String>,
    pub elements: Vec<UIElement>,
    pub resolution: (u32, u32),
    pub meta: ScreenshotMeta,   // ← add this field for coordinate reverse mapping
}
```

## Common Pitfalls

| Mistake | Symptom | Fix |
|---|---|---|
| Forgetting `scale_factor` | Click lands at 150% of intended position on HiDPI displays | Always multiply logical coords by `scale_factor` |
| Forgetting `monitor_x/y` | Click offset on secondary monitors or non-zero-origin primary | Always add monitor origin after scaling |
| Using screenshot physical size instead of logical | Coordinates doubled on HiDPI | Use `Monitor::width()`/`height()` (logical), not image pixel dimensions |
| Stale `meta` from previous loop iteration | Click targets old screen position | Always use `meta` captured **in the same perception cycle** as the screenshot |

## Important Notes

- `xcap::Monitor::scale_factor()` returns the Windows DPI scale as a float (e.g. `1.5` for 150%)
- `enigo` on Windows uses **physical** pixel coordinates in `Coordinate::Abs` mode — always pass physical coords
- Add `tokio::time::sleep(Duration::from_millis(500))` after each click before re-taking screenshot (wait for UI to settle)
- For multi-monitor: iterate all monitors if the target window may not be on the primary
