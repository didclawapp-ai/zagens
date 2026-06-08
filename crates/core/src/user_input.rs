//! Types for `request_user_input` tool payloads (P2 PR4 → `zagens-core`).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zagens_tools::ToolError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputQuestion {
    pub header: String,
    pub id: String,
    pub question: String,
    pub options: Vec<UserInputOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputRequest {
    pub questions: Vec<UserInputQuestion>,
}

impl UserInputRequest {
    pub fn from_value(value: &Value) -> Result<Self, ToolError> {
        let request: UserInputRequest = serde_json::from_value(value.clone()).map_err(|e| {
            ToolError::invalid_input(format!("Invalid request_user_input payload: {e}"))
        })?;
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<(), ToolError> {
        if self.questions.is_empty() {
            return Err(ToolError::invalid_input(
                "request_user_input.questions must be non-empty",
            ));
        }
        if self.questions.len() > 3 {
            return Err(ToolError::invalid_input(
                "request_user_input.questions must contain 1 to 3 items",
            ));
        }
        for q in &self.questions {
            if q.header.trim().is_empty() {
                return Err(ToolError::invalid_input(
                    "request_user_input.questions.header cannot be empty",
                ));
            }
            if q.id.trim().is_empty() {
                return Err(ToolError::invalid_input(
                    "request_user_input.questions.id cannot be empty",
                ));
            }
            if q.question.trim().is_empty() {
                return Err(ToolError::invalid_input(
                    "request_user_input.questions.question cannot be empty",
                ));
            }
            if q.options.len() < 2 || q.options.len() > 3 {
                return Err(ToolError::invalid_input(
                    "request_user_input.questions.options must contain 2 or 3 items",
                ));
            }
            for opt in &q.options {
                if opt.label.trim().is_empty() {
                    return Err(ToolError::invalid_input(
                        "request_user_input option label cannot be empty",
                    ));
                }
                if opt.description.trim().is_empty() {
                    return Err(ToolError::invalid_input(
                        "request_user_input option description cannot be empty",
                    ));
                }
            }
        }
        Ok(())
    }
}
