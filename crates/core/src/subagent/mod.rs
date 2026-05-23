//! Sub-agent event types (runtime impl stays in `deepseek-tui`).

pub mod mailbox;
pub mod types;

pub use mailbox::MailboxMessage;
pub use types::{
    StructuredVerdict, SubAgentAssignment, SubAgentResult, SubAgentStatus, SubAgentType,
    VerdictItem, VerdictLevel,
};
