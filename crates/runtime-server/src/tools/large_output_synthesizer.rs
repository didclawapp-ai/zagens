//! Host callback for live large-output synthesis (keeps ToolRegistry LLM-free).

use async_trait::async_trait;
use std::sync::Arc;

use zagens_core::chat::{
    ContentBlock, LlmClient, Message, MessageRequest, SystemPrompt, max_output_token_cap_for_model,
};

use super::large_output_router::LargeOutputRouter;

/// Synthesize a large tool result into a short faithful summary.
///
/// Implementations may call Flash/V4; the registry only awaits this trait and
/// falls back to extractive synthesis on `None` / empty.
#[async_trait]
pub trait LargeOutputSynthesizer: Send + Sync {
    async fn synthesize(
        &self,
        tool_name: &str,
        raw_output: &str,
        estimated_tokens: usize,
    ) -> Option<String>;
}

/// Flash / seam-model synthesizer wired from the engine host.
pub struct FlashLargeOutputSynthesizer {
    client: Arc<dyn LlmClient>,
    model: String,
}

impl FlashLargeOutputSynthesizer {
    #[must_use]
    pub fn new(client: Arc<dyn LlmClient>, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
        }
    }
}

#[async_trait]
impl LargeOutputSynthesizer for FlashLargeOutputSynthesizer {
    async fn synthesize(
        &self,
        tool_name: &str,
        raw_output: &str,
        estimated_tokens: usize,
    ) -> Option<String> {
        // Cap raw payload sent to Flash to keep synthesis cheap/fast.
        let capped: String = raw_output.chars().take(120_000).collect();
        let prompt = LargeOutputRouter::synthesis_prompt(tool_name, &capped, estimated_tokens);
        let max_tokens = max_output_token_cap_for_model(&self.model).min(2_048);
        let request = MessageRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: prompt,
                    cache_control: None,
                }],
            }],
            max_tokens,
            system: Some(SystemPrompt::Text(
                "You are a synthesis assistant. Produce a faithful, dense summary. \
                 Preserve paths, errors, numbers, and actionable facts. No invention."
                    .to_string(),
            )),
            tools: None,
            tool_choice: None,
            metadata: None,
            thinking: None,
            reasoning_effort: None,
            stream: Some(false),
            temperature: Some(0.1),
            top_p: None,
        };

        let response = self.client.create_message(request).await.ok()?;
        crate::cost_status::report(&response.model, &response.usage);
        let text = response
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}
