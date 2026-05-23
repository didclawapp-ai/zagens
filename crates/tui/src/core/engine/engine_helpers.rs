//! Small `Engine` helpers shared across submodules.

use tokio_util::sync::CancellationToken;

use crate::error_taxonomy::ErrorCategory;

use super::Engine;

impl Engine {
    pub(super) fn reset_cancel_token(&mut self) {
        let token = CancellationToken::new();
        match self.shared_cancel_token.lock() {
            Ok(mut shared) => *shared = token.clone(),
            Err(poisoned) => *poisoned.into_inner() = token.clone(),
        }
        self.cancel_token = token;
    }

    pub(super) fn decorate_auth_error_message(&self, message: String) -> String {
        let Some(hint) = self.api_key_env_only_recovery.as_ref() else {
            return message;
        };
        if crate::error_taxonomy::classify_error_message(&message) != ErrorCategory::Authentication
            || message.contains("no saved config key is present")
        {
            return message;
        }
        format!("{message}\n\n{hint}")
    }
}
