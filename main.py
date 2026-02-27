import time
import base64
import io
from PIL import ImageGrab
import pyautogui
from zhipuai import ZhipuAI
import json
import re
import cv2
import numpy as np

# ==========================================
# 配置区域
# ==========================================
# 1. 填入你的智谱 AI API Key
API_KEY = "4f0f9e54e52e43faadbf24c9a7754b00.Xon9PmRyJShYVVJ7" 

# 2. 使用的模型 
MODEL = "GLM-4.6V"

client = ZhipuAI(api_key=API_KEY)

# ==========================================
# 核心功能模块
# ==========================================

def capture_screen_base64():
    print("[System] 正在截屏...")
    # all_screens=True 确保多显示器也能截全
    screenshot = ImageGrab.grab(all_screens=True) 
    
    # === 新增调试代码: 保存这一帧看看 ===
    screenshot.save("debug_last_screen.png")
    print("[Debug] 已保存当前截图为 debug_last_screen.png，请检查图片是否正常！")
    # =================================
    
    width, height = screenshot.size
    buffered = io.BytesIO()
    screenshot.save(buffered, format="PNG") 
    img_str = base64.b64encode(buffered.getvalue()).decode('utf-8')
    return img_str, width, height
def ask_glm4v_for_coordinates(image_base64, goal_description):
    print(f"[AI] 正在思考如何执行指令: '{goal_description}'...")
    
    # === 修改点 1: 提示词优化，要求输出 0-1000 的归一化坐标 ===
    system_prompt = (
        "你是一个 Windows 桌面自动化助手。请分析提供的屏幕截图。"
        "你的目标是：找到用户指令中提到的元素，并返回其【边界框 (Bounding Box)】。"
        "IMPORTANT: 不要返回绝对像素值！必须返回归一化坐标，范围是 0 到 1000。"
        "归一化格式：[ymin, xmin, ymax, xmax] (左上角y, 左上角x, 右下角y, 右下角x)。"
        "输出格式必须严格遵循：'```json {\"box_2d\": [ymin, xmin, ymax, xmax]} ```'。"
        "如果找不到，输出 '```json {\"box_2d\": null} ```'。"
    )
    # =======================================================

    try:
        response = client.chat.completions.create(
            model=MODEL,
            messages=[
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "text",
                            "text": f"{system_prompt}\n用户指令：{goal_description}"
                        },
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": image_base64
                            }
                        }
                    ]
                }
            ],
            temperature=0.1, 
        )
        
        raw_content = response.choices[0].message.content
        print(f"\n[Debug] 大模型原始回复内容:\n{'-'*20}\n{raw_content}\n{'-'*20}\n")
        return raw_content
    except Exception as e:
        print(f"[Error] API 调用失败: {e}")
        return None

def parse_coordinates(vlm_response):
    """
    修改点 2: 解析 0-1000 的 box 坐标
    """
    if not vlm_response:
        return None
    
    match = re.search(r'```json\s*(\{.*?\})\s*```', vlm_response, re.DOTALL)
    if match:
        json_str = match.group(1)
        try:
            data = json.loads(json_str)
            # 获取 box [ymin, xmin, ymax, xmax]
            return data.get("box_2d") 
        except json.JSONDecodeError:
            print(f"[Error] JSON 解析失败: {json_str}")
            return None
    return None

def run_once(task_prompt):
    start_time = time.time()
    
    # 1. 看
    img_b64, screen_w, screen_h = capture_screen_base64()
    
    # 2. 想
    raw_response = ask_glm4v_for_coordinates(img_b64, task_prompt)
    
    # 3. 解析
    norm_box = parse_coordinates(raw_response)
    
    # 4. 做 (计算坐标转换)
    if norm_box:
        ymin, xmin, ymax, xmax = norm_box
        print(f"[Found] AI 返回归一化 Box (0-1000): {norm_box}")
        
        # === 修改点 3: 将 0-1000 映射回 真实屏幕像素 ===
        # 计算中心点 (归一化)
        center_x_norm = (xmin + xmax) / 2
        center_y_norm = (ymin + ymax) / 2
        
        # 映射回物理像素 (OpenCV 画图用)
        # 比如 x=500 (中间) -> 500/1000 * 2560 = 1280
        abs_x = int(center_x_norm / 1000 * screen_w)
        abs_y = int(center_y_norm / 1000 * screen_h)
        
        # 计算 Box 的物理角点 (画框用)
        box_x1 = int(xmin / 1000 * screen_w)
        box_y1 = int(ymin / 1000 * screen_h)
        box_x2 = int(xmax / 1000 * screen_w)
        box_y2 = int(ymax / 1000 * screen_h)
        
        # === 调试可视化 ===
        debug_img = cv2.imread("debug_last_screen.png")
        if debug_img is not None:
            # 画框 (绿色)
            cv2.rectangle(debug_img, (box_x1, box_y1), (box_x2, box_y2), (0, 255, 0), 2)
            # 画中心点 (红色)
            cv2.circle(debug_img, (abs_x, abs_y), 10, (0, 0, 255), -1)
            cv2.imwrite("debug_target_check.png", debug_img)
            print("[Debug] 已生成 debug_target_check.png，请查看现在的框准不准")

        # === 鼠标点击坐标修正 (处理 Windows 缩放) ===
        log_w, log_h = pyautogui.size()
        scale_x = log_w / screen_w
        scale_y = log_h / screen_h
        
        final_x = int(abs_x * scale_x)
        final_y = int(abs_y * scale_y)
        
        print(f"[Action] 点击坐标: ({final_x}, {final_y}) (缩放: {scale_x:.2f})")
        
        if 0 <= final_x <= log_w and 0 <= final_y <= log_h:
            pyautogui.moveTo(final_x, final_y, duration=0.5)
            pyautogui.click()
        else:
            print("[Error] 坐标超出屏幕范围")
            
    else:
        print("[Failed] AI 没找到目标。")

    end_time = time.time()
    print(f"[System] 耗时: {end_time - start_time:.2f} 秒")

if __name__ == "__main__":
    # ==============================
    # 测试场景
    # ==============================
    print("="*30)
    print("AutoGLM Windows 原型启动")
    print("="*30)
    
    # 请确保你要测试的目标在屏幕上可见
    
    # 测试 1: 点击桌面上的图标（例如“此电脑”或“Recycle Bin”）
    task = "点击桌面上的英雄联盟图标"
    
    # 测试 2: 点击浏览器里的按钮（先手动打开浏览器）
    # task = "点击浏览器地址栏"
    
    # 测试 3: 复杂语义（需要手动打开一个复杂的 UI）

    run_once(task)