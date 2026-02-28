# Models Directory

Place your ONNX model files here.

## YOLOv8 Nano for UI Element Detection

### Quick Start (COCO pre-trained)

```bash
pip install ultralytics
python scripts/export_yolo_onnx.py
```

This exports `yolov8n.onnx` to this directory (~12 MB).

### UI-Specific Model (recommended for best results)

For detecting UI elements (buttons, icons, input fields, scrollbars, etc.) you need a model
fine-tuned on UI screenshots. See `scripts/export_yolo_onnx.py` for training instructions.

### Expected Class Names

If using the default UI class list, the model should detect:

| Class ID | Name       | Description               |
|----------|------------|---------------------------|
| 0        | button     | Clickable buttons         |
| 1        | input      | Text fields / edit boxes  |
| 2        | link       | Hyperlinks                |
| 3        | icon       | Icons / small images      |
| 4        | checkbox   | Checkboxes                |
| 5        | radio      | Radio buttons             |
| 6        | menu       | Menus                     |
| 7        | menuitem   | Menu items                |
| 8        | scrollbar  | Scrollbars                |
| 9        | tab        | Tabs                      |
| 10       | toolbar    | Toolbars / taskbar        |
| 11       | window     | Windows / dialogs         |
| 12       | text       | Text labels               |
| 13       | image      | Images                    |
| 14       | container  | Panels / groups           |

You can customise the class names in `config.toml` under `[perception]`.
