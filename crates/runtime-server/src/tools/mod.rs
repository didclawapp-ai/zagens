//! Tool system modules and re-exports.

pub mod apply_patch;
pub mod approval_cache;
pub use zagens_runtime_adapters::tools::arg_repair;
pub mod automation;
pub mod describe_image;
pub mod diagnostics;
pub use zagens_runtime_adapters::tools::diff_format;
pub mod file;
pub mod file_info;
pub mod file_search;
pub mod finance;

pub mod fetch_url;
pub mod fim;
pub mod git;
pub mod git_history;
pub mod github;
pub mod glob_files;
pub mod host_impl;
pub(crate) mod html_page_text;
pub mod large_output_router;
pub mod office_common;
pub mod office_payload;
pub mod office_read;
pub mod office_write;

#[cfg(test)]
mod office_smoke;
pub mod parallel;
pub mod plan;
pub mod project;
pub mod recall_archive;
pub mod registry;
pub mod remember;
pub mod revert_turn;
pub mod review;
pub mod rlm;
pub mod scratchpad;
pub mod scratchpad_agent;
pub use zagens_runtime_adapters::tools::schema_sanitize;
pub mod search;
pub mod shell;
mod shell_output;
pub mod skill;
pub mod spec;
pub mod ssrf;
pub mod subagent;
pub mod tasks;
pub mod test_runner;
pub mod todo;
pub mod truncate;
pub mod user_input;
pub mod validate_data;
pub mod web_run;
pub mod web_search;
pub use zagens_runtime_adapters::tools::workspace_walk;

pub use registry::{ToolRegistry, ToolRegistryBuilder};
pub use spec::ToolContext;
