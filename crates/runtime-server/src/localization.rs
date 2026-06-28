//! Lightweight localization registry for high-visibility TUI strings.
//!
//! This intentionally covers UI chrome only. It does not change model prompts,
//! model output language, provider behavior, or media payload semantics.
//!
//! **D1 / 架构定型：** 本文件保持单体（~1.9k 行）。仅 TUI/CLI 使用；Zagens 桌面 i18n 在
//! `crates/desktop/web-ui/`。见
//! [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](../../../docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md) §5.1「D1 明确不拆分」。

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDirection {
    Ltr,
    Rtl,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaleCoverage {
    English,
    V076Core,
    PlannedQa,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleSpec {
    pub tag: &'static str,
    pub display_name: &'static str,
    pub script: &'static str,
    pub direction: TextDirection,
    pub fallback: &'static str,
    pub coverage: LocaleCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locale {
    En,
    Ja,
    ZhHans,
    PtBr,
}

impl Locale {
    pub fn tag(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ja => "ja",
            Self::ZhHans => "zh-Hans",
            Self::PtBr => "pt-BR",
        }
    }

    #[allow(dead_code)]
    pub fn spec(self) -> LocaleSpec {
        match self {
            Self::En => LocaleSpec {
                tag: "en",
                display_name: "English",
                script: "Latin",
                direction: TextDirection::Ltr,
                fallback: "en",
                coverage: LocaleCoverage::English,
            },
            Self::Ja => LocaleSpec {
                tag: "ja",
                display_name: "Japanese",
                script: "Jpan",
                direction: TextDirection::Ltr,
                fallback: "en",
                coverage: LocaleCoverage::V076Core,
            },
            Self::ZhHans => LocaleSpec {
                tag: "zh-Hans",
                display_name: "Chinese Simplified",
                script: "Hans",
                direction: TextDirection::Ltr,
                fallback: "en",
                coverage: LocaleCoverage::V076Core,
            },
            Self::PtBr => LocaleSpec {
                tag: "pt-BR",
                display_name: "Portuguese (Brazil)",
                script: "Latin",
                direction: TextDirection::Ltr,
                fallback: "en",
                coverage: LocaleCoverage::V076Core,
            },
        }
    }

    #[allow(dead_code)]
    pub fn shipped() -> &'static [Self] {
        &[Self::En, Self::Ja, Self::ZhHans, Self::PtBr]
    }
}

#[allow(dead_code)]
pub const PLANNED_QA_LOCALES: &[LocaleSpec] = &[
    LocaleSpec {
        tag: "ar",
        display_name: "Arabic",
        script: "Arab",
        direction: TextDirection::Rtl,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "hi",
        display_name: "Hindi",
        script: "Deva",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "bn",
        display_name: "Bengali",
        script: "Beng",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "id",
        display_name: "Indonesian",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "vi",
        display_name: "Vietnamese",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "sw",
        display_name: "Swahili",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "ha",
        display_name: "Hausa",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "yo",
        display_name: "Yoruba",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "es-419",
        display_name: "Spanish (Latin America)",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "fr",
        display_name: "French",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
    LocaleSpec {
        tag: "fil",
        display_name: "Filipino/Tagalog",
        script: "Latin",
        direction: TextDirection::Ltr,
        fallback: "en",
        coverage: LocaleCoverage::PlannedQa,
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MessageId {
    ComposerPlaceholder,
    HistorySearchPlaceholder,
    HistorySearchTitle,
    HistoryHintMove,
    HistoryHintAccept,
    HistoryHintRestore,
    HistoryNoMatches,
    ConfigTitle,
    ConfigModalTitle,
    ConfigSearchPlaceholder,
    ConfigNoSettings,
    ConfigNoMatchesPrefix,
    ConfigFilteredSettings,
    ConfigShowing,
    ConfigFooterDefault,
    ConfigFooterScrollable,
    ConfigFooterFiltered,
    HelpTitle,
    HelpFilterPlaceholder,
    HelpFilterPrefix,
    HelpNoMatches,
    HelpSlashCommands,
    HelpKeybindings,
    HelpFooterTypeFilter,
    HelpFooterMove,
    HelpFooterJump,
    HelpFooterClose,
    CmdAgentDescription,
    CmdAttachDescription,
    CmdAnchorDescription,
    CmdCacheDescription,
    CmdClearDescription,
    CmdCompactDescription,
    CmdConfigDescription,
    CmdContextDescription,
    CmdCostDescription,
    CmdCycleDescription,
    CmdCyclesDescription,
    CmdDiffDescription,
    CmdEditDescription,
    CmdExitDescription,
    CmdExportDescription,
    CmdHelpDescription,
    CmdHomeDescription,
    CmdHooksDescription,
    CmdGoalDescription,
    CmdInitDescription,
    CmdJobsDescription,
    CmdLinksDescription,
    CmdLoadDescription,
    CmdLogoutDescription,
    CmdMcpDescription,
    CmdMemoryDescription,
    CmdModelDescription,
    CmdModelsDescription,
    CmdNetworkDescription,
    CmdNoteDescription,
    CmdPlanDescription,
    CmdProviderDescription,
    CmdQueueDescription,
    CmdRecallDescription,
    CmdRenameDescription,
    CmdRestoreDescription,
    CmdRetryDescription,
    CmdReviewDescription,
    CmdRlmDescription,
    CmdSaveDescription,
    CmdSessionsDescription,
    CmdSettingsDescription,
    CmdSkillDescription,
    CmdSkillsDescription,
    CmdStashDescription,
    CmdStatuslineDescription,
    CmdSubagentsDescription,
    CmdSwarmDescription,
    CmdSystemDescription,
    CmdTaskDescription,
    CmdTokensDescription,
    CmdTrustDescription,
    CmdLspDescription,
    CmdShareDescription,
    CmdUndoDescription,
    CmdYoloDescription,
    CmdCacheAdvice,
    CmdCacheFootnote,
    CmdCacheHeader,
    CmdCacheNoData,
    CmdCacheTotals,
    CmdCostReport,
    CmdTokensCacheBoth,
    CmdTokensCacheHitOnly,
    CmdTokensCacheMissOnly,
    CmdTokensContextUnknownWindow,
    CmdTokensContextWithWindow,
    CmdTokensNotReported,
    CmdTokensReport,
    FooterAgentSingular,
    FooterAgentsPlural,
    FooterPressCtrlCAgain,
    FooterWorking,
    HelpSectionActions,
    HelpSectionClipboard,
    HelpSectionEditing,
    HelpSectionHelp,
    HelpSectionModes,
    HelpSectionNavigation,
    HelpSectionSessions,
    KbScrollTranscript,
    KbNavigateHistory,
    KbScrollTranscriptAlt,
    KbScrollPage,
    KbJumpTopBottom,
    KbJumpTopBottomEmpty,
    KbJumpToolBlocks,
    KbMoveCursor,
    KbJumpLineStartEnd,
    KbDeleteChar,
    KbClearDraft,
    KbStashDraft,
    KbSearchHistory,
    KbInsertNewline,
    KbSendDraft,
    KbCloseMenu,
    KbCancelOrExit,
    KbShellControls,
    KbExitEmpty,
    KbCommandPalette,
    KbFuzzyFilePicker,
    KbCompactInspector,
    KbLastMessagePager,
    KbSelectedDetails,
    KbToolDetailsPager,
    KbThinkingPager,
    KbLiveTranscript,
    KbBacktrackMessage,
    KbCompleteCycleModes,
    KbJumpPlanAgentYolo,
    KbAltJumpPlanAgentYolo,
    KbFocusSidebar,
    KbTogglePlanAgent,
    KbSessionPicker,
    KbPasteAttach,
    KbCopySelection,
    KbContextMenu,
    KbAttachPath,
    KbHelpOverlay,
    KbToggleHelp,
    KbToggleHelpSlash,
    HelpUsageLabel,
    HelpAliasesLabel,
    SettingsTitle,
    SettingsConfigFile,
    ClearConversation,
    ClearConversationBusy,
    ModelChanged,
    LinksTitle,
    LinksDashboard,
    LinksDocs,
    LinksTip,
    SubagentsFetching,
    HelpUnknownCommand,
    HomeDashboardTitle,
    HomeModel,
    HomeMode,
    HomeWorkspace,
    HomeHistory,
    HomeTokens,
    HomeQueued,
    HomeSubagents,
    HomeSkill,
    HomeQuickActions,
    HomeQuickLinks,
    HomeQuickSkills,
    HomeQuickConfig,
    HomeQuickSettings,
    HomeQuickModel,
    HomeQuickSubagents,
    HomeQuickTaskList,
    HomeQuickHelp,
    HomeModeTips,
    HomeAgentModeTip,
    HomeAgentModeReviewTip,
    HomeAgentModeYoloTip,
    HomeYoloModeTip,
    HomeYoloModeCaution,
    HomePlanModeTip,
    HomePlanModeChecklistTip,
    TuiPanelTranscript,
    TuiPanelTranscriptScroll,
    TuiPanelComposer,
    TuiPanelComposerScroll,
    TuiPanelComposerWaitingEdit,
    TuiPanelComposerWaitingScroll,
    TuiPanelStatus,
    TuiPanelLeft,
    TuiPanelLht,
    TuiPanelLhtFocused,
    TuiInspectorTabFiles,
    TuiInspectorTabDiff,
    TuiInspectorTabAgents,
    TuiInspectorTabMcp,
    TuiInspectorTabActivity,
    TuiInspectorTabContext,
    TuiLeftRailSessions,
    TuiLeftRailNoSessions,
    TuiLeftRailInspector,
    TuiLeftRailNavHint,
    TuiNewSession,
    TuiComposerHintWaitingEdit,
    TuiComposerHintWaitingScroll,
    TuiComposerHintTypePrompt,
    TuiComposerHintScrollMode,
    TuiApprovalTitle,
    TuiApprovalToolLabel,
    TuiApprovalKeyLabel,
    TuiApprovalAllow,
    TuiApprovalDeny,
    TuiApprovalAllowSession,
    TuiApprovalSummary,
    TuiApprovalDetail,
    TuiHelpTitle,
    TuiHelpCloseTitle,
    TuiHelpSectionFocus,
    TuiHelpSectionLeftRail,
    TuiHelpSectionRightRail,
    TuiHelpSectionChat,
    TuiHelpSectionApproval,
    TuiHelpSectionGlobal,
    TuiHelpSectionLaunch,
    TuiHelpSectionTerminalFont,
    TuiSlashWorkspace,
    TuiSlashCd,
    TuiSlashModel,
    TuiSlashModelAlias,
    TuiSlashLht,
    TuiSlashTheme,
    TuiSlashNew,
    TuiSlashHelp,
    TuiSlashAuto,
    TuiSlashClear,
    TuiInspectorHintFiles,
    TuiInspectorHintDiff,
    TuiInspectorHintAgents,
    TuiInspectorHintMcp,
    TuiInspectorHintActivity,
    TuiInspectorHintContext,
    TuiTranscriptEmpty,
    TuiResumedThread,
    TuiAutoTitle,
    TuiAutoListHint,
    TuiAutoEditRule,
    TuiAutoNewRule,
    TuiAutoEditHint,
    TuiAutoName,
    TuiAutoTrigger,
    TuiAutoSeconds,
    TuiAutoToolFilter,
    TuiAutoAnyTool,
    TuiAutoAction,
    TuiAutoPrompt,
    TuiAutoShellCmd,
    TuiAutoMessage,
    TuiAutoCommand,
    TuiSlashLocale,
    TuiSlashLanguage,
    TuiSlashApiKey,
    TuiSlashKey,
    TuiSlashLogin,
    TuiSlashLogout,
    TuiSlashApprove,
    TuiSlashApproval,
    TuiApiKeyCleared,
    TuiApiKeyUsage,
    TuiLocalePickerHint,
    TuiLocaleChanged,
    TuiPendingInputsTitle,
    TuiPendingQueuedKind,
    TuiPendingEditHint,
    TuiSteerInjected,
    TuiOnboardingTitle,
    TuiOnboardingWelcomeTitle,
    TuiOnboardingWelcomeBody,
    TuiOnboardingWorkspace,
    TuiOnboardingKeyTitle,
    TuiOnboardingKeyHint,
    TuiOnboardingModeTitle,
    TuiOnboardingModeAuto,
    TuiOnboardingModeAutoDesc,
    TuiOnboardingModeCode,
    TuiOnboardingModeCodeDesc,
    TuiOnboardingModeOffice,
    TuiOnboardingModeOfficeDesc,
    TuiOnboardingFooter,
    TuiOnboardingStepWelcome,
    TuiOnboardingStepKey,
    TuiOnboardingStepMode,
    TuiOnboardingKeySaved,
    TuiOnboardingComplete,
    TuiSlashMcp,
    TuiMcpTitle,
    TuiMcpPathLabel,
    TuiMcpSave,
    TuiMcpCancel,
    TuiMcpFooter,
    TuiMcpSaved,
    TuiMcpParseError,
    TuiMcpEmptyError,
}

#[allow(dead_code)]
pub const ALL_MESSAGE_IDS: &[MessageId] = &[
    MessageId::ComposerPlaceholder,
    MessageId::HistorySearchPlaceholder,
    MessageId::HistorySearchTitle,
    MessageId::HistoryHintMove,
    MessageId::HistoryHintAccept,
    MessageId::HistoryHintRestore,
    MessageId::HistoryNoMatches,
    MessageId::ConfigTitle,
    MessageId::ConfigModalTitle,
    MessageId::ConfigSearchPlaceholder,
    MessageId::ConfigNoSettings,
    MessageId::ConfigNoMatchesPrefix,
    MessageId::ConfigFilteredSettings,
    MessageId::ConfigShowing,
    MessageId::ConfigFooterDefault,
    MessageId::ConfigFooterScrollable,
    MessageId::ConfigFooterFiltered,
    MessageId::HelpTitle,
    MessageId::HelpFilterPlaceholder,
    MessageId::HelpFilterPrefix,
    MessageId::HelpNoMatches,
    MessageId::HelpSlashCommands,
    MessageId::HelpKeybindings,
    MessageId::HelpFooterTypeFilter,
    MessageId::HelpFooterMove,
    MessageId::HelpFooterJump,
    MessageId::HelpFooterClose,
    MessageId::CmdAgentDescription,
    MessageId::CmdAnchorDescription,
    MessageId::CmdAttachDescription,
    MessageId::CmdCacheDescription,
    MessageId::CmdClearDescription,
    MessageId::CmdCompactDescription,
    MessageId::CmdConfigDescription,
    MessageId::CmdContextDescription,
    MessageId::CmdCostDescription,
    MessageId::CmdCycleDescription,
    MessageId::CmdCyclesDescription,
    MessageId::CmdDiffDescription,
    MessageId::CmdEditDescription,
    MessageId::CmdExitDescription,
    MessageId::CmdExportDescription,
    MessageId::CmdHelpDescription,
    MessageId::CmdHomeDescription,
    MessageId::CmdHooksDescription,
    MessageId::CmdInitDescription,
    MessageId::CmdJobsDescription,
    MessageId::CmdLinksDescription,
    MessageId::CmdLoadDescription,
    MessageId::CmdLogoutDescription,
    MessageId::CmdMcpDescription,
    MessageId::CmdMemoryDescription,
    MessageId::CmdModelDescription,
    MessageId::CmdModelsDescription,
    MessageId::CmdNetworkDescription,
    MessageId::CmdNoteDescription,
    MessageId::CmdPlanDescription,
    MessageId::CmdProviderDescription,
    MessageId::CmdQueueDescription,
    MessageId::CmdRecallDescription,
    MessageId::CmdRenameDescription,
    MessageId::CmdRestoreDescription,
    MessageId::CmdRetryDescription,
    MessageId::CmdReviewDescription,
    MessageId::CmdRlmDescription,
    MessageId::CmdSaveDescription,
    MessageId::CmdSessionsDescription,
    MessageId::CmdSettingsDescription,
    MessageId::CmdSkillDescription,
    MessageId::CmdSkillsDescription,
    MessageId::CmdStashDescription,
    MessageId::CmdStatuslineDescription,
    MessageId::CmdSubagentsDescription,
    MessageId::CmdSwarmDescription,
    MessageId::CmdSystemDescription,
    MessageId::CmdTaskDescription,
    MessageId::CmdTokensDescription,
    MessageId::CmdTrustDescription,
    MessageId::CmdLspDescription,
    MessageId::CmdShareDescription,
    MessageId::CmdUndoDescription,
    MessageId::CmdYoloDescription,
    MessageId::CmdCacheAdvice,
    MessageId::CmdCacheFootnote,
    MessageId::CmdCacheHeader,
    MessageId::CmdCacheNoData,
    MessageId::CmdCacheTotals,
    MessageId::CmdCostReport,
    MessageId::CmdTokensCacheBoth,
    MessageId::CmdTokensCacheHitOnly,
    MessageId::CmdTokensCacheMissOnly,
    MessageId::CmdTokensContextUnknownWindow,
    MessageId::CmdTokensContextWithWindow,
    MessageId::CmdTokensNotReported,
    MessageId::CmdTokensReport,
    MessageId::FooterAgentSingular,
    MessageId::FooterAgentsPlural,
    MessageId::FooterPressCtrlCAgain,
    MessageId::FooterWorking,
    MessageId::HelpSectionActions,
    MessageId::HelpSectionClipboard,
    MessageId::HelpSectionEditing,
    MessageId::HelpSectionHelp,
    MessageId::HelpSectionModes,
    MessageId::HelpSectionNavigation,
    MessageId::HelpSectionSessions,
    MessageId::KbScrollTranscript,
    MessageId::KbNavigateHistory,
    MessageId::KbScrollTranscriptAlt,
    MessageId::KbScrollPage,
    MessageId::KbJumpTopBottom,
    MessageId::KbJumpTopBottomEmpty,
    MessageId::KbJumpToolBlocks,
    MessageId::KbMoveCursor,
    MessageId::KbJumpLineStartEnd,
    MessageId::KbDeleteChar,
    MessageId::KbClearDraft,
    MessageId::KbStashDraft,
    MessageId::KbSearchHistory,
    MessageId::KbInsertNewline,
    MessageId::KbSendDraft,
    MessageId::KbCloseMenu,
    MessageId::KbCancelOrExit,
    MessageId::KbShellControls,
    MessageId::KbExitEmpty,
    MessageId::KbCommandPalette,
    MessageId::KbFuzzyFilePicker,
    MessageId::KbCompactInspector,
    MessageId::KbLastMessagePager,
    MessageId::KbSelectedDetails,
    MessageId::KbToolDetailsPager,
    MessageId::KbThinkingPager,
    MessageId::KbLiveTranscript,
    MessageId::KbBacktrackMessage,
    MessageId::KbCompleteCycleModes,
    MessageId::KbJumpPlanAgentYolo,
    MessageId::KbAltJumpPlanAgentYolo,
    MessageId::KbFocusSidebar,
    MessageId::KbTogglePlanAgent,
    MessageId::KbSessionPicker,
    MessageId::KbPasteAttach,
    MessageId::KbCopySelection,
    MessageId::KbContextMenu,
    MessageId::KbAttachPath,
    MessageId::KbHelpOverlay,
    MessageId::KbToggleHelp,
    MessageId::KbToggleHelpSlash,
    MessageId::HelpUsageLabel,
    MessageId::HelpAliasesLabel,
    MessageId::SettingsTitle,
    MessageId::SettingsConfigFile,
    MessageId::ClearConversation,
    MessageId::ClearConversationBusy,
    MessageId::ModelChanged,
    MessageId::LinksTitle,
    MessageId::LinksDashboard,
    MessageId::LinksDocs,
    MessageId::LinksTip,
    MessageId::SubagentsFetching,
    MessageId::HelpUnknownCommand,
    MessageId::HomeDashboardTitle,
    MessageId::HomeModel,
    MessageId::HomeMode,
    MessageId::HomeWorkspace,
    MessageId::HomeHistory,
    MessageId::HomeTokens,
    MessageId::HomeQueued,
    MessageId::HomeSubagents,
    MessageId::HomeSkill,
    MessageId::HomeQuickActions,
    MessageId::HomeQuickLinks,
    MessageId::HomeQuickSkills,
    MessageId::HomeQuickConfig,
    MessageId::HomeQuickSettings,
    MessageId::HomeQuickModel,
    MessageId::HomeQuickSubagents,
    MessageId::HomeQuickTaskList,
    MessageId::HomeQuickHelp,
    MessageId::HomeModeTips,
    MessageId::HomeAgentModeTip,
    MessageId::HomeAgentModeReviewTip,
    MessageId::HomeAgentModeYoloTip,
    MessageId::HomeYoloModeTip,
    MessageId::HomeYoloModeCaution,
    MessageId::HomePlanModeTip,
    MessageId::HomePlanModeChecklistTip,
    MessageId::TuiPanelTranscript,
    MessageId::TuiPanelTranscriptScroll,
    MessageId::TuiPanelComposer,
    MessageId::TuiPanelComposerScroll,
    MessageId::TuiPanelComposerWaitingEdit,
    MessageId::TuiPanelComposerWaitingScroll,
    MessageId::TuiPanelStatus,
    MessageId::TuiPanelLeft,
    MessageId::TuiPanelLht,
    MessageId::TuiPanelLhtFocused,
    MessageId::TuiInspectorTabFiles,
    MessageId::TuiInspectorTabDiff,
    MessageId::TuiInspectorTabAgents,
    MessageId::TuiInspectorTabMcp,
    MessageId::TuiInspectorTabActivity,
    MessageId::TuiInspectorTabContext,
    MessageId::TuiLeftRailSessions,
    MessageId::TuiLeftRailNoSessions,
    MessageId::TuiLeftRailInspector,
    MessageId::TuiLeftRailNavHint,
    MessageId::TuiNewSession,
    MessageId::TuiComposerHintWaitingEdit,
    MessageId::TuiComposerHintWaitingScroll,
    MessageId::TuiComposerHintTypePrompt,
    MessageId::TuiComposerHintScrollMode,
    MessageId::TuiApprovalTitle,
    MessageId::TuiApprovalToolLabel,
    MessageId::TuiApprovalKeyLabel,
    MessageId::TuiApprovalAllow,
    MessageId::TuiApprovalDeny,
    MessageId::TuiApprovalAllowSession,
    MessageId::TuiApprovalSummary,
    MessageId::TuiApprovalDetail,
    MessageId::TuiHelpTitle,
    MessageId::TuiHelpCloseTitle,
    MessageId::TuiHelpSectionFocus,
    MessageId::TuiHelpSectionLeftRail,
    MessageId::TuiHelpSectionRightRail,
    MessageId::TuiHelpSectionChat,
    MessageId::TuiHelpSectionApproval,
    MessageId::TuiHelpSectionGlobal,
    MessageId::TuiHelpSectionLaunch,
    MessageId::TuiHelpSectionTerminalFont,
    MessageId::TuiSlashWorkspace,
    MessageId::TuiSlashCd,
    MessageId::TuiSlashModel,
    MessageId::TuiSlashModelAlias,
    MessageId::TuiSlashLht,
    MessageId::TuiSlashTheme,
    MessageId::TuiSlashNew,
    MessageId::TuiSlashHelp,
    MessageId::TuiSlashAuto,
    MessageId::TuiSlashClear,
    MessageId::TuiInspectorHintFiles,
    MessageId::TuiInspectorHintDiff,
    MessageId::TuiInspectorHintAgents,
    MessageId::TuiInspectorHintMcp,
    MessageId::TuiInspectorHintActivity,
    MessageId::TuiInspectorHintContext,
    MessageId::TuiTranscriptEmpty,
    MessageId::TuiResumedThread,
    MessageId::TuiAutoTitle,
    MessageId::TuiAutoListHint,
    MessageId::TuiAutoEditRule,
    MessageId::TuiAutoNewRule,
    MessageId::TuiAutoEditHint,
    MessageId::TuiAutoName,
    MessageId::TuiAutoTrigger,
    MessageId::TuiAutoSeconds,
    MessageId::TuiAutoToolFilter,
    MessageId::TuiAutoAnyTool,
    MessageId::TuiAutoAction,
    MessageId::TuiAutoPrompt,
    MessageId::TuiAutoShellCmd,
    MessageId::TuiAutoMessage,
    MessageId::TuiAutoCommand,
    MessageId::TuiSlashLocale,
    MessageId::TuiSlashLanguage,
    MessageId::TuiSlashApiKey,
    MessageId::TuiSlashKey,
    MessageId::TuiSlashLogin,
    MessageId::TuiSlashLogout,
    MessageId::TuiSlashApprove,
    MessageId::TuiSlashApproval,
    MessageId::TuiApiKeyCleared,
    MessageId::TuiApiKeyUsage,
    MessageId::TuiLocalePickerHint,
    MessageId::TuiLocaleChanged,
    MessageId::TuiPendingInputsTitle,
    MessageId::TuiPendingQueuedKind,
    MessageId::TuiPendingEditHint,
    MessageId::TuiSteerInjected,
    MessageId::TuiOnboardingTitle,
    MessageId::TuiOnboardingWelcomeTitle,
    MessageId::TuiOnboardingWelcomeBody,
    MessageId::TuiOnboardingWorkspace,
    MessageId::TuiOnboardingKeyTitle,
    MessageId::TuiOnboardingKeyHint,
    MessageId::TuiOnboardingModeTitle,
    MessageId::TuiOnboardingModeAuto,
    MessageId::TuiOnboardingModeAutoDesc,
    MessageId::TuiOnboardingModeCode,
    MessageId::TuiOnboardingModeCodeDesc,
    MessageId::TuiOnboardingModeOffice,
    MessageId::TuiOnboardingModeOfficeDesc,
    MessageId::TuiOnboardingFooter,
    MessageId::TuiOnboardingStepWelcome,
    MessageId::TuiOnboardingStepKey,
    MessageId::TuiOnboardingStepMode,
    MessageId::TuiOnboardingKeySaved,
    MessageId::TuiOnboardingComplete,
    MessageId::TuiSlashMcp,
    MessageId::TuiMcpTitle,
    MessageId::TuiMcpPathLabel,
    MessageId::TuiMcpSave,
    MessageId::TuiMcpCancel,
    MessageId::TuiMcpFooter,
    MessageId::TuiMcpSaved,
    MessageId::TuiMcpParseError,
    MessageId::TuiMcpEmptyError,
];

pub fn tr(locale: Locale, id: MessageId) -> &'static str {
    fallback_translation(translation(locale, id), id)
}

#[allow(dead_code)]
pub fn missing_message_ids(locale: Locale) -> Vec<MessageId> {
    ALL_MESSAGE_IDS
        .iter()
        .copied()
        .filter(|id| translation(locale, *id).is_none())
        .collect()
}

pub fn normalize_configured_locale(input: &str) -> Option<&'static str> {
    let normalized = normalize_locale_input(input);
    if matches!(normalized.as_str(), "" | "auto" | "system") {
        return Some("auto");
    }
    parse_locale(&normalized).map(Locale::tag)
}

pub fn resolve_locale(setting: &str) -> Locale {
    resolve_locale_with_env(setting, |key| std::env::var(key).ok())
}

pub fn resolve_locale_with_env<F>(setting: &str, env: F) -> Locale
where
    F: Fn(&str) -> Option<String>,
{
    let normalized = normalize_locale_input(setting);
    if !matches!(normalized.as_str(), "" | "auto" | "system") {
        return parse_locale(&normalized).unwrap_or(Locale::En);
    }

    for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Some(value) = env(key)
            && let Some(locale) = parse_locale(&normalize_locale_input(&value))
        {
            return locale;
        }
    }

    Locale::En
}

#[allow(dead_code)]
pub fn truncate_to_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }

    let ellipsis_width = '…'.width().unwrap_or(1);
    if max_width <= ellipsis_width {
        return "…".to_string();
    }

    let limit = max_width - ellipsis_width;
    let mut out = String::new();
    let mut width = 0usize;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if width + ch_width > limit {
            break;
        }
        out.push(ch);
        width += ch_width;
    }
    out.push('…');
    out
}

