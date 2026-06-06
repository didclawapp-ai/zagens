//! `describe_image` tool — use a vision model (e.g. Qwen3-VL on SiliconFlow)
//! to extract text / describe an image file, then feed the result into the
//! DeepSeek V4 chat context.
//!
//! Architecture:
//!   image file → base64 encode → POST vision model API → text
//!      → injected into the main conversation as a user message.
//!
//! Configuration (env vars, optional — defaults to SiliconFlow Qwen3-VL; see
//! `deepseek_config::DEFAULT_VISION_MODEL`):
//!   VISION_API_KEY  — Bearer token (required)
//!   VISION_BASE_URL — OpenAI-compatible endpoint root
//!                      (default: https://api.siliconflow.cn/v1)
//!   VISION_MODEL    — model id (default: Qwen/Qwen3-VL-32B-Instruct when unset)
//!
//! The tool itself is read-only and network-only; it does not write files.

use async_trait::async_trait;
use base64::Engine as _;
use deepseek_config::{
    DEFAULT_VISION_MODEL, vision_should_check_degenerate_ocr_template, vision_user_prompt_for_model,
};
use serde_json::Value;
use std::error::Error;

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_str, required_str,
};

fn chain_vision_transport_error_cn<E: Error + Send + Sync>(prefix: &str, err: &E) -> String {
    let mut msg = format!("{prefix}: {err}");
    let mut cur = err.source();
    while let Some(next) = cur {
        msg.push_str(" → ");
        msg.push_str(&next.to_string());
        cur = next.source();
    }
    msg.push_str(
        "。若在个别电脑上出现：请在同一台机器测试能否访问硅基流动 API（浏览器或 curl）；检查防火墙、公司代理 HTTPS_PROXY、DNS 与地区网络。",
    );
    msg
}

fn reject_known_degenerate_ocr_output(text: &str) -> Result<(), String> {
    let marker = "如果图中包含表格，请用表格形式输出";
    if text.matches(marker).count() >= 2 {
        return Err(
            "视觉模型输出为无效重复模板句式。若使用 DeepSeek-OCR，请按硅基流动文档采用官方 `<image>` + `<|grounding|>` 提示词。"
                .to_string(),
        );
    }
    Ok(())
}

// ── DescribeImageTool ───────────────────────────────────────────────────

pub struct DescribeImageTool;

#[async_trait]
impl ToolSpec for DescribeImageTool {
    fn name(&self) -> &'static str {
        "describe_image"
    }

    fn description(&self) -> &'static str {
        "Read an image file and extract its text content via a vision model. Returns faithfully transcribed text (tables as Markdown). Unrecognisable characters are marked [辨认不清]. If the image contains no text, a brief visual description is returned. Use this tool when the user attaches an image and asks about its contents."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the image file. Supported: png, jpg, jpeg, gif, bmp, webp."
                },
                "prompt": {
                    "type": "string",
                    "description": "Optional custom prompt for the vision model."
                }
            },
            "required": ["path"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly, ToolCapability::Network]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let path_str = required_str(&input, "path")?;
        let image_path = context.resolve_path(path_str)?;

        if !image_path.exists() {
            return Ok(ToolResult::error(format!(
                "图片文件不存在: {}",
                image_path.display()
            )));
        }

        let ext = image_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp"
        ) {
            return Ok(ToolResult::error(format!(
                "不支持的图片格式: .{ext}。支持: png, jpg, jpeg, gif, bmp, webp"
            )));
        }

        let image_bytes = std::fs::read(&image_path)
            .map_err(|e| ToolError::execution_failed(format!("读取图片失败: {e}")))?;

        let size_mb = image_bytes.len() as f64 / (1024.0 * 1024.0);
        if size_mb > 20.0 {
            return Ok(ToolResult::error(format!(
                "图片太大 ({:.1} MB)，最大支持 20 MB",
                size_mb
            )));
        }

        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "webp" => "image/webp",
            _ => "image/png",
        };
        let b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
        let data_uri = format!("data:{mime};base64,{b64}");

        let client = VisionClient::from_env();
        let default_prompt = vision_user_prompt_for_model(&client.model);
        let prompt = optional_str(&input, "prompt")
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(default_prompt);
        let request = VisionRequest {
            model: &client.model,
            prompt,
            data_uri: &data_uri,
        };

        match client.call(&request).await {
            Ok(text) => {
                let meta = serde_json::json!({
                    "path": image_path.to_string_lossy(),
                    "model": client.model,
                    "size_bytes": image_bytes.len(),
                });
                Ok(ToolResult::success(format!("图片文字提取结果:\n\n{text}")).with_metadata(meta))
            }
            Err(msg) => Ok(ToolResult::error(format!(
                "视觉模型调用失败: {msg}\n\n请在 Zagens 设置 → API Key 中配置视觉桥接密钥，或设置环境变量 VISION_API_KEY / SILICONFLOW_API_KEY，或在 config.toml 的 [vision] 表中填写 api_key。"
            ))),
        }
    }
}

