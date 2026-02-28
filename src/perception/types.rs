use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementType {
    Button,
    Input,
    Link,
    Text,
    Image,
    Checkbox,
    Radio,
    Select,
    Menu,
    MenuItem,
    Icon,
    Container,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UIElement {
    pub id: String,
    pub node_type: ElementType,
    /// Normalized bounding box [xmin, ymin, xmax, ymax] in range 0.0–1.0
    pub bbox: [f32; 4],
    pub content: Option<String>,
    pub confidence: f32,
    /// Optional parent element ID for hierarchy context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

impl UIElement {
    /// Centre of the bounding box in physical pixel coordinates.
    pub fn center_physical(&self, meta: &ScreenshotMeta) -> (i32, i32) {
        let cx = ((self.bbox[0] + self.bbox[2]) / 2.0 * meta.physical_width as f32).round() as i32;
        let cy = ((self.bbox[1] + self.bbox[3]) / 2.0 * meta.physical_height as f32).round() as i32;
        (cx, cy)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotMeta {
    pub monitor_index: u32,
    pub scale_factor: f64,
    pub physical_width: u32,
    pub physical_height: u32,
    pub logical_width: u32,
    pub logical_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerceptionContext {
    pub image_base64: Option<String>,
    pub elements: Vec<UIElement>,
    pub resolution: (u32, u32),
    pub meta: ScreenshotMeta,
    pub source: PerceptionSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionSource {
    Onnx,
    SomGrid,
    Accessibility,
    /// YOLO detection + optional UIA merge + annotation
    YoloAnnotated,
}
