import time
import base64
import io
from PIL import ImageGrab, ImageDraw, ImageFont
import pyautogui
from zhipuai import ZhipuAI
import json
import re
import cv2
import numpy as np

# ==========================================
# 配置区域
# ==========================================
API_KEY = "4f0f9e54e52e43faadbf24c9a7754b00.Xon9PmRyJShYVVJ7"
MODEL = "GLM-4.6V"

# 格子边长（像素）：越小定位越精确，但标签越密集
# 建议：40px（密集UI）~ 64px（稀疏桌面）
GRID_CELL_PX = 40

# 标注字体大小（pt）
FONT_SIZE_PT = 9

client = ZhipuAI(api_key=API_KEY)

# ==========================================
# 核心功能模块
# ==========================================

def capture_screen():
    """截取全屏（支持多显示器），返回 PIL Image。"""
    print("[Screen] 截屏中...")
    img = ImageGrab.grab(all_screens=True)
    img.save("debug_raw_screen.png")
    print(f"[Screen] 原始分辨率: {img.size[0]}x{img.size[1]}px  →  debug_raw_screen.png")
    return img


def draw_hex_grid(img: "PIL.Image.Image") -> "PIL.Image.Image":
    """
    在截图上叠加精细坐标网格。

    每个格子的左下角印刷该角点的物理像素坐标，格式为 4位十六进制：
        XXXX,YYYY（例如 0080,00C0 代表物理像素 (128, 192)）

    模型只需"读数"，无需估算——精度由格子尺寸直接决定。
    """
    # 使用 RGBA 模式支持半透明叠加
    canvas = img.convert("RGBA")
    overlay = ImageDraw.Draw(canvas, "RGBA")

    W, H = canvas.size
    C = GRID_CELL_PX

    cols = (W + C - 1) // C   # 列数
    rows = (H + C - 1) // C   # 行数

    # 尝试加载等宽字体（Consolas），保证 hex 字符对齐清晰
    font = None
    font_paths = [
        "C:/Windows/Fonts/consola.ttf",   # Windows Consolas
        "C:/Windows/Fonts/cour.ttf",       # Courier New fallback
    ]
    for fp in font_paths:
        try:
            font = ImageFont.truetype(fp, FONT_SIZE_PT)
            break
        except (IOError, OSError):
            continue
    if font is None:
        font = ImageFont.load_default()

    # 颜色（RGBA）
    LINE_COLOR  = (0, 210, 255, 45)    # 半透明青色网格线
    TEXT_BG     = (0, 0, 0, 150)       # 标签底色（深色半透明）
    TEXT_COLOR  = (0, 255, 140, 230)   # 亮绿色标签文字

    # ── 绘制网格线 ──────────────────────────────────────────────
    for c in range(cols + 1):
        x = min(c * C, W - 1)
        overlay.line([(x, 0), (x, H)], fill=LINE_COLOR, width=1)
    for r in range(rows + 1):
        y = min(r * C, H - 1)
        overlay.line([(0, y), (W, y)], fill=LINE_COLOR, width=1)

    # ── 在每个格子左下角绘制 hex 坐标标签 ──────────────────────
    # 格子 (col, row) 的左下角物理像素 = (col*C, (row+1)*C)
    # 但我们标注"左下角坐标"，即该格的 top-left 像素 (col*C, row*C)，
    # 让模型读取后直接加 C//2 得到格子中心，避免混淆。
    # 文字渲染位置：格子内部靠近底部，距左边 2px、距底边 11px

    LABEL_H = 11   # 单行标签高度（px）
    LABEL_W = 58   # 估计的最大标签宽度（px），"XXXX,XXXX" = 9 chars × ~6px

    for r in range(rows):
        for c in range(cols):
            # 格子 top-left 坐标（即我们要标注的"本格坐标"）
            px = c * C
            py = r * C

            label = f"{px:04X},{py:04X}"

            # 标签绘制位置：格子内左下角区域
            tx = px + 2
            ty = py + C - LABEL_H - 2
            if ty < py:     # 格子太小，退回到 top
                ty = py + 1

            # 背景矩形（提升在复杂背景下的可读性）
            overlay.rectangle(
                [tx, ty, tx + LABEL_W, ty + LABEL_H - 1],
                fill=TEXT_BG
            )
            overlay.text((tx + 1, ty), label, font=font, fill=TEXT_COLOR)

    result = canvas.convert("RGB")
    result.save("debug_grid_screen.png")
    print(f"[Grid] {cols}×{rows} 格（每格 {C}px），已保存 debug_grid_screen.png")
    return result


def img_to_base64(img: "PIL.Image.Image") -> str:
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    return base64.b64encode(buf.getvalue()).decode("utf-8")


