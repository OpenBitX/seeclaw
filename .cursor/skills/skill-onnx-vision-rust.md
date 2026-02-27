# 技能：Rust 集成 ONNX Runtime (方案B)

要在 Rust 中使用本地轻量级视觉模型，必须使用 `ort` crate。
1. 在 `Cargo.toml` 引入 `ort`。
2. 模型加载必须在 Tauri 初始化阶段单例加载（放进 Tauri 的 `managed` state），避免每次截图重新加载模型。
3. 图像预处理需使用 `image` crate，将其转换为 `ndarray` 格式传入 `ort` session。
4. 解析 YOLO 输出张量，提取 Bounding Box 并转化为 `(xmin, ymin, xmax, ymax)`。

