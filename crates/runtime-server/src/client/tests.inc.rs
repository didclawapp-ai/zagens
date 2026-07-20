use anyhow::Result;
use std::collections::HashMap;

use serde_json::Value;

use crate::config::ApiProvider;
use self::api_parse::{apply_reasoning_effort, parse_models_response, parse_usage};
use self::chat::{
    build_chat_messages, count_reasoning_replay_chars,
    parse_chat_message, parse_sse_chunk, sanitize_thinking_mode_messages, tool_to_chat,
};
use self::http::{api_url, force_http1_from_env};
use self::tool_names::{from_api_tool_name, to_api_tool_name};
use self::http::validate_base_url_security;
use self::types::{
    acquire_stream_buffer, apply_request_failure, apply_request_success, mark_recovery_probe_if_due,
    AvailableModel, ConnectionHealth, ConnectionState, TokenBucket, RECOVERY_PROBE_COOLDOWN,
    release_stream_buffer,
};
use std::time::{Duration, Instant};
use crate::models::{
    ContentBlock, ContentBlockStart, Delta, Message, MessageRequest, StreamEvent, Tool,
};
use serde_json::json;

#[test]
fn tool_name_roundtrip_dot() {
    let original = "multi_tool_use.parallel";
    let encoded = to_api_tool_name(original);
    assert_eq!(encoded, "multi_tool_use-x00002E-parallel");
    let decoded = from_api_tool_name(&encoded);
    assert_eq!(decoded, original);
}

#[test]
fn tool_name_decode_mangled_dot_prefix() {
    let mangled = "multi_tool_use.x00002E-parallel";
    let decoded = from_api_tool_name(mangled);
    assert_eq!(decoded, "multi_tool_use..parallel");
}

#[test]
fn tool_name_decode_bare_hex_no_trailing_dash() {
    let mangled = "foo_x00002Ebar";
    let decoded = from_api_tool_name(mangled);
    assert_eq!(decoded, "foo_.bar");
}

#[test]
fn tool_name_bare_hex_preserves_alnum() {
    let input = "foox000041bar";
    let decoded = from_api_tool_name(input);
    assert_eq!(decoded, input);
}

#[test]
fn tool_name_bare_hex_preserves_underscore() {
    let input = "foox00005Fbar";
    let decoded = from_api_tool_name(input);
    assert_eq!(decoded, input);
}

#[test]
fn tool_name_roundtrip_colon() {
    let original = "mcp__server:tool_name";
    let encoded = to_api_tool_name(original);
    let decoded = from_api_tool_name(&encoded);
    assert_eq!(decoded, original);
}

#[test]
fn api_url_handles_default_v1_and_beta_base_urls() {
    assert_eq!(
        api_url("https://api.deepseek.com", "chat/completions"),
        "https://api.deepseek.com/v1/chat/completions"
    );
    assert_eq!(
        api_url("https://api.deepseek.com/v1", "chat/completions"),
        "https://api.deepseek.com/v1/chat/completions"
    );
    assert_eq!(
        api_url("https://api.deepseek.com/beta", "chat/completions"),
        "https://api.deepseek.com/beta/chat/completions"
    );
}

#[test]
fn api_url_routes_beta_paths_from_any_deepseek_base() {
    assert_eq!(
        api_url("https://api.deepseek.com", "beta/completions"),
        "https://api.deepseek.com/beta/completions"
    );
    assert_eq!(
        api_url("https://api.deepseek.com/v1", "beta/completions"),
        "https://api.deepseek.com/beta/completions"
    );
    assert_eq!(
        api_url("https://api.deepseek.com/beta", "beta/completions"),
        "https://api.deepseek.com/beta/completions"
    );
}

#[test]
fn api_url_keeps_non_v1_versioned_openai_compatible_bases() {
    assert_eq!(
        api_url(
            "https://open.bigmodel.cn/api/paas/v4",
            "chat/completions"
        ),
        "https://open.bigmodel.cn/api/paas/v4/chat/completions"
    );
}

#[test]
fn default_headers_include_custom_headers_when_configured() {
    let mut extra = HashMap::new();
    extra.insert("X-Model-Provider-Id".to_string(), "tongyi".to_string());
    let headers = DeepSeekClient::default_headers("sk-test", &extra).expect("headers");
    assert_eq!(
        headers
            .get("x-model-provider-id")
            .and_then(|value| value.to_str().ok()),
        Some("tongyi")
    );
}

