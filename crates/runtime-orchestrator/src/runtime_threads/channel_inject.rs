//! External channel event injection (P2): thread-level push → steer / queue / start-turn.

use anyhow::{Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::engine_host::RuntimeThreadHost;
use super::manager::RuntimeThreadManager;
use super::prompt_inbox::PromptDelivery;
use super::thread_crud::SUMMARY_LIMIT;
use super::thread_status::ThreadStreamStatus;
use super::{StartTurnRequest, SteerTurnRequest, summarize_text};

/// Max inbound channel text (32 KiB).
pub const CHANNEL_TEXT_MAX_CHARS: usize = 32_768;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelEventType {
    Message,
    Steer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChannelIfIdle {
    #[default]
    StartTurn,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelEventRequest {
    #[serde(rename = "type")]
    pub event_type: ChannelEventType,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub if_idle: ChannelIfIdle,
    #[serde(default)]
    pub force_steer: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelEventAction {
    Started,
    Steered,
    Queued,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelEventResponse {
    pub action: ChannelEventAction,
    pub thread_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub admitted: Option<super::PromptAdmission>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelDispatchPlan {
    StartTurn { queue: bool },
    Steer,
    RejectIdle,
}

pub fn validate_channel_text(text: &str) -> Result<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        bail!("text is required");
    }
    if trimmed.chars().count() > CHANNEL_TEXT_MAX_CHARS {
        bail!("text exceeds {CHANNEL_TEXT_MAX_CHARS} characters");
    }
    Ok(trimmed.to_string())
}

/// Pure dispatch planner (unit-tested; mirrors TUI `submit_disposition`).
pub fn plan_channel_dispatch(
    event_type: ChannelEventType,
    has_active_turn: bool,
    assistant_streaming: bool,
    force_steer: bool,
    if_idle: ChannelIfIdle,
) -> Result<ChannelDispatchPlan> {
    if !has_active_turn {
        return match event_type {
            ChannelEventType::Message => Ok(ChannelDispatchPlan::StartTurn { queue: false }),
            ChannelEventType::Steer => match if_idle {
                ChannelIfIdle::StartTurn => Ok(ChannelDispatchPlan::StartTurn { queue: false }),
                ChannelIfIdle::Reject => Ok(ChannelDispatchPlan::RejectIdle),
            },
        };
    }

    match event_type {
        ChannelEventType::Steer => Ok(ChannelDispatchPlan::Steer),
        ChannelEventType::Message => {
            if assistant_streaming && !force_steer {
                Ok(ChannelDispatchPlan::StartTurn { queue: true })
            } else {
                Ok(ChannelDispatchPlan::Steer)
            }
        }
    }
}

pub async fn inject_channel_event<P, R, H>(
    mgr: &RuntimeThreadManager<P, R>,
    host: &H,
    thread_id: &str,
    req: ChannelEventRequest,
) -> Result<ChannelEventResponse>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
    H: RuntimeThreadHost<P, R> + 'static,
{
    let text = validate_channel_text(&req.text)?;

    let thread = mgr.get_thread(thread_id).await?;
    if thread.archived {
        bail!("thread {thread_id} is archived");
    }

    let active_turn_id = {
        let active = mgr.active.lock().await;
        active
            .engines
            .get(thread_id)
            .and_then(|st| st.active_turn.as_ref().map(|t| t.turn_id.clone()))
    };

    let assistant_streaming = mgr
        .thread_status
        .list()
        .await
        .into_iter()
        .find(|(id, _)| id == thread_id)
        .is_some_and(|(_, entry)| entry.status == ThreadStreamStatus::Streaming);

    let plan = plan_channel_dispatch(
        req.event_type,
        active_turn_id.is_some(),
        assistant_streaming,
        req.force_steer,
        req.if_idle,
    )?;

    let (response, audit_turn_id) = match plan {
        ChannelDispatchPlan::RejectIdle => {
            bail!("no active turn to steer; use type=message or if_idle=start_turn");
        }
        ChannelDispatchPlan::StartTurn { queue } => {
            let mut start_req = StartTurnRequest {
                prompt: text.clone(),
                ..Default::default()
            };
            if queue {
                start_req.delivery = Some(PromptDelivery::Queue);
            }
            let outcome =
                super::turn_lifecycle::start_turn(mgr, host, thread_id, start_req).await?;
            if let Some(admitted) = outcome.queued {
                (
                    ChannelEventResponse {
                        action: ChannelEventAction::Queued,
                        thread_id: thread_id.to_string(),
                        turn_id: Some(outcome.turn.id.clone()),
                        admitted: Some(admitted),
                    },
                    Some(outcome.turn.id),
                )
            } else {
                (
                    ChannelEventResponse {
                        action: ChannelEventAction::Started,
                        thread_id: thread_id.to_string(),
                        turn_id: Some(outcome.turn.id.clone()),
                        admitted: None,
                    },
                    Some(outcome.turn.id),
                )
            }
        }
        ChannelDispatchPlan::Steer => {
            let turn_id = active_turn_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("no active turn to steer"))?;
            let turn = mgr
                .steer_turn(
                    thread_id,
                    turn_id,
                    SteerTurnRequest {
                        prompt: text.clone(),
                    },
                )
                .await?;
            (
                ChannelEventResponse {
                    action: ChannelEventAction::Steered,
                    thread_id: thread_id.to_string(),
                    turn_id: Some(turn.id.clone()),
                    admitted: None,
                },
                Some(turn.id),
            )
        }
    };

    let preview = summarize_text(&text, SUMMARY_LIMIT);
    mgr.emit_event(
        thread_id,
        audit_turn_id.as_deref(),
        None,
        "channel.injected",
        json!({
            "source": req.source,
            "type": match req.event_type {
                ChannelEventType::Message => "message",
                ChannelEventType::Steer => "steer",
            },
            "action": match response.action {
                ChannelEventAction::Started => "started",
                ChannelEventAction::Steered => "steered",
                ChannelEventAction::Queued => "queued",
            },
            "text_preview": preview,
        }),
    )
    .await?;

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_message_starts_turn() {
        assert_eq!(
            plan_channel_dispatch(
                ChannelEventType::Message,
                false,
                false,
                false,
                ChannelIfIdle::StartTurn,
            )
            .unwrap(),
            ChannelDispatchPlan::StartTurn { queue: false }
        );
    }

    #[test]
    fn idle_steer_reject() {
        assert_eq!(
            plan_channel_dispatch(
                ChannelEventType::Steer,
                false,
                false,
                false,
                ChannelIfIdle::Reject,
            )
            .unwrap(),
            ChannelDispatchPlan::RejectIdle
        );
    }

    #[test]
    fn idle_steer_start_turn() {
        assert_eq!(
            plan_channel_dispatch(
                ChannelEventType::Steer,
                false,
                false,
                false,
                ChannelIfIdle::StartTurn,
            )
            .unwrap(),
            ChannelDispatchPlan::StartTurn { queue: false }
        );
    }

    #[test]
    fn active_streaming_queues_message() {
        assert_eq!(
            plan_channel_dispatch(
                ChannelEventType::Message,
                true,
                true,
                false,
                ChannelIfIdle::StartTurn,
            )
            .unwrap(),
            ChannelDispatchPlan::StartTurn { queue: true }
        );
    }

    #[test]
    fn active_streaming_force_steer() {
        assert_eq!(
            plan_channel_dispatch(
                ChannelEventType::Message,
                true,
                true,
                true,
                ChannelIfIdle::StartTurn,
            )
            .unwrap(),
            ChannelDispatchPlan::Steer
        );
    }

    #[test]
    fn active_tool_wait_steer_message() {
        assert_eq!(
            plan_channel_dispatch(
                ChannelEventType::Message,
                true,
                false,
                false,
                ChannelIfIdle::StartTurn,
            )
            .unwrap(),
            ChannelDispatchPlan::Steer
        );
    }

    #[test]
    fn active_steer_type_always_steer() {
        assert_eq!(
            plan_channel_dispatch(
                ChannelEventType::Steer,
                true,
                true,
                false,
                ChannelIfIdle::Reject,
            )
            .unwrap(),
            ChannelDispatchPlan::Steer
        );
    }

    #[test]
    fn validate_rejects_empty_and_long() {
        assert!(validate_channel_text("  ").is_err());
        let long = "x".repeat(CHANNEL_TEXT_MAX_CHARS + 1);
        assert!(validate_channel_text(&long).is_err());
        assert_eq!(validate_channel_text("  hi  ").unwrap(), "hi");
    }
}