def ask_glm4v_with_grid(image_base64: str, goal: str) -> str | None:
    """
    将带有 hex 坐标网格的截图发给 GLM-4.6V，
    让模型直接读取图上标签，返回目标元素所在格子的坐标标签。
    """
    print(f"[AI] 读取网格坐标，目标: '{goal}'")

    # ── 关键提示词设计 ──────────────────────────────────────────
    # 核心思路：模型不需要"估算"，只需"识字"——读取印在图上的标签。
    # 指令分层：先找元素 → 再找最近的标签 → 报告标签原文
    system_prompt = (
        "你是一名 Windows 桌面自动化精确点击助手。\n\n"
        "【屏幕说明】\n"
        "屏幕截图上叠加了一套精细的坐标参考网格。\n"
        "每个网格格子的左下角都印有一个 16 进制坐标标签，格式为：\n"
        "  XXXX,YYYY（示例：0280,01E0，代表物理像素位置 x=0x0280=640, y=0x01E0=480）\n"
        "这些标签就是实际的屏幕像素坐标，你只需要读取，不需要任何计算。\n\n"
        "【操作步骤】\n"
        "1. 在截图中找到与用户指令匹配的 UI 元素（图标、按钮、文字等）。\n"
        "2. 目视该元素附近，找到距离其中心最近的网格标签，原文读取该标签。\n"
        "   - 如果元素较大，选择最靠近元素中心的标签。\n"
        "   - 直接读图上印刷的字符，禁止自行估算或四舍五入。\n"
        "3. 以下列 JSON 格式回复（不要输出 markdown 代码块，只输出 JSON 本身）：\n"
        '   {"hex_coord": "XXXX,YYYY", "reasoning": "简要说明找到了什么元素以及标签读取位置"}\n\n'
        "【未找到时】\n"
        '   {"hex_coord": null, "reasoning": "未找到目标元素的原因"}'
    )

    try:
        response = client.chat.completions.create(
            model=MODEL,
            messages=[
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "image_url",
                            "image_url": {"url": image_base64},
                        },
                        {
                            "type": "text",
                            "text": f"{system_prompt}\n\n用户指令：{goal}",
                        },
                    ],
                }
            ],
            temperature=0.05,   # 接近 0，让模型"读数"而非"创造"
        )

        raw = response.choices[0].message.content
        print(f"\n[AI Raw]\n{'─'*40}\n{raw}\n{'─'*40}\n")
        return raw

    except Exception as e:
        print(f"[Error] API 调用失败: {e}")
        return None


def parse_hex_coord(vlm_response: str) -> tuple[int, int] | None:
    """
    从模型回复中提取 XXXX,YYYY 格式的 hex 坐标。
    返回 (x_pixel, y_pixel) 或 None。
    两阶段解析：先 JSON，再 regex fallback。
    """
    if not vlm_response:
        return None

    # ── 阶段一：JSON 解析 ────────────────────────────────────────
    json_match = re.search(r'\{[^{}]*"hex_coord"[^{}]*\}', vlm_response, re.DOTALL)
    if json_match:
        try:
            data = json.loads(json_match.group())
            coord_str = data.get("hex_coord")
            if coord_str and coord_str != "null":
                x, y = _parse_hex_pair(coord_str)
                if x is not None:
                    reasoning = data.get("reasoning", "")
                    print(f"[Parse] JSON 解析成功  坐标=({x}, {y})  理由: {reasoning}")
                    return x, y
        except (json.JSONDecodeError, ValueError) as e:
            print(f"[Warn] JSON 解析失败: {e}")

    # ── 阶段二：Regex fallback ────────────────────────────────────
    # 匹配 4位hex逗号4位hex，如 "0280,01E0" 或 "0280, 01E0"
    hex_match = re.search(
        r'\b([0-9A-Fa-f]{4})\s*,\s*([0-9A-Fa-f]{4})\b',
        vlm_response
    )
    if hex_match:
        x = int(hex_match.group(1), 16)
        y = int(hex_match.group(2), 16)
        print(f"[Parse] Regex fallback 解析  坐标=({x}, {y})")
        return x, y

    print("[Parse] 未能提取有效坐标")
    return None


def _parse_hex_pair(s: str) -> tuple[int | None, int | None]:
    """解析 'XXXX,YYYY' → (int, int)"""
    try:
        parts = s.strip().split(",")
        if len(parts) == 2:
            return int(parts[0].strip(), 16), int(parts[1].strip(), 16)
    except ValueError:
        pass
    return None, None


