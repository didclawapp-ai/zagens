//! Agent vs human navigation policy — chain + grace window (audit B-A3 / Batch 2).
//!
//! Agent click/navigate keeps **Agent** URL policy until the in-flight navigation chain
//! (Started/Finished, including redirects) completes, plus a short grace period for
//! delayed JS navigations (`setTimeout`, meta refresh).

use std::time::{Duration, Instant};

use super::url_policy::NavActor;

/// After the last in-flight load in an agent chain, keep Agent policy for delayed redirects.
pub const AGENT_NAV_GRACE_MS: u64 = 3_000;

#[derive(Debug, Clone, Copy)]
pub struct NavPolicyChain {
    pub actor: NavActor,
    /// Document loads currently in flight for an agent-initiated chain.
    pub inflight: u32,
    /// When set, Agent policy stays active until this instant (post-chain grace).
    pub grace_until: Option<Instant>,
}

impl Default for NavPolicyChain {
    fn default() -> Self {
        Self::human()
    }
}

impl NavPolicyChain {
    pub fn human() -> Self {
        Self {
            actor: NavActor::Human,
            inflight: 0,
            grace_until: None,
        }
    }

    /// Agent tool navigate / click / type — following navigations use agent URL policy.
    pub fn begin_agent(&mut self) {
        self.actor = NavActor::Agent;
        self.grace_until = Some(Instant::now() + Duration::from_millis(AGENT_NAV_GRACE_MS));
    }

    /// Human address bar / in-app open — clears agent chain state.
    pub fn begin_human(&mut self) {
        *self = Self::human();
    }

    pub fn on_page_started(&mut self) {
        if self.actor == NavActor::Agent {
            self.inflight = self.inflight.saturating_add(1);
            self.grace_until = None;
        }
    }

    pub fn on_page_finished(&mut self) {
        if self.actor == NavActor::Agent && self.inflight > 0 {
            self.inflight -= 1;
            if self.inflight == 0 {
                self.grace_until = Some(Instant::now() + Duration::from_millis(AGENT_NAV_GRACE_MS));
            }
        }
    }

    pub fn effective_actor(&self) -> NavActor {
        if self.actor == NavActor::Human {
            return NavActor::Human;
        }
        if self.inflight > 0 {
            return NavActor::Agent;
        }
        if let Some(until) = self.grace_until
            && Instant::now() < until
        {
            return NavActor::Agent;
        }
        NavActor::Human
    }

    /// Resolve actor for `on_navigation`; reset stored state when grace/chain expired.
    pub fn expire_if_needed(&mut self) -> NavActor {
        let effective = self.effective_actor();
        if self.actor == NavActor::Agent && effective == NavActor::Human {
            *self = Self::human();
        }
        effective
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_chain_keeps_policy_through_redirects_and_grace() {
        let mut chain = NavPolicyChain::human();
        chain.begin_agent();
        assert_eq!(chain.effective_actor(), NavActor::Agent);

        chain.on_page_started();
        assert_eq!(chain.inflight, 1);
        chain.on_page_started();
        assert_eq!(chain.inflight, 2);

        chain.on_page_finished();
        assert_eq!(chain.inflight, 1);
        assert_eq!(chain.effective_actor(), NavActor::Agent);

        chain.on_page_finished();
        assert_eq!(chain.inflight, 0);
        assert_eq!(chain.effective_actor(), NavActor::Agent);
        assert!(chain.grace_until.is_some());
    }

    #[test]
    fn human_navigation_clears_agent_state() {
        let mut chain = NavPolicyChain::human();
        chain.begin_agent();
        chain.on_page_started();
        chain.begin_human();
        assert_eq!(chain.effective_actor(), NavActor::Human);
        assert_eq!(chain.inflight, 0);
        assert!(chain.grace_until.is_none());
    }

    #[test]
    fn agent_click_without_load_gets_grace_window() {
        let mut chain = NavPolicyChain::human();
        chain.begin_agent();
        assert_eq!(chain.effective_actor(), NavActor::Agent);
        chain.expire_if_needed();
        assert_eq!(chain.actor, NavActor::Agent);
    }

    #[test]
    fn expire_resets_after_grace_elapses() {
        let mut chain = NavPolicyChain::human();
        chain.begin_agent();
        chain.grace_until = Some(Instant::now() - Duration::from_millis(1));
        assert_eq!(chain.expire_if_needed(), NavActor::Human);
        assert_eq!(chain.actor, NavActor::Human);
    }
}
