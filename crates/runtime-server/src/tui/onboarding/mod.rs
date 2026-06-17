//! First-run onboarding plan (welcome → API key → default task type).

mod handler;

pub use handler::{handle_onboarding_key, onboarding_paste};

use crate::cli::args::Cli;
use crate::cli::setup::{ApiKeySource, resolve_api_key_source};
use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingPhase {
    Welcome,
    ApiKey,
    TaskType,
}

#[derive(Debug, Clone)]
pub struct OnboardingPlan {
    pub phases: Vec<OnboardingPhase>,
}

impl OnboardingPlan {
    pub fn build(config: &Config) -> Self {
        let mut phases = vec![OnboardingPhase::Welcome];
        if !api_key_configured(config) {
            phases.push(OnboardingPhase::ApiKey);
        }
        if zagens_config::read_task_type_preference_setting()
            .ok()
            .flatten()
            .is_none()
        {
            phases.push(OnboardingPhase::TaskType);
        }
        Self { phases }
    }

    pub fn needs_overlay(&self) -> bool {
        !self.phases.is_empty()
    }
}

pub fn should_show_onboarding(cli: &Cli, config: &Config) -> bool {
    if cli.skip_onboarding {
        return false;
    }
    if zagens_config::read_onboarding_complete_setting()
        .ok()
        .unwrap_or(false)
    {
        return false;
    }
    OnboardingPlan::build(config).needs_overlay()
}

pub fn api_key_configured(config: &Config) -> bool {
    if resolve_api_key_source(config) != ApiKeySource::Missing {
        return true;
    }
    if config.deepseek_api_key().is_ok() {
        return true;
    }
    keyring_has_deepseek_key()
}

fn keyring_has_deepseek_key() -> bool {
    #[cfg(test)]
    {
        return false;
    }
    #[cfg(not(test))]
    {
        let secrets = zagens_secrets::Secrets::auto_detect();
        secrets
            .get("deepseek")
            .ok()
            .flatten()
            .is_some_and(|k| !k.trim().is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_includes_key_when_missing() {
        let config = Config::default();
        let plan = OnboardingPlan::build(&config);
        assert!(plan.phases.contains(&OnboardingPhase::Welcome));
        assert!(plan.phases.contains(&OnboardingPhase::ApiKey));
    }
}
