#!/usr/bin/env python3
"""
Download Salesforce/GPA-GUI-Detector from HuggingFace and export to ONNX.

This model is specifically trained for desktop GUI element detection
(buttons, icons, interactive elements) from the OmniParser ecosystem.

Usage:
    uv run python scripts/download_gui_detector.py

Produces: models/gpa_gui_detector.onnx
"""

import os
import sys

REPO_ID = "Salesforce/GPA-GUI-Detector"
MODEL_FILENAME = "model.pt"
DEST_DIR = os.path.join(os.path.dirname(__file__), "..", "models")
ONNX_NAME = "gpa_gui_detector.onnx"


def download_and_export():
    try:
        from huggingface_hub import hf_hub_download
    except ImportError:
        print("❌ huggingface_hub not installed. Run: uv add huggingface_hub")
        sys.exit(1)

    from ultralytics import YOLO

    print(f"📥 Downloading {REPO_ID}/{MODEL_FILENAME} ...")
    model_path = hf_hub_download(repo_id=REPO_ID, filename=MODEL_FILENAME)
    print(f"   Downloaded to: {model_path}")

    # Load the model and inspect
    model = YOLO(model_path)
    print(f"\n📊 Model info:")
    print(f"   Task: {model.task}")
    if hasattr(model.model, 'names'):
        names = model.model.names
        print(f"   Classes ({len(names)}): {names}")
    else:
        print("   Classes: unknown (will check after export)")

    # Export to ONNX
    print(f"\n🔄 Exporting to ONNX ...")
    onnx_path = model.export(
        format="onnx",
        imgsz=640,
        simplify=True,
        opset=17,
        dynamic=False,
    )
    print(f"   Exported: {onnx_path}")

    # Move to models/ directory
    os.makedirs(DEST_DIR, exist_ok=True)
    dest = os.path.join(DEST_DIR, ONNX_NAME)
    if os.path.abspath(onnx_path) != os.path.abspath(dest):
        import shutil
        shutil.move(onnx_path, dest)

    print(f"\n✅ GUI Detector model saved to: {dest}")
    print(f"   File size: {os.path.getsize(dest) / 1024 / 1024:.1f} MB")

    # Print class names for config
    if hasattr(model.model, 'names'):
        names = model.model.names
        print(f"\n📋 Class names for config.toml:")
        print(f'   class_names = {list(names.values())}')
    
    print(f"\n💡 Update config.toml:")
    print(f'   yolo_model_path = "models/{ONNX_NAME}"')


if __name__ == "__main__":
    download_and_export()
