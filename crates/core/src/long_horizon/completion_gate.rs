//! Composable harness — completion gate manifest types (§6.1–6.2).

use serde::{Deserialize, Serialize};

/// Observe records gaps without reinject; enforce forces rework until pass or exhaustion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompletionGateMode {
    #[default]
    Observe,
    Enforce,
}

impl CompletionGateMode {
    #[must_use]
    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "enforce" => Self::Enforce,
            _ => Self::Observe,
        }
    }
}

/// Tri-state mode for the task-agnostic layer-2 sources (§ generic layer-2):
/// re-run the model's own `[verify:]` commands and the toolchain-detected
/// build/test gate. Unlike the operator manifest these need **zero per-task
/// config** — a single global switch covers all code tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GenericGateMode {
    /// Disabled (default — behaviour identical to before).
    #[default]
    Off,
    /// Run and record gaps, but never reinject / block `graph_complete`.
    Observe,
    /// Run and force rework on failure (bounded rounds).
    Enforce,
}

impl GenericGateMode {
    #[must_use]
    pub fn from_optional_str(s: Option<&str>) -> Self {
        match s.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("enforce") => Self::Enforce,
            Some("observe") => Self::Observe,
            _ => Self::Off,
        }
    }

    #[must_use]
    pub fn is_on(self) -> bool {
        !matches!(self, Self::Off)
    }

    #[must_use]
    pub fn is_enforce(self) -> bool {
        matches!(self, Self::Enforce)
    }
}

/// Where a layer-2 verify entry came from — governs which mode enforces it and
/// surfaces source counts in telemetry. Operator manifest is trusted/global;
/// `ModelDeclared` re-runs the model's own `[verify:]` (no new trust surface —
/// already within the model's exec scope); `Toolchain` is a built-in detected
/// build/test command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifySource {
    #[default]
    Operator,
    ModelDeclared,
    Toolchain,
}

/// Shell selector for manifest verify entries (§6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ManifestShell {
    #[default]
    Default,
    Pwsh,
    Bash,
    Cmd,
    /// Direct `argv` execution — no string splitting.
    None,
}

impl ManifestShell {
    #[must_use]
    pub fn from_optional_str(s: Option<&str>) -> Self {
        match s.map(str::trim).map(str::to_ascii_lowercase) {
            Some(ref v) if v == "pwsh" || v == "powershell" => Self::Pwsh,
            Some(ref v) if v == "bash" => Self::Bash,
            Some(ref v) if v == "cmd" => Self::Cmd,
            Some(ref v) if v == "none" => Self::None,
            _ => Self::Default,
        }
    }
}

/// Layer-2 harness-active verify command (exit-code oracle).
#[derive(Debug, Clone)]
pub struct CompletionGateVerifyEntry {
    pub id: String,
    pub cmd: Option<String>,
    pub argv: Vec<String>,
    pub shell: ManifestShell,
    pub timeout_secs: u32,
    /// Provenance (operator manifest / model `[verify:]` / detected toolchain).
    pub source: VerifySource,
}

/// Layer-3 deliverable manifest entry (workspace path/glob reconciliation).
#[derive(Debug, Clone)]
pub struct CompletionGateDeliverableEntry {
    pub id: String,
    pub path: Option<String>,
    pub glob: Option<String>,
    pub optional_verify_cmd: Option<String>,
    pub tracked: bool,
}

