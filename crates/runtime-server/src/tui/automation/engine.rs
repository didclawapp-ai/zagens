//! Runtime timer/trigger engine for automation rules.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::{ActionKind, AutomationConfig, EventContext, TriggerKind};

// ── FiredResult ─────────────────────────────────────────────────────────────

/// A set of actions to fire, plus the runtime context for template substitution.
#[derive(Debug)]
pub struct FiredResult {
    pub actions: Vec<ActionKind>,
    pub ctx: EventContext,
}

impl FiredResult {
    fn new(actions: Vec<ActionKind>, ctx: EventContext) -> Self {
        Self { actions, ctx }
    }

    fn timer(actions: Vec<ActionKind>) -> Self {
        Self::new(actions, EventContext::default())
    }
}

// ── AutomationEngine ────────────────────────────────────────────────────────

/// Drives the timer and idle logic for all active automation rules.
pub struct AutomationEngine {
    pub config: AutomationConfig,
    /// Absolute `Instant` at which each rule (by ID) should next fire.
    next_fires: HashMap<u64, Instant>,
    /// Updated on every user keystroke; used to drive `Idle` triggers.
    last_activity: Instant,
}

impl AutomationEngine {
    pub fn new(config: AutomationConfig) -> Self {
        let now = Instant::now();
        let mut engine = Self {
            config,
            next_fires: HashMap::new(),
            last_activity: now,
        };
        engine.reset_timers(now);
        engine
    }

    /// Schedule (or re-schedule) all enabled interval/idle rules.
    /// Called on construction and whenever the rule set changes.
    pub fn reset_timers(&mut self, now: Instant) {
        self.next_fires.clear();
        for rule in &self.config.rules {
            if !rule.enabled {
                continue;
            }
            match &rule.trigger {
                TriggerKind::Interval { every_secs } => {
                    self.next_fires
                        .insert(rule.id, now + Duration::from_secs(*every_secs));
                }
                TriggerKind::Idle { after_secs } => {
                    self.next_fires
                        .insert(rule.id, now + Duration::from_secs(*after_secs));
                }
                _ => {}
            }
        }
    }

    /// Call this on every user keystroke to reset idle countdown timers.
    pub fn notify_activity(&mut self) {
        let now = Instant::now();
        self.last_activity = now;
        for rule in &self.config.rules {
            if !rule.enabled {
                continue;
            }
            if let TriggerKind::Idle { after_secs } = &rule.trigger {
                self.next_fires
                    .insert(rule.id, now + Duration::from_secs(*after_secs));
            }
        }
    }

    /// Return the elapsed time since the last user activity (keystroke).
    pub fn idle_duration(&self) -> Duration {
        self.last_activity.elapsed()
    }

    /// Call this when a new session is created or the user switches sessions.
    /// Returns `FiredResult`s for all enabled `SessionStart` rules.
    pub fn notify_session_start(&mut self) -> Vec<FiredResult> {
        self.config
            .rules
            .iter()
            .filter(|r| r.enabled && r.trigger == TriggerKind::SessionStart)
            .filter(|r| !r.actions.is_empty())
            .map(|r| FiredResult::timer(r.actions.clone()))
            .collect()
    }

    /// Check whether any enabled event-hook rules match the given engine event.
    /// Returns `FiredResult`s (with context populated from the event) for all
    /// matching rules. Call this for every engine event received in the main loop.
    pub fn check_engine_event(&self, event: &zagens_core::events::Event) -> Vec<FiredResult> {
        use zagens_core::events::Event;
        let mut results = Vec::new();

        // Build context once for this event.
        let mut ctx = EventContext::default();
        match event {
            Event::ToolCallComplete { name, .. } => {
                ctx.tool_name = Some(name.clone());
            }
            Event::Error { envelope, .. } => {
                ctx.error_message = Some(envelope.to_string());
            }
            _ => {}
        }

        for rule in &self.config.rules {
            if !rule.enabled || rule.actions.is_empty() {
                continue;
            }
            let matched = match &rule.trigger {
                TriggerKind::TurnComplete => matches!(event, Event::TurnComplete { .. }),
                TriggerKind::ToolComplete { tool_name } => {
                    if let Event::ToolCallComplete { name, .. } = event {
                        tool_name.as_deref().map(|f| f == name).unwrap_or(true)
                    } else {
                        false
                    }
                }
                TriggerKind::OnError => matches!(event, Event::Error { .. }),
                TriggerKind::OnApproval => matches!(event, Event::ApprovalRequired { .. }),
                _ => false,
            };
            if matched {
                results.push(FiredResult::new(rule.actions.clone(), ctx.clone()));
            }
        }
        results
    }

    /// Returns the earliest `Instant` at which any rule should fire.
    pub fn next_wake(&self) -> Option<Instant> {
        self.next_fires.values().copied().min()
    }

    /// Returns a `tokio::time::Instant` for `sleep_until`. If nothing is
    /// scheduled, returns a far-future deadline (24 h from now).
    pub fn next_wake_tokio(&self) -> tokio::time::Instant {
        match self.next_wake() {
            Some(std_instant) => tokio::time::Instant::from_std(std_instant),
            None => tokio::time::Instant::now() + Duration::from_secs(86_400),
        }
    }

    /// Returns all `FiredResult`s whose triggers are due right now, and
    /// reschedules repeating rules. Call this after `sleep_until(next_wake_tokio())`.
    pub fn poll_due(&mut self) -> Vec<FiredResult> {
        let now = Instant::now();
        let due_ids: Vec<u64> = self
            .next_fires
            .iter()
            .filter(|(_, t)| **t <= now)
            .map(|(&id, _)| id)
            .collect();

        let mut results = Vec::new();
        for id in due_ids {
            let Some(rule) = self.config.rules.iter().find(|r| r.id == id) else {
                self.next_fires.remove(&id);
                continue;
            };
            if !rule.enabled {
                self.next_fires.remove(&id);
                continue;
            }
            if !rule.actions.is_empty() {
                results.push(FiredResult::timer(rule.actions.clone()));
            }

            match &rule.trigger {
                TriggerKind::Interval { every_secs } => {
                    self.next_fires
                        .insert(id, now + Duration::from_secs(*every_secs));
                }
                TriggerKind::Idle { after_secs } => {
                    self.next_fires
                        .insert(id, now + Duration::from_secs(*after_secs));
                }
                _ => {
                    self.next_fires.remove(&id);
                }
            }
        }
        results
    }

    /// Replace the config and re-initialise all timers.
    pub fn update_config(&mut self, config: AutomationConfig) {
        self.config = config;
        self.reset_timers(Instant::now());
    }

    /// Persist the current config to disk; silently ignores I/O errors.
    pub fn save(&self) {
        let _ = self.config.save();
    }
}
