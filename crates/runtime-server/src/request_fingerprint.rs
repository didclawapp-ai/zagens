//! Wire-format prefix fingerprints for KV-cache CI (kernel-v2 M5).

use serde::Serialize;
use zagens_core::chat::MessageRequest;
use zagens_core::engine::request_fingerprint::{RequestFingerprint, compute_request_fingerprint};

use crate::client::{build_chat_messages_for_request, system_to_instructions};
use crate::prompts::COMPACT_TEMPLATE;

/// Static system layer: everything through the compaction handoff template.
///
/// Matches the volatile boundary documented in `prompts.rs` (blocks 1–5).
#[must_use]
pub fn static_system_instructions(full: &str) -> &str {
    if let Some(pos) = full.find(COMPACT_TEMPLATE) {
        &full[..pos + COMPACT_TEMPLATE.len()]
    } else {
        full
    }
}

fn canonical_json_bytes<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).expect("request fingerprint JSON serialization")
}

fn concat_prefix(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push(0);
        }
        out.extend_from_slice(part);
    }
    out
}

/// Fingerprint the prefix bytes that define DeepSeek KV-cache identity.
#[must_use]
pub fn fingerprint_message_request(request: &MessageRequest) -> RequestFingerprint {
    let system_full = system_to_instructions(request.system.clone()).unwrap_or_default();
    let static_system = static_system_instructions(&system_full);
    let tools = request.tools.as_deref().unwrap_or(&[]);
    let tools_bytes = canonical_json_bytes(tools);
    let wire_messages = build_chat_messages_for_request(request);
    let messages_bytes = canonical_json_bytes(&wire_messages);

    let static_prefix = concat_prefix(&[static_system.as_bytes(), &tools_bytes]);
    let full_prefix = concat_prefix(&[system_full.as_bytes(), &tools_bytes, &messages_bytes]);

    compute_request_fingerprint(&static_prefix, &full_prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    use tempfile::tempdir;
    use zagens_core::approval::ApprovalMode;
    use zagens_core::chat::{ContentBlock, Message, SystemPrompt, Tool};
    use zagens_core::task_type::TaskType;

    use crate::agent_surface::AppMode;
    use crate::prompts::{
        PromptSessionContext, system_prompt_for_mode_with_context_skills_session_and_approval,
    };
    use crate::tools::registry::ToolRegistryBuilder;
    use crate::tools::spec::ToolContext;

    fn sample_request(
        system: SystemPrompt,
        tools: Vec<Tool>,
        messages: Vec<Message>,
    ) -> MessageRequest {
        MessageRequest {
            model: "deepseek-v4-pro".to_string(),
            messages,
            max_tokens: 8192,
            system: Some(system),
            tools: Some(tools),
            tool_choice: None,
            metadata: None,
            thinking: None,
            reasoning_effort: Some("high".to_string()),
            stream: Some(true),
            temperature: None,
            top_p: None,
        }
    }

    fn agent_system_prompt(workspace: &Path) -> SystemPrompt {
        system_prompt_for_mode_with_context_skills_session_and_approval(
            AppMode::Agent,
            workspace,
            None,
            None,
            None,
            PromptSessionContext {
                user_memory_block: None,
                topic_memory_block: None,
                goal_objective: None,
                locale_tag: "en",
                task_type: TaskType::Code,
            },
            ApprovalMode::Suggest,
        )
    }

    #[test]
    fn static_prefix_stable_across_identical_rerender() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let a = agent_system_prompt(dir.path());
        let b = agent_system_prompt(dir.path());
        let tools = ToolRegistryBuilder::new().build(ctx).to_api_tools();
        let req_a = sample_request(a, tools.clone(), vec![]);
        let req_b = sample_request(b, tools, vec![]);
        let fp_a = fingerprint_message_request(&req_a);
        let fp_b = fingerprint_message_request(&req_b);
        assert_eq!(fp_a.static_prefix_sha256, fp_b.static_prefix_sha256);
        assert_eq!(fp_a.full_prefix_sha256, fp_b.full_prefix_sha256);
    }

    #[test]
    fn handoff_block_changes_full_fingerprint_not_static() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let tools = ToolRegistryBuilder::new().build(ctx).to_api_tools();
        let SystemPrompt::Text(base_prompt) = agent_system_prompt(dir.path()) else {
            panic!("expected text system prompt");
        };
        let static_prompt = static_system_instructions(&base_prompt).to_string();
        let handoff_prompt = format!(
            "{base_prompt}\n\n## Previous Session Handoff\n\nThe previous session left notes here."
        );

        let before = compute_request_fingerprint(
            &concat_prefix(&[static_prompt.as_bytes(), &canonical_json_bytes(&tools)]),
            &concat_prefix(&[
                base_prompt.as_bytes(),
                &canonical_json_bytes(&tools),
                &canonical_json_bytes(&Vec::<serde_json::Value>::new()),
            ]),
        );
        let after = compute_request_fingerprint(
            &concat_prefix(&[static_prompt.as_bytes(), &canonical_json_bytes(&tools)]),
            &concat_prefix(&[
                handoff_prompt.as_bytes(),
                &canonical_json_bytes(&tools),
                &canonical_json_bytes(&Vec::<serde_json::Value>::new()),
            ]),
        );

        assert_eq!(
            before.static_prefix_sha256, after.static_prefix_sha256,
            "handoff must not alter the static system layer"
        );
        assert_ne!(
            before.full_prefix_sha256, after.full_prefix_sha256,
            "handoff must change the full prefix fingerprint"
        );
    }

    #[test]
    fn static_layer_jitter_is_detected() {
        let dir = tempdir().expect("tempdir");
        let mut system = agent_system_prompt(dir.path());
        let baseline = fingerprint_message_request(&sample_request(system.clone(), vec![], vec![]));

        if let SystemPrompt::Text(text) = &mut system {
            if let Some(pos) = text.find(COMPACT_TEMPLATE) {
                text.insert_str(pos, "<!-- accidental static jitter -->");
            } else {
                panic!("compaction template must be present in agent system prompt");
            }
        }
        let jittered = fingerprint_message_request(&sample_request(system, vec![], vec![]));
        assert_ne!(
            baseline.static_prefix_sha256, jittered.static_prefix_sha256,
            "static-layer edits must change the static fingerprint"
        );
    }

    #[test]
    fn turn_meta_in_messages_changes_full_not_static() {
        let dir = tempdir().expect("tempdir");
        let system = agent_system_prompt(dir.path());
        let tools = vec![];
        let base_messages = vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "hello".to_string(),
                cache_control: None,
            }],
        }];
        let with_meta = vec![Message {
            role: "user".to_string(),
            content: vec![
                ContentBlock::Text {
                    text: "<turn_meta>\nCurrent local date: 2099-01-01\n</turn_meta>".to_string(),
                    cache_control: None,
                },
                ContentBlock::Text {
                    text: "hello".to_string(),
                    cache_control: None,
                },
            ],
        }];
        let base = fingerprint_message_request(&sample_request(
            system.clone(),
            tools.clone(),
            base_messages,
        ));
        let meta = fingerprint_message_request(&sample_request(system, tools, with_meta));
        assert_eq!(base.static_prefix_sha256, meta.static_prefix_sha256);
        assert_ne!(base.full_prefix_sha256, meta.full_prefix_sha256);
    }

    #[test]
    fn tool_catalog_bytes_stable_when_sorted() {
        let dir = tempdir().expect("tempdir");
        let ctx = ToolContext::new(dir.path().to_path_buf());
        let registry = ToolRegistryBuilder::new().build(ctx);
        let first = registry.to_api_tools();
        let second = registry.to_api_tools();
        assert_eq!(
            canonical_json_bytes(&first),
            canonical_json_bytes(&second),
            "tool catalog JSON must be byte-stable across reads"
        );
    }

    #[test]
    fn kernel_v2_workspace_seed_static_prefix_is_stable() {
        let seed = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v2-corpus/workspace-seed");
        assert!(
            seed.is_dir(),
            "workspace-seed fixture missing at {}",
            seed.display()
        );
        let ctx = ToolContext::new(seed.clone());
        let tools = ToolRegistryBuilder::new().build(ctx).to_api_tools();
        let fp_a = fingerprint_message_request(&sample_request(
            agent_system_prompt(&seed),
            tools.clone(),
            vec![],
        ));
        let fp_b =
            fingerprint_message_request(&sample_request(agent_system_prompt(&seed), tools, vec![]));
        assert_eq!(fp_a.static_prefix_sha256, fp_b.static_prefix_sha256);
        assert_eq!(fp_a.full_prefix_sha256, fp_b.full_prefix_sha256);
    }
}
