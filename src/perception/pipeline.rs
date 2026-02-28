/// Perception pipeline — integrates YOLO detection, UI Automation, annotation,
/// and SoM grid fallback into a single flow.
use base64::Engine as _;
use crate::errors::SeeClawResult;
use crate::perception::annotator;
use crate::perception::screenshot::{capture_primary, ScreenshotResult};
use crate::perception::types::{PerceptionContext, PerceptionSource};
use crate::perception::ui_automation;
use crate::perception::yolo_detector::YoloDetector;
use crate::perception::som_grid::draw_som_grid;

/// Run the full perception pipeline:
///
/// 1. Capture screenshot.
/// 2. If a YOLO detector is available, run inference → element detections.
/// 3. If `enable_uia` is true, collect Windows Accessibility elements and merge.
/// 4. Annotate the screenshot with bounding boxes and labels.
/// 5. If YOLO produced zero detections, fall back to SoM Grid overlay.
///
/// Returns a `PerceptionContext` containing the annotated image (base64),
/// the list of detected elements, and metadata.
pub async fn run(
    yolo: Option<&mut YoloDetector>,
    enable_uia: bool,
    grid_n: u32,
) -> SeeClawResult<(PerceptionContext, ScreenshotResult)> {
    // Step 1: capture
    let shot = capture_primary().await?;

    // Step 2: YOLO detection (on a blocking thread — inference is CPU-intensive)
    let mut elements = if let Some(detector) = yolo {
        let bytes = shot.image_bytes.clone();
        let det = detector as *mut YoloDetector;
        // SAFETY: detector lives at least as long as `run` and YoloDetector is Send+Sync.
        let det_ref = unsafe { &mut *det };
        tokio::task::spawn_blocking(move || det_ref.detect(&bytes))
            .await
            .map_err(|e| crate::errors::SeeClawError::Perception(format!("join: {e}")))??
    } else {
        Vec::new()
    };

    tracing::debug!(yolo_count = elements.len(), "YOLO detections");

    // Step 3: UIA merge
    if enable_uia {
        match ui_automation::collect_ui_elements(&shot.meta).await {
            Ok(uia_elements) => {
                tracing::debug!(
                    uia_count = uia_elements.len(),
                    "UIA elements (after smart filter + NMS)"
                );
                ui_automation::merge_detections(&mut elements, uia_elements, 0.3);
            }
            Err(e) => {
                tracing::warn!(error = %e, "UIA collection failed — continuing without");
            }
        }
    }

    tracing::debug!(total = elements.len(), "Total elements after merge");

    // Step 3.5: Compute containment hierarchy and assign short numeric IDs
    compute_hierarchy(&mut elements);

    // Step 4: Choose annotation strategy
    if !elements.is_empty() {
        // Annotate with bounding boxes
        let annotated_bytes = annotator::annotate_image(&shot.image_bytes, &elements)?;
        let annotated_b64 = base64::engine::general_purpose::STANDARD.encode(&annotated_bytes);

        let ctx = PerceptionContext {
            image_base64: Some(annotated_b64),
            elements,
            resolution: (shot.meta.physical_width, shot.meta.physical_height),
            meta: shot.meta.clone(),
            source: PerceptionSource::YoloAnnotated,
        };
        Ok((ctx, shot))
    } else {
        // Fallback: SoM grid
        tracing::info!("No YOLO/UIA detections — falling back to SoM grid");
        let grid_bytes = draw_som_grid(&shot.image_bytes, grid_n)
            .unwrap_or_else(|_| shot.image_bytes.clone());
        let grid_b64 = base64::engine::general_purpose::STANDARD.encode(&grid_bytes);

        let ctx = PerceptionContext {
            image_base64: Some(grid_b64),
            elements: Vec::new(),
            resolution: (shot.meta.physical_width, shot.meta.physical_height),
            meta: shot.meta.clone(),
            source: PerceptionSource::SomGrid,
        };
        Ok((ctx, shot))
    }
}

/// Compute containment hierarchy among detected elements.
///
/// For each element, find its *smallest* enclosing parent box.
/// Then reassign short numeric IDs ("1", "2", …) so labels on the
/// annotated image are compact.  The VLM can use the `parent_id` field
/// to resolve containment chains like `3>7>12`.
fn compute_hierarchy(elements: &mut Vec<crate::perception::types::UIElement>) {
    let n = elements.len();
    if n == 0 {
        return;
    }

    // Precompute areas
    let areas: Vec<f32> = elements
        .iter()
        .map(|e| {
            let [x1, y1, x2, y2] = e.bbox;
            (x2 - x1).max(0.0) * (y2 - y1).max(0.0)
        })
        .collect();

    // For each element, find the smallest box that fully contains it
    let mut parent_indices: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        let [ix1, iy1, ix2, iy2] = elements[i].bbox;
        let mut best_parent: Option<usize> = None;
        let mut best_area = f32::MAX;

        for j in 0..n {
            if i == j {
                continue;
            }
            let [jx1, jy1, jx2, jy2] = elements[j].bbox;
            // j contains i if j's box fully encloses i's box (with small tolerance)
            let tol = 0.005; // ~0.5% tolerance for imprecise boxes
            if jx1 <= ix1 + tol && jy1 <= iy1 + tol && jx2 >= ix2 - tol && jy2 >= iy2 - tol {
                if areas[j] < best_area && areas[j] > areas[i] {
                    best_area = areas[j];
                    best_parent = Some(j);
                }
            }
        }
        parent_indices[i] = best_parent;
    }

    // Reassign short numeric IDs
    for (idx, elem) in elements.iter_mut().enumerate() {
        elem.id = format!("{}", idx + 1);
    }

    // Set parent_id using the new short IDs
    for i in 0..n {
        elements[i].parent_id = parent_indices[i].map(|pi| elements[pi].id.clone());
    }

    tracing::debug!(count = n, "Hierarchy computed with short IDs");
}