#[test]
fn default_headers_ignore_blank_custom_headers() {
    let mut extra = HashMap::new();
    extra.insert("X-Blank".to_string(), "   ".to_string());
    let headers = DeepSeekClient::default_headers("sk-test", &extra).expect("headers");
    assert!(headers.get("x-blank").is_none());
}

#[test]
fn chat_messages_keep_reasoning_content_on_all_assistant_messages() {
    let message = Message {
        role: "assistant".to_string(),
        content: vec![
            ContentBlock::Thinking {
                thinking: "plan".to_string(),
            },
            ContentBlock::Text {
                text: "done".to_string(),
                cache_control: None,
            },
        ],
    };
    let out = build_chat_messages(None, &[message], "deepseek-v4-pro");
    let assistant = out
        .iter()
        .find(|value| value.get("role").and_then(Value::as_str) == Some("assistant"))
        .expect("assistant message");
    assert_eq!(
        assistant.get("content").and_then(Value::as_str),
        Some("done")
    );
    assert_eq!(
        assistant.get("reasoning_content").and_then(Value::as_str),
        Some("plan"),
        "thinking-mode models must keep reasoning_content on all assistant messages"
    );
}

#[test]
fn chat_messages_replay_prior_tool_round_reasoning_after_new_user_turn() {
    let messages = vec![
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "Need the date".to_string(),
                cache_control: None,
            }],
        },
        Message {
            role: "assistant".to_string(),
            content: vec![
                ContentBlock::Thinking {
                    thinking: "Need to call a tool".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "get_date".to_string(),
                    input: json!({}),
                    caller: None,
                },
            ],
        },
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "tool-1".to_string(),
                content: "2026-04-23".to_string(),
                is_error: None,
                content_blocks: None,
            }],
        },
        Message {
            role: "assistant".to_string(),
            content: vec![ContentBlock::Text {
                text: "It is 2026-04-23.".to_string(),
                cache_control: None,
            }],
        },
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "Thanks. Next question.".to_string(),
                cache_control: None,
            }],
        },
    ];
    let out = build_chat_messages(None, &messages, "deepseek-v4-pro");
    let tool_assistant = out
        .iter()
        .find(|value| {
            value.get("role").and_then(Value::as_str) == Some("assistant")
                && value.get("tool_calls").is_some()
        })
        .expect("tool-call assistant message");
    assert_eq!(
        tool_assistant
            .get("reasoning_content")
            .and_then(Value::as_str),
        Some("Need to call a tool"),
        "thinking-mode tool rounds must replay reasoning_content on later requests"
    );
}

