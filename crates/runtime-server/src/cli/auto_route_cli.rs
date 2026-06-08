//! CLI auto-model routing (`model = auto`).

use crate::agent_surface::ReasoningEffort;
use crate::auto_route::{AutoRouteSelection, resolve_auto_route_with_flash};
use crate::config::Config;

#[derive(Debug, Clone)]
pub struct CliAutoRoute {
    pub model: String,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub auto_model: bool,
}

pub async fn resolve_cli_auto_route(config: &Config, model: &str, prompt: &str) -> CliAutoRoute {
    if model.trim().eq_ignore_ascii_case("auto") {
        let selection: AutoRouteSelection =
            resolve_auto_route_with_flash(config, prompt, "", "auto", "auto").await;
        CliAutoRoute {
            model: selection.model,
            reasoning_effort: selection.reasoning_effort,
            auto_model: true,
        }
    } else {
        CliAutoRoute {
            model: model.to_string(),
            reasoning_effort: None,
            auto_model: false,
        }
    }
}
