//! Sidecar `RuntimeApiState` host trait impls (D16 E1-c phase 2).

use deepseek_runtime_api::{RuntimeApiAuthState, RuntimeApiProbeState};

use super::RuntimeApiState;

impl RuntimeApiAuthState for RuntimeApiState {
    fn runtime_token(&self) -> Option<&str> {
        self.runtime_token.as_deref()
    }
}

impl RuntimeApiProbeState for RuntimeApiState {
    fn process_started_at_ms(&self) -> u128 {
        self.process_started_at_ms
    }

    fn token_fingerprint(&self) -> &str {
        self.token_fingerprint.as_ref()
    }

    fn service_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}
