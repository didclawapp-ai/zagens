# 视觉（`describe_image`）

**`describe_image`** 让 Agent 分析你附加或指定路径的图片。

## 条件

- **代码**或**办公**模式（且启用 vision 特性）
- 配置 `VISION_API_KEY` 或支持图像的模型（见 `config.toml`）
- 图片需符合运行时大小限制

## 典型用途

- UI 截图排障 → 描述布局并建议修改
- 白板/架构图照片 → 提取结构
- 读取 `inbox/` 内图表写入办公摘要

## 如何触发

- 拖拽/粘贴图片到对话，或
- 将图片放在工作区并请 Agent 读取

视觉后端返回文字描述后，Agent 可继续调用其他工具。

## 隐私

图片会发往所配置的视觉服务商 — 敏感截图请谨慎。勿上传凭证或证件照，除非你接受相应风险。

相关：[文件工具](/zh-Hans/docs/tools/files) · [API Key](/zh-Hans/docs/settings/api-key)