#[test]
fn chat_messages_allow_tool_round_without_reasoning_when_thinking_disabled() {
    let request = MessageRequest {
        model: "deepseek-v4-pro".to_string(),
        messages: vec![
            Message {
                role: "assistant".to_string(),
                content: vec![ContentBlock::ToolUse {
                    id: "call-no-thinking".to_string(),
                    name: "read_file".to_string(),
                    input: json!({"path": "Cargo.toml"}),
                    caller: None,
                }],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "call-no-thinking".to_string(),
                    content: "workspace manifest".to_string(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
        ],
        max_tokens: 1024,
        system: None,
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,
        reasoning_effort: Some("off".to_string()),
        stream: None,
        temperature: None,
        top_p: None,
    };

    let out = build_chat_messages_for_request(&request);
    assert!(
        out.iter().any(
            |value| value.get("role").and_then(Value::as_str) == Some("assistant")
                && value.get("tool_calls").is_some()
        ),
        "tool calls remain valid when thinking mode is disabled"
    );
    assert!(
        out.iter()
            .any(|value| value.get("role").and_then(Value::as_str) == Some("tool")),
        "matching tool result should remain"
    );
}

#[test]
fn reasoning_effort_uses_deepseek_top_level_thinking_parameter() {
    let mut body = json!({});
    apply_reasoning_effort(&mut body, Some("max"), ApiProvider::Deepseek);

    assert_eq!(
        body.get("reasoning_effort").and_then(Value::as_str),
        Some("max")
    );
    assert_eq!(
        body.pointer("/thinking/type").and_then(Value::as_str),
        Some("enabled")
    );
    assert!(body.get("extra_body").is_none());
}

#[test]
fn reasoning_effort_off_disables_top_level_thinking() {
    let mut body = json!({});
    apply_reasoning_effort(&mut body, Some("off"), ApiProvider::Deepseek);

    assert_eq!(
        body.pointer("/thinking/type").and_then(Value::as_str),
        Some("disabled")
    );
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("extra_body").is_none());
}

#[test]
fn reasoning_effort_uses_nvidia_nim_chat_template_kwargs() {
    let mut body = json!({});
    apply_reasoning_effort(&mut body, Some("max"), ApiProvider::NvidiaNim);

    assert_eq!(
        body.pointer("/chat_template_kwargs/thinking")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        body.pointer("/chat_template_kwargs/reasoning_effort")
            .and_then(Value::as_str),
        Some("max")
    );
    assert!(body.get("thinking").is_none());
    assert!(body.get("reasoning_effort").is_none());
}

#[test]
fn reasoning_effort_off_disables_nvidia_nim_thinking() {
    let mut body = json!({});
    apply_reasoning_effort(&mut body, Some("off"), ApiProvider::NvidiaNim);

    assert_eq!(
        body.pointer("/chat_template_kwargs/thinking")
            .and_then(Value::as_bool),
        Some(false)
    );
    assert!(
        body.pointer("/chat_template_kwargs/reasoning_effort")
            .is_none()
    );
}

#[test]
fn reasoning_effort_moonshot_passes_low_high_max_as_is() {
    let mut low = json!({ "model": "kimi-k3" });
    apply_reasoning_effort(&mut low, Some("low"), ApiProvider::Moonshot);
    assert_eq!(
        low.get("reasoning_effort").and_then(Value::as_str),
        Some("low")
    );
    assert!(low.get("thinking").is_none());

    let mut high = json!({ "model": "kimi-k3" });
    apply_reasoning_effort(&mut high, Some("high"), ApiProvider::Moonshot);
    assert_eq!(
        high.get("reasoning_effort").and_then(Value::as_str),
        Some("high")
    );

    let mut max = json!({ "model": "kimi-k3" });
    apply_reasoning_effort(&mut max, Some("max"), ApiProvider::Moonshot);
    assert_eq!(
        max.get("reasoning_effort").and_then(Value::as_str),
        Some("max")
    );
}

#[test]
fn reasoning_effort_moonshot_off_maps_to_max_not_disabled() {
    let mut body = json!({ "model": "kimi-k3" });
    apply_reasoning_effort(&mut body, Some("off"), ApiProvider::Moonshot);
    assert_eq!(
        body.get("reasoning_effort").and_then(Value::as_str),
        Some("max")
    );
    assert!(body.get("thinking").is_none());
    assert!(body.pointer("/thinking/type").is_none());
}

#[test]
fn chat_parser_accepts_nvidia_nim_reasoning_field() -> Result<()> {
    let response = parse_chat_message(&json!({
        "id": "chatcmpl-test",
        "model": "deepseek-ai/deepseek-v4-pro",
        "choices": [{
            "message": {
                "role": "assistant",
                "reasoning": "thinking via NIM",
                "content": "final answer"
            },
            "finish_reason": "stop"
        }],
        "usage": {
            "prompt_tokens": 10,
            "completion_tokens": 3
        }
    }))?;

    assert!(matches!(
        response.content.first(),
        Some(ContentBlock::Thinking { thinking }) if thinking == "thinking via NIM"
    ));
    assert!(matches!(
        response.content.get(1),
        Some(ContentBlock::Text { text, .. }) if text == "final answer"
    ));
    Ok(())
}

#[test]
fn sse_parser_accepts_nvidia_nim_reasoning_delta() {
    let mut content_index = 0;
    let mut text_started = false;
    let mut thinking_started = false;
    let mut tool_indices = std::collections::HashMap::new();
    let events = parse_sse_chunk(
        &json!({
            "choices": [{
                "delta": {
                    "reasoning": "nim thought"
                }
            }]
        }),
        &mut content_index,
        &mut text_started,
        &mut thinking_started,
        &mut tool_indices,
        true,
    );

    assert!(events.iter().any(|event| matches!(
        event,
        StreamEvent::ContentBlockDelta {
            delta: Delta::ThinkingDelta { thinking },
            ..
        } if thinking == "nim thought"
    )));
}

#[test]
fn chat_tool_strict_flag_is_nested_under_function() {
    let tool = Tool {
        tool_type: Some("function".to_string()),
        name: "emit_json".to_string(),
        description: "Emit JSON".to_string(),
        input_schema: json!({"type": "object", "properties": {}}),
        allowed_callers: None,
        defer_loading: None,
        input_examples: None,
        strict: Some(true),
        cache_control: None,
    };
    let encoded = tool_to_chat(&tool);
    assert_eq!(
        encoded
            .get("function")
            .and_then(|function| function.get("strict"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert!(encoded.get("strict").is_none());
}

#[test]
fn chat_messages_drop_thinking_only_assistant_for_non_reasoning_model() {
    let message = Message {
        role: "assistant".to_string(),
        content: vec![ContentBlock::Thinking {
            thinking: "plan".to_string(),
        }],
    };
    let out = build_chat_messages(None, &[message], "some-non-deepseek-model");
    assert!(
        !out.iter()
            .any(|value| value.get("role").and_then(Value::as_str) == Some("assistant")),
        "non-reasoning model should drop thinking-only assistant"
    );
}

#[test]
fn parse_sse_chunk_closes_each_tool_block_with_matching_index() {
    let chunk = json!({
        "choices": [{
            "delta": {
                "tool_calls": [
                    {
                        "index": 0,
                        "id": "call_0",
                        "function": {"name": "read_file", "arguments": "{\"path\":\"a\"}"}
                    },
                    {
                        "index": 1,
                        "id": "call_1",
                        "function": {"name": "read_file", "arguments": "{\"path\":\"b\"}"}
                    }
                ]
            },
            "finish_reason": "tool_calls"
        }]
    });

    let mut content_index = 0;
    let mut text_started = false;
    let mut thinking_started = false;
    let mut tool_indices: std::collections::HashMap<u32, u32> =
        std::collections::HashMap::new();
    let events = parse_sse_chunk(
        &chunk,
        &mut content_index,
        &mut text_started,
        &mut thinking_started,
        &mut tool_indices,
        false,
    );

    let starts: Vec<u32> = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::ContentBlockStart {
                index,
                content_block: ContentBlockStart::ToolUse { .. },
            } => Some(*index),
            _ => None,
        })
        .collect();
    let stops: Vec<u32> = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::ContentBlockStop { index } => Some(*index),
            _ => None,
        })
        .collect();
    let deltas: Vec<u32> = events
        .iter()
        .filter_map(|event| match event {
            StreamEvent::ContentBlockDelta {
                index,
                delta: Delta::InputJsonDelta { .. },
            } => Some(*index),
            _ => None,
        })
        .collect();

    assert_eq!(starts, vec![0, 1]);
    assert_eq!(stops, vec![0, 1]);
    assert_eq!(deltas, vec![0, 1]);
}