fn normalize_locale_input(input: &str) -> String {
    input
        .split('.')
        .next()
        .unwrap_or(input)
        .split('@')
        .next()
        .unwrap_or(input)
        .trim()
        .replace('_', "-")
        .to_lowercase()
}

fn parse_locale(value: &str) -> Option<Locale> {
    if value == "c" || value == "posix" || value.starts_with("en") {
        return Some(Locale::En);
    }
    if value.starts_with("ja") {
        return Some(Locale::Ja);
    }
    if value.starts_with("zh") {
        if value.contains("hant")
            || value.contains("-tw")
            || value.contains("-hk")
            || value.contains("-mo")
        {
            return None;
        }
        return Some(Locale::ZhHans);
    }
    if value.starts_with("pt") || value == "br" {
        return Some(Locale::PtBr);
    }
    None
}

fn fallback_translation(candidate: Option<&'static str>, id: MessageId) -> &'static str {
    candidate.unwrap_or_else(|| english(id))
}

fn english(id: MessageId) -> &'static str {
    match id {
        MessageId::ComposerPlaceholder => "Write a task or use /.",
        MessageId::HistorySearchPlaceholder => "Search prompt history...",
        MessageId::HistorySearchTitle => "History Search",
        MessageId::HistoryHintMove => "Up/Down move",
        MessageId::HistoryHintAccept => "Enter accept",
        MessageId::HistoryHintRestore => "Esc restore",
        MessageId::HistoryNoMatches => "  No matches",
        MessageId::ConfigTitle => "Session Configuration",
        MessageId::ConfigModalTitle => " Config ",
        MessageId::ConfigSearchPlaceholder => "type to filter",
        MessageId::ConfigNoSettings => "  No settings available.",
        MessageId::ConfigNoMatchesPrefix => "  No settings match ",
        MessageId::ConfigFilteredSettings => "  Filtered settings",
        MessageId::ConfigShowing => "  Showing",
        MessageId::ConfigFooterDefault => {
            " type=filter, Up/Down=select, Enter/e=edit, Esc/q=close "
        }
        MessageId::ConfigFooterScrollable => {
            " type=filter, Up/Down=select, Enter/e=edit, PgUp/PgDn=scroll, Esc/q=close "
        }
        MessageId::ConfigFooterFiltered => {
            " type=filter, Backspace=delete, Ctrl+U/Esc=clear, Enter=edit "
        }
        MessageId::HelpTitle => "Help",
        MessageId::HelpFilterPlaceholder => "Type to filter",
        MessageId::HelpFilterPrefix => "Filter: ",
        MessageId::HelpNoMatches => "  No matches.",
        MessageId::HelpSlashCommands => "Slash commands",
        MessageId::HelpKeybindings => "Keybindings",
        MessageId::HelpFooterTypeFilter => " type to filter ",
        MessageId::HelpFooterMove => "  Up/Down move ",
        MessageId::HelpFooterJump => " PgUp/PgDn jump ",
        MessageId::HelpFooterClose => " Esc close ",
        MessageId::CmdAgentDescription => "Switch to agent mode",
        MessageId::CmdAnchorDescription => {
            "Pin a fact that survives compaction (auto-injected into context)"
        }
        MessageId::CmdAttachDescription => {
            "Attach image/video media; use @path for text files or directories"
        }
        MessageId::CmdCacheDescription => {
            "Show DeepSeek prefix-cache hit/miss stats for the last N turns"
        }
        MessageId::CmdClearDescription => "Clear conversation history",
        MessageId::CmdCompactDescription => {
            "Archive earlier turns into a reversible [COMPACTED_HISTORY] summary to free up space (manual; large-window models prefer seam/cycle)"
        }
        MessageId::CmdConfigDescription => "Open interactive configuration editor",
        MessageId::CmdContextDescription => "Open compact session context inspector",
        MessageId::CmdCostDescription => "Show session cost breakdown",
        MessageId::CmdCycleDescription => "Show the carry-forward briefing for a specific cycle",
        MessageId::CmdCyclesDescription => "List checkpoint-restart cycle handoffs in this session",
        MessageId::CmdDiffDescription => "Show file changes since session start",
        MessageId::CmdEditDescription => "Revise and resubmit the last message",
        MessageId::CmdExitDescription => "Exit the application",
        MessageId::CmdExportDescription => "Export conversation to markdown",
        MessageId::CmdHelpDescription => "Show help information",
        MessageId::CmdHomeDescription => "Show home dashboard with stats and quick actions",
        MessageId::CmdHooksDescription => "List configured lifecycle hooks (read-only)",
        MessageId::CmdGoalDescription => "Set a session goal with optional token budget",
        MessageId::CmdInitDescription => "Generate AGENTS.md for project",
        MessageId::CmdLspDescription => "Toggle LSP diagnostics on or off",
        MessageId::CmdShareDescription => "Export current session as a shareable web URL",
        MessageId::CmdJobsDescription => "Inspect and control background shell jobs",
        MessageId::CmdLinksDescription => "Show DeepSeek dashboard and docs links",
        MessageId::CmdLoadDescription => "Load session from file",
        MessageId::CmdLogoutDescription => "Clear API key and return to setup",
        MessageId::CmdMcpDescription => "Open or manage MCP servers",
        MessageId::CmdMemoryDescription => "Inspect or manage the persistent user-memory file",
        MessageId::CmdModelDescription => "Switch or view current model",
        MessageId::CmdModelsDescription => "List available models from API",
        MessageId::CmdNetworkDescription => "Manage network allow and deny rules",
        MessageId::CmdNoteDescription => {
            "Append note to persistent notes file (.deepseek/notes.md)"
        }
        MessageId::CmdPlanDescription => {
            "Switch to plan mode and review suggested implementation steps"
        }
        MessageId::CmdProviderDescription => {
            "Switch or view the active LLM backend (deepseek | nvidia-nim | ollama)"
        }
        MessageId::CmdQueueDescription => "View or edit queued messages",
        MessageId::CmdRecallDescription => "Search prior cycle archives (BM25 over message text)",
        MessageId::CmdRenameDescription => "Rename the current session",
        MessageId::CmdRestoreDescription => {
            "Roll back the workspace to a prior pre/post-turn snapshot. With no arg, lists recent snapshots."
        }
        MessageId::CmdRetryDescription => "Retry the last request",
        MessageId::CmdReviewDescription => "Run a structured code review on a file, diff, or PR",
        MessageId::CmdRlmDescription => {
            "Recursive Language Model (RLM) turn — store the prompt in a Python REPL and let the model write code to process it, with `llm_query()` / `sub_rlm()` for sub-LLM calls."
        }
        MessageId::CmdSaveDescription => "Save session to file",
        MessageId::CmdSessionsDescription => "Open session picker",
        MessageId::CmdSettingsDescription => "Show persistent settings",
        MessageId::CmdSkillDescription => {
            "Activate a skill, or install/update/uninstall/trust a community skill"
        }
        MessageId::CmdSkillsDescription => {
            "List local skills (or --remote to browse the curated registry)"
        }
        MessageId::CmdStashDescription => {
            "Park or restore a composer draft (Ctrl+S to push, /stash list/pop)"
        }
        MessageId::CmdStatuslineDescription => "Configure which items appear in the footer",
        MessageId::CmdSubagentsDescription => "List sub-agent status",
        MessageId::CmdSwarmDescription => {
            "Run a multi-agent fanout turn (sequential | mixture | distill | deliberate)"
        }
        MessageId::CmdSystemDescription => "Show current system prompt",
        MessageId::CmdTaskDescription => "Manage background tasks",
        MessageId::CmdTokensDescription => "Show token usage for session",
        MessageId::CmdTrustDescription => {
            "Manage workspace trust and per-path allowlist (`/trust add <path>`, `/trust list`, `/trust on|off`)"
        }
        MessageId::CmdUndoDescription => "Remove last message pair",
        MessageId::CmdYoloDescription => "Enable YOLO mode (shell + trust + auto-approve)",
        MessageId::CmdCacheAdvice => {
            "Hit/miss ratios over ~70% after the third turn indicate a stable cache prefix; \n\
             lower than that on long sessions suggests prefix churn worth investigating (#263)."
        }
        MessageId::CmdCacheFootnote => {
            "* miss inferred from input − hit when the provider did not report it explicitly.\n"
        }
        MessageId::CmdCacheHeader => {
            "Cache telemetry — last {count} of {total} turn(s) (model: {model})\n"
        }
        MessageId::CmdCacheNoData => {
            "Cache history: no turns recorded yet.\n\n\
             DeepSeek surfaces `prompt_cache_hit_tokens` / `prompt_cache_miss_tokens` \
             on every API turn that the model supports it (V4 family). Run a turn \
             and try /cache again."
        }
        MessageId::CmdCacheTotals => {
            "Σ in: {sum_in}   Σ hit: {sum_hit}   Σ miss: {sum_miss}   avg hit ratio: {avg}\n"
        }
        MessageId::CmdCostReport => {
            "Session Cost:\n\
             ─────────────────────────────\n\
             Approx total spent: {cost}\n\n\
             Cost estimates are approximate and use provider usage telemetry when available.\n\n\
             DeepSeek API Pricing:\n\
             ─────────────────────────────\n\
             Pricing details are not configured in this CLI."
        }
        MessageId::CmdTokensCacheBoth => "{hit} hit / {miss} miss",
        MessageId::CmdTokensCacheHitOnly => "{hit} hit / miss not reported",
        MessageId::CmdTokensCacheMissOnly => "hit not reported / {miss} miss",
        MessageId::CmdTokensContextUnknownWindow => "~{estimated} / unknown window",
        MessageId::CmdTokensContextWithWindow => "~{used} / {window} ({percent}%)",
        MessageId::FooterAgentSingular => "1 agent",
        MessageId::FooterAgentsPlural => "{count} agents",
        MessageId::FooterPressCtrlCAgain => "Press Ctrl+C again to quit",
        MessageId::FooterWorking => "working",
        MessageId::HelpSectionActions => "Actions",
        MessageId::HelpSectionClipboard => "Clipboard",
        MessageId::HelpSectionEditing => "Input editing",
        MessageId::HelpSectionHelp => "Help",
        MessageId::HelpSectionModes => "Modes",
        MessageId::HelpSectionNavigation => "Navigation",
        MessageId::HelpSectionSessions => "Sessions",
        MessageId::CmdTokensNotReported => "not reported",
        MessageId::CmdTokensReport => {
            "Token Usage:\n\
             ─────────────────────────────\n\
             Active context:        {active}\n\
             Last API input:        {input} (turn telemetry; may count repeated prefix across tool rounds)\n\
             Last API output:       {output}\n\
             Cache hit/miss:        {cache} (telemetry/cost only)\n\
             Cumulative tokens:     {total} (session usage telemetry)\n\
             Approx session cost:   {cost}\n\
             API messages:          {api_messages}\n\
             Chat messages:         {chat_messages}\n\
             Model:                 {model}"
        }
        MessageId::KbScrollTranscript => {
            "Scroll transcript, navigate input history, or select composer attachments"
        }
        MessageId::KbNavigateHistory => "Navigate input history",
        MessageId::KbScrollTranscriptAlt => "Scroll transcript",
        MessageId::KbScrollPage => "Scroll transcript by page",
        MessageId::KbJumpTopBottom => "Jump to top / bottom of transcript",
        MessageId::KbJumpTopBottomEmpty => "Jump to top / bottom (when input is empty)",
        MessageId::KbJumpToolBlocks => "Jump between tool output blocks",
        MessageId::KbMoveCursor => "Move cursor in composer",
        MessageId::KbJumpLineStartEnd => "Jump to start / end of line",
        MessageId::KbDeleteChar => {
            "Delete character before / after the cursor, or remove selected attachment"
        }
        MessageId::KbClearDraft => "Clear the current draft",
        MessageId::KbStashDraft => "Stash the current draft (`/stash pop` to restore)",
        MessageId::KbSearchHistory => "Search prompt history and recover local drafts",
        MessageId::KbInsertNewline => "Insert a newline in the composer",
        MessageId::KbSendDraft => "Send the current draft",
        MessageId::KbCloseMenu => "Close menu, cancel request, discard draft, or clear input",
        MessageId::KbCancelOrExit => "Cancel request, or exit when idle",
        MessageId::KbShellControls => "Open shell controls for a running foreground command",
        MessageId::KbExitEmpty => "Exit when input is empty",
        MessageId::KbCommandPalette => "Open the command palette",
        MessageId::KbFuzzyFilePicker => "Open the fuzzy file picker (insert @path on Enter)",
        MessageId::KbCompactInspector => "Open compact session context inspector",
        MessageId::KbLastMessagePager => "Open pager for the last message (when input is empty)",
        MessageId::KbSelectedDetails => {
            "Open details for the selected tool or message (when input is empty)"
        }
        MessageId::KbToolDetailsPager => "Open tool-details pager",
        MessageId::KbThinkingPager => "Open thinking pager",
        MessageId::KbLiveTranscript => "Open live transcript overlay (sticky-tail auto-scroll)",
        MessageId::KbBacktrackMessage => {
            "Backtrack to a previous user message (Left/Right step, Enter to rewind)"
        }
        MessageId::KbCompleteCycleModes => {
            "Complete /command, queue running-turn follow-up, cycle modes; Shift+Tab cycles reasoning effort"
        }
        MessageId::KbJumpPlanAgentYolo => "Jump directly to Plan / Agent / YOLO mode",
        MessageId::KbAltJumpPlanAgentYolo => "Alternative jump to Plan / Agent / YOLO mode",
        MessageId::KbFocusSidebar => "Focus Plan / Todos / Tasks / Agents / Auto sidebar",
        MessageId::KbTogglePlanAgent => "Toggle between Plan and Agent modes",
        MessageId::KbSessionPicker => "Open the session picker",
        MessageId::KbPasteAttach => "Paste text or attach a clipboard image",
        MessageId::KbCopySelection => "Copy the current selection (Cmd+C on macOS)",
        MessageId::KbContextMenu => {
            "Open context actions for paste, selection, message details, context, and help"
        }
        MessageId::KbAttachPath => "Add a local text file or directory to context",
        MessageId::KbHelpOverlay => "Open this help overlay (when input is empty)",
        MessageId::KbToggleHelp => "Toggle help overlay",
        MessageId::KbToggleHelpSlash => "Toggle help overlay",
        MessageId::HelpUsageLabel => "Usage:",
        MessageId::HelpAliasesLabel => "Aliases:",
        MessageId::SettingsTitle => "Settings:",
        MessageId::SettingsConfigFile => "Config file:",
        MessageId::ClearConversation => "Conversation cleared",
        MessageId::ClearConversationBusy => {
            "Conversation cleared (plan state busy; run /clear again if needed)"
        }
        MessageId::ModelChanged => "Model changed: {old} \u{2192} {new}",
        MessageId::LinksTitle => "DeepSeek Links:",
        MessageId::LinksDashboard => "Dashboard:",
        MessageId::LinksDocs => "Docs:",
        MessageId::LinksTip => "Tip: API keys are available in the dashboard console.",
        MessageId::SubagentsFetching => "Fetching sub-agent status...",
        MessageId::HelpUnknownCommand => "Unknown command: {topic}",
        MessageId::HomeDashboardTitle => "DeepSeek TUI Home Dashboard",
        MessageId::HomeModel => "Model:",
        MessageId::HomeMode => "Mode:",
        MessageId::HomeWorkspace => "Workspace:",
        MessageId::HomeHistory => "History:",
        MessageId::HomeTokens => "Tokens:",
        MessageId::HomeQueued => "Queued:",
        MessageId::HomeSubagents => "Sub-agents:",
        MessageId::HomeSkill => "Skill:",
        MessageId::HomeQuickActions => "Quick Actions",
        MessageId::HomeQuickLinks => "/links      - Dashboard & API links",
        MessageId::HomeQuickSkills => "/skills      - List available skills",
        MessageId::HomeQuickConfig => "/config      - Open interactive configuration editor",
        MessageId::HomeQuickSettings => "/settings    - Show persistent settings",
        MessageId::HomeQuickModel => "/model       - Switch or view model",
        MessageId::HomeQuickSubagents => "/subagents   - List sub-agent status",
        MessageId::HomeQuickTaskList => "/task list   - Show background task queue",
        MessageId::HomeQuickHelp => "/help        - Show help",
        MessageId::HomeModeTips => "Mode Tips",
        MessageId::HomeAgentModeTip => "Agent mode - Use tools for autonomous tasks",
        MessageId::HomeAgentModeReviewTip => "  Use Ctrl+X to review in Plan mode before executing",
        MessageId::HomeAgentModeYoloTip => "  Type /yolo to enable full tool access",
        MessageId::HomeYoloModeTip => "YOLO mode - Full tool access, no approvals",
        MessageId::HomeYoloModeCaution => "  Be careful with destructive operations!",
        MessageId::HomePlanModeTip => "Plan mode - Design before implementing",
        MessageId::HomePlanModeChecklistTip => "  Use /plan to create structured checklists",
        MessageId::TuiPanelTranscript => " Transcript ",
        MessageId::TuiPanelTranscriptScroll => " Transcript (scroll) ",
        MessageId::TuiPanelComposer => " Composer ",
        MessageId::TuiPanelComposerScroll => " Composer (scroll) ",
        MessageId::TuiPanelComposerWaitingEdit => " Composer (waiting · edit) ",
        MessageId::TuiPanelComposerWaitingScroll => " Composer (waiting · scroll) ",
        MessageId::TuiPanelStatus => " Status ",
        MessageId::TuiPanelLeft => " Left ",
        MessageId::TuiPanelLht => " LHT ",
        MessageId::TuiPanelLhtFocused => " LHT | j/k scroll l toggle i inspector ",
        MessageId::TuiInspectorTabFiles => "Files",
        MessageId::TuiInspectorTabDiff => "Diff",
        MessageId::TuiInspectorTabAgents => "Agents",
        MessageId::TuiInspectorTabMcp => "MCP",
        MessageId::TuiInspectorTabActivity => "Activity",
        MessageId::TuiInspectorTabContext => "Context",
        MessageId::TuiLeftRailSessions => "Sessions",
        MessageId::TuiLeftRailNoSessions => "(no sessions)",
        MessageId::TuiLeftRailInspector => "Inspector",
        MessageId::TuiLeftRailNavHint => "j/k Enter Ctrl+N",
        MessageId::TuiNewSession => "New Session",
        MessageId::TuiComposerHintWaitingEdit => {
            " waiting...  Enter queue  Ctrl+Enter steer  type to continue  Ctrl+C interrupt"
        }
        MessageId::TuiComposerHintWaitingScroll => {
            " waiting...  Esc edit  Enter queue  Ctrl+Enter steer  j/k scroll"
        }
        MessageId::TuiComposerHintTypePrompt => {
            " type prompt...  Ctrl+V paste (recommended)  Shift+Enter newline  Enter send"
        }
        MessageId::TuiComposerHintScrollMode => " Esc edit  j/k scroll transcript  Tab focus panes",
        MessageId::TuiApprovalTitle => " Approval required ",
        MessageId::TuiApprovalToolLabel => "Tool",
        MessageId::TuiApprovalKeyLabel => "Key",
        MessageId::TuiApprovalAllow => "Allow",
        MessageId::TuiApprovalDeny => "Deny",
        MessageId::TuiApprovalAllowSession => "Allow session",
        MessageId::TuiApprovalSummary => "Summary",
        MessageId::TuiApprovalDetail => "Detail",
        MessageId::TuiHelpTitle => "Zagens TUI - shortcuts",
        MessageId::TuiHelpCloseTitle => " Help (? to close) ",
        MessageId::TuiHelpSectionFocus => {
            "Focus\n  Tab / Shift+Tab     Rotate Left / Chat / Right (Right lands on upper inspector)\n  [ / ]               Collapse left / right rail"
        }
        MessageId::TuiHelpSectionLeftRail => {
            "Left rail (sessions)\n  j / k               Select session\n  Enter               Switch session\n  Ctrl+N              New session"
        }
        MessageId::TuiHelpSectionRightRail => {
            "Right rail (inspector + LHT)\n  Tab                 Focus right column\n  1-6                 Files / Diff / Agents / MCP / Activity / Context\n  j / k               Scroll inspector (or LHT pane when focused)\n  Enter               Files: expand dir / preview file / Diff: patch / MCP: tools\n  Esc                 Back from detail view\n  s                   Diff: toggle staged vs worktree\n  - / =               Narrow / widen right rail (saved to tui-layout.toml)\n  l                   Toggle LHT lower pane\n  i                   Focus upper inspector (when LHT visible)"
        }
        MessageId::TuiHelpSectionChat => {
            "Chat\n  Tab                 Input -> scroll transcript -> side columns\n  Shift+Tab           Reverse focus order\n  Esc                 Toggle input / scroll (cancel slash menu when typing /)\n  Enter               Send prompt (input mode)\n  Shift+Enter         Insert newline (input or scroll mode — focuses composer)\n  Up / Down           Cursor up/down line in prompt (history browse at boundary)\n  Left / Right        Move cursor; Ctrl+Left word-jump\n  Home / End          Line start / end\n  Ctrl+W              Delete word backward\n  Ctrl+U              Delete to line start\n  Ctrl+V              Paste from clipboard (multiline; preferred)\n  Shift+Insert        Paste from clipboard (Windows)\n  Note                Terminal right-click paste may warn/split lines — use Ctrl+V\n  /commands           Slash menu - ^v select  Enter run\n  /model <id>         Switch text model (alias /m)\n  /lht [auto|strict|off]  LHT composer mode (empty cycles)\n  /theme [name]       Switch TUI color theme (empty cycles)\n  /approve [policy]   Approval policy (empty cycles; alias /approval)\n  j / k / Up / Down   Scroll transcript (Shift+Enter starts multiline input)\n  PgUp / PgDn         Scroll transcript (auto-enter scroll mode)\n  Ctrl+A              Cycle approval policy (4 modes, saved to config)\n  o                   Expand/collapse last tool block"
        }
        MessageId::TuiHelpSectionApproval => {
            "Approval modal\n  y / Enter           Allow\n  n / Esc             Deny\n  a                   Allow for session\n  v                   Toggle detail view"
        }
        MessageId::TuiHelpSectionGlobal => {
            "Global\n  Ctrl+C              Interrupt turn\n  Ctrl+C twice        Quit\n  Ctrl+Q              Quit\n  ?                   Toggle this help"
        }
        MessageId::TuiHelpSectionLaunch => {
            "Launch (CLI)\n  --fresh             New session; default resumes last session in workspace\n  --mouse-capture     Enable mouse wheel scrolling"
        }
        MessageId::TuiHelpSectionTerminalFont => {
            "Terminal font (recommended)\n  Windows Terminal    Cascadia Mono, JetBrains Mono, Consolas\n  Legacy console      Consolas 11+ or NSimSun for CJK\n  Set in terminal profile - zagens-tui uses your terminal font"
        }
        MessageId::TuiSlashWorkspace => "Switch workspace directory",
        MessageId::TuiSlashCd => "Switch workspace (alias)",
        MessageId::TuiSlashModel => "Switch text model for this session",
        MessageId::TuiSlashModelAlias => "Switch model (alias)",
        MessageId::TuiSlashLht => "LHT mode: auto / strict / off (empty cycles)",
        MessageId::TuiSlashTheme => "Switch TUI color theme (empty cycles)",
        MessageId::TuiSlashNew => "New session in current workspace",
        MessageId::TuiSlashHelp => "Show keyboard shortcuts",
        MessageId::TuiSlashAuto => "Automation rules: hooks, timers, triggers",
        MessageId::TuiSlashClear => "Clear composer input",
        MessageId::TuiInspectorHintFiles => "j/k nav Enter file/dir Esc back",
        MessageId::TuiInspectorHintDiff => "j/k nav Enter patch s staged Esc",
        MessageId::TuiInspectorHintAgents => "j/k nav",
        MessageId::TuiInspectorHintMcp => "j/k nav Enter tools",
        MessageId::TuiInspectorHintActivity => "j/k scroll log",
        MessageId::TuiInspectorHintContext => "j/k scroll breakdown",
        MessageId::TuiTranscriptEmpty => {
            "Transcript empty - type a prompt in Composer and press Enter."
        }
        MessageId::TuiResumedThread => "resumed thread {id}",
        MessageId::TuiAutoTitle => " Automation (/auto) ",
        MessageId::TuiAutoListHint => {
            " j/k move  Space toggle  n new  Enter edit  d delete  e editor  Esc close "
        }
        MessageId::TuiAutoEditRule => " Edit Rule ",
        MessageId::TuiAutoNewRule => " New Rule ",
        MessageId::TuiAutoEditHint => {
            " Tab next  Shift+Tab prev  ←/→ cycle  Enter save  Esc cancel "
        }
        MessageId::TuiAutoName => "Name",
        MessageId::TuiAutoTrigger => "Trigger",
        MessageId::TuiAutoSeconds => "Seconds",
        MessageId::TuiAutoToolFilter => "Tool name",
        MessageId::TuiAutoAnyTool => "any tool (leave blank)",
        MessageId::TuiAutoAction => "Action",
        MessageId::TuiAutoPrompt => "Prompt",
        MessageId::TuiAutoShellCmd => "Shell cmd",
        MessageId::TuiAutoMessage => "Message",
        MessageId::TuiAutoCommand => "Command",
        MessageId::TuiSlashLocale => "Switch UI language (empty cycles)",
        MessageId::TuiSlashLanguage => "Switch UI language (alias)",
        MessageId::TuiSlashApiKey => "Save or clear DeepSeek API key",
        MessageId::TuiSlashKey => "Save or clear API key (alias)",
        MessageId::TuiSlashLogin => "Save DeepSeek API key (CLI alias)",
        MessageId::TuiSlashLogout => "Clear saved DeepSeek API key",
        MessageId::TuiSlashApprove => {
            "Approval policy: on-request / untrusted / never / auto (empty cycles)"
        }
        MessageId::TuiSlashApproval => "Approval policy (alias)",
        MessageId::TuiApiKeyCleared => "API key cleared",
        MessageId::TuiApiKeyUsage => "/api-key sk-… save · /api-key clear or /logout remove",
        MessageId::TuiLocalePickerHint => {
            " Locale | ^v select  Enter apply  empty /locale cycles  Esc cancel "
        }
        MessageId::TuiLocaleChanged => {
            "locale: {locale} (UI updated; model replies follow on next turn)"
        }
        MessageId::TuiPendingInputsTitle => "Pending inputs",
        MessageId::TuiPendingQueuedKind => "Queued follow-up",
        MessageId::TuiPendingEditHint => " ↑ edit last queued message",
        MessageId::TuiSteerInjected => "steer: injected into current turn",
        MessageId::TuiOnboardingTitle => "Setup",
        MessageId::TuiOnboardingWelcomeTitle => "Welcome to Zagens",
        MessageId::TuiOnboardingWelcomeBody => {
            "Get started in a few steps — configure your API key and default mode."
        }
        MessageId::TuiOnboardingWorkspace => "Workspace:",
        MessageId::TuiOnboardingKeyTitle => "Enter your DeepSeek API key",
        MessageId::TuiOnboardingKeyHint => "Stored only on this machine. Esc to skip this step.",
        MessageId::TuiOnboardingModeTitle => "Choose a default mode",
        MessageId::TuiOnboardingModeAuto => "Auto",
        MessageId::TuiOnboardingModeAutoDesc => "Zagens picks code vs office from the task.",
        MessageId::TuiOnboardingModeCode => "Code",
        MessageId::TuiOnboardingModeCodeDesc => {
            "Engineering: files, shell, long-horizon refactors."
        }
        MessageId::TuiOnboardingModeOffice => "Office",
        MessageId::TuiOnboardingModeOfficeDesc => {
            "Documents: writing, spreadsheets, reports, slides."
        }
        MessageId::TuiOnboardingFooter => "Enter next · Esc back · Esc on key step skips",
        MessageId::TuiOnboardingStepWelcome => "Welcome",
        MessageId::TuiOnboardingStepKey => "API Key",
        MessageId::TuiOnboardingStepMode => "Mode",
        MessageId::TuiOnboardingKeySaved => "API key saved",
        MessageId::TuiOnboardingComplete => "Setup complete — happy building!",
        MessageId::TuiSlashMcp => "Edit MCP servers JSON (mcp.json)",
        MessageId::TuiMcpTitle => "MCP config",
        MessageId::TuiMcpPathLabel => "File:",
        MessageId::TuiMcpSave => "Save",
        MessageId::TuiMcpCancel => "Cancel",
        MessageId::TuiMcpFooter => {
            "Type or paste JSON · Ctrl+V paste · Tab focus · Enter save/cancel · Ctrl+S save · Esc cancel"
        }
        MessageId::TuiMcpSaved => "MCP config saved (applies on next turn)",
        MessageId::TuiMcpParseError => "Invalid JSON",
        MessageId::TuiMcpEmptyError => "MCP config cannot be empty",
    }
}

