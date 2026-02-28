#!/usr/bin/env python3
"""
Export a YOLOv8 Nano model to ONNX format for SeeClaw.

Usage:
    pip install ultralytics
    python scripts/export_yolo_onnx.py

This produces models/yolov8n.onnx (COCO pre-trained, ~12 MB).
For UI-specific detection, fine-tune the model first (see below).
"""

import os
import sys

def export_coco_pretrained():
    """Export the standard YOLOv8n model pre-trained on COCO."""
    from ultralytics import YOLO

    model = YOLO("yolov8n.pt")  # downloads automatically
    out = model.export(
        format="onnx",
        imgsz=640,
        simplify=True,
        opset=17,
        dynamic=False,
    )
    dest = os.path.join(os.path.dirname(__file__), "..", "models", "yolov8n.onnx")
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    if os.path.abspath(out) != os.path.abspath(dest):
        import shutil
        shutil.move(out, dest)
    print(f"\n✅ Exported to {dest}")
    print("Note: This detects COCO objects (80 classes), not UI elements.")
    print("For UI element detection, fine-tune the model — see instructions below.\n")


def export_ui_model():
    """
    Fine-tune YOLOv8n on a UI element dataset, then export.

    Example dataset format (YOLO format):
        dataset/
            images/
                train/
                    screenshot_001.png
                    ...
                val/
                    screenshot_100.png
                    ...
            labels/
                train/
                    screenshot_001.txt   # class cx cy w h (normalized)
                    ...
                val/
                    screenshot_100.txt
                    ...
            data.yaml
                path: ./dataset
                train: images/train
                val: images/val
                nc: 15
                names: [button, input, link, icon, checkbox, radio, menu,
                        menuitem, scrollbar, tab, toolbar, window, text, image, container]
    """
    from ultralytics import YOLO

    dataset_yaml = os.path.join(os.path.dirname(__file__), "..", "dataset", "data.yaml")
    if not os.path.exists(dataset_yaml):
        print("❌ dataset/data.yaml not found.")
        print("Please prepare a UI element dataset in YOLO format.")
        print("See the docstring in this function for the expected structure.")
        sys.exit(1)

    model = YOLO("yolov8n.pt")
    model.train(
        data=dataset_yaml,
        epochs=100,
        imgsz=640,
        batch=16,
        name="yolov8n_ui",
        project="runs/train",
    )

    best = os.path.join("runs", "train", "yolov8n_ui", "weights", "best.pt")
    trained = YOLO(best)
    out = trained.export(format="onnx", imgsz=640, simplify=True, opset=17, dynamic=False)
    dest = os.path.join(os.path.dirname(__file__), "..", "models", "yolov8n_ui.onnx")
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    import shutil
    shutil.move(out, dest)
    print(f"\n✅ UI model exported to {dest}")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--train-ui":
        export_ui_model()
    else:
        export_coco_pretrained()
        print("Tip: Run with --train-ui to fine-tune on a UI element dataset.")
