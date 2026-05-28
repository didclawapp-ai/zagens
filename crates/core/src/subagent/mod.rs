//! Sub-agent event types (runtime implementation in `deepseek-runtime/tools/subagent`).

pub mod mailbox;
pub mod types;

pub use mailbox::MailboxMessage;
pub use types::{
    AuditFindingItem, StructuredFindings, StructuredVerdict, SubAgentAssignment, SubAgentResult,
    SubAgentStatus, SubAgentType, VerdictItem, VerdictLevel,
};
