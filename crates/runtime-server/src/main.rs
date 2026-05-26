//! Zagens / headless HTTP runtime sidecar (D6).
//!
//! Replaces `deepseek-tui serve --http` for desktop embedding — no ratatui link.

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let args = std::env::args();
    deepseek_runtime::runtime_serve::run_from_args(args).await;
}
