# API Key 与模型

Zagens 通过 **API Key** 与 `~/.zagens/config.toml`（与 runtime 侧车共享）访问大模型。

## 首次配置

1. 向提供商申请 Key（如 [DeepSeek 平台](https://platform.deepseek.com/)）。
2. 打开 **设置 → API Key**。
3. 粘贴 Key — 桌面端优先存入 **系统钥匙串**。

未配置时侧栏会提示 **API Key 未配置**。

## Vision bridge（可选）

**设置 → API Key → Vision bridge** 配置视觉模型端点（扫描件、`describe_image` 等）：

- **API Key** 存入 OS 钥匙串（`vision` 条目）
- **Base URL / Model** 写入 `~/.zagens/config.toml` 的 `[vision]`

未配置时办公场景仍可用 `read_office`；扫描件 OCR 路径依赖视觉桥接。

## 多提供商

可配置多个 OpenAI 兼容端点并运行时切换：DeepSeek、NVIDIA NIM、Fireworks、OpenRouter、本地 vLLM/Ollama 等。

示例见仓库 `config.example.toml` 的 `[providers]`。

## 模型选择

在输入区或设置栏选择模型。具体 model ID 随提供商目录变化。

## 安全

- 勿将 Key 提交 git 或写入对外分享的导出文件。
- 本地侧车 runtime token 与提供商 Key 分离。

相关：[视觉工具](/zh-Hans/docs/tools/vision) · [工具审批](/zh-Hans/docs/settings/approval) · [模型路由](/zh-Hans/docs/settings/routing)
