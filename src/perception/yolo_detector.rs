/// ONNX YOLOv8 inference for UI element detection.
///
/// Loads a YOLOv8 nano ONNX model and runs detection on screenshots.
/// Falls back gracefully if the model file is missing.
use crate::errors::{SeeClawError, SeeClawResult};
use crate::perception::types::{ElementType, UIElement};

use ndarray::Array4;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;
use std::path::Path;

/// Raw detection before NMS and ID assignment.
#[derive(Debug, Clone)]
struct RawDetection {
    bbox: [f32; 4], // [x1, y1, x2, y2] normalised to [0,1]
    confidence: f32,
    class_id: usize,
}

/// Holds the ONNX Runtime session and inference configuration.
pub struct YoloDetector {
    session: Session,
    input_size: u32,
    conf_threshold: f32,
    iou_threshold: f32,
    class_names: Vec<String>,
}

impl YoloDetector {
    /// Try to construct a detector.  Returns `None` if the model file does not exist.
    pub fn try_new(
        model_path: &str,
        conf_threshold: f32,
        iou_threshold: f32,
        class_names: Vec<String>,
    ) -> Option<Self> {
        if !Path::new(model_path).exists() {
            tracing::warn!(path = %model_path, "YOLO model not found — detection disabled");
            return None;
        }
        match Self::build(model_path, conf_threshold, iou_threshold, class_names) {
            Ok(det) => {
                tracing::info!(path = %model_path, "YOLO detector loaded");
                Some(det)
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to load YOLO model");
                None
            }
        }
    }

