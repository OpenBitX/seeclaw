# SeeClaw 部署和打包指南

## 开发环境 vs 生产环境

### 开发环境（cargo tauri dev）
- **exe 位置**：`target/debug/seeclaw.exe`
- **config.toml 位置**：项目根目录 `C:\Learning\sample\synchronous-github\OpenBitX\seeclaw\config.toml`
- **配置更新**：直接修改根目录的 `config.toml`，重启后生效
- **目录结构**：完整的开发目录结构（src/, prompts/, models/ 等）

### 生产环境（打包后）
- **exe 位置**：`target/release/bundle/msi/SeeClaw_0.1.0_x64_en-US.msi` 或 `target/release/bundle/nsis/SeeClaw_0.1.0_x64-setup.exe`
- **安装后位置**：通常在 `C:\Program Files\SeeClaw\` 或用户自选目录
- **config.toml 位置**：**exe 所在目录**（自动创建）
- **配置更新**：通过应用内设置界面修改，保存到 exe 旁边的 `config.toml`
- **目录结构**：仅包含必要的运行时文件

---

## 配置文件查找逻辑

代码位置：[src/config.rs](../src/config.rs#L194-L212)

```rust
fn find_config_path() -> SeeClawResult<PathBuf> {
    // 1. 优先查找：exe 所在目录的 config.toml（生产环境）
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("config.toml");
            if candidate.exists() {
                return Ok(candidate);
            }
        }
    }
    // 2. 备用：当前工作目录的 config.toml（开发环境）
    let cwd = std::env::current_dir()?;
    let candidate = cwd.join("config.toml");
    if candidate.exists() {
        return Ok(candidate);
    }
    Err(SeeClawError::Config("config.toml not found"))
}
```

### 配置保存逻辑
- **开发时**：保存到 `项目根目录/config.toml`
- **打包后**：保存到 `exe所在目录/config.toml`

---

## 打包步骤

### 1. 打包前准备

确保以下文件存在且配置正确：

```
seeclaw/
├── config.toml          # 默认配置（会被打包）
├── prompts/             # 提示词模板（会被打包）
│   ├── system/
│   └── tools/
└── models/              # AI 模型（会被打包）
    └── gpa_gui_detector.onnx
```

### 2. 执行打包

```powershell
# 在项目根目录执行
cargo tauri build
```

### 3. 打包产物位置

打包完成后，安装包会生成在：

```
target/release/bundle/
├── msi/
│   └── SeeClaw_0.1.0_x64_en-US.msi       # Windows Installer
└── nsis/
    └── SeeClaw_0.1.0_x64-setup.exe       # NSIS 安装程序
```

---

## 真实用户的目录结构

### 安装后的目录结构（示例：C:\Program Files\SeeClaw\）

```
SeeClaw/
├── SeeClaw.exe          # 主程序
├── config.toml          # 用户配置（首次运行时创建）
├── prompts/             # 提示词模板
│   ├── system/
│   │   ├── agent_system.md
│   │   ├── router.md
│   │   └── ...
│   └── tools/
│       └── builtin.json
├── models/              # AI 模型
│   └── gpa_gui_detector.onnx
└── resources/           # Tauri 运行时资源
```

### 用户配置说明

1. **首次启动**：
   - 应用会在 exe 所在目录创建默认的 `config.toml`
   - API keys 从环境变量读取（如果设置了 `SEECLAW_ZHIPU_API_KEY`）

2. **修改设置**：
   - 用户通过应用内设置界面修改
   - 保存后写入 `exe所在目录/config.toml`
   - 无需重启即可生效（内存中的 ProviderRegistry 会重新加载）

3. **环境变量（可选）**：
   - 用户可以设置系统环境变量 `SEECLAW_ZHIPU_API_KEY` 等
   - 当 config.toml 中 api_key 为空时，会自动读取环境变量

---

## 部署检查清单

### ✅ 必需的文件

- [x] `config.toml` - 默认配置
- [x] `prompts/system/` - 系统提示词
- [x] `prompts/tools/builtin.json` - 工具定义
- [x] `models/gpa_gui_detector.onnx` - YOLO 模型（如果启用）

### ✅ 配置验证

```powershell
# 开发环境测试
cargo tauri dev

# 验证配置文件位置
# 查看日志输出：
# INFO seeclaw_lib::config: config loaded path=...
# INFO seeclaw_lib::config: config saved path=...
```

### ✅ 打包测试

```powershell
# 1. 打包
cargo tauri build

# 2. 清理开发环境的配置影响
$env:SEECLAW_ZHIPU_API_KEY = ""

# 3. 安装并测试
# 在另一台干净的机器上测试安装包
```

---

## 常见问题

### Q: 为什么我修改设置后重启就没了？

**A:** 在开发环境下，如果你直接编辑根目录的 `config.toml`，然后通过设置界面修改并保存，两个操作可能互相覆盖。

**解决方案**：
- 开发时统一通过设置界面修改
- 或者只编辑 config.toml 文件，不要通过界面保存

### Q: 打包后用户需要手动创建 config.toml 吗？

**A:** 不需要。应用会在首次运行时自动创建默认配置。

### Q: 用户的配置会保存在哪里？

**A:** 保存在 **exe 所在目录** 的 `config.toml`。如果安装在 `C:\Program Files\SeeClaw\`，则配置在 `C:\Program Files\SeeClaw\config.toml`。

### Q: 如何查看当前使用的配置文件路径？

**A:** 启动应用后，查看日志输出（开发模式）或添加一个命令：

```rust
#[tauri::command]
pub async fn get_config_file_path() -> Result<String, String> {
    get_config_path().map_err(|e| e.to_string())
}
```

然后在设置界面显示这个路径。

---

## 下一步优化

1. **配置位置改进**：
   - 考虑使用用户目录（如 `%APPDATA%\SeeClaw\config.toml`）
   - 避免安装在 Program Files 时权限问题

2. **自动更新**：
   - 集成 Tauri Updater
   - 配置文件迁移策略

3. **便携版**：
   - 提供 zip 压缩包，解压即用
   - 不需要安装程序