#[test]
fn parse_sse_chunk_handles_empty_choices_usage_chunk() {
    let chunk = json!({
        "choices": [],
        "usage": {
            "prompt_tokens": 100,
            "completion_tokens": 20,
            "prompt_cache_hit_tokens": 70,
            "prompt_cache_miss_tokens": 30
        }
    });

    let mut content_index = 0;
    let mut text_started = false;
    let mut thinking_started = false;
    let mut tool_indices: std::collections::HashMap<u32, u32> =
        std::collections::HashMap::new();
    let events = parse_sse_chunk(
        &chunk,
        &mut content_index,
        &mut text_started,
        &mut thinking_started,
        &mut tool_indices,
        false,
    );

    let StreamEvent::MessageDelta {
        usage: Some(usage), ..
    } = &events[0]
    else {
        panic!("expected usage delta");
    };
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.prompt_cache_hit_tokens, Some(70));
    assert_eq!(usage.prompt_cache_miss_tokens, Some(30));
}

#[test]
fn chat_messages_drop_orphan_tool_results() {
    let messages = vec![Message {
        role: "user".to_string(),
        content: vec![ContentBlock::ToolResult {
            tool_use_id: "tool-1".to_string(),
            content: "ok".to_string(),
            is_error: None,
            content_blocks: None,
        }],
    }];

    let out = build_chat_messages(None, &messages, "deepseek-v4-flash");
    assert!(
        !out.iter()
            .any(|value| { value.get("role").and_then(Value::as_str) == Some("tool") })
    );
}

