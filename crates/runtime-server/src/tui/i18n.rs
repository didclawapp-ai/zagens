//! TUI surface strings (panels, overlays, slash picker).
//!
//! Zagens desktop Web UI uses `crates/desktop/web-ui/src/i18n/`; runtime shared
//! strings live in `crate::localization`. This module covers ratatui chrome only.

use crate::localization::{Locale, MessageId, resolve_locale, tr};
use crate::settings::Settings;

use super::composer_slash::SlashActionKind;
use super::layout::InspectorTab;

/// Load the active UI locale from `~/.zagens/settings.toml`.
pub fn load_locale() -> Locale {
    resolve_locale(&Settings::load().unwrap_or_default().locale)
}

pub fn inspector_tab_label(locale: Locale, tab: InspectorTab) -> &'static str {
    tr(
        locale,
        match tab {
            InspectorTab::Files => MessageId::TuiInspectorTabFiles,
            InspectorTab::Diff => MessageId::TuiInspectorTabDiff,
            InspectorTab::Agents => MessageId::TuiInspectorTabAgents,
            InspectorTab::Mcp => MessageId::TuiInspectorTabMcp,
            InspectorTab::Activity => MessageId::TuiInspectorTabActivity,
        },
    )
}

pub fn inspector_tab_hint(locale: Locale, tab: InspectorTab) -> &'static str {
    tr(
        locale,
        match tab {
            InspectorTab::Files => MessageId::TuiInspectorHintFiles,
            InspectorTab::Diff => MessageId::TuiInspectorHintDiff,
            InspectorTab::Agents => MessageId::TuiInspectorHintAgents,
            InspectorTab::Mcp => MessageId::TuiInspectorHintMcp,
            InspectorTab::Activity => MessageId::TuiInspectorHintActivity,
        },
    )
}

pub fn slash_description(locale: Locale, kind: SlashActionKind) -> &'static str {
    tr(
        locale,
        match kind {
            SlashActionKind::Workspace => MessageId::TuiSlashWorkspace,
            SlashActionKind::Model => MessageId::TuiSlashModel,
            SlashActionKind::Lht => MessageId::TuiSlashLht,
            SlashActionKind::Theme => MessageId::TuiSlashTheme,
            SlashActionKind::New => MessageId::TuiSlashNew,
            SlashActionKind::Help => MessageId::TuiSlashHelp,
            SlashActionKind::Automation => MessageId::TuiSlashAuto,
            SlashActionKind::Clear => MessageId::TuiSlashClear,
            SlashActionKind::Locale => MessageId::TuiSlashLocale,
            SlashActionKind::ApiKey => MessageId::TuiSlashApiKey,
            SlashActionKind::Login => MessageId::TuiSlashLogin,
            SlashActionKind::Logout => MessageId::TuiSlashLogout,
        },
    )
}

/// Slash alias rows reuse the primary command description.
pub fn slash_alias_description(locale: Locale, kind: SlashActionKind) -> &'static str {
    match kind {
        SlashActionKind::Workspace => tr(locale, MessageId::TuiSlashCd),
        SlashActionKind::Model => tr(locale, MessageId::TuiSlashModelAlias),
        SlashActionKind::ApiKey => tr(locale, MessageId::TuiSlashKey),
        SlashActionKind::Login => tr(locale, MessageId::TuiSlashLogin),
        _ => slash_description(locale, kind),
    }
}

pub fn slash_language_description(locale: Locale) -> &'static str {
    tr(locale, MessageId::TuiSlashLanguage)
}

pub fn build_help_text(locale: Locale) -> String {
    [
        tr(locale, MessageId::TuiHelpTitle),
        "",
        tr(locale, MessageId::TuiHelpSectionFocus),
        tr(locale, MessageId::TuiHelpSectionLeftRail),
        tr(locale, MessageId::TuiHelpSectionRightRail),
        tr(locale, MessageId::TuiHelpSectionChat),
        tr(locale, MessageId::TuiHelpSectionApproval),
        tr(locale, MessageId::TuiHelpSectionGlobal),
        tr(locale, MessageId::TuiHelpSectionLaunch),
        tr(locale, MessageId::TuiHelpSectionTerminalFont),
    ]
    .join("\n")
}

pub fn approval_body(locale: Locale, pending: &super::overlay::PendingApproval) -> String {
    let tool = tr(locale, MessageId::TuiApprovalToolLabel);
    let allow = tr(locale, MessageId::TuiApprovalAllow);
    let deny = tr(locale, MessageId::TuiApprovalDeny);
    let allow_session = tr(locale, MessageId::TuiApprovalAllowSession);
    if pending.show_detail {
        let key = tr(locale, MessageId::TuiApprovalKeyLabel);
        let summary = tr(locale, MessageId::TuiApprovalSummary);
        format!(
            "{tool}: {}\n{key}: {}\n\n{}\n\n[y] {allow}   [n] {deny}   [a] {allow_session}   [v] {summary}\n[Esc] {deny}",
            pending.tool_name, pending.approval_key, pending.description
        )
    } else {
        let detail = tr(locale, MessageId::TuiApprovalDetail);
        format!(
            "{tool}: {}\n\n{}\n\n[y] {allow}   [n] {deny}   [a] {allow_session}   [v] {detail}\n[Esc] {deny}",
            pending.tool_name, pending.description
        )
    }
}

pub fn resumed_thread_banner(locale: Locale, thread_id: &str) -> String {
    tr(locale, MessageId::TuiResumedThread).replace("{id}", thread_id)
}
