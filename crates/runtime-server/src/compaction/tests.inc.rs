use std::collections::BTreeSet;
use std::path::PathBuf;

use super::*;
use crate::models::{ContentBlock, Message, SystemBlock, SystemPrompt};
use serde_json::json;

fn msg(role: &str, text: &str) -> Message {
        Message {
            role: role.to_string(),
            content: vec![ContentBlock::Text {
                text: text.to_string(),
                cache_control: None,
            }],
        }
    }

    fn tool_use(id: &str, name: &str, input: serde_json::Value) -> Message {
        Message {
            role: "assistant".to_string(),
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input,
                caller: None,
            }],
        }
    }

    fn tool_result(id: &str, content: &str) -> Message {
        Message {
            role: "user".to_string(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: content.to_string(),
                is_error: None,
                content_blocks: None,
            }],
        }
    }

    #[test]
    fn anchor_summary_section_is_empty_without_workspace_or_file() {
        assert!(anchor_summary_section(None).is_empty());

        let tmpdir = tempfile::TempDir::new().unwrap();
        assert!(anchor_summary_section(Some(tmpdir.path())).is_empty());
    }

    #[test]
    fn anchor_summary_section_parses_anchor_file_into_bullets() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let meta_dir = zagens_config::workspace_meta_dir(tmpdir.path());
        std::fs::create_dir_all(&meta_dir).unwrap();
        std::fs::write(
            meta_dir.join("anchors.md"),
            "\n---\nDo not touch .ssh\n---\nStatus field is unreliable\n",
        )
        .unwrap();

        let section = anchor_summary_section(Some(tmpdir.path()));

        assert!(section.contains("## Pinned Facts (User Anchors)"));
        assert!(section.contains("- Do not touch .ssh\n"));
        assert!(section.contains("- Status field is unreliable\n"));
        assert!(!section.contains("\n---\nDo not touch"));
    }

    #[test]
    fn truncate_chars_respects_unicode_boundaries() {
        let text = "abc😀é";
        assert_eq!(truncate_chars(text, 0), "");
        assert_eq!(truncate_chars(text, 1), "a");
        assert_eq!(truncate_chars(text, 3), "abc");
        assert_eq!(truncate_chars(text, 4), "abc😀");
        assert_eq!(truncate_chars(text, 5), "abc😀é");
    }

    #[test]
    fn prune_tool_results_summarizes_old_verbose_outputs() {
        let verbose = "x".repeat(SUMMARY_TOOL_RESULT_SNIPPET_CHARS + 80);
        let mut messages = vec![
            tool_use("call-1", "read_file", json!({"path": "Cargo.toml"})),
            tool_result("call-1", &verbose),
            msg("user", "recent question"),
            msg("assistant", "recent answer"),
        ];

        let saved = prune_tool_results(&mut messages, 2);

        assert!(saved > 0);
        let ContentBlock::ToolResult { content, .. } = &messages[1].content[0] else {
            panic!("expected tool result");
        };
        assert!(content.contains("[read_file] tool result pruned"));
        assert!(content.contains("Cargo.toml"));
        assert!(content.len() < verbose.len());
    }

    #[test]
    fn prune_tool_results_preserves_protected_tail() {
        let verbose = "x".repeat(SUMMARY_TOOL_RESULT_SNIPPET_CHARS + 80);
        let mut messages = vec![
            msg("user", "older context"),
            tool_use("call-1", "read_file", json!({"path": "Cargo.toml"})),
            tool_result("call-1", &verbose),
        ];

        let saved = prune_tool_results(&mut messages, 2);

        assert_eq!(saved, 0);
        let ContentBlock::ToolResult { content, .. } = &messages[2].content[0] else {
            panic!("expected tool result");
        };
        assert_eq!(content, &verbose);
    }

    #[test]
    fn prune_tool_results_dedupes_identical_reads_but_keeps_latest_full_body() {
        let first = "first ".repeat(80);
        let second = "second ".repeat(80);
        let mut messages = vec![
            tool_use("call-1", "read_file", json!({"path": "Cargo.toml"})),
            tool_result("call-1", &first),
            tool_use("call-2", "read_file", json!({"path": "Cargo.toml"})),
            tool_result("call-2", &second),
            msg("user", "tail"),
        ];

        let saved = prune_tool_results(&mut messages, 1);

        assert!(saved > 0);
        let ContentBlock::ToolResult { content: older, .. } = &messages[1].content[0] else {
            panic!("expected older tool result");
        };
        assert!(older.contains("tool result pruned"));
        let ContentBlock::ToolResult {
            content: latest, ..
        } = &messages[3].content[0]
        else {
            panic!("expected latest tool result");
        };
        assert_eq!(latest, &second);
    }

    #[test]
    fn is_transient_error_detects_network_issues() {
        let timeout_err = anyhow::anyhow!("Connection timeout");
        assert!(is_transient_error(&timeout_err));

        let rate_limit_err = anyhow::anyhow!("429 Too Many Requests");
        assert!(is_transient_error(&rate_limit_err));

        let service_err = anyhow::anyhow!("503 Service Unavailable");
        assert!(is_transient_error(&service_err));

        let network_err = anyhow::anyhow!("network error: connection refused");
        assert!(is_transient_error(&network_err));
    }

    #[test]
    fn is_transient_error_rejects_permanent_errors() {
        let auth_err = anyhow::anyhow!("401 Unauthorized: Invalid API key");
        assert!(!is_transient_error(&auth_err));

        let parse_err = anyhow::anyhow!("Failed to parse JSON response");
        assert!(!is_transient_error(&parse_err));

        let validation_err = anyhow::anyhow!("Invalid request: missing required field");
        assert!(!is_transient_error(&validation_err));
    }

    #[test]
    fn summary_limits_expand_for_v4_context() {
        let legacy = summary_input_limits_for_model("deepseek-v3.2-128k");
        let v4 = summary_input_limits_for_model("deepseek-v4-pro");

        assert!(v4.input_max_chars > legacy.input_max_chars);
        assert!(v4.tool_result_snippet_chars > legacy.tool_result_snippet_chars);
        assert!(v4.max_tokens > legacy.max_tokens);
    }

    #[test]
    fn cache_aligned_summary_is_used_for_v4_scale_contexts() {
        let messages = vec![msg("user", "Please edit crates/tui/src/compaction.rs")];

        assert!(should_use_cache_aligned_summary(
            "deepseek-v4-flash",
            &messages
        ));
        assert!(!should_use_cache_aligned_summary(
            "deepseek-v3.2-128k",
            &messages
        ));
    }

    /// #584: the summary cache-hit percentage must be computed against
    /// `input_tokens`, not `cache_hit + cache_miss`. Providers that
    /// only populate `prompt_cache_hit_tokens` (and leave the miss
    /// field at `None`) would otherwise be reported as a flat 100%
    /// hit rate even when most of the prompt was uncached.
    #[test]
    fn summary_cache_hit_percent_uses_input_tokens_as_denominator() {
        // Both fields populated and consistent.
        assert!((summary_cache_hit_percent(800, 1000) - 80.0).abs() < f64::EPSILON);
        // No cache hit at all.
        assert!((summary_cache_hit_percent(0, 1000) - 0.0).abs() < f64::EPSILON);
        // Full cache hit.
        assert!((summary_cache_hit_percent(1000, 1000) - 100.0).abs() < f64::EPSILON);
        // Partial-telemetry guard: provider reports `cache_hit` only,
        // miss is unknown (treated as 0 by the caller). Naive
        // `hit / (hit + miss)` would have reported 100%; against
        // `input_tokens` the answer is the real share.
        assert!((summary_cache_hit_percent(200, 1000) - 20.0).abs() < f64::EPSILON);
        // Defensive: zero `input_tokens` short-circuits without a
        // divide-by-zero.
        assert!((summary_cache_hit_percent(0, 0) - 0.0).abs() < f64::EPSILON);
        assert!((summary_cache_hit_percent(50, 0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cache_aligned_summary_request_preserves_message_prefix() {
        let messages = vec![
            msg("user", "Please edit crates/tui/src/compaction.rs"),
            msg("assistant", "I will inspect the file."),
        ];
        let limits = summary_input_limits_for_model("deepseek-v4-pro");
        let request = build_cache_aligned_summary_request("deepseek-v4-pro", &messages, limits);

        assert_eq!(request.system, None);
        assert_eq!(&request.messages[..messages.len()], &messages[..]);
        assert_eq!(request.messages.len(), messages.len() + 1);
        let last = request.messages.last().expect("summary instruction");
        assert_eq!(last.role, "user");
        assert!(matches!(
            &last.content[..],
            [ContentBlock::Text { text, .. }] if text.contains("conversation above")
        ));
    }

    #[test]
    fn estimate_tokens_empty_messages() {
        let messages: Vec<Message> = vec![];
        assert_eq!(estimate_tokens(&messages), 0);
    }

    #[test]
    fn estimate_tokens_with_text() {
        let messages = vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "Hello, world!".to_string(), // 13 chars = ~3 tokens
                cache_control: None,
            }],
        }];
        let tokens = estimate_tokens(&messages);
        assert!(tokens > 0 && tokens < 10);
    }

    #[test]
    fn estimate_tokens_counts_tool_round_thinking_across_turns() {
        // Per DeepSeek thinking-mode rules, any assistant message that
        // performed a tool call keeps its reasoning_content in the request
        // forever, including across new user turns. Token estimates must
        // count those bytes.
        let thinking = "reasoning ".repeat(800);
        let current_messages = vec![
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: "Use a tool".to_string(),
                    cache_control: None,
                }],
            },
            Message {
                role: "assistant".to_string(),
                content: vec![
                    ContentBlock::Thinking {
                        thinking: thinking.clone(),
                    },
                    ContentBlock::ToolUse {
                        id: "tool-1".to_string(),
                        name: "read_file".to_string(),
                        input: serde_json::json!({"path": "Cargo.toml"}),
                        caller: None,
                    },
                ],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tool-1".to_string(),
                    content: "manifest".to_string(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
        ];
        let historical_messages = {
            let mut messages = current_messages.clone();
            messages.push(Message {
                role: "assistant".to_string(),
                content: vec![ContentBlock::Text {
                    text: "Done.".to_string(),
                    cache_control: None,
                }],
            });
            messages.push(Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: "Next question.".to_string(),
                    cache_control: None,
                }],
            });
            messages
        };
        let completed_messages = {
            let mut messages = current_messages.clone();
            messages.push(Message {
                role: "assistant".to_string(),
                content: vec![ContentBlock::Text {
                    text: "Done.".to_string(),
                    cache_control: None,
                }],
            });
            messages
        };

        let lower_bound = thinking.len() / 5;
        assert!(estimate_tokens(&current_messages) > lower_bound);
        assert!(estimate_tokens(&completed_messages) > lower_bound);
        assert!(estimate_tokens(&historical_messages) > lower_bound);
    }

    #[test]
    fn unified_calibration_core_and_compaction_agree() {
        // M0.3 acceptance: UI usage / capacity ratio (core estimator) and the
        // compaction threshold (this module's estimator) must read the same
        // number for the same content now that both walk the single
        // `estimate_text_tokens` entry. Thinking blocks are excluded here:
        // the two walkers intentionally differ on historical reasoning
        // (DeepSeek replay semantics), which is message-shape semantics, not
        // text calibration.
        let messages = vec![
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: "Summarize the design doc, 中英混排内容若干。".to_string(),
                    cache_control: None,
                }],
            },
            Message {
                role: "assistant".to_string(),
                content: vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "read_file".to_string(),
                    input: serde_json::json!({"path": "docs/design.md"}),
                    caller: None,
                }],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tool-1".to_string(),
                    content: "# design\n架构说明 body ".repeat(40),
                    is_error: None,
                    content_blocks: None,
                }],
            },
        ];
        for system in [
            None,
            Some(SystemPrompt::Text("static prompt 静态层内容".to_string())),
        ] {
            assert_eq!(
                zagens_core::engine::estimate_input_tokens_conservative(
                    &messages,
                    system.as_ref()
                ),
                estimate_input_tokens_conservative(&messages, system.as_ref()),
                "core and compaction estimators diverged (system={:?})",
                system.is_some()
            );
        }
    }

    #[test]
    fn should_compact_respects_enabled_flag() {
        let config = CompactionConfig {
            enabled: false,
            ..Default::default()
        };
        // Even with many messages, disabled compaction should return false
        let messages: Vec<Message> = (0..100)
            .map(|_| Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: "test".to_string(),
                    cache_control: None,
                }],
            })
            .collect();
        assert!(!should_compact(&messages, &config, None, None, None));
    }

    /// v0.8.11: message-count is no longer a compaction trigger. Long
    /// chats of small messages stay uncompacted because rewriting the V4
    /// prefix cache for a tiny budget reclaim is net-negative. Only token
    /// pressure (and the explicit `/compact` slash command) trigger
    /// compaction.
    #[test]
    fn message_count_no_longer_triggers_compaction() {
        let config = CompactionConfig {
            enabled: true,
            token_threshold: 1_000_000,
            auto_floor_tokens: 0,
            ..Default::default()
        };

        // 200 tiny messages, well above the prior message threshold.
        let many_messages: Vec<Message> = (0..200)
            .map(|_| Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: "x".to_string(),
                    cache_control: None,
                }],
            })
            .collect();
        // Token total stays minuscule so the token threshold is not hit;
        // without the prior message-count trigger, no compaction.
        assert!(!should_compact(&many_messages, &config, None, None, None));
    }

    #[test]
    fn plan_compaction_pins_recent_and_working_set_paths() {
        let messages = vec![
            msg("user", "General discussion"),
            msg("assistant", "Unrelated note"),
            msg("user", "Earlier we touched src/core/engine.rs"),
            msg("assistant", "More unrelated chatter"),
            msg("user", "Let's keep working on src/core/engine.rs"),
            msg("assistant", "Tool output mentions src/core/engine.rs too"),
            msg("assistant", "Recent reasoning"),
            msg("user", "Final recent instruction"),
        ];

        let plan = plan_compaction(&messages, None, KEEP_RECENT_MESSAGES, None, None);

        assert!(plan.pinned_indices.contains(&2));
        for idx in 4..messages.len() {
            assert!(plan.pinned_indices.contains(&idx));
        }
        assert!(plan.summarize_indices.contains(&0));
        assert!(plan.summarize_indices.contains(&1));
        assert!(plan.summarize_indices.contains(&3));
    }

    #[test]
    fn plan_compaction_respects_external_pins() {
        let messages = vec![
            msg("user", "noise 0"),
            msg("assistant", "noise 1"),
            msg("user", "noise 2"),
            msg("assistant", "noise 3"),
            msg("user", "recent 4"),
            msg("assistant", "recent 5"),
            msg("assistant", "recent 6"),
            msg("user", "recent 7"),
        ];

        let pins = vec![1usize];
        let plan = plan_compaction(&messages, None, KEEP_RECENT_MESSAGES, Some(&pins), None);

        assert!(plan.pinned_indices.contains(&1));
        assert!(!plan.summarize_indices.contains(&1));
    }

    #[test]
    fn plan_compaction_context_partition_hot_cold() {
        use zagens_core::context_partition::{MessageContextTier, message_has_external_ref};

        let messages = vec![
            msg("user", "summarize me"),
            msg(
                "tool",
                "[workshop-ref: {\"ref_id\":\"lout_test\"}]\n[workshop-synthesis: tool=read_file]\n\nsummary only",
            ),
            msg("user", "recent tail"),
        ];
        let plan = plan_compaction(&messages, None, 1, None, None);
        let partition = plan.context_partition(&messages, 1);

        assert_eq!(partition.hot.message_indices, vec![2]);
        assert_eq!(partition.tiers[2], MessageContextTier::Hot);
        assert_eq!(partition.cold.external_ref_indices, vec![1]);
        assert_eq!(partition.tiers[1], MessageContextTier::ColdExternalRef);
        assert!(message_has_external_ref(&message_text(&messages[1])));
    }

    /// A1-MVP.2 — working-set pins survive LLM compaction (pin not summarized away).
    #[tokio::test]
    async fn compact_messages_preserves_working_set_pinned_message() {
        use zagens_core::working_set::WorkingSet;
        use tempfile::tempdir;

        use crate::llm_client::mock::MockLlmClient;
        use crate::models::{MessageResponse, Usage};

        const MARKER: &str = "A1_MVP2_WORKING_SET_PIN_SURVIVES";
        let dir = tempdir().expect("tempdir");
        let workspace = dir.path();
        std::fs::create_dir_all(workspace.join("src/core")).expect("mkdir");

        let mut messages = vec![
            msg("user", "old noise to summarize"),
            msg(
                "assistant",
                &format!("please patch src/core/engine.rs — {MARKER}"),
            ),
        ];
        messages.extend((2..14).map(|i| msg("user", &format!("filler {i}"))));

        let mut ws = WorkingSet::default();
        ws.observe_user_message(
            &format!("working on src/core/engine.rs — {MARKER}"),
            workspace,
        );
        let pins = ws.pinned_message_indices(&messages, workspace);
        let paths = ws.top_paths(24);
        assert!(
            !pins.is_empty(),
            "working set should pin the path mention at index 1"
        );

        let mock = MockLlmClient::new(vec![]);
        mock.push_message_response(MessageResponse {
            id: "msg_summary".into(),
            r#type: "message".into(),
            role: "assistant".into(),
            content: vec![ContentBlock::Text {
                text: "condensed summary".into(),
                cache_control: None,
            }],
            model: "mock".into(),
            stop_reason: Some("end_turn".into()),
            stop_sequence: None,
            container: None,
            usage: Usage::default(),
        });

        let config = CompactionConfig {
            enabled: true,
            token_threshold: 1,
            ..Default::default()
        };

        let (pinned_messages, summary_prompt, removed, _artifact) = compact_messages(
            &mock,
            &messages,
            &config,
            Some(workspace),
            Some(&pins),
            Some(&paths),
        )
        .await
        .expect("compact_messages");

        assert!(summary_prompt.is_some(), "expected LLM summary block");
        assert!(!removed.is_empty(), "expected summarized messages");
        let preserved = pinned_messages.iter().any(|message| {
            message.content.iter().any(|block| {
                matches!(block, ContentBlock::Text { text, .. } if text.contains(MARKER))
            })
        });
        assert!(
            preserved,
            "working-set pinned message must survive compaction"
        );
    }

    /// A1.4 — after compaction, pinned user-visible text remains in the session message list.
    #[tokio::test]
    async fn compact_messages_safe_preserves_pinned_text_in_result_messages() {
        use crate::llm_client::mock::MockLlmClient;
        use crate::models::{MessageResponse, Usage};

        const PINNED: &str = "A1_4_PINNED_USER_VISIBLE";
        let mut messages = vec![
            msg("user", "old context to drop"),
            msg("user", PINNED),
            msg("assistant", "ack"),
        ];
        for i in 0..12 {
            messages.push(msg("user", &format!("filler {i}")));
        }

        let mock = MockLlmClient::new(vec![]);
        mock.push_message_response(MessageResponse {
            id: "msg_summary".into(),
            r#type: "message".into(),
            role: "assistant".into(),
            content: vec![ContentBlock::Text {
                text: "summary only".into(),
                cache_control: None,
            }],
            model: "mock".into(),
            stop_reason: Some("end_turn".into()),
            stop_sequence: None,
            container: None,
            usage: Usage::default(),
        });

        let config = CompactionConfig {
            enabled: true,
            token_threshold: 1,
            ..Default::default()
        };
        let pins = vec![1usize];
        let result = compact_messages_safe(
            &mock,
            &messages,
            &config,
            None,
            Some(&pins),
            None,
        )
        .await
        .expect("compact_messages_safe");

        let pinned_still_present = result.messages.iter().any(|message| {
            message.content.iter().any(|block| {
                matches!(block, ContentBlock::Text { text, .. } if text.contains(PINNED))
            })
        });
        assert!(
            pinned_still_present,
            "compacted session must retain pinned message text (A1.4 isomorphism)"
        );
        assert!(
            crate::transcript_isomorphism::history_transcript_core_matches_messages(
                &result.messages
            ),
            "compacted messages must round-trip through TUI history rebuild (A1.4)"
        );
    }

    #[test]
    fn plan_compaction_uses_external_working_set_paths() {
        let mut messages = vec![msg("user", "edit src/core/engine.rs now")];
        messages.extend((1..20).map(|i| msg("assistant", &format!("noise {i}"))));

        let working_set_paths = vec!["src/core/engine.rs".to_string()];
        let plan = plan_compaction(
            &messages,
            None,
            KEEP_RECENT_MESSAGES,
            None,
            Some(&working_set_paths),
        );

        assert!(plan.pinned_indices.contains(&0));
    }

    #[test]
    fn plan_compaction_pins_tool_calls_for_tool_results() {
        let messages = vec![
            msg("user", "noise"),
            Message {
                role: "assistant".to_string(),
                content: vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({"path": "src/main.rs"}),
                    caller: None,
                }],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tool-1".to_string(),
                    content: "ok src/main.rs".to_string(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
        ];

        let plan = plan_compaction(&messages, None, 1, None, None);
        assert!(plan.pinned_indices.contains(&2));
        assert!(plan.pinned_indices.contains(&1));
    }

    #[test]
    fn should_compact_ignores_fully_pinned_context() {
        let config = CompactionConfig {
            enabled: true,
            token_threshold: 10,
            ..Default::default()
        };

        let messages: Vec<Message> = (0..12)
            .map(|_| msg("user", "Work on src/compaction.rs right now"))
            .collect();

        assert!(!should_compact(&messages, &config, None, None, None));
    }

    // v0.8.11: removed `should_compact_counts_only_unpinned_messages` and
    // `should_compact_when_pins_consume_budget` — both tested the
    // message-count compaction trigger that v0.8.11 deleted. The
    // pinned-tokens accounting they exercised is still tested by
    // `should_compact_ignores_fully_pinned_context` below; the rest of
    // their setup has no contemporary contract to pin.

    #[test]
    fn enforce_tool_call_pairs_removes_orphaned_tool_call() {
        // An assistant message with a tool call but no matching result anywhere
        // in the history should be removed from the pinned set.
        let messages = vec![
            msg("user", "noise"),
            Message {
                role: "assistant".to_string(),
                content: vec![ContentBlock::ToolUse {
                    id: "orphan-call".to_string(),
                    name: "read_file".to_string(),
                    input: json!({"path": "src/main.rs"}),
                    caller: None,
                }],
            },
            msg("assistant", "recent"),
        ];

        let mut pinned = BTreeSet::from([0, 1, 2]);
        enforce_tool_call_pairs(&messages, &mut pinned);

        // The orphaned tool call message (index 1) should be removed.
        assert!(
            !pinned.contains(&1),
            "orphaned tool call should be removed from pinned set"
        );
        // Other messages stay.
        assert!(pinned.contains(&0));
        assert!(pinned.contains(&2));
    }

    #[test]
    fn enforce_tool_call_pairs_removes_orphaned_tool_result() {
        // A tool result whose call doesn't exist anywhere should be removed.
        let messages = vec![
            msg("user", "noise"),
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "orphan-result".to_string(),
                    content: "ok".to_string(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
            msg("assistant", "recent"),
        ];

        let mut pinned = BTreeSet::from([0, 1, 2]);
        enforce_tool_call_pairs(&messages, &mut pinned);

        assert!(
            !pinned.contains(&1),
            "orphaned tool result should be removed from pinned set"
        );
        assert!(pinned.contains(&0));
        assert!(pinned.contains(&2));
    }

    #[test]
    fn enforce_tool_call_pairs_preserves_valid_pairs() {
        // A complete call+result pair should remain intact.
        let messages = vec![
            msg("user", "do something"),
            Message {
                role: "assistant".to_string(),
                content: vec![ContentBlock::ToolUse {
                    id: "tool-ok".to_string(),
                    name: "list_dir".to_string(),
                    input: json!({}),
                    caller: None,
                }],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tool-ok".to_string(),
                    content: "files here".to_string(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
            msg("assistant", "done"),
        ];

        let mut pinned = BTreeSet::from([1, 2, 3]);
        enforce_tool_call_pairs(&messages, &mut pinned);

        assert!(pinned.contains(&1), "tool call should stay pinned");
        assert!(pinned.contains(&2), "tool result should stay pinned");
        assert!(pinned.contains(&3));
    }

    #[test]
    fn enforce_tool_call_pairs_pins_transitive_pairs() {
        // If only the result is initially pinned, the call should be pulled in.
        // The call message may also contain another tool call whose result should
        // then be pulled in transitively.
        let messages = vec![
            msg("user", "start"),
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
                ],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t1".to_string(),
                    content: "content of a.rs".to_string(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "t2".to_string(),
                    content: "content of b.rs".to_string(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
            msg("assistant", "done"),
        ];

        // Only pin the result for t1 initially.
        let mut pinned = BTreeSet::from([2, 4]);
        enforce_tool_call_pairs(&messages, &mut pinned);

        // The call message (index 1) should be pulled in because t1's result is pinned.
        assert!(
            pinned.contains(&1),
            "call message should be transitively pinned"
        );
        // Since the call message also contains t2, t2's result (index 3) should also be pinned.
        assert!(
            pinned.contains(&3),
            "t2 result should be transitively pinned via the call message"
        );
    }

    #[test]
    fn enforce_tool_call_pairs_cascading_removal() {
        // Removing an orphaned call should cascade to remove its result.
        // Message 1: assistant with t1 (call) — t1 has a result at index 2
        // Message 2: user with t1 (result)
        // Message 3: assistant with t2 (call) — t2 has NO result
        // Message 4: user with t2 result referencing the call
        //
        // If t2 has no result in history, message 3 is removed. That's straightforward.
        // Here we test: if a call message is removed because ONE of its calls is orphaned,
        // the result for the other call also gets removed in subsequent iterations.
        let messages = vec![
            msg("user", "start"),
            Message {
                role: "assistant".to_string(),
                content: vec![
                    ContentBlock::ToolUse {
                        id: "good".to_string(),
                        name: "read_file".to_string(),
                        input: json!({}),
                        caller: None,
                    },
                    ContentBlock::ToolUse {
                        id: "orphan".to_string(),
                        name: "shell".to_string(),
                        input: json!({}),
                        caller: None,
                    },
                ],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "good".to_string(),
                    content: "ok".to_string(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
            // Note: NO result for "orphan" exists anywhere
            msg("assistant", "done"),
        ];

        let mut pinned = BTreeSet::from([1, 2, 3]);
        enforce_tool_call_pairs(&messages, &mut pinned);

        // Message 1 has an orphaned tool call ("orphan"), so it's removed.
        assert!(
            !pinned.contains(&1),
            "message with orphaned call should be removed"
        );
        // Message 2 (result for "good") now has no matching call pinned, so it's also removed.
        assert!(
            !pinned.contains(&2),
            "result whose call was removed should cascade-remove"
        );
        // Message 3 (plain text) stays.
        assert!(pinned.contains(&3));
    }

    #[test]
    fn enforce_tool_call_pairs_converges_long_chain() {
        let mut messages = vec![msg("user", "start")];
        for i in 0..15 {
            messages.push(Message {
                role: "assistant".to_string(),
                content: vec![ContentBlock::ToolUse {
                    id: format!("t{i}"),
                    name: "read_file".to_string(),
                    input: json!({}),
                    caller: None,
                }],
            });
            messages.push(Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: format!("t{i}"),
                    content: format!("result {i}"),
                    is_error: None,
                    content_blocks: None,
                }],
            });
        }
        messages.push(msg("assistant", "done"));

        let mut pinned: BTreeSet<usize> = (0..messages.len()).collect();
        enforce_tool_call_pairs(&messages, &mut pinned);

        // All pairs should remain intact (no orphans)
        assert_eq!(pinned.len(), messages.len());
    }

    // ========================================================================
    // Additional Compaction Trigger Tests
    // ========================================================================

    #[test]
    fn test_should_compact_token_threshold_triggers() {
        let config = CompactionConfig {
            enabled: true,
            token_threshold: 100, // Low threshold for testing
            auto_floor_tokens: 0,
            ..Default::default()
        };

        // Create messages that exceed token threshold
        let messages: Vec<Message> = (0..10)
            .map(|_| msg("user", &"x".repeat(50))) // 50 chars = ~12 tokens each
            .collect();

        // Total tokens: ~120, which exceeds 100
        assert!(should_compact(&messages, &config, None, None, None));
    }

    #[test]
    fn test_should_compact_below_token_threshold() {
        let config = CompactionConfig {
            enabled: true,
            token_threshold: 1000,
            ..Default::default()
        };

        // Create short messages
        let messages: Vec<Message> = (0..5).map(|_| msg("user", "short")).collect();

        assert!(!should_compact(&messages, &config, None, None, None));
    }

    /// v0.8.11: the 500K hard floor blocks auto-compaction even when the
    /// token-percentage threshold would otherwise fire. This is the V4
    /// prefix-cache protection — below 500K total tokens, rewriting the
    /// prefix loses cache for tiny budget gains.
    #[test]
    fn auto_compaction_floor_blocks_below_500k_even_when_threshold_says_yes() {
        let config = CompactionConfig {
            enabled: true,
            token_threshold: 100, // would normally fire instantly
            // Use the production default explicitly so this test pins the
            // floor's contract rather than relying on `Default`.
            auto_floor_tokens: MINIMUM_AUTO_COMPACTION_TOKENS,
            ..Default::default()
        };

        let messages: Vec<Message> = (0..10).map(|_| msg("user", &"x".repeat(50))).collect();
        // Total tokens way under 500K, so floor blocks compaction.
        assert!(!should_compact(&messages, &config, None, None, None));
    }

    /// v0.8.11: when total tokens cross the 500K floor, the existing
    /// threshold/message-count logic takes over again.
    #[test]
    fn auto_compaction_floor_yields_to_threshold_logic_above_500k() {
        let config = CompactionConfig {
            enabled: true,
            token_threshold: 2_000_000,
            auto_floor_tokens: MINIMUM_AUTO_COMPACTION_TOKENS,
            ..Default::default()
        };

        // Each message ~500 tokens; 1100 messages → ~550K total tokens.
        // That's above the floor (500K) AND below the deliberately high
        // token_threshold, so auto-compaction stays off — by threshold,
        // not floor.
        let messages: Vec<Message> = (0..1100).map(|_| msg("user", &"x".repeat(2000))).collect();
        assert!(!should_compact(&messages, &config, None, None, None));

        // Crank threshold below total → compaction fires now that we're
        // past the floor.
        let config_lower = CompactionConfig {
            token_threshold: 100_000,
            ..config
        };
        assert!(should_compact(&messages, &config_lower, None, None, None));
    }

    /// `CompactionConfig::default()` ships with the 500K floor on by
    /// default — production callers via `..Default::default()` get the
    /// safety guarantee automatically.
    #[test]
    fn compaction_config_default_carries_500k_floor() {
        let config = CompactionConfig::default();
        assert_eq!(config.auto_floor_tokens, MINIMUM_AUTO_COMPACTION_TOKENS);
        assert_eq!(config.auto_floor_tokens, 500_000);
    }

    #[test]
    fn test_plan_compaction_pins_error_messages() {
        let messages = vec![
            msg("user", "normal message"),
            msg("assistant", "error: compilation failed"),
            msg("user", "another message"),
            msg("assistant", "panic at src/main.rs:42"),
            msg("user", "more chat"),
            msg("assistant", "Traceback (most recent call last):"),
            msg("user", "recent 1"),
            msg("assistant", "recent 2"),
        ];

        let plan = plan_compaction(&messages, None, KEEP_RECENT_MESSAGES, None, None);

        // Error messages should be pinned
        assert!(plan.pinned_indices.contains(&1)); // error:
        assert!(plan.pinned_indices.contains(&3)); // panic
        assert!(plan.pinned_indices.contains(&5)); // traceback
    }

    #[test]
    fn test_plan_compaction_pins_patch_messages() {
        let messages = vec![
            msg("user", "normal chat"),
            msg("assistant", "diff --git a/src/main.rs b/src/main.rs"),
            msg("user", "more chat"),
            msg("assistant", "+++ b/src/core.rs"),
            msg("user", "chat"),
            msg("assistant", "```diff\n-some code\n+new code\n```"),
            msg("user", "recent 1"),
            msg("assistant", "recent 2"),
        ];

        let plan = plan_compaction(&messages, None, KEEP_RECENT_MESSAGES, None, None);

        // Patch/diff messages should be pinned
        assert!(plan.pinned_indices.contains(&1)); // diff --git
        assert!(plan.pinned_indices.contains(&3)); // +++ b/
        assert!(plan.pinned_indices.contains(&5)); // ```diff
    }

    #[test]
    fn test_plan_compaction_pins_apply_patch_tool_calls() {
        let messages = vec![
            msg("user", "normal chat"),
            Message {
                role: "assistant".to_string(),
                content: vec![ContentBlock::ToolUse {
                    id: "patch-1".to_string(),
                    name: "apply_patch".to_string(),
                    input: json!({"patch": "diff content"}),
                    caller: None,
                }],
            },
            Message {
                role: "user".to_string(),
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "patch-1".to_string(),
                    content: "Patch applied successfully".to_string(),
                    is_error: None,
                    content_blocks: None,
                }],
            },
            msg("assistant", "more chat"),
            msg("user", "even more"),
            msg("assistant", "recent 1"),
            msg("user", "recent 2"),
            msg("assistant", "recent 3"),
        ];

        let plan = plan_compaction(&messages, None, KEEP_RECENT_MESSAGES, None, None);

        // Message 1 contains apply_patch tool call with matching result (message 2)
        // Both should be pinned due to tool call pairing
        // Messages 5, 6, 7, 8 are recent (last 4 messages)
        eprintln!("Pinned indices: {:?}", plan.pinned_indices);

        // apply_patch tool call and its result should be pinned
        assert!(
            plan.pinned_indices.contains(&1),
            "apply_patch tool call should be pinned"
        );
        assert!(
            plan.pinned_indices.contains(&2),
            "apply_patch tool result should be pinned"
        );
    }

    #[test]
    fn test_extract_paths_from_text_finds_various_formats() {
        let text = r#"
            I'm working on src/main.rs
            Also check Cargo.toml
            The error is in src/core/engine.rs:42
            See docs/API.md for details
            Config at config.example.toml
        "#;

        let paths = extract_paths_from_text(text, None);

        assert!(paths.iter().any(|p| p == "src/main.rs"));
        assert!(paths.iter().any(|p| p == "Cargo.toml"));
        assert!(paths.iter().any(|p| p == "src/core/engine.rs"));
        assert!(paths.iter().any(|p| p == "docs/API.md"));
        assert!(paths.iter().any(|p| p == "config.example.toml"));
    }

    #[test]
    fn test_extract_paths_from_tool_input_finds_path_field() {
        let input = json!({
            "path": "src/main.rs",
            "content": "test"
        });

        let paths = extract_paths_from_tool_input(&input, None);
        assert!(paths.iter().any(|p| p == "src/main.rs"));
    }

    #[test]
    fn test_extract_paths_from_tool_input_finds_paths_array() {
        let input = json!({
            "paths": ["src/main.rs", "src/core.rs", "tests/test.rs"]
        });

        let paths = extract_paths_from_tool_input(&input, None);
        assert_eq!(paths.len(), 3);
        assert!(paths.iter().any(|p| p == "src/main.rs"));
        assert!(paths.iter().any(|p| p == "src/core.rs"));
        assert!(paths.iter().any(|p| p == "tests/test.rs"));
    }

    #[test]
    fn test_extract_paths_from_tool_input_finds_cwd() {
        let input = json!({
            "cwd": "src/core",
            "command": "cargo build"
        });

        let paths = extract_paths_from_tool_input(&input, None);
        assert!(paths.iter().any(|p| p == "src/core"));
    }

    #[test]
    fn test_normalize_path_candidate_handles_absolute_paths() {
        use std::env;
        let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // Create an absolute path
        let absolute_path = current_dir.join("src/main.rs");
        let absolute_path_str = absolute_path.to_string_lossy();

        let normalized = normalize_path_candidate(&absolute_path_str, Some(&current_dir));

        assert_eq!(normalized, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_normalize_path_candidate_rejects_parent_refs() {
        let normalized = normalize_path_candidate("../outside/file.rs", Some(&PathBuf::from(".")));
        assert_eq!(normalized, None);
    }

    #[test]
    fn test_normalize_path_candidate_cleans_backslashes() {
        let normalized = normalize_path_candidate("src\\main.rs", Some(&PathBuf::from(".")));
        assert_eq!(normalized, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_merge_system_prompts_none_none() {
        let result = merge_system_prompts(None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_merge_system_prompts_some_text_none() {
        let original = Some(SystemPrompt::Text("original".to_string()));
        let result = merge_system_prompts(original.as_ref(), None);
        assert!(matches!(result, Some(SystemPrompt::Text(s)) if s == "original"));
    }

    #[test]
    fn test_merge_system_prompts_none_some_blocks() {
        let summary = Some(SystemPrompt::Blocks(vec![SystemBlock {
            block_type: "text".to_string(),
            text: "summary".to_string(),
            cache_control: None,
        }]));
        let result = merge_system_prompts(None, summary);
        assert!(matches!(result, Some(SystemPrompt::Blocks(b)) if b.len() == 1));
    }

    #[test]
    fn test_merge_system_prompts_text_plus_blocks() {
        let original = Some(SystemPrompt::Text("original".to_string()));
        let summary = Some(SystemPrompt::Blocks(vec![SystemBlock {
            block_type: "text".to_string(),
            text: "summary".to_string(),
            cache_control: None,
        }]));

        let result = merge_system_prompts(original.as_ref(), summary);

        match result {
            Some(SystemPrompt::Blocks(blocks)) => {
                assert_eq!(blocks.len(), 2);
                assert!(matches!(&blocks[0], SystemBlock { text, .. } if text == "original"));
                assert!(matches!(&blocks[1], SystemBlock { text, .. } if text == "summary"));
            }
            _ => panic!("Expected Blocks"),
        }
    }

    #[test]
    fn test_merge_system_prompts_blocks_plus_blocks() {
        let original = Some(SystemPrompt::Blocks(vec![
            SystemBlock {
                block_type: "text".to_string(),
                text: "orig1".to_string(),
                cache_control: None,
            },
            SystemBlock {
                block_type: "text".to_string(),
                text: "orig2".to_string(),
                cache_control: None,
            },
        ]));

        let summary = Some(SystemPrompt::Blocks(vec![SystemBlock {
            block_type: "text".to_string(),
            text: "summary".to_string(),
            cache_control: None,
        }]));

        let result = merge_system_prompts(original.as_ref(), summary);

        match result {
            Some(SystemPrompt::Blocks(blocks)) => {
                assert_eq!(blocks.len(), 3);
                assert!(matches!(&blocks[0], SystemBlock { text, .. } if text == "orig1"));
                assert!(matches!(&blocks[1], SystemBlock { text, .. } if text == "orig2"));
                assert!(matches!(&blocks[2], SystemBlock { text, .. } if text == "summary"));
            }
            _ => panic!("Expected Blocks"),
        }
    }

    #[test]
    fn test_merge_system_prompts_blocks_plus_text() {
        let original = Some(SystemPrompt::Blocks(vec![SystemBlock {
            block_type: "text".to_string(),
            text: "original".to_string(),
            cache_control: None,
        }]));

        let summary = Some(SystemPrompt::Text("summary".to_string()));

        let result = merge_system_prompts(original.as_ref(), summary);

        match result {
            Some(SystemPrompt::Blocks(blocks)) => {
                assert_eq!(blocks.len(), 2);
                assert!(matches!(&blocks[0], SystemBlock { text, .. } if text == "original"));
                assert!(matches!(&blocks[1], SystemBlock { text, .. } if text == "summary"));
            }
            _ => panic!("Expected Blocks"),
        }
    }

    #[test]
    fn test_compaction_result_retries_used() {
        // This test verifies the CompactionResult structure
        let result = CompactionResult {
            messages: vec![],
            summary_prompt: None,
            removed_messages: vec![],
            retries_used: 2,
            artifact: None,
        };

        assert_eq!(result.retries_used, 2);
        assert!(result.messages.is_empty());
        assert!(result.removed_messages.is_empty());
    }

    #[test]
    fn test_should_compact_with_workspace_path_detection() {
        use std::env;
        let workspace = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        let _config = CompactionConfig {
            enabled: true,
            token_threshold: 1000,
            ..Default::default()
        };

        // Create messages mentioning workspace paths
        let messages = vec![
            msg("user", "working on src/main.rs"),
            msg("assistant", "noise 1"),
            msg("user", "noise 2"),
            msg("assistant", "noise 3"),
            msg("user", "noise 4"),
            msg("assistant", "noise 5"),
            msg("user", "recent 1"),
            msg("assistant", "recent 2"),
        ];

        // src/main.rs mention should pin message 0 in the plan.
        let plan = plan_compaction(
            &messages,
            Some(&workspace),
            KEEP_RECENT_MESSAGES,
            None,
            None,
        );
        assert!(plan.pinned_indices.contains(&0)); // src/main.rs mention
    }
