/// Windows UI Automation (UIA) element collection.
///
/// Walks the accessibility tree of the desktop and returns visible interactive
/// elements with their bounding rectangles, control types, and names.
/// On non-Windows platforms this module is a no-op stub.
use crate::errors::SeeClawResult;
use crate::perception::types::{ElementType, ScreenshotMeta, UIElement};

// ── Windows implementation ──────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod win {
    use super::*;
    use crate::errors::SeeClawError;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
        UIA_CONTROLTYPE_ID,
    };

    /// RAII guard for COM initialization on the current thread.
    struct ComGuard;
    impl ComGuard {
        fn new() -> Result<Self, SeeClawError> {
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED)
                    .ok()
                    .map_err(|e| SeeClawError::Perception(format!("CoInitializeEx: {e}")))?;
            }
            Ok(Self)
        }
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    /// Maximum normalised area — elements larger than this fraction of the screen
    /// are treated as background containers and dropped (unless they are
    /// explicitly interactive with a name, e.g. a named full-screen button).
    const MAX_AREA_RATIO: f32 = 0.25;

    /// Minimum normalised edge length — elements smaller than this are noise.
    const MIN_EDGE: f32 = 0.008;

    /// Bottom region of the screen considered as taskbar (normalised Y).
    /// Elements entirely within this strip are likely taskbar/tray items.
    const TASKBAR_Y_THRESHOLD: f32 = 0.96;

    /// Returns `true` for element types that are *primary* interactive controls.
    /// Menu/MenuItem are excluded because taskbar & system tray flood the view
    /// with unnamed MenuItem elements.
    fn is_interactive(et: &ElementType) -> bool {
        matches!(
            et,
            ElementType::Button
                | ElementType::Input
                | ElementType::Link
                | ElementType::Checkbox
                | ElementType::Radio
                | ElementType::Select
                | ElementType::Icon
        )
    }

    /// Collects visible UI elements from the accessibility tree.
    /// Must be called from a blocking thread (COM is not async-safe).
    ///
    /// Improvements over the original collector:
    /// - Walks up to 7 levels deep (was 4) for finer-grained elements.
    /// - Filters out oversized background containers (area > 40 % of screen)
    ///   unless they are interactive controls with a name.
    /// - Unnamed `Container` / `Unknown` types are skipped.
    /// - Tracks parent IDs so VLM can understand nesting.
    /// - Post-processes with NMS to remove highly overlapping boxes.
    pub fn collect_elements_sync(meta: &ScreenshotMeta) -> SeeClawResult<Vec<UIElement>> {
        let _com = ComGuard::new()?;

        let automation: IUIAutomation = unsafe {
            CoCreateInstance(&CUIAutomation, None, CLSCTX_ALL)
                .map_err(|e| SeeClawError::Perception(format!("CoCreateInstance UIA: {e}")))?
        };

        let root = unsafe {
            automation
                .GetRootElement()
                .map_err(|e| SeeClawError::Perception(format!("GetRootElement: {e}")))?
        };

        let walker = unsafe {
            automation
                .ControlViewWalker()
                .map_err(|e| SeeClawError::Perception(format!("ControlViewWalker: {e}")))?
        };

        let mut elements = Vec::new();
        let mut counters = std::collections::HashMap::<String, u32>::new();

        walk_tree(
            &walker,
            &root,
            meta,
            None,        // parent_id
            0,
            7,           // max depth (was 4)
            500,         // max elements
            &mut elements,
            &mut counters,
        );

        // ── Post-collection NMS ─────────────────────────────────────────
        let elements = nms_elements(elements, 0.50);

        tracing::debug!(count = elements.len(), "UIA elements collected (after filter+NMS)");
        Ok(elements)
    }

    fn walk_tree(
        walker: &IUIAutomationTreeWalker,
        element: &IUIAutomationElement,
        meta: &ScreenshotMeta,
        parent_id: Option<&str>,
        depth: u32,
        max_depth: u32,
        max_elements: usize,
        out: &mut Vec<UIElement>,
        counters: &mut std::collections::HashMap<String, u32>,
    ) {
        if depth > max_depth || out.len() >= max_elements {
            return;
        }

        // Extract element properties (ignore errors — some elements are inaccessible)
        let current_id: Option<String> =
            if let Ok(mut ui_elem) = extract_element(element, meta, counters) {
                let bw = ui_elem.bbox[2] - ui_elem.bbox[0];
                let bh = ui_elem.bbox[3] - ui_elem.bbox[1];
                let area = bw * bh;

                // ── Smart filtering ────────────────────────────────────────
                let too_small = bw < MIN_EDGE || bh < MIN_EDGE;
                let too_large = area > MAX_AREA_RATIO
                    && !(is_interactive(&ui_elem.node_type) && ui_elem.content.is_some());

                // Drop unnamed elements of low-signal types (containers,
                // text labels, menu items, images without a name, etc.)
                let unnamed_low_signal = ui_elem.content.is_none()
                    && matches!(
                        ui_elem.node_type,
                        ElementType::Container
                            | ElementType::Unknown
                            | ElementType::Text
                            | ElementType::MenuItem
                            | ElementType::Menu
                            | ElementType::Image
                    );

                // Elements sitting entirely in the bottom taskbar strip
                let in_taskbar = ui_elem.bbox[1] >= TASKBAR_Y_THRESHOLD;

                if !too_small && !too_large && !unnamed_low_signal && !in_taskbar
                    && bw < 1.0 && bh < 1.0
                {
                    // Record parent_id for hierarchy context
                    ui_elem.parent_id = parent_id.map(|s| s.to_string());
                    let id_clone = ui_elem.id.clone();
                    out.push(ui_elem);
                    Some(id_clone)
                } else {
                    None
                }
            } else {
                None
            };

        // The parent_id for children: use this element's ID if it was kept,
        // otherwise inherit the grandparent.
        let child_parent = current_id.as_deref().or(parent_id);

        // Walk children
        let child = unsafe { walker.GetFirstChildElement(element) };
        let Ok(mut child) = child else { return };

        loop {
            walk_tree(
                walker,
                &child,
                meta,
                child_parent,
                depth + 1,
                max_depth,
                max_elements,
                out,
                counters,
            );

            match unsafe { walker.GetNextSiblingElement(&child) } {
                Ok(next) => child = next,
                Err(_) => break,
            }
        }
    }

    fn extract_element(
        element: &IUIAutomationElement,
        meta: &ScreenshotMeta,
        counters: &mut std::collections::HashMap<String, u32>,
    ) -> SeeClawResult<UIElement> {
        let rect: RECT = unsafe {
            element
                .CurrentBoundingRectangle()
                .map_err(|e| SeeClawError::Perception(format!("bbox: {e}")))?
        };
        let name = unsafe {
            element
                .CurrentName()
                .unwrap_or_default()
                .to_string()
        };
        let control_type = unsafe {
            element.CurrentControlType().unwrap_or(UIA_CONTROLTYPE_ID(0))
        };
        let is_offscreen = unsafe { element.CurrentIsOffscreen().unwrap_or_default().as_bool() };
        if is_offscreen {
            return Err(SeeClawError::Perception("offscreen".into()));
        }

        let node_type = control_type_to_element(control_type.0);
        let prefix = element_type_prefix(&node_type);

        let count = counters.entry(prefix.to_string()).or_insert(0);
        *count += 1;
        let id = format!("uia_{}_{}", prefix, count);

        // Convert screen rect to normalised [0, 1] using physical dimensions
        let pw = meta.physical_width as f32;
        let ph = meta.physical_height as f32;

        // UIA BoundingRectangle is in screen coordinates.
        // On DPI-aware processes these are physical pixels; on unaware they're logical.
        // We treat them as physical and clamp.
        let x1 = (rect.left as f32 / pw).clamp(0.0, 1.0);
        let y1 = (rect.top as f32 / ph).clamp(0.0, 1.0);
        let x2 = (rect.right as f32 / pw).clamp(0.0, 1.0);
        let y2 = (rect.bottom as f32 / ph).clamp(0.0, 1.0);

        Ok(UIElement {
            id,
            node_type,
            bbox: [x1, y1, x2, y2],
            content: if name.is_empty() { None } else { Some(name) },
            confidence: 0.9,
            parent_id: None, // set later in walk_tree
        })
    }

    /// NMS for UIA elements: among highly overlapping boxes, keep the *more
    /// specific* one (smaller area, or interactive type).
    /// Also performs **containment suppression**: if a larger box fully contains
    /// a smaller one and the larger box is not a primary interactive control,
    /// the larger box is suppressed.
    fn nms_elements(elems: Vec<UIElement>, iou_threshold: f32) -> Vec<UIElement> {
        if elems.is_empty() {
            return elems;
        }
        // Score: smaller area + interactive bonus → higher priority
        let scores: Vec<f32> = elems
            .iter()
            .map(|e| {
                let area = (e.bbox[2] - e.bbox[0]).max(0.0) * (e.bbox[3] - e.bbox[1]).max(0.0);
                let interactive_bonus = if is_interactive(&e.node_type) { 0.5 } else { 0.0 };
                let named_bonus = if e.content.is_some() { 0.3 } else { 0.0 };
                // Lower area is better → invert; add bonuses
                (1.0 - area) + interactive_bonus + named_bonus
            })
            .collect();

        let mut indices: Vec<usize> = (0..elems.len()).collect();
        indices.sort_by(|&a, &b| {
            scores[b]
                .partial_cmp(&scores[a])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut suppressed = vec![false; elems.len()];

        // ── Pass 1: Containment suppression ─────────────────────────────
        // If box A fully contains box B, suppress the LARGER one (A) unless
        // A is an interactive control (button, input, etc.).
        for i in 0..elems.len() {
            if suppressed[i] { continue; }
            for j in 0..elems.len() {
                if i == j || suppressed[j] { continue; }
                let (a, b) = (&elems[i].bbox, &elems[j].bbox);
                // Check if i fully contains j
                if a[0] <= b[0] && a[1] <= b[1] && a[2] >= b[2] && a[3] >= b[3] {
                    // i contains j → suppress i (the bigger one) if it's not interactive
                    if !is_interactive(&elems[i].node_type) {
                        suppressed[i] = true;
                        break;
                    }
                }
            }
        }

        // ── Pass 2: IoU-based NMS ───────────────────────────────────────
        let mut keep = Vec::new();
        for &i in &indices {
            if suppressed[i] {
                continue;
            }
            keep.push(i);
            for &j in &indices {
                if suppressed[j] || j == i {
                    continue;
                }
                if super::bbox_iou(&elems[i].bbox, &elems[j].bbox) > iou_threshold {
                    suppressed[j] = true;
                }
            }
        }

        // Preserve original order for determinism
        keep.sort();
        let keep_set: std::collections::HashSet<usize> = keep.into_iter().collect();
        elems
            .into_iter()
            .enumerate()
            .filter(|(i, _)| keep_set.contains(i))
            .map(|(_, e)| e)
            .collect()
    }

    fn control_type_to_element(ct: i32) -> ElementType {
        // UIA_*ControlTypeId values
        match ct {
            50000 => ElementType::Button,      // Button
            50002 => ElementType::Checkbox,     // CheckBox
            50003 => ElementType::Select,       // ComboBox
            50004 => ElementType::Input,        // Edit
            50005 => ElementType::Link,         // Hyperlink
            50006 => ElementType::Image,        // Image
            50007 => ElementType::MenuItem,     // ListItem
            50008 => ElementType::Container,    // List
            50009 => ElementType::Menu,         // Menu
            50010 => ElementType::Menu,         // MenuBar
            50011 => ElementType::MenuItem,     // MenuItem
            50012 => ElementType::Container,    // ProgressBar
            50013 => ElementType::Radio,        // RadioButton
            50014 => ElementType::Select,       // ScrollBar
            50015 => ElementType::Container,    // Slider
            50018 => ElementType::Container,    // Tab
            50019 => ElementType::Container,    // TabItem
            50020 => ElementType::Text,         // Text
            50021 => ElementType::Container,    // ToolBar
            50032 => ElementType::Container,    // Window
            50033 => ElementType::Text,         // TitleBar
            _     => ElementType::Unknown,
        }
    }

    fn element_type_prefix(et: &ElementType) -> &'static str {
        match et {
            ElementType::Button => "btn",
            ElementType::Input => "input",
            ElementType::Link => "link",
            ElementType::Icon => "icon",
            ElementType::Checkbox => "chk",
            ElementType::Radio => "radio",
            ElementType::Select => "sel",
            ElementType::Menu => "menu",
            ElementType::MenuItem => "mi",
            ElementType::Text => "txt",
            ElementType::Image => "img",
            ElementType::Container => "cont",
            ElementType::Unknown => "unk",
        }
    }
}