#[test]
fn chat_messages_include_tool_results_when_call_present() {
    let messages = vec![
        Message {
            role: "assistant".to_string(),
            content: vec![
                ContentBlock::Thinking {
                    thinking: "Need to inspect the directory".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "list_dir".to_string(),
                    input: json!({}),
                    caller: None,
                },
            ],
        },
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "tool-1".to_string(),
                content: "ok".to_string(),
                is_error: None,
                content_blocks: None,
            }],
        },
    ];

    let out = build_chat_messages(None, &messages, "deepseek-v4-flash");
    assert!(
        out.iter()
            .any(|value| { value.get("role").and_then(Value::as_str) == Some("tool") })
    );
    let assistant = out
        .iter()
        .find(|value| value.get("role").and_then(Value::as_str) == Some("assistant"))
        .expect("assistant message");
    assert!(assistant.get("tool_calls").is_some());
}

#[test]
fn chat_messages_encode_tool_call_names() {
    let messages = vec![
        Message {
            role: "assistant".to_string(),
            content: vec![
                ContentBlock::Thinking {
                    thinking: "Need to search".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "web.run".to_string(),
                    input: json!({}),
                    caller: None,
                },
            ],
        },
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "tool-1".to_string(),
                content: "ok".to_string(),
                is_error: None,
                content_blocks: None,
            }],
        },
    ];

    let out = build_chat_messages(None, &messages, "deepseek-v4-flash");
    let assistant = out
        .iter()
        .find(|value| value.get("role").and_then(Value::as_str) == Some("assistant"))
        .expect("assistant message");
    let tool_calls = assistant
        .get("tool_calls")
        .and_then(Value::as_array)
        .expect("tool_calls array");
    let function_name = tool_calls
        .first()
        .and_then(|call| call.get("function"))
        .and_then(|func| func.get("name"))
        .and_then(Value::as_str)
        .expect("tool call function name");

    assert_eq!(function_name, to_api_tool_name("web.run"));
}

#[test]
fn chat_messages_strips_orphaned_tool_calls_after_compaction() {
    // Simulates post-compaction state: assistant has tool_calls but the
    // tool result messages were summarized away.
    let messages = vec![
        Message {
            role: "assistant".to_string(),
            content: vec![ContentBlock::ToolUse {
                id: "tool-orphan".to_string(),
                name: "read_file".to_string(),
                input: json!({"path": "src/main.rs"}),
                caller: None,
            }],
        },
        // No tool result follows — it was removed by compaction.
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "continue".to_string(),
                cache_control: None,
            }],
        },
    ];

    let out = build_chat_messages(None, &messages, "deepseek-v4-flash");
    let assistant = out
        .iter()
        .find(|value| value.get("role").and_then(Value::as_str) == Some("assistant"));
    // The safety net may drop the assistant message entirely if it only
    // contained orphaned tool_calls and no text content.
    assert!(
        assistant.is_none(),
        "assistant without content/tool_calls should be removed"
    );
    assert!(
        !out.iter()
            .any(|v| v.get("role").and_then(Value::as_str) == Some("tool")),
        "orphaned tool results should also be removed"
    );
}

#[test]
fn chat_messages_keeps_valid_tool_calls_intact() {
    // Complete call+result pair should NOT be stripped.
    let messages = vec![
        Message {
            role: "assistant".to_string(),
            content: vec![
                ContentBlock::Thinking {
                    thinking: "Need to list files".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "tool-ok".to_string(),
                    name: "list_dir".to_string(),
                    input: json!({}),
                    caller: None,
                },
            ],
        },
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "tool-ok".to_string(),
                content: "files".to_string(),
                is_error: None,
                content_blocks: None,
            }],
        },
    ];

    let out = build_chat_messages(None, &messages, "deepseek-v4-flash");
    let assistant = out
        .iter()
        .find(|value| value.get("role").and_then(Value::as_str) == Some("assistant"))
        .expect("assistant message");
    assert!(
        assistant.get("tool_calls").is_some(),
        "valid tool_calls should remain intact"
    );
    assert!(
        out.iter()
            .any(|value| value.get("role").and_then(Value::as_str) == Some("tool")),
        "tool result should remain"
    );
}

