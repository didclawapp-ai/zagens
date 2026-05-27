//! Shared state ports for auth, probe, and router composition (D16 E1-c phase 2).

/// Bearer token gate for `/v1/*` routes.
pub trait RuntimeApiAuthState: Clone + Send + Sync + 'static {
    fn runtime_token(&self) -> Option<&str>;
}

/// `/internal/probe` fields supplied by the sidecar host.
pub trait RuntimeApiProbeState: Clone + Send + Sync + 'static {
    fn process_started_at_ms(&self) -> u128;

    fn token_fingerprint(&self) -> &str;

    fn service_version(&self) -> &'static str;
}

/// Combined bounds for [`crate::router::compose_router`].
pub trait RuntimeApiHostState: RuntimeApiAuthState + RuntimeApiProbeState {}

impl<T> RuntimeApiHostState for T where T: RuntimeApiAuthState + RuntimeApiProbeState {}