// ── Async wrapper ───────────────────────────────────────────────────────────

/// Async entry point: spawns collection on a blocking thread.
#[cfg(target_os = "windows")]
pub async fn collect_ui_elements(meta: &ScreenshotMeta) -> SeeClawResult<Vec<UIElement>> {
    let meta = meta.clone();
    tokio::task::spawn_blocking(move || win::collect_elements_sync(&meta))
        .await
        .map_err(|e| crate::errors::SeeClawError::Perception(format!("join: {e}")))?
}

#[cfg(not(target_os = "windows"))]
pub async fn collect_ui_elements(_meta: &ScreenshotMeta) -> SeeClawResult<Vec<UIElement>> {
    Ok(Vec::new())
}

// ── Merge YOLO + UIA ────────────────────────────────────────────────────────

/// Merge YOLO detections with UIA elements.
/// - If a UIA element overlaps (IoU > threshold) with a YOLO detection, enrich the YOLO
///   detection with the UIA name/content.
/// - If a UIA element has no YOLO overlap, add it as a new element.
pub fn merge_detections(
    yolo: &mut Vec<UIElement>,
    uia: Vec<UIElement>,
    iou_threshold: f32,
) {
    for uia_elem in uia {
        let best_match = yolo
            .iter_mut()
            .filter(|y| bbox_iou(&y.bbox, &uia_elem.bbox) > iou_threshold)
            .max_by(|a, b| {
                bbox_iou(&a.bbox, &uia_elem.bbox)
                    .partial_cmp(&bbox_iou(&b.bbox, &uia_elem.bbox))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        if let Some(matched) = best_match {
            // Enrich: add UIA name if YOLO detection has none
            if matched.content.is_none() {
                matched.content = uia_elem.content.clone();
            }
        } else {
            // No YOLO match — add the UIA element
            yolo.push(uia_elem);
        }
    }
}

fn bbox_iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let ix1 = a[0].max(b[0]);
    let iy1 = a[1].max(b[1]);
    let ix2 = a[2].min(b[2]);
    let iy2 = a[3].min(b[3]);
    let inter = (ix2 - ix1).max(0.0) * (iy2 - iy1).max(0.0);
    let area_a = (a[2] - a[0]).max(0.0) * (a[3] - a[1]).max(0.0);
    let area_b = (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}