def click_at_grid_cell(cell_origin_x: int, cell_origin_y: int, screen_w: int, screen_h: int):
    """
    给定格子左上角的物理像素坐标，计算格子中心，
    应用 Windows DPI 缩放后执行点击。
    """
    C = GRID_CELL_PX

    # 格子中心（物理像素）
    center_x = cell_origin_x + C // 2
    center_y = cell_origin_y + C // 2

    # 边界夹紧（防止超出截图范围）
    center_x = max(0, min(center_x, screen_w - 1))
    center_y = max(0, min(center_y, screen_h - 1))

    # DPI 缩放：pyautogui.size() 返回"逻辑像素"分辨率
    # ImageGrab 返回"物理像素"分辨率，两者之比即缩放因子
    log_w, log_h = pyautogui.size()
    scale_x = log_w / screen_w
    scale_y = log_h / screen_h

    final_x = int(center_x * scale_x)
    final_y = int(center_y * scale_y)

    print(
        f"[Click] 格子中心 物理像素=({center_x},{center_y})  "
        f"缩放={scale_x:.3f}×{scale_y:.3f}  "
        f"逻辑坐标=({final_x},{final_y})"
    )

    if not (0 <= final_x <= log_w and 0 <= final_y <= log_h):
        print(f"[Error] 坐标 ({final_x},{final_y}) 超出屏幕范围 {log_w}×{log_h}")
        return False

    pyautogui.moveTo(final_x, final_y, duration=0.4)
    pyautogui.click()
    print("[Done] 点击完成！")
    return True


def visualize_target(grid_img_path: str, x: int, y: int):
    """在网格截图上标注最终点击位置，保存为 debug_target_check.png。"""
    C = GRID_CELL_PX
    cx = x + C // 2
    cy = y + C // 2

    img = cv2.imread(grid_img_path)
    if img is None:
        return

    # 外圈（黄色）+ 内圈（红色）十字准星
    cv2.circle(img, (cx, cy), 16, (0, 220, 255), 2)   # 青色外圈
    cv2.circle(img, (cx, cy), 4,  (0, 0, 255), -1)    # 红色圆心
    cv2.line(img, (cx - 20, cy), (cx + 20, cy), (0, 255, 0), 1)
    cv2.line(img, (cx, cy - 20), (cx, cy + 20), (0, 255, 0), 1)

    # 标注坐标文字
    label = f"({x:04X},{y:04X})  px=({cx},{cy})"
    cv2.putText(img, label, (cx + 18, cy - 8),
                cv2.FONT_HERSHEY_SIMPLEX, 0.45, (0, 255, 140), 1, cv2.LINE_AA)

    cv2.imwrite("debug_target_check.png", img)
    print("[Debug] 目标标注图 → debug_target_check.png（请核查红点是否对准目标）")


# ==========================================
# 主流程
# ==========================================

def run_once(task: str):
    """
    SoM 精确坐标点击流程：
      截屏 → 绘制 hex 网格 → AI 读标签 → 解析 → 可视化 → 点击
    """
    t0 = time.time()

    # ── 1. 截屏 ─────────────────────────────────────────────────
    screenshot = capture_screen()
    screen_w, screen_h = screenshot.size

    # ── 2. 绘制精细 hex 坐标网格 ─────────────────────────────────
    grid_img = draw_hex_grid(screenshot)

    # ── 3. 发给 AI，让它"读"标签 ─────────────────────────────────
    img_b64 = img_to_base64(grid_img)
    raw_response = ask_glm4v_with_grid(img_b64, task)

    # ── 4. 解析 hex 坐标 ─────────────────────────────────────────
    result = parse_hex_coord(raw_response)

    if result is None:
        print("\n[Failed] 未获取到有效坐标。")
        print("  请检查 debug_grid_screen.png 确认网格标签是否清晰可读。")
        print(f"  耗时: {time.time() - t0:.2f}s")
        return

    cell_x, cell_y = result   # 格子左上角物理像素坐标

    # ── 5. 可视化目标位置 ─────────────────────────────────────────
    visualize_target("debug_grid_screen.png", cell_x, cell_y)

    # ── 6. DPI 校正 + 点击 ───────────────────────────────────────
    click_at_grid_cell(cell_x, cell_y, screen_w, screen_h)

    print(f"\n[System] 总耗时: {time.time() - t0:.2f}s")


# ==========================================
# 入口
# ==========================================

if __name__ == "__main__":
    print("=" * 45)
    print("  SeeClaw SoM HexGrid 精确点击原型")
    print(f"  格子尺寸: {GRID_CELL_PX}px  |  模型: {MODEL}")
    print("=" * 45)

    # ── 修改这里来测试不同目标 ──────────────────────────────────
    task = "点击桌面上的英雄联盟图标"

    # task = "点击浏览器地址栏"
    # task = "点击任务栏上的开始菜单按钮"
    # task = "点击屏幕右下角的时间显示区域"

    run_once(task)