fn translation(locale: Locale, id: MessageId) -> Option<&'static str> {
    match locale {
        Locale::En => Some(english(id)),
        Locale::Ja => japanese(id),
        Locale::ZhHans => chinese_simplified(id),
        Locale::PtBr => portuguese_brazil(id),
    }
}

fn japanese(id: MessageId) -> Option<&'static str> {
    Some(match id {
        MessageId::ComposerPlaceholder => "タスクを書くか / を使う。",
        MessageId::HistorySearchPlaceholder => "プロンプト履歴を検索...",
        MessageId::HistorySearchTitle => "履歴検索",
        MessageId::HistoryHintMove => "Up/Down 移動",
        MessageId::HistoryHintAccept => "Enter 確定",
        MessageId::HistoryHintRestore => "Esc 復元",
        MessageId::HistoryNoMatches => "  一致なし",
        MessageId::ConfigTitle => "セッション設定",
        MessageId::ConfigModalTitle => " 設定 ",
        MessageId::ConfigSearchPlaceholder => "入力して絞り込み",
        MessageId::ConfigNoSettings => "  設定がありません。",
        MessageId::ConfigNoMatchesPrefix => "  一致する設定なし: ",
        MessageId::ConfigFilteredSettings => "  絞り込み後の設定",
        MessageId::ConfigShowing => "  表示",
        MessageId::ConfigFooterDefault => {
            " 入力=絞り込み, Up/Down=選択, Enter/e=編集, Esc/q=閉じる "
        }
        MessageId::ConfigFooterScrollable => {
            " 入力=絞り込み, Up/Down=選択, Enter/e=編集, PgUp/PgDn=スクロール, Esc/q=閉じる "
        }
        MessageId::ConfigFooterFiltered => {
            " 入力=絞り込み, Backspace=削除, Ctrl+U/Esc=クリア, Enter=編集 "
        }
        MessageId::HelpTitle => "ヘルプ",
        MessageId::HelpFilterPlaceholder => "入力して絞り込み",
        MessageId::HelpFilterPrefix => "絞り込み: ",
        MessageId::HelpNoMatches => "  一致なし。",
        MessageId::HelpSlashCommands => "スラッシュコマンド",
        MessageId::HelpKeybindings => "キー操作",
        MessageId::HelpFooterTypeFilter => " 入力して絞り込み ",
        MessageId::HelpFooterMove => "  Up/Down 移動 ",
        MessageId::HelpFooterJump => " PgUp/PgDn ジャンプ ",
        MessageId::HelpFooterClose => " Esc 閉じる ",
        MessageId::CmdAgentDescription => "Agent モードに切り替え",
        MessageId::CmdAnchorDescription => {
            "コンパクション後も保持される重要な事実をピン留め（コンテキストに自動注入）"
        }
        MessageId::CmdAttachDescription => {
            "画像・動画メディアを添付（テキストファイルやディレクトリは @path）"
        }
        MessageId::CmdCacheDescription => {
            "直近 N ターンの DeepSeek プレフィックスキャッシュのヒット/ミス統計を表示"
        }
        MessageId::CmdClearDescription => "会話履歴をクリア",
        MessageId::CmdCompactDescription => {
            "以前の会話を可逆的な [COMPACTED_HISTORY] 要約へアーカイブして容量を確保（手動。大窓モデルは seam/サイクルを優先）"
        }
        MessageId::CmdConfigDescription => "インタラクティブな設定エディタを開く",
        MessageId::CmdContextDescription => "コンパクトなセッションコンテキスト検査ツールを開く",
        MessageId::CmdCostDescription => "セッションのコスト内訳を表示",
        MessageId::CmdCycleDescription => "指定したサイクルの引き継ぎブリーフィングを表示",
        MessageId::CmdCyclesDescription => {
            "セッション内のチェックポイント再起動サイクルの引き継ぎを一覧表示"
        }
        MessageId::CmdDiffDescription => "セッション開始以降のファイル変更を表示",
        MessageId::CmdEditDescription => "最後のメッセージを編集して再送信",
        MessageId::CmdExitDescription => "アプリを終了",
        MessageId::CmdExportDescription => "会話を Markdown にエクスポート",
        MessageId::CmdHelpDescription => "ヘルプを表示",
        MessageId::CmdHomeDescription => "統計とクイックアクション付きのホームダッシュボードを表示",
        MessageId::CmdHooksDescription => {
            "設定済みのライフサイクルフックを一覧表示（読み取り専用）"
        }
        MessageId::CmdGoalDescription => "トークンバジェット付きのセッション目標を設定",
        MessageId::CmdInitDescription => "プロジェクト用に AGENTS.md を生成",
        MessageId::CmdLspDescription => "LSP 診断のオン・オフを切り替え",
        MessageId::CmdShareDescription => "現在のセッションを共有可能な Web URL としてエクスポート",
        MessageId::CmdJobsDescription => "バックグラウンドのシェルジョブを確認・制御",
        MessageId::CmdLinksDescription => "DeepSeek ダッシュボードとドキュメントへのリンクを表示",
        MessageId::CmdLoadDescription => "ファイルからセッションを読み込み",
        MessageId::CmdLogoutDescription => "API キーを消去してセットアップに戻る",
        MessageId::CmdMcpDescription => "MCP サーバを開く・管理する",
        MessageId::CmdMemoryDescription => "永続ユーザーメモリファイルを確認・管理",
        MessageId::CmdModelDescription => "現在のモデルを切り替え・確認",
        MessageId::CmdModelsDescription => "API から利用可能なモデルを一覧表示",
        MessageId::CmdNetworkDescription => "ネットワーク許可・拒否ルールを管理",
        MessageId::CmdNoteDescription => "永続ノートファイル（.deepseek/notes.md）に追記",
        MessageId::CmdPlanDescription => "Plan モードに切り替え、推奨される実装手順を確認",
        MessageId::CmdProviderDescription => {
            "現在の LLM バックエンドを切り替え・確認（deepseek | nvidia-nim | ollama）"
        }
        MessageId::CmdQueueDescription => "キューされたメッセージを確認・編集",
        MessageId::CmdRecallDescription => {
            "過去のサイクルアーカイブを検索（メッセージ本文への BM25 検索）"
        }
        MessageId::CmdRenameDescription => "現在のセッションの名前を変更",
        MessageId::CmdRestoreDescription => {
            "ワークスペースを以前のターン前/後スナップショットへロールバック。引数なしで最近のスナップショットを一覧表示。"
        }
        MessageId::CmdRetryDescription => "直前のリクエストを再試行",
        MessageId::CmdReviewDescription => "ファイル・diff・PR に対して構造化コードレビューを実行",
        MessageId::CmdRlmDescription => {
            "再帰言語モデル（RLM）ターン — プロンプトを Python REPL に格納し、モデルが処理コードを記述。サブ LLM 呼び出しは `llm_query()` / `sub_rlm()`。"
        }
        MessageId::CmdSaveDescription => "セッションをファイルに保存",
        MessageId::CmdSessionsDescription => "セッションピッカーを開く",
        MessageId::CmdSettingsDescription => "永続化された設定を表示",
        MessageId::CmdSkillDescription => {
            "スキルを有効化、またはコミュニティスキルをインストール／更新／アンインストール／信頼"
        }
        MessageId::CmdSkillsDescription => {
            "ローカルスキルを一覧表示（--remote で精選レジストリを参照）"
        }
        MessageId::CmdStashDescription => {
            "コンポーザーの下書きを退避／復元（Ctrl+S で退避、/stash list|pop）"
        }
        MessageId::CmdStatuslineDescription => "フッターに表示する項目を設定",
        MessageId::CmdSubagentsDescription => "サブエージェントの状態を一覧表示",
        MessageId::CmdSwarmDescription => {
            "マルチエージェントのファンアウトターンを実行（sequential | mixture | distill | deliberate）"
        }
        MessageId::CmdSystemDescription => "現在のシステムプロンプトを表示",
        MessageId::CmdTaskDescription => "バックグラウンドタスクを管理",
        MessageId::CmdTokensDescription => "セッションのトークン使用量を表示",
        MessageId::CmdTrustDescription => {
            "ワークスペースの信頼設定とパス別許可リストを管理（`/trust add <path>`、`/trust list`、`/trust on|off`）"
        }
        MessageId::CmdUndoDescription => "最後のメッセージ対を削除",
        MessageId::CmdYoloDescription => "YOLO モードを有効化（shell + 信頼 + 自動承認）",
        MessageId::CmdCacheAdvice => {
            "3 ターン目以降にヒット率が ~70% 以上で安定していれば、プレフィックスキャッシュは健全。\n\
             長いセッションでこれを下回る場合はプレフィックスのドリフトの可能性あり (#263)。"
        }
        MessageId::CmdCacheFootnote => {
            "* プロバイダがミスを単独で報告しない場合は「入力 − ヒット」から推定。\n"
        }
        MessageId::CmdCacheHeader => {
            "キャッシュテレメトリ — 直近 {count} / {total} ターン（モデル: {model}）\n"
        }
        MessageId::CmdCacheNoData => {
            "キャッシュ履歴: まだターンを記録していません。\n\n\
             DeepSeek は対応モデル (V4 系) の各 API ターンで `prompt_cache_hit_tokens` / \
             `prompt_cache_miss_tokens` を返します。1 ターン実行してから /cache を再度試してください。"
        }
        MessageId::CmdCacheTotals => {
            "Σ 入力: {sum_in}   Σ ヒット: {sum_hit}   Σ ミス: {sum_miss}   平均ヒット率: {avg}\n"
        }
        MessageId::CmdCostReport => {
            "セッション費用:\n\
             ─────────────────────────────\n\
             累計概算: {cost}\n\n\
             費用は概算値。プロバイダの使用量テレメトリがあれば優先して使用します。\n\n\
             DeepSeek API 料金:\n\
             ─────────────────────────────\n\
             本 CLI には詳細な料金表は組み込まれていません。"
        }
        MessageId::CmdTokensCacheBoth => "ヒット {hit} / ミス {miss}",
        MessageId::CmdTokensCacheHitOnly => "ヒット {hit} / ミスは未報告",
        MessageId::CmdTokensCacheMissOnly => "ヒットは未報告 / ミス {miss}",
        MessageId::CmdTokensContextUnknownWindow => "~{estimated} / コンテキスト窓不明",
        MessageId::CmdTokensContextWithWindow => "~{used} / {window} ({percent}%)",
        MessageId::FooterAgentSingular => "1 エージェント",
        MessageId::FooterAgentsPlural => "{count} エージェント",
        MessageId::FooterPressCtrlCAgain => "もう一度 Ctrl+C で終了",
        MessageId::FooterWorking => "処理中",
        MessageId::HelpSectionActions => "操作",
        MessageId::HelpSectionClipboard => "クリップボード",
        MessageId::HelpSectionEditing => "入力編集",
        MessageId::HelpSectionHelp => "ヘルプ",
        MessageId::HelpSectionModes => "モード",
        MessageId::HelpSectionNavigation => "ナビゲーション",
        MessageId::HelpSectionSessions => "セッション",
        MessageId::CmdTokensNotReported => "未報告",
        MessageId::CmdTokensReport => {
            "トークン使用量:\n\
             ─────────────────────────────\n\
             アクティブコンテキスト: {active}\n\
             直近の API 入力:        {input}（ターン単位のテレメトリ。複数回のツール往復で同じプレフィックスが重複してカウントされる場合あり）\n\
             直近の API 出力:        {output}\n\
             キャッシュヒット/ミス:  {cache}（テレメトリ/コスト用のみ）\n\
             累計トークン:           {total}（セッション使用量テレメトリ）\n\
             セッション費用概算:     {cost}\n\
             API メッセージ:         {api_messages}\n\
             チャットメッセージ:     {chat_messages}\n\
             モデル:                 {model}"
        }
        MessageId::KbScrollTranscript => {
            "会話履歴をスクロール、入力履歴を移動、または添付ファイルを選択"
        }
        MessageId::KbNavigateHistory => "入力履歴を移動",
        MessageId::KbScrollTranscriptAlt => "会話履歴をスクロール",
        MessageId::KbScrollPage => "ページ単位で会話履歴をスクロール",
        MessageId::KbJumpTopBottom => "会話履歴の先頭/末尾へジャンプ",
        MessageId::KbJumpTopBottomEmpty => "先頭/末尾へジャンプ（入力が空の時）",
        MessageId::KbJumpToolBlocks => "ツール出力ブロック間をジャンプ",
        MessageId::KbMoveCursor => "コンポーザー内でカーソルを移動",
        MessageId::KbJumpLineStartEnd => "行の先頭/末尾へジャンプ",
        MessageId::KbDeleteChar => "カーソル前/後の文字を削除、または選択中の添付を削除",
        MessageId::KbClearDraft => "現在の下書きをクリア",
        MessageId::KbStashDraft => "現在の下書きをスタッシュ（`/stash pop`で復元）",
        MessageId::KbSearchHistory => "プロンプト履歴を検索してローカル下書きを復元",
        MessageId::KbInsertNewline => "コンポーザーに改行を挿入",
        MessageId::KbSendDraft => "現在の下書きを送信",
        MessageId::KbCloseMenu => {
            "メニューを閉じる、リクエストをキャンセル、下書きを破棄、または入力をクリア"
        }
        MessageId::KbCancelOrExit => "リクエストをキャンセル、またはアイドル時に終了",
        MessageId::KbShellControls => "実行中のフォアグラウンドコマンドのシェル制御を開く",
        MessageId::KbExitEmpty => "入力が空の時に終了",
        MessageId::KbCommandPalette => "コマンドパレットを開く",
        MessageId::KbFuzzyFilePicker => "ファジーファイルピッカーを開く（Enter で @path を挿入）",
        MessageId::KbCompactInspector => "コンパクトなセッションコンテキスト検査ツールを開く",
        MessageId::KbLastMessagePager => "最後のメッセージのページャーを開く（入力が空の時）",
        MessageId::KbSelectedDetails => {
            "選択中のツールまたはメッセージの詳細を開く（入力が空の時）"
        }
        MessageId::KbToolDetailsPager => "ツール詳細のページャーを開く",
        MessageId::KbThinkingPager => "思考内容のページャーを開く",
        MessageId::KbLiveTranscript => "ライブ会話履歴オーバーレイを開く（自動追尾スクロール）",
        MessageId::KbBacktrackMessage => {
            "前のユーザーメッセージに戻る（左右でステップ、Enter で巻き戻し）"
        }
        MessageId::KbCompleteCycleModes => {
            "/command を補完、実行中ターンのフォローアップをキュー、モードを切り替え；Shift+Tab で推論強度を切り替え"
        }
        MessageId::KbJumpPlanAgentYolo => "Plan / Agent / YOLO モードに直接ジャンプ",
        MessageId::KbAltJumpPlanAgentYolo => "Plan / Agent / YOLO モードへの代替ジャンプ",
        MessageId::KbFocusSidebar => "Plan / Todos / Tasks / Agents / Auto サイドバーにフォーカス",
        MessageId::KbTogglePlanAgent => "Plan モードと Agent モードを切り替え",
        MessageId::KbSessionPicker => "セッションピッカーを開く",
        MessageId::KbPasteAttach => "テキストを貼り付けまたはクリップボード画像を添付",
        MessageId::KbCopySelection => "現在の選択をコピー（macOS は Cmd+C）",
        MessageId::KbContextMenu => {
            "貼り付け、選択、メッセージ詳細、コンテキスト、ヘルプのコンテキスト操作を開く"
        }
        MessageId::KbAttachPath => {
            "ローカルのテキストファイルまたはディレクトリをコンテキストに追加"
        }
        MessageId::KbHelpOverlay => "このヘルプオーバーレイを開く（入力が空の時）",
        MessageId::KbToggleHelp => "ヘルプオーバーレイを切り替え",
        MessageId::KbToggleHelpSlash => "ヘルプオーバーレイを切り替え",
        MessageId::HelpUsageLabel => "使い方：",
        MessageId::HelpAliasesLabel => "エイリアス：",
        MessageId::SettingsTitle => "設定：",
        MessageId::SettingsConfigFile => "設定ファイル：",
        MessageId::ClearConversation => "会話履歴をクリアしました",
        MessageId::ClearConversationBusy => {
            "会話履歴をクリアしました（plan 状態が忙しい；必要なら /clear を再度実行）"
        }
        MessageId::ModelChanged => "モデルを変更しました: {old} → {new}",
        MessageId::LinksTitle => "DeepSeek リンク：",
        MessageId::LinksDashboard => "ダッシュボード：",
        MessageId::LinksDocs => "ドキュメント：",
        MessageId::LinksTip => "ヒント: API キーはダッシュボードコンソールで取得できます。",
        MessageId::SubagentsFetching => "サブエージェントの状態を取得中...",
        MessageId::HelpUnknownCommand => "不明なコマンド: {topic}",
        MessageId::HomeDashboardTitle => "DeepSeek TUI ホームダッシュボード",
        MessageId::HomeModel => "モデル：",
        MessageId::HomeMode => "モード：",
        MessageId::HomeWorkspace => "ワークスペース：",
        MessageId::HomeHistory => "履歴：",
        MessageId::HomeTokens => "トークン：",
        MessageId::HomeQueued => "キュー：",
        MessageId::HomeSubagents => "サブエージェント：",
        MessageId::HomeSkill => "スキル：",
        MessageId::HomeQuickActions => "クイックアクション",
        MessageId::HomeQuickLinks => "/links      - ダッシュボードと API リンク",
        MessageId::HomeQuickSkills => "/skills      - 利用可能なスキルを一覧",
        MessageId::HomeQuickConfig => "/config      - インタラクティブな設定エディタを開く",
        MessageId::HomeQuickSettings => "/settings    - 永続化された設定を表示",
        MessageId::HomeQuickModel => "/model       - モデルを切り替え・確認",
        MessageId::HomeQuickSubagents => "/subagents   - サブエージェントの状態を一覧",
        MessageId::HomeQuickTaskList => "/task list   - バックグラウンドタスクキューを表示",
        MessageId::HomeQuickHelp => "/help        - ヘルプを表示",
        MessageId::HomeModeTips => "モードヒント",
        MessageId::HomeAgentModeTip => "Agent モード - ツールを使って自律的なタスクを実行",
        MessageId::HomeAgentModeReviewTip => "  実行前に Ctrl+X で Plan モードでレビュー",
        MessageId::HomeAgentModeYoloTip => "  /yolo と入力して完全なツールアクセスを有効化",
        MessageId::HomeYoloModeTip => "YOLO モード - 完全なツールアクセス、承認なし",
        MessageId::HomeYoloModeCaution => "  破壊的な操作には注意してください！",
        MessageId::HomePlanModeTip => "Plan モード - 実装前に設計",
        MessageId::HomePlanModeChecklistTip => "  /plan を使って構造化されたチェックリストを作成",
        MessageId::TuiPanelTranscript => " トランスクリプト ",
        MessageId::TuiPanelTranscriptScroll => " トランスクリプト (スクロール) ",
        MessageId::TuiPanelComposer => " コンポーザー ",
        MessageId::TuiPanelComposerScroll => " コンポーザー (スクロール) ",
        MessageId::TuiPanelComposerWaitingEdit => " コンポーザー (待機 · 編集) ",
        MessageId::TuiPanelComposerWaitingScroll => " コンポーザー (待機 · スクロール) ",
        MessageId::TuiPanelStatus => " ステータス ",
        MessageId::TuiPanelLeft => " 左 ",
        MessageId::TuiPanelLht => " LHT ",
        MessageId::TuiPanelLhtFocused => " LHT | j/k スクロール l 切替 i インスペクタ ",
        MessageId::TuiInspectorTabFiles => "ファイル",
        MessageId::TuiInspectorTabDiff => "Diff",
        MessageId::TuiInspectorTabAgents => "Agents",
        MessageId::TuiInspectorTabMcp => "MCP",
        MessageId::TuiInspectorTabActivity => "アクティビティ",
        MessageId::TuiInspectorTabContext => "Context",
        MessageId::TuiLeftRailSessions => "セッション",
        MessageId::TuiLeftRailNoSessions => "(セッションなし)",
        MessageId::TuiLeftRailInspector => "インスペクタ",
        MessageId::TuiLeftRailNavHint => "j/k Enter Ctrl+N",
        MessageId::TuiNewSession => "新規セッション",
        MessageId::TuiComposerHintWaitingEdit => {
            " 待機中...  Enter キュー  Ctrl+Enter ステア  入力継続  Ctrl+C 中断"
        }
        MessageId::TuiComposerHintWaitingScroll => {
            " 待機中...  Esc 編集  Enter キュー  Ctrl+Enter ステア  j/k スクロール"
        }
        MessageId::TuiComposerHintTypePrompt => {
            " プロンプト入力...  Ctrl+V 貼付  Shift+Enter 改行  Enter 送信"
        }
        MessageId::TuiComposerHintScrollMode => " Esc 編集  j/k スクロール  Tab パネル切替",
        MessageId::TuiApprovalTitle => " 承認が必要 ",
        MessageId::TuiApprovalToolLabel => "ツール",
        MessageId::TuiApprovalKeyLabel => "キー",
        MessageId::TuiApprovalAllow => "許可",
        MessageId::TuiApprovalDeny => "拒否",
        MessageId::TuiApprovalAllowSession => "セッション許可",
        MessageId::TuiApprovalSummary => "概要",
        MessageId::TuiApprovalDetail => "詳細",
        MessageId::TuiHelpTitle => "Zagens TUI - ショートカット",
        MessageId::TuiHelpCloseTitle => " ヘルプ (? 閉じる) ",
        MessageId::TuiHelpSectionFocus => {
            "フォーカス\n  Tab / Shift+Tab     左 / チャット / 右を切替\n  [ / ]               左右レールを折りたたみ"
        }
        MessageId::TuiHelpSectionLeftRail => {
            "左レール (セッション)\n  j / k               セッション選択\n  Enter               セッション切替\n  Ctrl+N              新規セッション"
        }
        MessageId::TuiHelpSectionRightRail => {
            "右レール (インスペクタ + LHT)\n  Tab                 右カラムにフォーカス\n  1-6                 ファイル / Diff / Agents / MCP / アクティビティ / Context\n  j / k               インスペクタをスクロール\n  Enter               ファイル: 展開/プレビュー  Diff: パッチ  MCP: ツール\n  Esc                 詳細から戻る\n  s                   Diff: staged/worktree 切替\n  - / =               右レール幅調整\n  l                   LHT 下ペイン切替\n  i                   上インスペクタにフォーカス"
        }
        MessageId::TuiHelpSectionChat => {
            "チャット\n  Tab                 入力 -> スクロール -> サイド\n  Shift+Tab           逆順フォーカス\n  Esc                 入力/スクロール切替\n  Enter               送信\n  Shift+Enter         改行\n  Ctrl+V              クリップボード貼付\n  /commands           スラッシュメニュー\n  /model <id>         モデル切替\n  /lht [auto|strict|off]  LHT モード\n  /theme [name]       テーマ切替\n  /approve [policy]   承認ポリシー（空で循環）\n  Ctrl+A              承認ポリシー循環\n  o                   最後のツールブロック展開/折畳"
        }
        MessageId::TuiHelpSectionApproval => {
            "承認モーダル\n  y / Enter           許可\n  n / Esc             拒否\n  a                   セッション許可\n  v                   詳細切替"
        }
        MessageId::TuiHelpSectionGlobal => {
            "グローバル\n  Ctrl+C              ターン中断\n  Ctrl+C 2回          終了\n  Ctrl+Q              終了\n  ?                   ヘルプ切替"
        }
        MessageId::TuiHelpSectionLaunch => {
            "起動 (CLI)\n  --fresh             新規セッション\n  --mouse-capture     マウスホイール有効"
        }
        MessageId::TuiHelpSectionTerminalFont => {
            "ターミナルフォント (推奨)\n  Windows Terminal    Cascadia Mono, JetBrains Mono, Consolas\n  レガシーコンソール  Consolas 11+ または NSimSun (CJK)\n  ターミナル設定で指定"
        }
        MessageId::TuiSlashWorkspace => "ワークスペースディレクトリを切替",
        MessageId::TuiSlashCd => "ワークスペース切替（別名）",
        MessageId::TuiSlashModel => "このセッションのテキストモデルを切替",
        MessageId::TuiSlashModelAlias => "モデル切替（別名）",
        MessageId::TuiSlashLht => "LHT モード: auto / strict / off（空で循環）",
        MessageId::TuiSlashTheme => "TUI 配色を切替（空で循環）",
        MessageId::TuiSlashNew => "現在のワークスペースで新規セッション",
        MessageId::TuiSlashHelp => "キーボードショートカットを表示",
        MessageId::TuiSlashAuto => "自動化ルール: フック、タイマー、トリガー",
        MessageId::TuiSlashClear => "コンポーザー入力をクリア",
        MessageId::TuiInspectorHintFiles => "j/k nav Enter file/dir Esc back",
        MessageId::TuiInspectorHintDiff => "j/k nav Enter patch s staged Esc",
        MessageId::TuiInspectorHintAgents => "j/k nav",
        MessageId::TuiInspectorHintMcp => "j/k nav Enter tools",
        MessageId::TuiInspectorHintActivity => "j/k scroll log",
        MessageId::TuiInspectorHintContext => "j/k scroll breakdown",
        MessageId::TuiTranscriptEmpty => "トランスクリプトが空 — コンポーザーに入力して Enter。",
        MessageId::TuiResumedThread => "スレッド {id} を再開",
        MessageId::TuiAutoTitle => " 自動化 (/auto) ",
        MessageId::TuiAutoListHint => {
            " j/k 移動  Space 切替  n 新規  Enter 編集  d 削除  e エディタ  Esc 閉じる "
        }
        MessageId::TuiAutoEditRule => " ルール編集 ",
        MessageId::TuiAutoNewRule => " 新規ルール ",
        MessageId::TuiAutoEditHint => " Tab 次  Shift+Tab 前  ←/→ 循環  Enter 保存  Esc 取消",
        MessageId::TuiAutoName => "名前",
        MessageId::TuiAutoTrigger => "トリガー",
        MessageId::TuiAutoSeconds => "秒",
        MessageId::TuiAutoToolFilter => "ツール名",
        MessageId::TuiAutoAnyTool => "任意のツール（空欄可）",
        MessageId::TuiAutoAction => "アクション",
        MessageId::TuiAutoPrompt => "Prompt",
        MessageId::TuiAutoShellCmd => "Shell コマンド",
        MessageId::TuiAutoMessage => "メッセージ",
        MessageId::TuiAutoCommand => "コマンド",
        MessageId::TuiSlashLocale => "UI 言語を切替（空で循環）",
        MessageId::TuiSlashLanguage => "UI 言語を切替（別名）",
        MessageId::TuiSlashApiKey => "DeepSeek API キーを保存または削除",
        MessageId::TuiSlashKey => "API キー保存/削除（別名）",
        MessageId::TuiSlashLogin => "DeepSeek API キーを保存（CLI 別名）",
        MessageId::TuiSlashLogout => "保存済み DeepSeek API キーを削除",
        MessageId::TuiSlashApprove => {
            "承認ポリシー: on-request / untrusted / never / auto（空で循環）"
        }
        MessageId::TuiSlashApproval => "承認ポリシー（別名）",
        MessageId::TuiApiKeyCleared => "API キーを削除しました",
        MessageId::TuiApiKeyUsage => "/api-key sk-… 保存 · /api-key clear または /logout で削除",
        MessageId::TuiLocalePickerHint => " 言語 | ^v 選択  Enter 適用  空 /locale 循環  Esc 取消 ",
        MessageId::TuiLocaleChanged => "locale: {locale}（UI 更新済み；モデル返答は次ターンから）",
        MessageId::TuiPendingInputsTitle => "保留入力",
        MessageId::TuiPendingQueuedKind => "キュー",
        MessageId::TuiPendingEditHint => " ↑ 最後のキューを編集",
        MessageId::TuiSteerInjected => "steer: 現在ターンに注入しました",
        MessageId::TuiOnboardingTitle => "セットアップ",
        MessageId::TuiOnboardingWelcomeTitle => "Zagens へようこそ",
        MessageId::TuiOnboardingWelcomeBody => {
            "数ステップで始められます — API キーとデフォルトモードを設定してください。"
        }
        MessageId::TuiOnboardingWorkspace => "ワークスペース:",
        MessageId::TuiOnboardingKeyTitle => "DeepSeek API キーを入力",
        MessageId::TuiOnboardingKeyHint => "この端末にのみ保存されます。Esc でスキップ。",
        MessageId::TuiOnboardingModeTitle => "デフォルトモードを選択",
        MessageId::TuiOnboardingModeAuto => "自動",
        MessageId::TuiOnboardingModeAutoDesc => "タスクに応じて code / office を選択。",
        MessageId::TuiOnboardingModeCode => "Code",
        MessageId::TuiOnboardingModeCodeDesc => "開発: ファイル、シェル、大規模リファクタ。",
        MessageId::TuiOnboardingModeOffice => "Office",
        MessageId::TuiOnboardingModeOfficeDesc => {
            "ドキュメント: 執筆、表計算、レポート、スライド。"
        }
        MessageId::TuiOnboardingFooter => "Enter 次へ · Esc 戻る · キー入力で Esc はスキップ",
        MessageId::TuiOnboardingStepWelcome => "ようこそ",
        MessageId::TuiOnboardingStepKey => "API キー",
        MessageId::TuiOnboardingStepMode => "モード",
        MessageId::TuiOnboardingKeySaved => "API キーを保存しました",
        MessageId::TuiOnboardingComplete => "セットアップ完了 — さあ始めましょう！",
        MessageId::TuiSlashMcp => "MCP サーバ JSON を編集 (mcp.json)",
        MessageId::TuiMcpTitle => "MCP 設定",
        MessageId::TuiMcpPathLabel => "ファイル:",
        MessageId::TuiMcpSave => "保存",
        MessageId::TuiMcpCancel => "キャンセル",
        MessageId::TuiMcpFooter => {
            "JSON を入力/貼り付け · Ctrl+V · Tab フォーカス · Enter 保存/取消 · Ctrl+S 保存 · Esc 取消"
        }
        MessageId::TuiMcpSaved => "MCP 設定を保存しました（次のターンから反映）",
        MessageId::TuiMcpParseError => "JSON が無効です",
        MessageId::TuiMcpEmptyError => "MCP 設定を空にできません",
    })
}