// ── VisionClient ────────────────────────────────────────────────────────

/// Read `[vision]` section from the TUI config file (if present).
fn load_vision_from_config() -> (Option<String>, Option<String>, Option<String>) {
    let path = deepseek_config::default_config_path()
        .ok()
        .filter(|p| p.exists());
    let path = match path {
        Some(p) => p,
        None => return (None, None, None),
    };
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return (None, None, None),
    };
    let doc: toml::Value = match toml::from_str(&contents) {
        Ok(d) => d,
        Err(_) => return (None, None, None),
    };
    let vision = match doc.get("vision") {
        Some(toml::Value::Table(t)) => t,
        _ => return (None, None, None),
    };
    let api_key = vision
        .get("api_key")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let base_url = vision
        .get("base_url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let model = vision
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    (api_key, base_url, model)
}

struct VisionClient {
    model: String,
    base_url: String,
    api_key: String,
}

#[derive(Debug)]
struct VisionRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    data_uri: &'a str,
}

impl VisionClient {
    fn from_env() -> Self {
        // 1. Try config file ([vision] section in config.toml)
        let (cfg_api_key, cfg_base_url, cfg_model) = load_vision_from_config();

        // 2. Env vars override config (both VISION_* and SILICONFLOW_*)
        let api_key = std::env::var("VISION_API_KEY")
            .or_else(|_| std::env::var("SILICONFLOW_API_KEY"))
            .ok()
            .or(cfg_api_key)
            .filter(|k| !k.is_empty())
            .unwrap_or_default();
        let base_url = std::env::var("VISION_BASE_URL")
            .ok()
            .or(cfg_base_url)
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "https://api.siliconflow.cn/v1".to_string());
        let model = std::env::var("VISION_MODEL")
            .ok()
            .or(cfg_model)
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| DEFAULT_VISION_MODEL.to_string());
        Self {
            model,
            base_url,
            api_key,
        }
    }

    async fn call(&self, request: &VisionRequest<'_>) -> Result<String, String> {
        let timeout_secs = std::env::var("VISION_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|&t| t >= 30)
            .unwrap_or(120)
            .min(600);
        // C4: async reqwest so the vision HTTP round-trip doesn't block a tokio
        // worker for up to `timeout_secs` (was `reqwest::blocking`).
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(30))
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;

        let body = serde_json::json!({
            "model": request.model,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "image_url", "image_url": {"url": request.data_uri, "detail": "high"}},
                    {"type": "text", "text": request.prompt}
                ]
            }],
            "max_tokens": 4096,
            "temperature": 0.0,
            "stream": false,
        });

        let resp = client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| chain_vision_transport_error_cn("视觉模型 HTTP 请求失败", &e))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("解析 API 响应失败: {e}"))?;

        if !status.is_success() {
            let msg = resp_body
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            return Err(format!("API 返回错误 (HTTP {status}): {msg}"));
        }

        let raw = resp_body["choices"][0]["message"]["content"].clone();
        let text = match raw {
            serde_json::Value::String(s) => s,
            serde_json::Value::Array(parts) => {
                let mut chunks = Vec::new();
                for item in parts {
                    if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                        if !t.trim().is_empty() {
                            chunks.push(t.to_string());
                        }
                    }
                }
                if chunks.is_empty() {
                    return Err("API message.content 为数组但无任何文本段落".to_string());
                }
                chunks.join("\n")
            }
            _ => {
                return Err("API message.content 格式无法解析".to_string());
            }
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err("API 返回空文本内容".to_string());
        }
        if vision_should_check_degenerate_ocr_template(self.model.as_str()) {
            reject_known_degenerate_ocr_output(&text)?;
        }
        Ok(text)
    }
}