    fn build(
        model_path: &str,
        conf_threshold: f32,
        iou_threshold: f32,
        class_names: Vec<String>,
    ) -> SeeClawResult<Self> {
        let session = Session::builder()
            .map_err(|e| SeeClawError::Perception(format!("ort session builder: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| SeeClawError::Perception(format!("ort opt-level: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| SeeClawError::Perception(format!("ort load model: {e}")))?;

        Ok(Self {
            session,
            input_size: 640,
            conf_threshold,
            iou_threshold,
            class_names,
        })
    }

    // ── Public API ──────────────────────────────────────────────────────────

    /// Run detection.  `image_bytes` should be JPEG or PNG.
    /// Returns a list of `UIElement` with unique IDs per class (e.g. btn_1, icon_2).
    pub fn detect(&mut self, image_bytes: &[u8]) -> SeeClawResult<Vec<UIElement>> {
        let img = image::load_from_memory(image_bytes)
            .map_err(|e| SeeClawError::Perception(format!("image load: {e}")))?;
        let (orig_w, orig_h) = (img.width(), img.height());

        let (input_tensor, pad_x, pad_y, scale) = self.preprocess(&img)?;

        // Inference — convert ndarray to ort Tensor, then run
        let input_value = Tensor::from_array(input_tensor)
            .map_err(|e| SeeClawError::Perception(format!("ort tensor: {e}")))?;

        let output_owned = {
            let outputs = self
                .session
                .run(ort::inputs![input_value])
                .map_err(|e| SeeClawError::Perception(format!("ort run: {e}")))?;

            outputs[0]
                .try_extract_array::<f32>()
                .map_err(|e| SeeClawError::Perception(format!("extract tensor: {e}")))?
                .to_owned()
            // `outputs` (and the mutable borrow on session) is dropped here
        };

        let raw = self.postprocess(&output_owned.view(), orig_w, orig_h, pad_x, pad_y, scale)?;
        let elements = self.assign_ids(raw);
        Ok(elements)
    }

    // ── Pre-processing ──────────────────────────────────────────────────────

    /// Resize + letterbox + normalise → NCHW f32 tensor.
    fn preprocess(
        &self,
        img: &image::DynamicImage,
    ) -> SeeClawResult<(Array4<f32>, f32, f32, f32)> {
        let sz = self.input_size;
        let (ow, oh) = (img.width() as f32, img.height() as f32);
        let scale = (sz as f32 / ow).min(sz as f32 / oh);
        let nw = (ow * scale).round() as u32;
        let nh = (oh * scale).round() as u32;
        let pad_x = (sz - nw) as f32 / 2.0;
        let pad_y = (sz - nh) as f32 / 2.0;

        let resized =
            img.resize_exact(nw, nh, image::imageops::FilterType::CatmullRom);
        let rgb = resized.to_rgb8();

        // Grey‐fill canvas
        let mut canvas =
            image::RgbImage::from_pixel(sz, sz, image::Rgb([114, 114, 114]));
        image::imageops::overlay(
            &mut canvas,
            &rgb,
            pad_x.round() as i64,
            pad_y.round() as i64,
        );

        // HWC → NCHW normalised [0, 1]
        let mut tensor = Array4::<f32>::zeros((1, 3, sz as usize, sz as usize));
        for y in 0..sz {
            for x in 0..sz {
                let p = canvas.get_pixel(x, y);
                tensor[[0, 0, y as usize, x as usize]] = p[0] as f32 / 255.0;
                tensor[[0, 1, y as usize, x as usize]] = p[1] as f32 / 255.0;
                tensor[[0, 2, y as usize, x as usize]] = p[2] as f32 / 255.0;
            }
        }

        Ok((tensor, pad_x, pad_y, scale))
    }

    // ── Post-processing ─────────────────────────────────────────────────────

    fn postprocess(
        &self,
        output: &ndarray::ArrayViewD<f32>,
        orig_w: u32,
        orig_h: u32,
        pad_x: f32,
        pad_y: f32,
        scale: f32,
    ) -> SeeClawResult<Vec<RawDetection>> {
        // YOLOv8 output: [1, 4+num_classes, num_proposals]
        let shape = output.shape();
        if shape.len() < 3 {
            return Err(SeeClawError::Perception(format!(
                "unexpected output shape: {:?}",
                shape
            )));
        }
        let num_classes = shape[1] - 4;
        let num_preds = shape[2];

        let mut detections: Vec<RawDetection> = Vec::new();

        for i in 0..num_preds {
            let cx = output[[0, 0, i]];
            let cy = output[[0, 1, i]];
            let w = output[[0, 2, i]];
            let h = output[[0, 3, i]];

            // Best class
            let mut max_score = 0.0f32;
            let mut max_class = 0usize;
            for c in 0..num_classes {
                let s = output[[0, 4 + c, i]];
                if s > max_score {
                    max_score = s;
                    max_class = c;
                }
            }
            if max_score < self.conf_threshold {
                continue;
            }

            // Undo letterbox → original pixel space → normalise to [0,1]
            let x1 = ((cx - w / 2.0) - pad_x) / scale;
            let y1 = ((cy - h / 2.0) - pad_y) / scale;
            let x2 = ((cx + w / 2.0) - pad_x) / scale;
            let y2 = ((cy + h / 2.0) - pad_y) / scale;

            let nx1 = (x1 / orig_w as f32).clamp(0.0, 1.0);
            let ny1 = (y1 / orig_h as f32).clamp(0.0, 1.0);
            let nx2 = (x2 / orig_w as f32).clamp(0.0, 1.0);
            let ny2 = (y2 / orig_h as f32).clamp(0.0, 1.0);

            detections.push(RawDetection {
                bbox: [nx1, ny1, nx2, ny2],
                confidence: max_score,
                class_id: max_class,
            });
        }

        // Per-class NMS
        let kept = self.nms(&detections);
        Ok(kept.into_iter().map(|i| detections[i].clone()).collect())
    }

    /// Greedy NMS.
    fn nms(&self, dets: &[RawDetection]) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..dets.len()).collect();
        indices.sort_by(|&a, &b| {
            dets[b]
                .confidence
                .partial_cmp(&dets[a].confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut keep = Vec::new();
        let mut suppressed = vec![false; dets.len()];

        for &i in &indices {
            if suppressed[i] {
                continue;
            }
            keep.push(i);
            for &j in &indices {
                if suppressed[j] || i == j {
                    continue;
                }
                if dets[i].class_id == dets[j].class_id
                    && iou(&dets[i].bbox, &dets[j].bbox) > self.iou_threshold
                {
                    suppressed[j] = true;
                }
            }
        }
        keep
    }

    /// Assign unique semantic IDs per class, e.g. btn_1, icon_2.
    fn assign_ids(&self, raws: Vec<RawDetection>) -> Vec<UIElement> {
        let mut counters = std::collections::HashMap::<usize, u32>::new();
        let mut elements = Vec::with_capacity(raws.len());

        for det in raws {
            let count = counters.entry(det.class_id).or_insert(0);
            *count += 1;

            let prefix = self.class_prefix(det.class_id);
            let node_type = self.class_to_element_type(det.class_id);
            let id = format!("{}_{}", prefix, count);

            elements.push(UIElement {
                id,
                node_type,
                bbox: det.bbox,
                content: None,
                confidence: det.confidence,
                parent_id: None,
            });
        }
        elements
    }

    fn class_prefix(&self, class_id: usize) -> &str {
        if class_id < self.class_names.len() {
            return match self.class_names[class_id].as_str() {
                "button" => "btn",
                "input" => "input",
                "link" => "link",
                "icon" => "ui",  // GPA-GUI-Detector single-class: use generic "ui" prefix
                "checkbox" => "chk",
                "radio" => "radio",
                "menu" => "menu",
                "menuitem" => "mi",
                "scrollbar" => "scroll",
                "tab" => "tab",
                "toolbar" => "tb",
                "window" => "win",
                "text" => "txt",
                "image" => "img",
                "container" => "cont",
                other => other,
            };
        }
        // Fallback for COCO or unknown class list
        "obj"
    }

    fn class_to_element_type(&self, class_id: usize) -> ElementType {
        if class_id < self.class_names.len() {
            return match self.class_names[class_id].as_str() {
                "button" => ElementType::Button,
                "input" => ElementType::Input,
                "link" => ElementType::Link,
                "icon" => ElementType::Icon,  // GPA-GUI-Detector: all detections are interactive UI elements
                "checkbox" => ElementType::Checkbox,
                "radio" => ElementType::Radio,
                "menu" => ElementType::Menu,
                "menuitem" => ElementType::MenuItem,
                "scrollbar" => ElementType::Select,
                "tab" => ElementType::Container,
                "toolbar" => ElementType::Container,
                "window" => ElementType::Container,
                "text" => ElementType::Text,
                "image" => ElementType::Image,
                "container" => ElementType::Container,
                _ => ElementType::Unknown,
            };
        }
        ElementType::Unknown
    }
}

// ── Utilities ────────────────────────────────────────────────────────────────

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let ix1 = a[0].max(b[0]);
    let iy1 = a[1].max(b[1]);
    let ix2 = a[2].min(b[2]);
    let iy2 = a[3].min(b[3]);

    let inter = (ix2 - ix1).max(0.0) * (iy2 - iy1).max(0.0);
    let area_a = (a[2] - a[0]) * (a[3] - a[1]);
    let area_b = (b[2] - b[0]) * (b[3] - b[1]);
    let union = area_a + area_b - inter;

    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Default class names for the GPA-GUI-Detector (single-class icon detection).
pub fn default_ui_class_names() -> Vec<String> {
    vec!["icon"]
        .into_iter()
        .map(String::from)
        .collect()
}

/// Legacy UI element class names for a custom multi-class trained model.
pub fn legacy_ui_class_names() -> Vec<String> {
    vec![
        "button", "input", "link", "icon", "checkbox", "radio", "menu",
        "menuitem", "scrollbar", "tab", "toolbar", "window", "text",
        "image", "container",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

/// COCO 80-class names (fallback when using a generic YOLOv8n).
pub fn coco_class_names() -> Vec<String> {
    vec![
        "person","bicycle","car","motorcycle","airplane","bus","train","truck",
        "boat","traffic light","fire hydrant","stop sign","parking meter","bench",
        "bird","cat","dog","horse","sheep","cow","elephant","bear","zebra",
        "giraffe","backpack","umbrella","handbag","tie","suitcase","frisbee",
        "skis","snowboard","sports ball","kite","baseball bat","baseball glove",
        "skateboard","surfboard","tennis racket","bottle","wine glass","cup",
        "fork","knife","spoon","bowl","banana","apple","sandwich","orange",
        "broccoli","carrot","hot dog","pizza","donut","cake","chair","couch",
        "potted plant","bed","dining table","toilet","tv","laptop","mouse",
        "remote","keyboard","cell phone","microwave","oven","toaster","sink",
        "refrigerator","book","clock","vase","scissors","teddy bear",
        "hair drier","toothbrush",
    ].into_iter().map(String::from).collect()
}