fn chinese_simplified(id: MessageId) -> Option<&'static str> {
    Some(match id {
        MessageId::ComposerPlaceholder => "编写任务或使用 /。",
        MessageId::HistorySearchPlaceholder => "搜索提示历史...",
        MessageId::HistorySearchTitle => "历史搜索",
        MessageId::HistoryHintMove => "Up/Down 移动",
        MessageId::HistoryHintAccept => "Enter 接受",
        MessageId::HistoryHintRestore => "Esc 还原",
        MessageId::HistoryNoMatches => "  无匹配",
        MessageId::ConfigTitle => "会话配置",
        MessageId::ConfigModalTitle => " 配置 ",
        MessageId::ConfigSearchPlaceholder => "输入以筛选",
        MessageId::ConfigNoSettings => "  没有可用设置。",
        MessageId::ConfigNoMatchesPrefix => "  没有匹配设置: ",
        MessageId::ConfigFilteredSettings => "  已筛选设置",
        MessageId::ConfigShowing => "  显示",
        MessageId::ConfigFooterDefault => " 输入=筛选, Up/Down=选择, Enter/e=编辑, Esc/q=关闭 ",
        MessageId::ConfigFooterScrollable => {
            " 输入=筛选, Up/Down=选择, Enter/e=编辑, PgUp/PgDn=滚动, Esc/q=关闭 "
        }
        MessageId::ConfigFooterFiltered => {
            " 输入=筛选, Backspace=删除, Ctrl+U/Esc=清除, Enter=编辑 "
        }
        MessageId::HelpTitle => "帮助",
        MessageId::HelpFilterPlaceholder => "输入以筛选",
        MessageId::HelpFilterPrefix => "筛选: ",
        MessageId::HelpNoMatches => "  无匹配。",
        MessageId::HelpSlashCommands => "斜杠命令",
        MessageId::HelpKeybindings => "快捷键",
        MessageId::HelpFooterTypeFilter => " 输入以筛选 ",
        MessageId::HelpFooterMove => "  Up/Down 移动 ",
        MessageId::HelpFooterJump => " PgUp/PgDn 跳转 ",
        MessageId::HelpFooterClose => " Esc 关闭 ",
        MessageId::CmdAgentDescription => "切换到 Agent 模式",
        MessageId::CmdAnchorDescription => "钉选关键事实，在压缩后自动注入上下文",
        MessageId::CmdAttachDescription => "附加图片或视频媒体；文本文件或目录请使用 @path",
        MessageId::CmdCacheDescription => "显示最近 N 轮的 DeepSeek 前缀缓存命中/未命中统计",
        MessageId::CmdClearDescription => "清除对话历史",
        MessageId::CmdCompactDescription => {
            "将较早的对话归档为可逆的 [COMPACTED_HISTORY] 摘要以释放空间（手动命令；大窗口模型优先使用 seam/循环）"
        }
        MessageId::CmdConfigDescription => "打开交互式配置编辑器",
        MessageId::CmdContextDescription => "打开紧凑会话上下文检查器",
        MessageId::CmdCostDescription => "显示本次会话的费用明细",
        MessageId::CmdCycleDescription => "显示指定循环的延续简报",
        MessageId::CmdCyclesDescription => "列出本次会话中的检查点重启循环交接",
        MessageId::CmdDiffDescription => "显示会话开始以来的文件变更",
        MessageId::CmdEditDescription => "修改并重新提交最后一条消息",
        MessageId::CmdExitDescription => "退出应用",
        MessageId::CmdExportDescription => "将对话导出为 Markdown",
        MessageId::CmdHelpDescription => "显示帮助信息",
        MessageId::CmdHomeDescription => "显示主页面板，含统计与快捷操作",
        MessageId::CmdHooksDescription => "列出已配置的生命周期钩子（只读）",
        MessageId::CmdGoalDescription => "设置带有可选令牌预算的会话目标",
        MessageId::CmdInitDescription => "为项目生成 AGENTS.md",
        MessageId::CmdLspDescription => "切换 LSP 诊断的开启或关闭",
        MessageId::CmdShareDescription => "将当前会话导出为可共享的 Web URL",
        MessageId::CmdJobsDescription => "查看并管理后台 shell 作业",
        MessageId::CmdLinksDescription => "显示 DeepSeek 控制台与文档链接",
        MessageId::CmdLoadDescription => "从文件加载会话",
        MessageId::CmdLogoutDescription => "清除 API 密钥并返回设置",
        MessageId::CmdMcpDescription => "打开或管理 MCP 服务器",
        MessageId::CmdMemoryDescription => "查看或管理持久用户记忆文件",
        MessageId::CmdModelDescription => "切换或查看当前模型",
        MessageId::CmdModelsDescription => "列出 API 中可用的模型",
        MessageId::CmdNetworkDescription => "管理网络允许和拒绝规则",
        MessageId::CmdNoteDescription => "将笔记追加到持久笔记文件（.deepseek/notes.md）",
        MessageId::CmdPlanDescription => "切换到 Plan 模式并查看建议的实现步骤",
        MessageId::CmdProviderDescription => {
            "切换或查看当前 LLM 后端（deepseek | nvidia-nim | ollama）"
        }
        MessageId::CmdQueueDescription => "查看或编辑已排队的消息",
        MessageId::CmdRecallDescription => "搜索此前的循环归档（基于消息文本的 BM25 检索）",
        MessageId::CmdRenameDescription => "重命名当前会话",
        MessageId::CmdRestoreDescription => {
            "将工作区回滚到此前的轮次前/后快照。不带参数时列出最近的快照。"
        }
        MessageId::CmdRetryDescription => "重试上一次请求",
        MessageId::CmdReviewDescription => "对文件、diff 或 PR 进行结构化代码审查",
        MessageId::CmdRlmDescription => {
            "递归语言模型（RLM）轮次 —— 将提示词存入 Python REPL，让模型编写代码进行处理；可用 `llm_query()` / `sub_rlm()` 调用子 LLM。"
        }
        MessageId::CmdSaveDescription => "将会话保存到文件",
        MessageId::CmdSessionsDescription => "打开会话选择器",
        MessageId::CmdSettingsDescription => "显示持久化设置",
        MessageId::CmdSkillDescription => "激活技能，或安装/更新/卸载/信任社区技能",
        MessageId::CmdSkillsDescription => "列出本地技能（或使用 --remote 浏览精选注册表）",
        MessageId::CmdStashDescription => "暂存或恢复输入草稿（Ctrl+S 暂存，/stash list|pop）",
        MessageId::CmdStatuslineDescription => "配置底栏要显示哪些条目",
        MessageId::CmdSubagentsDescription => "列出子代理状态",
        MessageId::CmdSwarmDescription => {
            "运行多代理扇出轮次（sequential | mixture | distill | deliberate）"
        }
        MessageId::CmdSystemDescription => "显示当前系统提示词",
        MessageId::CmdTaskDescription => "管理后台任务",
        MessageId::CmdTokensDescription => "显示本次会话的 token 用量",
        MessageId::CmdTrustDescription => {
            "管理工作区信任与按路径的白名单（`/trust add <path>`、`/trust list`、`/trust on|off`）"
        }
        MessageId::CmdUndoDescription => "移除最后一组消息对",
        MessageId::CmdYoloDescription => "启用 YOLO 模式（shell + 信任 + 自动批准）",
        MessageId::CmdCacheAdvice => {
            "第 3 轮起命中率稳定在 ~70% 以上即表示前缀缓存稳定；\n\
             长会话中明显偏低则意味着前缀有抖动，值得排查（#263）。"
        }
        MessageId::CmdCacheFootnote => "* 当提供方未单独上报未命中时，由「输入 − 命中」推算。\n",
        MessageId::CmdCacheHeader => "缓存遥测 —— 最近 {count} / {total} 轮（模型：{model}）\n",
        MessageId::CmdCacheNoData => {
            "缓存历史：尚未记录任何轮次。\n\n\
             DeepSeek 在受支持的模型（V4 系列）每个 API 轮次都会返回 `prompt_cache_hit_tokens` / \
             `prompt_cache_miss_tokens`。请先运行一个轮次再试 /cache。"
        }
        MessageId::CmdCacheTotals => {
            "Σ 输入：{sum_in}   Σ 命中：{sum_hit}   Σ 未命中：{sum_miss}   平均命中率：{avg}\n"
        }
        MessageId::CmdCostReport => {
            "会话费用：\n\
             ─────────────────────────────\n\
             预估累计消耗：{cost}\n\n\
             费用为估算值；如有提供方用量遥测会优先使用。\n\n\
             DeepSeek API 计费：\n\
             ─────────────────────────────\n\
             此 CLI 中未配置详细计费规则。"
        }
        MessageId::CmdTokensCacheBoth => "命中 {hit} / 未命中 {miss}",
        MessageId::CmdTokensCacheHitOnly => "命中 {hit} / 未命中未上报",
        MessageId::CmdTokensCacheMissOnly => "命中未上报 / 未命中 {miss}",
        MessageId::CmdTokensContextUnknownWindow => "~{estimated} / 窗口未知",
        MessageId::CmdTokensContextWithWindow => "~{used} / {window}（{percent}%）",
        MessageId::FooterAgentSingular => "1 个子代理",
        MessageId::FooterAgentsPlural => "{count} 个子代理",
        MessageId::FooterPressCtrlCAgain => "再次按 Ctrl+C 退出",
        MessageId::FooterWorking => "工作中",
        MessageId::HelpSectionActions => "操作",
        MessageId::HelpSectionClipboard => "剪贴板",
        MessageId::HelpSectionEditing => "输入编辑",
        MessageId::HelpSectionHelp => "帮助",
        MessageId::HelpSectionModes => "模式",
        MessageId::HelpSectionNavigation => "导航",
        MessageId::HelpSectionSessions => "会话",
        MessageId::CmdTokensNotReported => "未上报",
        MessageId::CmdTokensReport => {
            "令牌用量：\n\
             ─────────────────────────────\n\
             活动上下文：       {active}\n\
             上次 API 输入：    {input}（来自轮次遥测；多轮工具调用中相同前缀可能被重复计入）\n\
             上次 API 输出：    {output}\n\
             缓存命中/未命中：  {cache}（仅用于遥测/计费）\n\
             累计令牌：         {total}（会话用量遥测）\n\
             预估会话费用：     {cost}\n\
             API 消息数：       {api_messages}\n\
             聊天消息数：       {chat_messages}\n\
             模型：             {model}"
        }
        MessageId::KbScrollTranscript => "滚动对话记录、浏览输入历史或选择附件",
        MessageId::KbNavigateHistory => "浏览输入历史",
        MessageId::KbScrollTranscriptAlt => "滚动对话记录",
        MessageId::KbScrollPage => "按页滚动对话记录",
        MessageId::KbJumpTopBottom => "跳转到对话顶部/底部",
        MessageId::KbJumpTopBottomEmpty => "跳转到顶部/底部（输入框为空时）",
        MessageId::KbJumpToolBlocks => "在工具输出块之间跳转",
        MessageId::KbMoveCursor => "在输入框中移动光标",
        MessageId::KbJumpLineStartEnd => "跳转到行首/行尾",
        MessageId::KbDeleteChar => "删除光标前/后的字符，或移除已选附件",
        MessageId::KbClearDraft => "清空当前草稿",
        MessageId::KbStashDraft => "暂存当前草稿（用 `/stash pop` 恢复）",
        MessageId::KbSearchHistory => "搜索提示历史并恢复本地草稿",
        MessageId::KbInsertNewline => "在输入框中插入换行",
        MessageId::KbSendDraft => "发送当前草稿",
        MessageId::KbCloseMenu => "关闭菜单、取消请求、丢弃草稿或清空输入",
        MessageId::KbCancelOrExit => "取消请求，或空闲时退出",
        MessageId::KbShellControls => "打开正在运行的前台命令的 shell 控制",
        MessageId::KbExitEmpty => "输入框为空时退出",
        MessageId::KbCommandPalette => "打开命令面板",
        MessageId::KbFuzzyFilePicker => "打开模糊文件选择器（按 Enter 插入 @path）",
        MessageId::KbCompactInspector => "打开紧凑会话上下文检查器",
        MessageId::KbLastMessagePager => "打开最后一条消息的分页器（输入框为空时）",
        MessageId::KbSelectedDetails => "打开选中工具或消息的详情（输入框为空时）",
        MessageId::KbToolDetailsPager => "打开工具详情分页器",
        MessageId::KbThinkingPager => "打开思考内容分页器",
        MessageId::KbLiveTranscript => "打开实时对话覆盖层（自动滚动尾随）",
        MessageId::KbBacktrackMessage => "回退到之前的用户消息（左右键步进，Enter 回退）",
        MessageId::KbCompleteCycleModes => {
            "补全 /command、排队运行轮次跟进、切换模式；Shift+Tab 切换推理强度"
        }
        MessageId::KbJumpPlanAgentYolo => "直接跳转到 Plan / Agent / YOLO 模式",
        MessageId::KbAltJumpPlanAgentYolo => "替代快捷键跳转到 Plan / Agent / YOLO 模式",
        MessageId::KbFocusSidebar => "聚焦 Plan / 待办 / 任务 / 代理 / 代理 / 自动侧边栏",
        MessageId::KbTogglePlanAgent => "在 Plan 和 Agent 模式之间切换",
        MessageId::KbSessionPicker => "打开会话选择器",
        MessageId::KbPasteAttach => "粘贴文本或附加剪贴板图片",
        MessageId::KbCopySelection => "复制当前选中内容（macOS 为 Cmd+C）",
        MessageId::KbContextMenu => "打开上下文操作菜单，用于粘贴、选择、消息详情、上下文和帮助",
        MessageId::KbAttachPath => "添加本地文本文件或目录到上下文",
        MessageId::KbHelpOverlay => "打开此帮助覆盖层（输入框为空时）",
        MessageId::KbToggleHelp => "切换帮助覆盖层",
        MessageId::KbToggleHelpSlash => "切换帮助覆盖层",
        MessageId::HelpUsageLabel => "用法：",
        MessageId::HelpAliasesLabel => "别名：",
        MessageId::SettingsTitle => "设置：",
        MessageId::SettingsConfigFile => "配置文件：",
        MessageId::ClearConversation => "对话已清空",
        MessageId::ClearConversationBusy => {
            "对话已清空（Plan 状态忙碌；如需再次清空请运行 /clear）"
        }
        MessageId::ModelChanged => "模型已切换：{old} \u{2192} {new}",
        MessageId::LinksTitle => "DeepSeek 链接：",
        MessageId::LinksDashboard => "控制台：",
        MessageId::LinksDocs => "文档：",
        MessageId::LinksTip => "提示：API 密钥可在控制台中获取。",
        MessageId::SubagentsFetching => "正在获取子代理状态...",
        MessageId::HelpUnknownCommand => "未知命令：{topic}",
        MessageId::HomeDashboardTitle => "DeepSeek TUI 主面板",
        MessageId::HomeModel => "模型：",
        MessageId::HomeMode => "模式：",
        MessageId::HomeWorkspace => "工作区：",
        MessageId::HomeHistory => "历史：",
        MessageId::HomeTokens => "令牌：",
        MessageId::HomeQueued => "队列：",
        MessageId::HomeSubagents => "子代理：",
        MessageId::HomeSkill => "技能：",
        MessageId::HomeQuickActions => "快捷操作",
        MessageId::HomeQuickLinks => "/links      - 控制台与 API 链接",
        MessageId::HomeQuickSkills => "/skills      - 列出可用技能",
        MessageId::HomeQuickConfig => "/config      - 打开交互式配置编辑器",
        MessageId::HomeQuickSettings => "/settings    - 显示持久化设置",
        MessageId::HomeQuickModel => "/model       - 切换或查看模型",
        MessageId::HomeQuickSubagents => "/subagents   - 列出子代理状态",
        MessageId::HomeQuickTaskList => "/task list   - 显示后台任务队列",
        MessageId::HomeQuickHelp => "/help        - 显示帮助",
        MessageId::HomeModeTips => "模式提示",
        MessageId::HomeAgentModeTip => "Agent 模式 - 使用工具执行自主任务",
        MessageId::HomeAgentModeReviewTip => "  按 Ctrl+X 可在 Plan 模式下审查后再执行",
        MessageId::HomeAgentModeYoloTip => "  输入 /yolo 启用完整工具访问",
        MessageId::HomeYoloModeTip => "YOLO 模式 - 完整工具访问，无需审批",
        MessageId::HomeYoloModeCaution => "  请小心破坏性操作！",
        MessageId::HomePlanModeTip => "Plan 模式 - 先设计再实现",
        MessageId::HomePlanModeChecklistTip => "  使用 /plan 创建结构化检查清单",
        MessageId::TuiPanelTranscript => " 对话 ",
        MessageId::TuiPanelTranscriptScroll => " 对话 (滚动) ",
        MessageId::TuiPanelComposer => " 输入 ",
        MessageId::TuiPanelComposerScroll => " 输入 (滚动) ",
        MessageId::TuiPanelComposerWaitingEdit => " 输入 (等待回复 · 编辑) ",
        MessageId::TuiPanelComposerWaitingScroll => " 输入 (等待回复 · 滚动) ",
        MessageId::TuiPanelStatus => " 状态 ",
        MessageId::TuiPanelLeft => " 左侧 ",
        MessageId::TuiPanelLht => " LHT ",
        MessageId::TuiPanelLhtFocused => " LHT | j/k 滚动 l 切换 i 检查器 ",
        MessageId::TuiInspectorTabFiles => "文件",
        MessageId::TuiInspectorTabDiff => "Diff",
        MessageId::TuiInspectorTabAgents => "Agents",
        MessageId::TuiInspectorTabMcp => "MCP",
        MessageId::TuiInspectorTabActivity => "活动",
        MessageId::TuiInspectorTabContext => "Context",
        MessageId::TuiLeftRailSessions => "会话",
        MessageId::TuiLeftRailNoSessions => "(无会话)",
        MessageId::TuiLeftRailInspector => "检查器",
        MessageId::TuiLeftRailNavHint => "j/k Enter Ctrl+N",
        MessageId::TuiNewSession => "新会话",
        MessageId::TuiComposerHintWaitingEdit => {
            " 等待中...  Enter 排队  Ctrl+Enter 注入  可继续输入  Ctrl+C 中断"
        }
        MessageId::TuiComposerHintWaitingScroll => {
            " 等待中...  Esc 编辑  Enter 排队  Ctrl+Enter 注入  j/k 滚动"
        }
        MessageId::TuiComposerHintTypePrompt => {
            " 输入提示...  Ctrl+V 粘贴(推荐)  Shift+Enter 换行  Enter 发送"
        }
        MessageId::TuiComposerHintScrollMode => " Esc 编辑  j/k 滚动对话  Tab 切换面板",
        MessageId::TuiApprovalTitle => " 需要审批 ",
        MessageId::TuiApprovalToolLabel => "工具",
        MessageId::TuiApprovalKeyLabel => "键",
        MessageId::TuiApprovalAllow => "允许",
        MessageId::TuiApprovalDeny => "拒绝",
        MessageId::TuiApprovalAllowSession => "本会话允许",
        MessageId::TuiApprovalSummary => "摘要",
        MessageId::TuiApprovalDetail => "详情",
        MessageId::TuiHelpTitle => "Zagens TUI - 快捷键",
        MessageId::TuiHelpCloseTitle => " 帮助 (? 关闭) ",
        MessageId::TuiHelpSectionFocus => {
            "焦点\n  Tab / Shift+Tab     轮换 左栏 / 对话 / 右栏（右栏聚焦上层检查器）\n  [ / ]               折叠左/右栏"
        }
        MessageId::TuiHelpSectionLeftRail => {
            "左栏 (会话)\n  j / k               选择会话\n  Enter               切换会话\n  Ctrl+N              新建会话"
        }
        MessageId::TuiHelpSectionRightRail => {
            "右栏 (检查器 + LHT)\n  Tab                 聚焦右栏\n  1-6                 文件 / Diff / Agents / MCP / 活动 / Context\n  j / k               滚动检查器（或 LHT 面板）\n  Enter               文件: 展开目录/预览  Diff: 补丁  MCP: 工具\n  Esc                 从详情返回\n  s                   Diff: 切换 staged/worktree\n  - / =               收窄/加宽右栏（保存到 tui-layout.toml）\n  l                   切换 LHT 下方面板\n  i                   聚焦上层检查器（LHT 可见时）"
        }
        MessageId::TuiHelpSectionChat => {
            "对话\n  Tab                 输入 -> 滚动对话 -> 侧栏\n  Shift+Tab           反向切换焦点\n  Esc                 切换输入/滚动（输入 / 时取消斜杠菜单）\n  Enter               发送（输入模式）\n  Shift+Enter         插入换行（输入或滚动模式 — 聚焦输入框）\n  Up / Down           在提示中上下移动光标（边界处浏览历史）\n  Left / Right        移动光标；Ctrl+Left 按词跳转\n  Home / End          行首 / 行尾\n  Ctrl+W              向后删除词\n  Ctrl+U              删除到行首\n  Ctrl+V              从剪贴板粘贴（多行；推荐）\n  Shift+Insert        从剪贴板粘贴（Windows）\n  注意                终端右键粘贴可能警告/拆行 — 请用 Ctrl+V\n  /commands           斜杠菜单 - ^v 选择  Enter 运行\n  /model <id>         切换文本模型（别名 /m）\n  /lht [auto|strict|off]  LHT Composer 模式（空参数循环）\n  /theme [name]       切换 TUI 配色（空参数循环）\n  /approve [policy]   审批策略（空参数循环；别名 /approval）\n  j / k / Up / Down   滚动对话（Shift+Enter 开始多行输入）\n  PgUp / PgDn         滚动对话（自动进入滚动模式）\n  Ctrl+A              循环审批策略（4 种，保存到 config）\n  o                   展开/折叠最后一个工具块"
        }
        MessageId::TuiHelpSectionApproval => {
            "审批弹窗\n  y / Enter           允许\n  n / Esc             拒绝\n  a                   本会话允许\n  v                   切换详情视图"
        }
        MessageId::TuiHelpSectionGlobal => {
            "全局\n  Ctrl+C              中断回合\n  Ctrl+C 两次         退出\n  Ctrl+Q              退出\n  ?                   切换此帮助"
        }
        MessageId::TuiHelpSectionLaunch => {
            "启动 (CLI)\n  --fresh             新会话；默认恢复工作区上次会话\n  --mouse-capture     启用鼠标滚轮滚动"
        }
        MessageId::TuiHelpSectionTerminalFont => {
            "终端字体 (推荐)\n  Windows Terminal    Cascadia Mono, JetBrains Mono, Consolas\n  旧版控制台          Consolas 11+ 或 NSimSun（CJK）\n  在终端配置中设置 — zagens-tui 使用你的终端字体"
        }
        MessageId::TuiSlashWorkspace => "切换工作区目录",
        MessageId::TuiSlashCd => "切换工作区（别名）",
        MessageId::TuiSlashModel => "切换本会话文本模型",
        MessageId::TuiSlashModelAlias => "切换模型（别名）",
        MessageId::TuiSlashLht => "LHT 模式：auto / strict / off（空参数循环）",
        MessageId::TuiSlashTheme => "切换 TUI 配色（空参数循环）",
        MessageId::TuiSlashNew => "在当前工作区新建会话",
        MessageId::TuiSlashHelp => "显示键盘快捷键",
        MessageId::TuiSlashAuto => "自动化规则：钩子、定时器、触发器",
        MessageId::TuiSlashClear => "清空输入框",
        MessageId::TuiInspectorHintFiles => "j/k 导航 Enter 文件/目录 Esc 返回",
        MessageId::TuiInspectorHintDiff => "j/k 导航 Enter 补丁 s staged Esc",
        MessageId::TuiInspectorHintAgents => "j/k 导航",
        MessageId::TuiInspectorHintMcp => "j/k 导航 Enter 工具",
        MessageId::TuiInspectorHintActivity => "j/k 滚动日志",
        MessageId::TuiInspectorHintContext => "j/k 滚动分项",
        MessageId::TuiTranscriptEmpty => "对话为空 — 在输入框中输入提示并按 Enter。",
        MessageId::TuiResumedThread => "已恢复线程 {id}",
        MessageId::TuiAutoTitle => " 自动化 (/auto) ",
        MessageId::TuiAutoListHint => {
            " j/k 移动  Space 开关  n 新建  Enter 编辑  d 删除  e 编辑器  Esc 关闭 "
        }
        MessageId::TuiAutoEditRule => " 编辑规则 ",
        MessageId::TuiAutoNewRule => " 新建规则 ",
        MessageId::TuiAutoEditHint => {
            " Tab 下一项  Shift+Tab 上一项  ←/→ 循环  Enter 保存  Esc 取消 "
        }
        MessageId::TuiAutoName => "名称",
        MessageId::TuiAutoTrigger => "触发器",
        MessageId::TuiAutoSeconds => "秒数",
        MessageId::TuiAutoToolFilter => "工具名",
        MessageId::TuiAutoAnyTool => "任意工具（留空）",
        MessageId::TuiAutoAction => "动作",
        MessageId::TuiAutoPrompt => "Prompt",
        MessageId::TuiAutoShellCmd => "Shell 命令",
        MessageId::TuiAutoMessage => "消息",
        MessageId::TuiAutoCommand => "命令",
        MessageId::TuiSlashLocale => "切换界面语言（空参数循环）",
        MessageId::TuiSlashLanguage => "切换界面语言（别名）",
        MessageId::TuiSlashApiKey => "保存或清除 DeepSeek API 密钥",
        MessageId::TuiSlashKey => "保存或清除 API 密钥（别名）",
        MessageId::TuiSlashLogin => "保存 DeepSeek API 密钥（CLI 别名）",
        MessageId::TuiSlashLogout => "清除已保存的 DeepSeek API 密钥",
        MessageId::TuiSlashApprove => {
            "审批策略：on-request / untrusted / never / auto（空参数循环）"
        }
        MessageId::TuiSlashApproval => "审批策略（别名）",
        MessageId::TuiApiKeyCleared => "API 密钥已清除",
        MessageId::TuiApiKeyUsage => "/api-key sk-… 保存 · /api-key clear 或 /logout 清除",
        MessageId::TuiLocalePickerHint => " 语言 | ^v 选择  Enter 应用  空 /locale 循环  Esc 取消 ",
        MessageId::TuiLocaleChanged => "locale: {locale}（界面已更新；模型回复从下一回合起生效）",
        MessageId::TuiPendingInputsTitle => "待发送输入",
        MessageId::TuiPendingQueuedKind => "排队",
        MessageId::TuiPendingEditHint => " ↑ 编辑最后一条排队消息",
        MessageId::TuiSteerInjected => "steer: 已注入当前回合",
        MessageId::TuiOnboardingTitle => "初始化",
        MessageId::TuiOnboardingWelcomeTitle => "欢迎使用 Zagens",
        MessageId::TuiOnboardingWelcomeBody => "只需几步 — 配置 API 密钥与默认模式即可开始。",
        MessageId::TuiOnboardingWorkspace => "工作区:",
        MessageId::TuiOnboardingKeyTitle => "输入 DeepSeek API 密钥",
        MessageId::TuiOnboardingKeyHint => "仅保存在本机。Esc 可跳过此步。",
        MessageId::TuiOnboardingModeTitle => "选择默认模式",
        MessageId::TuiOnboardingModeAuto => "自动",
        MessageId::TuiOnboardingModeAutoDesc => "根据任务在 code / office 间自动选择。",
        MessageId::TuiOnboardingModeCode => "Code",
        MessageId::TuiOnboardingModeCodeDesc => "工程开发：文件、命令行、长周期重构。",
        MessageId::TuiOnboardingModeOffice => "Office",
        MessageId::TuiOnboardingModeOfficeDesc => "文档办公：写作、表格、报告、演示。",
        MessageId::TuiOnboardingFooter => "Enter 下一步 · Esc 返回 · 密钥页 Esc 跳过",
        MessageId::TuiOnboardingStepWelcome => "欢迎",
        MessageId::TuiOnboardingStepKey => "API 密钥",
        MessageId::TuiOnboardingStepMode => "模式",
        MessageId::TuiOnboardingKeySaved => "API 密钥已保存",
        MessageId::TuiOnboardingComplete => "初始化完成 — 开始使用吧！",
        MessageId::TuiSlashMcp => "编辑 MCP 服务器 JSON（mcp.json）",
        MessageId::TuiMcpTitle => "MCP 配置",
        MessageId::TuiMcpPathLabel => "文件:",
        MessageId::TuiMcpSave => "保存",
        MessageId::TuiMcpCancel => "取消",
        MessageId::TuiMcpFooter => {
            "输入或粘贴 JSON · Ctrl+V 粘贴 · Tab 切换焦点 · Enter 保存/取消 · Ctrl+S 保存 · Esc 取消"
        }
        MessageId::TuiMcpSaved => "MCP 配置已保存（下次对话生效）",
        MessageId::TuiMcpParseError => "JSON 无效",
        MessageId::TuiMcpEmptyError => "MCP 配置不能为空",
    })
}

