use anyhow::Result;

use crate::llm_client::LlmClient;
use crate::models::{MessageRequest, MessageResponse};

use super::http::api_url;
use super::types::DeepSeekClient;

#[async_trait::async_trait]
impl LlmClient for DeepSeekClient {
    fn provider_name(&self) -> &'static str {
        self.api_provider.as_str()
    }

    fn model(&self) -> &str {
        &self.default_model
    }

    async fn health_check(&self) -> Result<bool> {
        let health_url = api_url(&self.base_url, "models");
        self.wait_for_rate_limit().await;
        let response = self.http_client.get(health_url).send().await;
        match response {
            Ok(resp) if resp.status().is_success() => {
                self.mark_request_success().await;
                Ok(true)
            }
            Ok(resp) => {
                self.mark_request_failure(&format!("health status={}", resp.status()))
                    .await;
                Ok(false)
            }
            Err(err) => {
                self.mark_request_failure(&format!("health error={err}"))
                    .await;
                Ok(false)
            }
        }
    }

    async fn create_message(&self, request: MessageRequest) -> Result<MessageResponse> {
        self.create_message_chat(&request).await
    }

    async fn create_message_stream(
        &self,
        request: MessageRequest,
    ) -> Result<crate::llm_client::StreamEventBox> {
        self.handle_chat_completion_stream(request).await
    }
}