/// Resolved completion gate configuration (opt-in: empty verify + deliverable = disabled).
#[derive(Debug, Clone, Default)]
pub struct CompletionGateConfig {
    pub mode: CompletionGateMode,
    pub max_manifest_rounds: u32,
    pub max_audit_rounds: u32,
    /// Consecutive layer-2 rounds where every failure is infra-class → `audit_unmet` (§6.4).
    pub max_infra_strikes: u32,
    pub verify: Vec<CompletionGateVerifyEntry>,
    pub deliverable: Vec<CompletionGateDeliverableEntry>,
    /// Task-agnostic: harness re-runs the model's own `[verify:]` commands at
    /// `graph_complete` (zero per-task config).
    pub auto_verify_replay: GenericGateMode,
    /// Task-agnostic: harness runs a toolchain-detected canonical build/test
    /// gate at `graph_complete` (zero per-task config).
    pub toolchain_gate: GenericGateMode,
    /// Task-agnostic: scan the workspace for high-signal "intentionally
    /// unfinished" markers (Rust's `todo!` / `unimplemented!` macros,
    /// Python's `NotImplemented` error type, "not yet implemented"
    /// exception patterns) at `graph_complete`. Catches the "compiles but
    /// feature is a stub" false-completion without any per-task manifest.
    /// `observe` records counts; `enforce` blocks until the stubs are gone
    /// (bounded by `max_manifest_rounds`). Zero per-task config.
    pub stub_gate: GenericGateMode,
    /// Optional minimum line counts per bucket (Layer 3, TS-11).
    pub min_lines: MinLinesGateConfig,
}

/// `[long_horizon.completion_gate.min_lines]` — machine line-count floors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MinLinesGateConfig {
    pub frontend: Option<u32>,
    pub backend: Option<u32>,
    pub frontend_glob: Option<String>,
    pub backend_glob: Option<String>,
}

impl MinLinesGateConfig {
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.frontend.is_some() || self.backend.is_some()
    }
}

impl CompletionGateConfig {
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.verify.is_empty()
            || !self.deliverable.is_empty()
            || self.auto_verify_replay.is_on()
            || self.toolchain_gate.is_on()
            || self.stub_gate.is_on()
            || self.min_lines.is_active()
    }

    /// Layer-2 may have entries from the operator manifest **or** a task-agnostic
    /// generic source (model `[verify:]` / toolchain). The concrete entry list is
    /// assembled at gate time from the live checklist + workspace.
    #[must_use]
    pub fn has_layer2(&self) -> bool {
        !self.verify.is_empty() || self.auto_verify_replay.is_on() || self.toolchain_gate.is_on()
    }

    #[must_use]
    pub fn has_layer3(&self) -> bool {
        !self.deliverable.is_empty() || self.min_lines.is_active()
    }

    /// Downgrade an executable enforce manifest when its source is not trusted
    /// (§6.1 / §6.4, v0.4): only user-global config / built-in or trusted
    /// fixtures may auto-execute commands in `enforce`. Workspace
    /// `.deepseek/config.toml`, issue/PR text, and model-generated docs may at
    /// most *observe* — never silently gain enforce execution rights.
    ///
    /// Today the loader reads a single trusted path so callers pass `true`; this
    /// hook exists so any future workspace/overlay merge **must** pass `false`
    /// and thereby fall back to observe instead of running untrusted commands.
    #[must_use]
    pub fn sanitized_for_source(mut self, trusted: bool) -> Self {
        if !trusted && self.mode == CompletionGateMode::Enforce {
            self.mode = CompletionGateMode::Observe;
        }
        // The stub gate does not execute commands, but an untrusted source must
        // still not gain the power to *block* the turn in enforce — downgrade to
        // observe so a drive-by config can at most record.
        if !trusted && self.stub_gate.is_enforce() {
            self.stub_gate = GenericGateMode::Observe;
        }
        self
    }
}