fn portuguese_brazil(id: MessageId) -> Option<&'static str> {
    Some(match id {
        MessageId::ComposerPlaceholder => "Escreva uma tarefa ou use /.",
        MessageId::HistorySearchPlaceholder => "Pesquisar histórico de prompts...",
        MessageId::HistorySearchTitle => "Busca no histórico",
        MessageId::HistoryHintMove => "Up/Down move",
        MessageId::HistoryHintAccept => "Enter aceita",
        MessageId::HistoryHintRestore => "Esc restaura",
        MessageId::HistoryNoMatches => "  Sem resultados",
        MessageId::ConfigTitle => "Configuração da sessão",
        MessageId::ConfigModalTitle => " Config ",
        MessageId::ConfigSearchPlaceholder => "digite para filtrar",
        MessageId::ConfigNoSettings => "  Nenhuma configuração disponível.",
        MessageId::ConfigNoMatchesPrefix => "  Nenhuma configuração corresponde a ",
        MessageId::ConfigFilteredSettings => "  Configurações filtradas",
        MessageId::ConfigShowing => "  Mostrando",
        MessageId::ConfigFooterDefault => {
            " digite=filtrar, Up/Down=selecionar, Enter/e=editar, Esc/q=fechar "
        }
        MessageId::ConfigFooterScrollable => {
            " digite=filtrar, Up/Down=selecionar, Enter/e=editar, PgUp/PgDn=rolar, Esc/q=fechar "
        }
        MessageId::ConfigFooterFiltered => {
            " digite=filtrar, Backspace=apagar, Ctrl+U/Esc=limpar, Enter=editar "
        }
        MessageId::HelpTitle => "Ajuda",
        MessageId::HelpFilterPlaceholder => "Digite para filtrar",
        MessageId::HelpFilterPrefix => "Filtro: ",
        MessageId::HelpNoMatches => "  Sem resultados.",
        MessageId::HelpSlashCommands => "Comandos com barra",
        MessageId::HelpKeybindings => "Atalhos",
        MessageId::HelpFooterTypeFilter => " digite para filtrar ",
        MessageId::HelpFooterMove => "  Up/Down move ",
        MessageId::HelpFooterJump => " PgUp/PgDn salta ",
        MessageId::HelpFooterClose => " Esc fecha ",
        MessageId::CmdAgentDescription => "Mudar para o modo agent",
        MessageId::CmdAnchorDescription => {
            "Fixar um fato que sobrevive à compactação (injetado automaticamente no contexto)"
        }
        MessageId::CmdAttachDescription => {
            "Anexar imagem ou vídeo; use @path para arquivos de texto ou diretórios"
        }
        MessageId::CmdCacheDescription => {
            "Exibir estatísticas de hit/miss do cache de prefixo DeepSeek nas últimas N rodadas"
        }
        MessageId::CmdClearDescription => "Limpar o histórico da conversa",
        MessageId::CmdCompactDescription => {
            "Arquivar turnos anteriores em um resumo reversível [COMPACTED_HISTORY] para liberar espaço (manual; modelos de janela grande preferem seam/ciclo)"
        }
        MessageId::CmdConfigDescription => "Abrir o editor interativo de configuração",
        MessageId::CmdContextDescription => "Abrir o inspetor compacto de contexto da sessão",
        MessageId::CmdCostDescription => "Exibir o detalhamento de custo da sessão",
        MessageId::CmdCycleDescription => {
            "Exibir o briefing de continuidade de um ciclo específico"
        }
        MessageId::CmdCyclesDescription => {
            "Listar as transferências dos ciclos checkpoint-restart desta sessão"
        }
        MessageId::CmdDiffDescription => "Mostrar alterações em arquivos desde o início da sessão",
        MessageId::CmdEditDescription => "Revisar e reenviar a última mensagem",
        MessageId::CmdExitDescription => "Sair do aplicativo",
        MessageId::CmdExportDescription => "Exportar a conversa para markdown",
        MessageId::CmdHelpDescription => "Exibir informações de ajuda",
        MessageId::CmdHomeDescription => "Exibir o painel inicial com estatísticas e ações rápidas",
        MessageId::CmdHooksDescription => {
            "Listar hooks de ciclo de vida configurados (somente leitura)"
        }
        MessageId::CmdGoalDescription => {
            "Definir uma meta de sessão com orçamento de tokens opcional"
        }
        MessageId::CmdInitDescription => "Gerar AGENTS.md para o projeto",
        MessageId::CmdLspDescription => "Alternar diagnóstico LSP ligado ou desligado",
        MessageId::CmdShareDescription => "Exportar a sessão atual como uma URL web compartilhável",
        MessageId::CmdJobsDescription => "Inspecionar e controlar jobs de shell em segundo plano",
        MessageId::CmdLinksDescription => "Exibir links do painel e da documentação do DeepSeek",
        MessageId::CmdLoadDescription => "Carregar a sessão de um arquivo",
        MessageId::CmdLogoutDescription => "Limpar a chave de API e voltar à configuração",
        MessageId::CmdMcpDescription => "Abrir ou gerenciar servidores MCP",
        MessageId::CmdMemoryDescription => {
            "Inspecionar ou gerenciar o arquivo persistente de memória do usuário"
        }
        MessageId::CmdModelDescription => "Trocar ou exibir o modelo atual",
        MessageId::CmdModelsDescription => "Listar os modelos disponíveis pela API",
        MessageId::CmdNetworkDescription => "Gerenciar regras de rede permitidas e bloqueadas",
        MessageId::CmdNoteDescription => {
            "Adicionar nota ao arquivo persistente (.deepseek/notes.md)"
        }
        MessageId::CmdPlanDescription => {
            "Mudar para o modo plan e revisar os passos de implementação sugeridos"
        }
        MessageId::CmdProviderDescription => {
            "Trocar ou exibir o backend LLM ativo (deepseek | nvidia-nim | ollama)"
        }
        MessageId::CmdQueueDescription => "Ver ou editar mensagens enfileiradas",
        MessageId::CmdRecallDescription => {
            "Buscar arquivos de ciclos anteriores (BM25 sobre o texto das mensagens)"
        }
        MessageId::CmdRenameDescription => "Renomear a sessão atual",
        MessageId::CmdRestoreDescription => {
            "Reverter o workspace a um snapshot pré/pós-turno anterior. Sem argumento, lista os snapshots recentes."
        }
        MessageId::CmdRetryDescription => "Repetir a última requisição",
        MessageId::CmdReviewDescription => {
            "Executar uma revisão de código estruturada em um arquivo, diff ou PR"
        }
        MessageId::CmdRlmDescription => {
            "Turno do Recursive Language Model (RLM) — guarda o prompt em um REPL Python e deixa o modelo escrever o código que o processa; use `llm_query()` / `sub_rlm()` para chamadas a sub-LLMs."
        }
        MessageId::CmdSaveDescription => "Salvar a sessão em arquivo",
        MessageId::CmdSessionsDescription => "Abrir o seletor de sessões",
        MessageId::CmdSettingsDescription => "Exibir as configurações persistidas",
        MessageId::CmdSkillDescription => {
            "Ativar uma skill, ou instalar/atualizar/desinstalar/confiar em uma skill da comunidade"
        }
        MessageId::CmdSkillsDescription => {
            "Listar skills locais (ou --remote para navegar pelo registro curado)"
        }
        MessageId::CmdStashDescription => {
            "Estacionar ou restaurar rascunho do compositor (Ctrl+S estaciona, /stash list|pop)"
        }
        MessageId::CmdStatuslineDescription => "Configurar quais itens aparecem no rodapé",
        MessageId::CmdSubagentsDescription => "Listar o status dos sub-agentes",
        MessageId::CmdSwarmDescription => {
            "Executar turno fanout multi-agente (sequential | mixture | distill | deliberate)"
        }
        MessageId::CmdSystemDescription => "Exibir o prompt de sistema atual",
        MessageId::CmdTaskDescription => "Gerenciar tarefas em segundo plano",
        MessageId::CmdTokensDescription => "Exibir o uso de tokens da sessão",
        MessageId::CmdTrustDescription => {
            "Gerenciar a confiança do workspace e a allowlist por caminho (`/trust add <path>`, `/trust list`, `/trust on|off`)"
        }
        MessageId::CmdUndoDescription => "Remover o último par de mensagens",
        MessageId::CmdYoloDescription => {
            "Ativar o modo YOLO (shell + confiança + aprovação automática)"
        }
        MessageId::CmdCacheAdvice => {
            "Taxas de hit/miss acima de ~70% a partir do terceiro turno indicam um prefixo de cache estável;\n\
             valores menores em sessões longas sugerem instabilidade no prefixo, vale investigar (#263)."
        }
        MessageId::CmdCacheFootnote => {
            "* miss inferido a partir de entrada − hit quando o provedor não o reporta separadamente.\n"
        }
        MessageId::CmdCacheHeader => {
            "Telemetria do cache — últimos {count} de {total} turno(s) (modelo: {model})\n"
        }
        MessageId::CmdCacheNoData => {
            "Histórico do cache: nenhum turno registrado ainda.\n\n\
             O DeepSeek expõe `prompt_cache_hit_tokens` / `prompt_cache_miss_tokens` em cada turno \
             da API onde o modelo suporta (família V4). Execute um turno e tente /cache de novo."
        }
        MessageId::CmdCacheTotals => {
            "Σ entrada: {sum_in}   Σ hit: {sum_hit}   Σ miss: {sum_miss}   taxa média de hit: {avg}\n"
        }
        MessageId::CmdCostReport => {
            "Custo da sessão:\n\
             ─────────────────────────────\n\
             Total aproximado: {cost}\n\n\
             Estimativas de custo são aproximadas e usam a telemetria de uso do provedor quando disponível.\n\n\
             Preços da API DeepSeek:\n\
             ─────────────────────────────\n\
             Os detalhes de preço não estão configurados nesta CLI."
        }
        MessageId::CmdTokensCacheBoth => "{hit} hit / {miss} miss",
        MessageId::CmdTokensCacheHitOnly => "{hit} hit / miss não reportado",
        MessageId::CmdTokensCacheMissOnly => "hit não reportado / {miss} miss",
        MessageId::CmdTokensContextUnknownWindow => "~{estimated} / janela desconhecida",
        MessageId::CmdTokensContextWithWindow => "~{used} / {window} ({percent}%)",
        MessageId::FooterAgentSingular => "1 sub-agente",
        MessageId::FooterAgentsPlural => "{count} sub-agentes",
        MessageId::FooterPressCtrlCAgain => "Pressione Ctrl+C novamente para sair",
        MessageId::FooterWorking => "trabalhando",
        MessageId::HelpSectionActions => "Ações",
        MessageId::HelpSectionClipboard => "Área de transferência",
        MessageId::HelpSectionEditing => "Edição de entrada",
        MessageId::HelpSectionHelp => "Ajuda",
        MessageId::HelpSectionModes => "Modos",
        MessageId::HelpSectionNavigation => "Navegação",
        MessageId::HelpSectionSessions => "Sessões",
        MessageId::CmdTokensNotReported => "não reportado",
        MessageId::CmdTokensReport => {
            "Uso de tokens:\n\
             ─────────────────────────────\n\
             Contexto ativo:           {active}\n\
             Última entrada da API:    {input} (telemetria por turno; pode contar o mesmo prefixo várias vezes em rodadas com ferramentas)\n\
             Última saída da API:      {output}\n\
             Hit/miss do cache:        {cache} (apenas para telemetria/custo)\n\
             Tokens acumulados:        {total} (telemetria de uso da sessão)\n\
             Custo aproximado:         {cost}\n\
             Mensagens da API:         {api_messages}\n\
             Mensagens do chat:        {chat_messages}\n\
             Modelo:                   {model}"
        }
        MessageId::KbScrollTranscript => {
            "Rolar transcrição, navegar histórico de entrada ou selecionar anexos do compositor"
        }
        MessageId::KbNavigateHistory => "Navegar histórico de entrada",
        MessageId::KbScrollTranscriptAlt => "Rolar transcrição",
        MessageId::KbScrollPage => "Rolar transcrição por página",
        MessageId::KbJumpTopBottom => "Pular para topo / fim da transcrição",
        MessageId::KbJumpTopBottomEmpty => "Pular para topo / fim (quando entrada vazia)",
        MessageId::KbJumpToolBlocks => "Pular entre blocos de saída de ferramentas",
        MessageId::KbMoveCursor => "Mover cursor no compositor",
        MessageId::KbJumpLineStartEnd => "Pular para início / fim da linha",
        MessageId::KbDeleteChar => {
            "Excluir caractere antes / depois do cursor, ou remover anexo selecionado"
        }
        MessageId::KbClearDraft => "Limpar rascunho atual",
        MessageId::KbStashDraft => "Estacionar rascunho atual (`/stash pop` restaura)",
        MessageId::KbSearchHistory => "Buscar histórico de prompts e recuperar rascunhos locais",
        MessageId::KbInsertNewline => "Inserir nova linha no compositor",
        MessageId::KbSendDraft => "Enviar rascunho atual",
        MessageId::KbCloseMenu => {
            "Fechar menu, cancelar requisição, descartar rascunho ou limpar entrada"
        }
        MessageId::KbCancelOrExit => "Cancelar requisição ou sair quando ocioso",
        MessageId::KbShellControls => "Abrir controles de shell para comando em primeiro plano",
        MessageId::KbExitEmpty => "Sair quando entrada vazia",
        MessageId::KbCommandPalette => "Abrir paleta de comandos",
        MessageId::KbFuzzyFilePicker => {
            "Abrir seletor de arquivo fuzzy (insere @path ao pressionar Enter)"
        }
        MessageId::KbCompactInspector => "Abrir inspetor compacto de contexto da sessão",
        MessageId::KbLastMessagePager => {
            "Abrir paginador para última mensagem (quando entrada vazia)"
        }
        MessageId::KbSelectedDetails => {
            "Abrir detalhes da ferramenta ou mensagem selecionada (quando entrada vazia)"
        }
        MessageId::KbToolDetailsPager => "Abrir paginador de detalhes da ferramenta",
        MessageId::KbThinkingPager => "Abrir paginador de raciocínio",
        MessageId::KbLiveTranscript => "Abrir sobreposição de transcrição ao vivo (auto-scroll)",
        MessageId::KbBacktrackMessage => {
            "Retroceder para mensagem anterior do usuário (esquerda/direita, Enter para rebobinar)"
        }
        MessageId::KbCompleteCycleModes => {
            "Completar /command, enfileirar follow-up, ciclar modos; Shift+Tab cicla esforço de raciocínio"
        }
        MessageId::KbJumpPlanAgentYolo => "Pular direto para modo Plan / Agent / YOLO",
        MessageId::KbAltJumpPlanAgentYolo => "Salto alternativo para modo Plan / Agent / YOLO",
        MessageId::KbFocusSidebar => "Focar barra lateral Plan / Todos / Tasks / Agents / Auto",
        MessageId::KbTogglePlanAgent => "Alternar entre modos Plan e Agent",
        MessageId::KbSessionPicker => "Abrir seletor de sessões",
        MessageId::KbPasteAttach => "Colar texto ou anexar imagem da área de transferência",
        MessageId::KbCopySelection => "Copiar seleção atual (Cmd+C no macOS)",
        MessageId::KbContextMenu => {
            "Abrir ações de contexto para colar, seleção, detalhes, contexto e ajuda"
        }
        MessageId::KbAttachPath => "Adicionar arquivo ou diretório local ao contexto",
        MessageId::KbHelpOverlay => "Abrir esta sobreposição de ajuda (quando entrada vazia)",
        MessageId::KbToggleHelp => "Alternar sobreposição de ajuda",
        MessageId::KbToggleHelpSlash => "Alternar sobreposição de ajuda",
        MessageId::HelpUsageLabel => "Uso:",
        MessageId::HelpAliasesLabel => "Apelidos:",
        MessageId::SettingsTitle => "Configurações:",
        MessageId::SettingsConfigFile => "Arquivo de configuração:",
        MessageId::ClearConversation => "Conversa limpa",
        MessageId::ClearConversationBusy => {
            "Conversa limpa (estado do plano ocupado; execute /clear novamente se necessário)"
        }
        MessageId::ModelChanged => "Modelo alterado: {old} \u{2192} {new}",
        MessageId::LinksTitle => "Links do DeepSeek:",
        MessageId::LinksDashboard => "Painel:",
        MessageId::LinksDocs => "Documentação:",
        MessageId::LinksTip => "Dica: chaves de API estão disponíveis no console do painel.",
        MessageId::SubagentsFetching => "Buscando status dos sub-agentes...",
        MessageId::HelpUnknownCommand => "Comando desconhecido: {topic}",
        MessageId::HomeDashboardTitle => "Painel Inicial do DeepSeek TUI",
        MessageId::HomeModel => "Modelo:",
        MessageId::HomeMode => "Modo:",
        MessageId::HomeWorkspace => "Workspace:",
        MessageId::HomeHistory => "Histórico:",
        MessageId::HomeTokens => "Tokens:",
        MessageId::HomeQueued => "Enfileirado:",
        MessageId::HomeSubagents => "Sub-agentes:",
        MessageId::HomeSkill => "Skill:",
        MessageId::HomeQuickActions => "Ações Rápidas",
        MessageId::HomeQuickLinks => "/links      - Links do painel e API",
        MessageId::HomeQuickSkills => "/skills      - Listar skills disponíveis",
        MessageId::HomeQuickConfig => "/config      - Abrir editor interativo de configuração",
        MessageId::HomeQuickSettings => "/settings    - Exibir configurações persistentes",
        MessageId::HomeQuickModel => "/model       - Alternar ou visualizar modelo",
        MessageId::HomeQuickSubagents => "/subagents   - Listar status dos sub-agentes",
        MessageId::HomeQuickTaskList => "/task list   - Exibir fila de tarefas em segundo plano",
        MessageId::HomeQuickHelp => "/help        - Exibir ajuda",
        MessageId::HomeModeTips => "Dicas de Modo",
        MessageId::HomeAgentModeTip => "Modo Agent - Use ferramentas para tarefas autônomas",
        MessageId::HomeAgentModeReviewTip => {
            "  Use Ctrl+X para revisar no modo Plan antes de executar"
        }
        MessageId::HomeAgentModeYoloTip => {
            "  Digite /yolo para habilitar acesso total às ferramentas"
        }
        MessageId::HomeYoloModeTip => "Modo YOLO - Acesso total a ferramentas, sem aprovações",
        MessageId::HomeYoloModeCaution => "  Tenha cuidado com operações destrutivas!",
        MessageId::HomePlanModeTip => "Modo Plan - Planeje antes de implementar",
        MessageId::HomePlanModeChecklistTip => "  Use /plan para criar checklists estruturados",
        MessageId::TuiPanelTranscript => " Transcrição ",
        MessageId::TuiPanelTranscriptScroll => " Transcrição (rolagem) ",
        MessageId::TuiPanelComposer => " Composer ",
        MessageId::TuiPanelComposerScroll => " Composer (rolagem) ",
        MessageId::TuiPanelComposerWaitingEdit => " Composer (aguardando · editar) ",
        MessageId::TuiPanelComposerWaitingScroll => " Composer (aguardando · rolar) ",
        MessageId::TuiPanelStatus => " Status ",
        MessageId::TuiPanelLeft => " Esquerda ",
        MessageId::TuiPanelLht => " LHT ",
        MessageId::TuiPanelLhtFocused => " LHT | j/k rolar l alternar i inspetor ",
        MessageId::TuiInspectorTabFiles => "Arquivos",
        MessageId::TuiInspectorTabDiff => "Diff",
        MessageId::TuiInspectorTabAgents => "Agents",
        MessageId::TuiInspectorTabMcp => "MCP",
        MessageId::TuiInspectorTabActivity => "Atividade",
        MessageId::TuiInspectorTabContext => "Context",
        MessageId::TuiLeftRailSessions => "Sessões",
        MessageId::TuiLeftRailNoSessions => "(sem sessões)",
        MessageId::TuiLeftRailInspector => "Inspetor",
        MessageId::TuiLeftRailNavHint => "j/k Enter Ctrl+N",
        MessageId::TuiNewSession => "Nova sessão",
        MessageId::TuiComposerHintWaitingEdit => {
            " aguardando...  Enter enfileirar  Ctrl+Enter steer  digite para continuar  Ctrl+C interromper"
        }
        MessageId::TuiComposerHintWaitingScroll => {
            " aguardando...  Esc editar  Enter enfileirar  Ctrl+Enter steer  j/k rolar"
        }
        MessageId::TuiComposerHintTypePrompt => {
            " digite prompt...  Ctrl+V colar  Shift+Enter nova linha  Enter enviar"
        }
        MessageId::TuiComposerHintScrollMode => " Esc editar  j/k rolar  Tab focar painéis",
        MessageId::TuiApprovalTitle => " Aprovação necessária ",
        MessageId::TuiApprovalToolLabel => "Ferramenta",
        MessageId::TuiApprovalKeyLabel => "Chave",
        MessageId::TuiApprovalAllow => "Permitir",
        MessageId::TuiApprovalDeny => "Negar",
        MessageId::TuiApprovalAllowSession => "Permitir sessão",
        MessageId::TuiApprovalSummary => "Resumo",
        MessageId::TuiApprovalDetail => "Detalhe",
        MessageId::TuiHelpTitle => "Zagens TUI - atalhos",
        MessageId::TuiHelpCloseTitle => " Ajuda (? fechar) ",
        MessageId::TuiHelpSectionFocus => {
            "Foco\n  Tab / Shift+Tab     Alternar Esquerda / Chat / Direita\n  [ / ]               Recolher trilhos esquerdo/direito"
        }
        MessageId::TuiHelpSectionLeftRail => {
            "Trilho esquerdo (sessões)\n  j / k               Selecionar sessão\n  Enter               Trocar sessão\n  Ctrl+N              Nova sessão"
        }
        MessageId::TuiHelpSectionRightRail => {
            "Trilho direito (inspetor + LHT)\n  Tab                 Focar coluna direita\n  1-6                 Arquivos / Diff / Agents / MCP / Atividade / Context\n  j / k               Rolar inspetor\n  Enter               Arquivos: expandir/preview  Diff: patch  MCP: ferramentas\n  Esc                 Voltar do detalhe\n  s                   Diff: alternar staged/worktree\n  - / =               Ajustar largura do trilho direito\n  l                   Alternar painel LHT inferior\n  i                   Focar inspetor superior"
        }
        MessageId::TuiHelpSectionChat => {
            "Chat\n  Tab                 Entrada -> rolar -> colunas\n  Shift+Tab           Ordem inversa de foco\n  Esc                 Alternar entrada/rolagem\n  Enter               Enviar prompt\n  Shift+Enter         Nova linha\n  Ctrl+V              Colar da área de transferência\n  /commands           Menu slash\n  /model <id>         Alternar modelo\n  /lht [auto|strict|off]  Modo LHT\n  /theme [name]       Alternar tema\n  /approve [policy]   Política de aprovação (vazio alterna)\n  Ctrl+A              Ciclar política de aprovação\n  o                   Expandir/recolher último bloco de ferramenta"
        }
        MessageId::TuiHelpSectionApproval => {
            "Modal de aprovação\n  y / Enter           Permitir\n  n / Esc             Negar\n  a                   Permitir sessão\n  v                   Alternar detalhe"
        }
        MessageId::TuiHelpSectionGlobal => {
            "Global\n  Ctrl+C              Interromper turno\n  Ctrl+C duas vezes   Sair\n  Ctrl+Q              Sair\n  ?                   Alternar ajuda"
        }
        MessageId::TuiHelpSectionLaunch => {
            "Inicialização (CLI)\n  --fresh             Nova sessão\n  --mouse-capture     Ativar roda do mouse"
        }
        MessageId::TuiHelpSectionTerminalFont => {
            "Fonte do terminal (recomendado)\n  Windows Terminal    Cascadia Mono, JetBrains Mono, Consolas\n  Console legado      Consolas 11+ ou NSimSun para CJK\n  Configure no perfil do terminal"
        }
        MessageId::TuiSlashWorkspace => "Alternar diretório do workspace",
        MessageId::TuiSlashCd => "Alternar workspace (alias)",
        MessageId::TuiSlashModel => "Alternar modelo de texto desta sessão",
        MessageId::TuiSlashModelAlias => "Alternar modelo (alias)",
        MessageId::TuiSlashLht => "Modo LHT: auto / strict / off (vazio alterna)",
        MessageId::TuiSlashTheme => "Alternar tema de cores do TUI (vazio alterna)",
        MessageId::TuiSlashNew => "Nova sessão no workspace atual",
        MessageId::TuiSlashHelp => "Mostrar atalhos de teclado",
        MessageId::TuiSlashAuto => "Regras de automação: hooks, timers, gatilhos",
        MessageId::TuiSlashClear => "Limpar entrada do composer",
        MessageId::TuiInspectorHintFiles => "j/k nav Enter file/dir Esc back",
        MessageId::TuiInspectorHintDiff => "j/k nav Enter patch s staged Esc",
        MessageId::TuiInspectorHintAgents => "j/k nav",
        MessageId::TuiInspectorHintMcp => "j/k nav Enter tools",
        MessageId::TuiInspectorHintActivity => "j/k scroll log",
        MessageId::TuiInspectorHintContext => "j/k scroll breakdown",
        MessageId::TuiTranscriptEmpty => {
            "Transcrição vazia — digite um prompt no Composer e pressione Enter."
        }
        MessageId::TuiResumedThread => "thread {id} retomada",
        MessageId::TuiAutoTitle => " Automação (/auto) ",
        MessageId::TuiAutoListHint => {
            " j/k mover  Space alternar  n novo  Enter editar  d excluir  e editor  Esc fechar "
        }
        MessageId::TuiAutoEditRule => " Editar regra ",
        MessageId::TuiAutoNewRule => " Nova regra ",
        MessageId::TuiAutoEditHint => {
            " Tab próx  Shift+Tab ant  ←/→ ciclar  Enter salvar  Esc cancelar"
        }
        MessageId::TuiAutoName => "Nome",
        MessageId::TuiAutoTrigger => "Gatilho",
        MessageId::TuiAutoSeconds => "Segundos",
        MessageId::TuiAutoToolFilter => "Nome da ferramenta",
        MessageId::TuiAutoAnyTool => "qualquer ferramenta (deixe em branco)",
        MessageId::TuiAutoAction => "Ação",
        MessageId::TuiAutoPrompt => "Prompt",
        MessageId::TuiAutoShellCmd => "Cmd shell",
        MessageId::TuiAutoMessage => "Mensagem",
        MessageId::TuiAutoCommand => "Comando",
        MessageId::TuiSlashLocale => "Alternar idioma da UI (vazio alterna)",
        MessageId::TuiSlashLanguage => "Alternar idioma da UI (alias)",
        MessageId::TuiSlashApiKey => "Salvar ou remover chave DeepSeek API",
        MessageId::TuiSlashKey => "Salvar ou remover chave API (alias)",
        MessageId::TuiSlashLogin => "Salvar chave DeepSeek API (alias CLI)",
        MessageId::TuiSlashLogout => "Remover chave DeepSeek API salva",
        MessageId::TuiSlashApprove => {
            "Política de aprovação: on-request / untrusted / never / auto (vazio alterna)"
        }
        MessageId::TuiSlashApproval => "Política de aprovação (alias)",
        MessageId::TuiApiKeyCleared => "Chave API removida",
        MessageId::TuiApiKeyUsage => "/api-key sk-… salvar · /api-key clear ou /logout remover",
        MessageId::TuiLocalePickerHint => {
            " Idioma | ^v selecionar  Enter aplicar  vazio /locale alterna  Esc cancelar "
        }
        MessageId::TuiLocaleChanged => {
            "locale: {locale} (UI atualizada; respostas do modelo na próxima rodada)"
        }
        MessageId::TuiPendingInputsTitle => "Entradas pendentes",
        MessageId::TuiPendingQueuedKind => "Fila",
        MessageId::TuiPendingEditHint => " ↑ editar última mensagem na fila",
        MessageId::TuiSteerInjected => "steer: injetado na rodada atual",
        MessageId::TuiOnboardingTitle => "Configuração",
        MessageId::TuiOnboardingWelcomeTitle => "Bem-vindo ao Zagens",
        MessageId::TuiOnboardingWelcomeBody => {
            "Comece em poucos passos — configure a chave de API e o modo padrão."
        }
        MessageId::TuiOnboardingWorkspace => "Workspace:",
        MessageId::TuiOnboardingKeyTitle => "Informe sua chave DeepSeek API",
        MessageId::TuiOnboardingKeyHint => "Armazenada apenas neste computador. Esc para pular.",
        MessageId::TuiOnboardingModeTitle => "Escolha o modo padrão",
        MessageId::TuiOnboardingModeAuto => "Auto",
        MessageId::TuiOnboardingModeAutoDesc => "Zagens escolhe code ou office conforme a tarefa.",
        MessageId::TuiOnboardingModeCode => "Code",
        MessageId::TuiOnboardingModeCodeDesc => "Engenharia: arquivos, shell, refatorações longas.",
        MessageId::TuiOnboardingModeOffice => "Office",
        MessageId::TuiOnboardingModeOfficeDesc => {
            "Documentos: redação, planilhas, relatórios, slides."
        }
        MessageId::TuiOnboardingFooter => "Enter avançar · Esc voltar · Esc na chave pula",
        MessageId::TuiOnboardingStepWelcome => "Boas-vindas",
        MessageId::TuiOnboardingStepKey => "Chave API",
        MessageId::TuiOnboardingStepMode => "Modo",
        MessageId::TuiOnboardingKeySaved => "Chave API salva",
        MessageId::TuiOnboardingComplete => "Configuração concluída — bom trabalho!",
        MessageId::TuiSlashMcp => "Editar JSON de servidores MCP (mcp.json)",
        MessageId::TuiMcpTitle => "Config MCP",
        MessageId::TuiMcpPathLabel => "Arquivo:",
        MessageId::TuiMcpSave => "Salvar",
        MessageId::TuiMcpCancel => "Cancelar",
        MessageId::TuiMcpFooter => {
            "Digite ou cole JSON · Ctrl+V colar · Tab foco · Enter salvar/cancelar · Ctrl+S salvar · Esc cancelar"
        }
        MessageId::TuiMcpSaved => "Config MCP salva (vale na próxima rodada)",
        MessageId::TuiMcpParseError => "JSON inválido",
        MessageId::TuiMcpEmptyError => "A config MCP não pode ficar vazia",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_setting_normalizes_supported_tags() {
        assert_eq!(normalize_configured_locale("auto"), Some("auto"));
        assert_eq!(normalize_configured_locale("ja_JP.UTF-8"), Some("ja"));
        assert_eq!(normalize_configured_locale("zh-CN"), Some("zh-Hans"));
        assert_eq!(normalize_configured_locale("pt"), Some("pt-BR"));
        assert_eq!(normalize_configured_locale("pt-PT"), Some("pt-BR"));
        assert_eq!(normalize_configured_locale("zh-TW"), None);
    }

    #[test]
    fn locale_resolution_uses_config_then_environment_then_english() {
        assert_eq!(
            resolve_locale_with_env("ja", |_| Some("pt_BR.UTF-8".to_string())),
            Locale::Ja
        );
        assert_eq!(
            resolve_locale_with_env("auto", |key| {
                (key == "LANG").then(|| "zh_CN.UTF-8".to_string())
            }),
            Locale::ZhHans
        );
        assert_eq!(resolve_locale_with_env("auto", |_| None), Locale::En);
    }

    #[test]
    fn shipped_first_pack_has_no_missing_core_messages() {
        for locale in Locale::shipped() {
            assert!(
                missing_message_ids(*locale).is_empty(),
                "{} is missing messages",
                locale.tag()
            );
        }
    }

    #[test]
    fn unsupported_locale_falls_back_to_english() {
        assert_eq!(
            resolve_locale_with_env("ar", |_| None),
            Locale::En,
            "Arabic is planned for QA but not shipped in the v0.7.6 core pack"
        );
    }

    #[test]
    fn missing_translation_falls_back_to_english() {
        assert_eq!(
            fallback_translation(None, MessageId::ComposerPlaceholder),
            english(MessageId::ComposerPlaceholder)
        );
    }

    #[test]
    fn width_truncation_handles_cjk_rtl_indic_and_latin_samples() {
        let samples = [
            ("zh-Hans", "输入以筛选配置"),
            ("ar", "تصفية الإعدادات"),
            ("hi", "सेटिंग खोजें"),
            ("pt-BR", "configurações filtradas"),
        ];

        for (tag, sample) in samples {
            let truncated = truncate_to_width(sample, 12);
            assert!(
                truncated.width() <= 12,
                "{tag} sample overflowed: {truncated:?}"
            );
        }
    }
}
