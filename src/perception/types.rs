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
}