/// Deserializable `[long_horizon.completion_gate]` table.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CompletionGateConfigToml {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub max_manifest_rounds: Option<u32>,
    #[serde(default)]
    pub max_audit_rounds: Option<u32>,
    #[serde(default)]
    pub max_infra_strikes: Option<u32>,
    #[serde(default)]
    pub verify: Vec<CompletionGateVerifyToml>,
    #[serde(default)]
    pub deliverable: Vec<CompletionGateDeliverableToml>,
    /// `off` | `observe` | `enforce` — re-run the model's own `[verify:]`.
    #[serde(default)]
    pub auto_verify_replay: Option<String>,
    /// `off` | `observe` | `enforce` — toolchain-detected build/test gate.
    #[serde(default)]
    pub toolchain_gate: Option<String>,
    /// `off` | `observe` | `enforce` — stub / incompleteness scan. Absent
    /// defaults to `observe` ("先量后调") when a `[completion_gate]` table exists.
    #[serde(default)]
    pub stub_gate: Option<String>,
    #[serde(default)]
    pub min_lines: Option<MinLinesGateToml>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MinLinesGateToml {
    #[serde(default)]
    pub frontend: Option<u32>,
    #[serde(default)]
    pub backend: Option<u32>,
    #[serde(default)]
    pub frontend_glob: Option<String>,
    #[serde(default)]
    pub backend_glob: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CompletionGateVerifyToml {
    pub id: String,
    #[serde(default)]
    pub cmd: Option<String>,
    #[serde(default)]
    pub argv: Option<Vec<String>>,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CompletionGateDeliverableToml {
    pub id: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub glob: Option<String>,
    #[serde(default)]
    pub optional_verify_cmd: Option<String>,
    #[serde(default)]
    pub tracked: Option<bool>,
}

impl CompletionGateConfigToml {
    #[must_use]
    pub fn into_runtime(self) -> CompletionGateConfig {
        CompletionGateConfig {
            mode: self
                .mode
                .map(|m| CompletionGateMode::from_str_lossy(&m))
                .unwrap_or_default(),
            max_manifest_rounds: self.max_manifest_rounds.unwrap_or(5),
            max_audit_rounds: self.max_audit_rounds.unwrap_or(5),
            max_infra_strikes: self.max_infra_strikes.unwrap_or(3),
            verify: self.verify.into_iter().map(Into::into).collect(),
            deliverable: self.deliverable.into_iter().map(Into::into).collect(),
            auto_verify_replay: GenericGateMode::from_optional_str(
                self.auto_verify_replay.as_deref(),
            ),
            toolchain_gate: GenericGateMode::from_optional_str(self.toolchain_gate.as_deref()),
            // Absent → observe ("先量后调"): once an operator opts into a
            // completion gate at all, surface stub counts by default; they must
            // explicitly set `stub_gate = "off"` to silence or `"enforce"` to block.
            stub_gate: self
                .stub_gate
                .as_deref()
                .map(|s| GenericGateMode::from_optional_str(Some(s)))
                .unwrap_or(GenericGateMode::Observe),
            min_lines: self
                .min_lines
                .map(|m| MinLinesGateConfig {
                    frontend: m.frontend,
                    backend: m.backend,
                    frontend_glob: m.frontend_glob,
                    backend_glob: m.backend_glob,
                })
                .unwrap_or_default(),
        }
    }
}

impl From<CompletionGateVerifyToml> for CompletionGateVerifyEntry {
    fn from(v: CompletionGateVerifyToml) -> Self {
        Self {
            id: v.id,
            cmd: v.cmd,
            argv: v.argv.unwrap_or_default(),
            shell: ManifestShell::from_optional_str(v.shell.as_deref()),
            timeout_secs: v.timeout_secs.unwrap_or(300),
            source: VerifySource::Operator,
        }
    }
}

impl From<CompletionGateDeliverableToml> for CompletionGateDeliverableEntry {
    fn from(d: CompletionGateDeliverableToml) -> Self {
        Self {
            id: d.id,
            path: d.path,
            glob: d.glob,
            optional_verify_cmd: d.optional_verify_cmd,
            tracked: d.tracked.unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_source_downgrades_enforce_to_observe() {
        let cfg = CompletionGateConfig {
            mode: CompletionGateMode::Enforce,
            ..Default::default()
        };
        assert_eq!(
            cfg.clone().sanitized_for_source(true).mode,
            CompletionGateMode::Enforce
        );
        assert_eq!(
            cfg.sanitized_for_source(false).mode,
            CompletionGateMode::Observe
        );
    }

    #[test]
    fn untrusted_source_leaves_observe_unchanged() {
        let cfg = CompletionGateConfig {
            mode: CompletionGateMode::Observe,
            ..Default::default()
        };
        assert_eq!(
            cfg.sanitized_for_source(false).mode,
            CompletionGateMode::Observe
        );
    }
}