#[test]
fn chat_messages_strips_partial_tool_results() {
    let messages = vec![
        Message {
            role: "assistant".to_string(),
            content: vec![
                ContentBlock::ToolUse {
                    id: "t1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({"path": "a.rs"}),
                    caller: None,
                },
                ContentBlock::ToolUse {
                    id: "t2".to_string(),
                    name: "read_file".to_string(),
                    input: json!({"path": "b.rs"}),
                    caller: None,
                },
                ContentBlock::ToolUse {
                    id: "t3".to_string(),
                    name: "shell".to_string(),
                    input: json!({"cmd": "ls"}),
                    caller: None,
                },
            ],
        },
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t1".to_string(),
                content: "content a".to_string(),
                is_error: None,
                content_blocks: None,
            }],
        },
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t2".to_string(),
                content: "content b".to_string(),
                is_error: None,
                content_blocks: None,
            }],
        },
        // No result for t3
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "continue".to_string(),
                cache_control: None,
            }],
        },
    ];

    let out = build_chat_messages(None, &messages, "deepseek-v4-flash");
    let assistant = out
        .iter()
        .find(|v| v.get("role").and_then(Value::as_str) == Some("assistant"));
    assert!(
        assistant.is_none(),
        "assistant with only partial tool_calls should be removed"
    );
    assert!(
        !out.iter()
            .any(|v| v.get("role").and_then(Value::as_str) == Some("tool")),
        "all orphaned tool results should be removed"
    );
}

#[test]
fn parse_models_response_parses_and_deduplicates() {
    let payload = r#"{
        "object": "list",
        "data": [
            {"id": "deepseek-v4-pro", "object": "model", "owned_by": "deepseek", "created": 1},
            {"id": "deepseek-v4-flash", "object": "model"},
            {"id": "deepseek-v4-pro", "object": "model", "owned_by": "deepseek", "created": 1}
        ]
    }"#;

    let models = parse_models_response(payload).expect("parse models");
    assert_eq!(
        models,
        vec![
            AvailableModel {
                id: "deepseek-v4-flash".to_string(),
                owned_by: None,
                created: None
            },
            AvailableModel {
                id: "deepseek-v4-pro".to_string(),
                owned_by: Some("deepseek".to_string()),
                created: Some(1)
            }
        ]
    );
}

#[test]
fn parse_models_response_accepts_ollama_tag_ids() {
    let payload = r#"{
        "object": "list",
        "data": [
            {"id": "qwen2.5-coder:7b", "object": "model", "owned_by": "library"},
            {"id": "deepseek-coder-v2:16b", "object": "model"}
        ]
    }"#;

    let models = parse_models_response(payload).expect("parse models");
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["deepseek-coder-v2:16b", "qwen2.5-coder:7b"]
    );
}

#[test]
fn parse_usage_reads_deepseek_cache_and_reasoning_tokens() {
    let usage = parse_usage(Some(&json!({
        "prompt_tokens": 100,
        "completion_tokens": 20,
        "prompt_cache_hit_tokens": 70,
        "prompt_cache_miss_tokens": 30,
        "completion_tokens_details": {
            "reasoning_tokens": 12
        }
    })));

    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 20);
    assert_eq!(usage.prompt_cache_hit_tokens, Some(70));
    assert_eq!(usage.prompt_cache_miss_tokens, Some(30));
    assert_eq!(usage.reasoning_tokens, Some(12));
}

#[test]
fn parse_usage_counts_reasoning_tokens_when_completion_tokens_are_zero() {
    let usage = parse_usage(Some(&json!({
        "prompt_tokens": 100,
        "completion_tokens": 0,
        "completion_tokens_details": {
            "reasoning_tokens": 12
        }
    })));

    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 12);
    assert_eq!(usage.reasoning_tokens, Some(12));
    assert!(
        crate::pricing::calculate_turn_cost_from_usage("deepseek-v4-pro", &usage)
            .expect("DeepSeek V4 Pro pricing should apply")
            > 0.0
    );
}

#[test]
fn parse_usage_reads_v4_prompt_tokens_details_cached_tokens() {
    let usage = parse_usage(Some(&json!({
        "prompt_tokens": 4000,
        "completion_tokens": 20,
        "prompt_tokens_details": {
            "cached_tokens": 3000
        }
    })));

    assert_eq!(usage.input_tokens, 4000);
    assert_eq!(usage.output_tokens, 20);
    assert_eq!(usage.prompt_cache_hit_tokens, Some(3000));
    assert_eq!(usage.prompt_cache_miss_tokens, Some(1000));
}

