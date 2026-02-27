---
name: onnx-vision-rust
description: Integrate a local ONNX/YOLO vision model into the Rust/Tauri backend for UI element detection. Use when asked to "add vision", "set up ONNX", "detect UI elements", "load the model", or "implement screenshot detection".
argument-hint: <target-module-or-feature>
---

# Skill: Rust ONNX Runtime Integration (Plan B — Local Vision Model)

## Overview

Use the `ort` crate to run a lightweight YOLO-style UI detection model locally in Rust.
All model inference must be non-blocking and managed as a singleton Tauri state.

## Step 1 — Cargo.toml Dependencies

```toml
[dependencies]
# ort 2.x: Environment is removed; use Session::builder() directly
ort = { version = "2", features = ["load-dynamic"] }
image = "0.25"                                  # Image loading & resizing
ndarray = "0.16"                                # Tensor format conversion
```

## Step 2 — Singleton Model Loading at Tauri Init

Load the model **once** during `tauri::Builder` setup. Never reload per screenshot.

> **ort 2.x breaking change**: `Environment` and `SessionBuilder::new(&env)` are removed.
> Use `Session::builder()` directly. The runtime is initialized globally on first use.

```rust
// src/vision/model.rs

use ort::{GraphOptimizationLevel, Session};

pub struct VisionModel {
    pub session: Session,
}

impl VisionModel {
    pub fn load(model_path: &str) -> ort::Result<Self> {
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(model_path)?;
        Ok(Self { session })
    }
}

// In main.rs — register as managed state
tauri::Builder::default()
    .manage(VisionModel::load("models/ui_yolo_nano.onnx").expect("failed to load model"))
    .run(tauri::generate_context!())
    .expect("error while running tauri app");
```

## Step 3 — Image Preprocessing

Convert a screenshot to the float32 tensor format YOLO expects (CHW, normalized 0–1).

```rust
// src/vision/preprocess.rs

use image::{DynamicImage, GenericImageView};
use ndarray::{Array, Array4};

const INPUT_W: u32 = 640;
const INPUT_H: u32 = 640;

pub fn preprocess(img: &DynamicImage) -> Array4<f32> {
    let resized = img.resize_exact(INPUT_W, INPUT_H, image::imageops::FilterType::Lanczos3);
    let (w, h) = resized.dimensions();

    // Build CHW tensor: shape [1, 3, H, W]
    let mut tensor = Array4::<f32>::zeros((1, 3, h as usize, w as usize));
    for (x, y, pixel) in resized.pixels() {
        let [r, g, b, _] = pixel.0;
        tensor[[0, 0, y as usize, x as usize]] = r as f32 / 255.0;
        tensor[[0, 1, y as usize, x as usize]] = g as f32 / 255.0;
        tensor[[0, 2, y as usize, x as usize]] = b as f32 / 255.0;
    }
    tensor
}
```

## Step 4 — Post-processing: Extract Bounding Boxes

```rust
// src/vision/postprocess.rs

#[derive(Debug, Clone, serde::Serialize)]
pub struct BBox {
    pub id: u32,
    pub xmin: f32,
    pub ymin: f32,
    pub xmax: f32,
    pub ymax: f32,
    pub confidence: f32,
    pub class_id: u32,
}

pub fn parse_yolo_output(output: &[f32], orig_w: f32, orig_h: f32, conf_thresh: f32) -> Vec<BBox> {
    // YOLO output shape: [1, num_boxes, 6] where each row is [cx, cy, w, h, conf, class_id]
    let mut boxes = Vec::new();
    let stride = 6;
    let mut id = 0u32;

    for chunk in output.chunks(stride) {
        let conf = chunk[4];
        if conf < conf_thresh { continue; }

        let (cx, cy, bw, bh) = (chunk[0], chunk[1], chunk[2], chunk[3]);
        // Scale back to original image coordinates
        let xmin = ((cx - bw / 2.0) * orig_w).max(0.0);
        let ymin = ((cy - bh / 2.0) * orig_h).max(0.0);
        let xmax = ((cx + bw / 2.0) * orig_w).min(orig_w);
        let ymax = ((cy + bh / 2.0) * orig_h).min(orig_h);

        boxes.push(BBox { id, xmin, ymin, xmax, ymax, confidence: conf, class_id: chunk[5] as u32 });
        id += 1;
    }
    boxes
}
```

## Important Notes

- Always handle DPI scaling: multiply physical coords by `window.scale_factor()` before passing to `enigo`
- Store the processed `Vec<BBox>` in Tauri state or pass directly to the LLM prompt builder
- For debug mode: draw numbered boxes on the screenshot using `imageproc` and save to `%TEMP%/seeclaw_debug.png`
