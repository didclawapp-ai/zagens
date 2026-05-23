//! Tool and types for requesting user input via the TUI.

use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use deepseek_core::user_input::{
    UserInputOption, UserInputQuestion, UserInputRequest,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputAnswer {
    pub id: String,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputResponse {
    pub answers: Vec<UserInputAnswer>,
}

pub struct RequestUserInputTool;

#[async_trait]
impl ToolSpec for RequestUserInputTool {
    fn name(&self) -> &'static str {
        "request_user_input"
    }

    fn description(&self) -> &'static str {
        "Ask the user 1-3 short questions and return their selections."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "header": { "type": "string" },
                            "id": { "type": "string" },
                            "question": { "type": "string" },
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label": { "type": "string" },
                                        "description": { "type": "string" }
                                    },
                                    "required": ["label", "description"]
                                },
                                "minItems": 2,
                                "maxItems": 3
                            }
                        },
                        "required": ["header", "id", "question", "options"]
                    },
                    "minItems": 1,
                    "maxItems": 3
                }
            },
            "required": ["questions"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    async fn execute(
        &self,
        _input: Value,
        _context: &ToolContext,
    ) -> Result<ToolResult, ToolError> {
        Err(ToolError::execution_failed(
            "request_user_input must be handled by the engine",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_request_shape() {
        let request = UserInputRequest {
            questions: vec![UserInputQuestion {
                header: "Pick".to_string(),
                id: "choice".to_string(),
                question: "Which option?".to_string(),
                options: vec![
                    UserInputOption {
                        label: "A".to_string(),
                        description: "Option A".to_string(),
                    },
                    UserInputOption {
                        label: "B".to_string(),
                        description: "Option B".to_string(),
                    },
                ],
            }],
        };
        assert!(request.validate().is_ok());
    }

    #[test]
    fn rejects_too_many_questions() {
        let request = UserInputRequest {
            questions: vec![
                UserInputQuestion {
                    header: "Q1".to_string(),
                    id: "q1".to_string(),
                    question: "?".to_string(),
                    options: vec![
                        UserInputOption {
                            label: "A".to_string(),
                            description: "A".to_string(),
                        },
                        UserInputOption {
                            label: "B".to_string(),
                            description: "B".to_string(),
                        },
                    ],
                },
                UserInputQuestion {
                    header: "Q2".to_string(),
                    id: "q2".to_string(),
                    question: "?".to_string(),
                    options: vec![
                        UserInputOption {
                            label: "A".to_string(),
                            description: "A".to_string(),
                        },
                        UserInputOption {
                            label: "B".to_string(),
                            description: "B".to_string(),
                        },
                    ],
                },
                UserInputQuestion {
                    header: "Q3".to_string(),
                    id: "q3".to_string(),
                    question: "?".to_string(),
                    options: vec![
                        UserInputOption {
                            label: "A".to_string(),
                            description: "A".to_string(),
                        },
                        UserInputOption {
                            label: "B".to_string(),
                            description: "B".to_string(),
                        },
                    ],
                },
                UserInputQuestion {
                    header: "Q4".to_string(),
                    id: "q4".to_string(),
                    question: "?".to_string(),
                    options: vec![
                        UserInputOption {
                            label: "A".to_string(),
                            description: "A".to_string(),
                        },
                        UserInputOption {
                            label: "B".to_string(),
                            description: "B".to_string(),
                        },
                    ],
                },
            ],
        };
        assert!(request.validate().is_err());
    }
}