#[test]
fn sanitize_thinking_mode_counts_reasoning_replay_across_assistant_turns() {
    // Multi-turn body that mimics two prior tool-calling rounds: each
    // assistant message carries its `reasoning_content`. The sanitizer
    // should keep all of them and the count helper should tally bytes
    // across every assistant message.
    let mut body = json!({
        "model": "deepseek-v4-pro",
        "messages": [
            { "role": "system", "content": "you are helpful" },
            { "role": "user", "content": "step 1" },
            {
                "role": "assistant",
                "content": "",
                "reasoning_content": "I need to call tool A first.",
                "tool_calls": [{ "id": "1", "type": "function" }]
            },
            { "role": "tool", "tool_call_id": "1", "content": "ok" },
            {
                "role": "assistant",
                "content": "",
                "reasoning_content": "Now I call tool B.",
                "tool_calls": [{ "id": "2", "type": "function" }]
            },
            { "role": "tool", "tool_call_id": "2", "content": "ok" },
            { "role": "user", "content": "step 2" }
        ]
    });

    let approx_tokens =
        sanitize_thinking_mode_messages(&mut body, "deepseek-v4-pro", Some("max"))
            .expect("multi-turn thinking-mode conversation should report replay tokens");
    // ~4 chars/token; 46 bytes of reasoning -> 11 tokens.
    assert_eq!(approx_tokens, 11);

    let chars = count_reasoning_replay_chars(&body);
    // "I need to call tool A first." (28) + "Now I call tool B." (18) = 46
    assert_eq!(chars, 46);

    // No assistant messages should have lost or had their reasoning_content blanked.
    let messages = body["messages"].as_array().unwrap();
    let assistant_with_reasoning: usize = messages
        .iter()
        .filter(|m| m["role"] == "assistant")
        .filter(|m| {
            m["reasoning_content"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
        })
        .count();
    assert_eq!(assistant_with_reasoning, 2);
}

/// Issue #30: when no thinking-mode replay applies (non-thinking model or
/// empty conversation), the sanitizer returns `None` so the footer chip
/// stays hidden.
#[test]
fn sanitize_thinking_mode_returns_none_for_non_thinking_model() {
    let mut body = json!({
        "model": "deepseek-v4-flash",
        "messages": [
            { "role": "user", "content": "hi" }
        ]
    });
    let result = sanitize_thinking_mode_messages(&mut body, "deepseek-v4-flash", None);
    // reasoning_effort is None → no thinking injection, result is None
    assert!(result.is_none());
}

#[test]
fn sanitize_thinking_mode_counts_substituted_placeholder() {
    // An assistant tool-call message is missing reasoning_content; the
    // sanitizer must inject the placeholder, and the count helper must
    // include the placeholder in the total (since it's in the wire
    // payload that ships to DeepSeek).
    let mut body = json!({
        "model": "deepseek-v4-pro",
        "messages": [
            { "role": "user", "content": "hi" },
            {
                "role": "assistant",
                "content": "",
                "tool_calls": [{ "id": "1", "type": "function" }]
            }
        ]
    });

    sanitize_thinking_mode_messages(&mut body, "deepseek-v4-pro", Some("max"));

    let chars = count_reasoning_replay_chars(&body);
    // "(reasoning omitted)" is 19 bytes.
    assert_eq!(chars, 19);
}

#[test]
fn token_bucket_enforces_delay_when_empty() {
    let now = Instant::now();
    let mut bucket = TokenBucket {
        enabled: true,
        capacity: 1.0,
        tokens: 1.0,
        refill_per_sec: 2.0,
        last_refill: now,
    };

    assert!(bucket.delay_until_available(1.0).is_none());
    let delay = bucket
        .delay_until_available(1.0)
        .expect("bucket should require refill delay");
    assert!(
        delay >= Duration::from_millis(400) && delay <= Duration::from_millis(600),
        "unexpected refill delay: {delay:?}"
    );
}

#[test]
fn stream_buffer_pool_reuses_released_buffers() {
    let mut first = acquire_stream_buffer();
    first.extend_from_slice(b"hello");
    let released_capacity = first.capacity();
    release_stream_buffer(first);

    let second = acquire_stream_buffer();
    assert!(second.is_empty());
    assert!(
        second.capacity() >= released_capacity,
        "pooled buffer capacity should be reused"
    );
}

#[test]
fn base_url_security_rejects_insecure_non_local_http() {
    let err = validate_base_url_security("http://api.deepseek.com")
        .expect_err("non-local insecure HTTP should be rejected");
    assert!(err.to_string().contains("Refusing insecure base URL"));
}

#[test]
fn base_url_security_allows_localhost_http() {
    assert!(validate_base_url_security("http://localhost:8080").is_ok());
    assert!(validate_base_url_security("http://127.0.0.1:8080").is_ok());
}

#[test]
fn connection_health_degrades_and_recovers() {
    let now = Instant::now();
    let mut health = ConnectionHealth::default();
    assert_eq!(health.state, ConnectionState::Healthy);

    apply_request_failure(&mut health, now);
    assert_eq!(health.state, ConnectionState::Healthy);

    apply_request_failure(&mut health, now + Duration::from_millis(1));
    assert_eq!(health.state, ConnectionState::Degraded);
    assert_eq!(health.consecutive_failures, 2);

    let recovered = apply_request_success(&mut health, now + Duration::from_secs(1));
    assert!(recovered);
    assert_eq!(health.state, ConnectionState::Healthy);
    assert_eq!(health.consecutive_failures, 0);
}

#[test]
fn recovery_probe_respects_cooldown() {
    let now = Instant::now();
    let mut health = ConnectionHealth {
        state: ConnectionState::Degraded,
        ..ConnectionHealth::default()
    };

    assert!(mark_recovery_probe_if_due(&mut health, now));
    assert_eq!(health.state, ConnectionState::Recovering);
    assert!(!mark_recovery_probe_if_due(
        &mut health,
        now + Duration::from_secs(1)
    ));
    assert!(mark_recovery_probe_if_due(
        &mut health,
        now + RECOVERY_PROBE_COOLDOWN + Duration::from_millis(1)
    ));
}

// === #103 Phase 2: HTTP/1 escape hatch ===================================

/// Serialize tests that mutate `DEEPSEEK_FORCE_HTTP1` so they don't race
/// against each other — env vars are process-global.
static FORCE_HTTP1_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct ForceHttp1EnvGuard {
    prior: Option<std::ffi::OsString>,
}
impl ForceHttp1EnvGuard {
    fn capture() -> Self {
        Self {
            prior: std::env::var_os("DEEPSEEK_FORCE_HTTP1"),
        }
    }
}
impl Drop for ForceHttp1EnvGuard {
    fn drop(&mut self) {
        // Safety: scoped to test process; reverts to the captured value.
        match &self.prior {
            Some(v) => unsafe { std::env::set_var("DEEPSEEK_FORCE_HTTP1", v) },
            None => unsafe { std::env::remove_var("DEEPSEEK_FORCE_HTTP1") },
        }
    }
}

#[test]
fn force_http1_unset_is_false() {
    let _lock = FORCE_HTTP1_ENV_LOCK.lock().unwrap();
    let _guard = ForceHttp1EnvGuard::capture();
    unsafe { std::env::remove_var("DEEPSEEK_FORCE_HTTP1") };
    assert!(!force_http1_from_env());
}

#[test]
fn force_http1_truthy_values() {
    let _lock = FORCE_HTTP1_ENV_LOCK.lock().unwrap();
    let _guard = ForceHttp1EnvGuard::capture();
    for value in ["1", "true", "True", "YES", "on", " 1 "] {
        // Safety: serialized by FORCE_HTTP1_ENV_LOCK; reverted by guard.
        unsafe { std::env::set_var("DEEPSEEK_FORCE_HTTP1", value) };
        assert!(
            force_http1_from_env(),
            "{value:?} should be parsed as truthy",
        );
    }
}

#[test]
fn force_http1_falsy_values() {
    let _lock = FORCE_HTTP1_ENV_LOCK.lock().unwrap();
    let _guard = ForceHttp1EnvGuard::capture();
    for value in ["0", "false", "no", "off", "", "garbage", "2"] {
        unsafe { std::env::set_var("DEEPSEEK_FORCE_HTTP1", value) };
        assert!(
            !force_http1_from_env(),
            "{value:?} should NOT be parsed as truthy",
        );
    }
}
