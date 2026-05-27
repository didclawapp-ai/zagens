//! In-memory web.run session and page store.

use super::types::{StoredWebPage, WebPage, WebRunState};
use super::{MAX_PAGES_PER_SESSION, MAX_WEB_RUN_SESSIONS, WEB_RUN_SESSION_TTL};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static WEB_RUN_STATE: OnceLock<Mutex<WebRunState>> = OnceLock::new();

impl WebRunState {
    fn cleanup(&mut self) {
        let now = Instant::now();
        let expired = self
            .sessions
            .iter()
            .filter_map(|(namespace, session)| {
                if now.duration_since(session.last_access) > WEB_RUN_SESSION_TTL {
                    Some(namespace.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for namespace in expired {
            self.remove_session(&namespace);
        }

        while self.sessions.len() > MAX_WEB_RUN_SESSIONS {
            let Some(oldest_namespace) = self
                .sessions
                .iter()
                .min_by_key(|(_, session)| session.last_access)
                .map(|(namespace, _)| namespace.clone())
            else {
                break;
            };
            self.remove_session(&oldest_namespace);
        }
    }

    fn remove_session(&mut self, namespace: &str) {
        if let Some(session) = self.sessions.remove(namespace) {
            for ref_id in session.refs {
                self.pages.remove(&ref_id);
            }
        }
    }

    fn touch_session(&mut self, namespace: &str) {
        self.cleanup();
        if !self.sessions.contains_key(namespace)
            && self.sessions.len() >= MAX_WEB_RUN_SESSIONS
            && let Some(oldest_namespace) = self
                .sessions
                .iter()
                .min_by_key(|(_, session)| session.last_access)
                .map(|(existing_namespace, _)| existing_namespace.clone())
        {
            self.remove_session(&oldest_namespace);
        }

        let session = self.sessions.entry(namespace.to_string()).or_default();
        session.last_access = Instant::now();
    }

    pub(in crate::tools::web_run) fn next_turn(&mut self, namespace: &str) -> u64 {
        self.touch_session(namespace);
        let session = self
            .sessions
            .get_mut(namespace)
            .expect("session should exist after touch");
        let current = session.next_turn;
        session.next_turn = session.next_turn.saturating_add(1);
        current
    }

    fn store_page(&mut self, namespace: &str, ref_id: &str, page: WebPage) {
        self.touch_session(namespace);
        let mut evicted_refs = Vec::new();
        {
            let session = self
                .sessions
                .get_mut(namespace)
                .expect("session should exist after touch");
            if let Some(existing_idx) = session.refs.iter().position(|existing| existing == ref_id)
            {
                session.refs.remove(existing_idx);
            }
            session.refs.push_back(ref_id.to_string());

            while session.refs.len() > MAX_PAGES_PER_SESSION {
                if let Some(evicted_ref) = session.refs.pop_front() {
                    evicted_refs.push(evicted_ref);
                }
            }
        }

        self.pages.insert(
            ref_id.to_string(),
            StoredWebPage {
                namespace: namespace.to_string(),
                page,
            },
        );
        for evicted_ref in evicted_refs {
            self.pages.remove(&evicted_ref);
        }
    }

    fn get_page(&mut self, ref_id: &str) -> Option<WebPage> {
        self.cleanup();
        let stored = self.pages.get(ref_id)?.clone();
        if let Some(session) = self.sessions.get_mut(&stored.namespace) {
            session.last_access = Instant::now();
        }
        Some(stored.page)
    }
}

pub(in crate::tools::web_run) fn with_state<T>(f: impl FnOnce(&mut WebRunState) -> T) -> T {
    let lock = WEB_RUN_STATE.get_or_init(|| Mutex::new(WebRunState::default()));
    let mut state = lock
        .lock()
        .expect("web run state mutex should not be poisoned");
    state.cleanup();
    f(&mut state)
}

pub(in crate::tools::web_run) fn scoped_ref_prefix(namespace: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    namespace.hash(&mut hasher);
    format!("s{:016x}_", hasher.finish())
}

pub(in crate::tools::web_run) fn store_page(namespace: &str, ref_id: &str, page: WebPage) {
    with_state(|state| {
        state.store_page(namespace, ref_id, page);
    });
}

pub(in crate::tools::web_run) fn get_page(ref_id: &str) -> Option<WebPage> {
    with_state(|state| state.get_page(ref_id))
}

#[cfg(test)]
pub(in crate::tools::web_run) fn reset_web_run_state() {
    with_state(|state| {
        *state = WebRunState::default();
    });
}

#[cfg(test)]
pub(in crate::tools::web_run) fn next_turn_for_namespace(namespace: &str) -> u64 {
    with_state(|state| state.next_turn(namespace))
}
