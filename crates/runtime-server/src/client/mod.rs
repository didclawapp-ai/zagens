//! HTTP client for DeepSeek's OpenAI-compatible Chat Completions API.
//!
//! DeepSeek documents `/chat/completions` as the primary endpoint, and this
//! client now routes all normal traffic through that surface.

mod api_parse;
mod chat;
mod client_impl;
mod fim;
mod http;
mod llm;
mod tool_names;
mod types;

pub(crate) use api_parse::system_to_instructions;
pub(crate) use chat::build_chat_messages_for_request;
pub use types::DeepSeekClient;

#[cfg(test)]
include!("tests.inc.rs");
