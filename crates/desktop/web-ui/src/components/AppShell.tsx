import { useCallback } from 'react';
import { useT } from '../i18n';
import ChatView from './ChatView';
import type { SessionRestoreSource } from '../hooks/useSessionNavigation';
import ChatActionDialogs from './ChatActionDialogs';
import Composer, { type ComposerOutboundMessage } from './Composer';
import ModelParamsDialog, { type ModelParams } from './ModelParamsDialog';
import IconRail from './chrome/IconRail';
import SessionStrip, { type SessionStripSession } from './chrome/SessionStrip';
import type { HarnessCardId } from './chrome/HarnessCard';
import ApprovalDialog from './ApprovalDialog';
import RightPanel, { type RightPanelView } from './RightPanel';
import HarnessFloatStack from './chrome/HarnessFloatStack';
import { HARNESS_CARD_VIEWS } from '../lib/harnessCardViews';
import TitleBar from './TitleBar';
import StoragePressureBanner from './StoragePressureBanner';
import SidecarRestartPendingBanner from './SidecarRestartPendingBanner';
import type { StoragePressureSnapshot } from '../lib/storagePressure';
import SkipToMainLink from './SkipToMainLink';
import { createAgentWindow } from '../lib/windowBridge';
import { toast } from '../lib/toast';
import type { PreviewState } from './preview/types';
import type { ApprovalState } from '../hooks/useTurnApproval';
import type { TurnChatMessage } from '../hooks/useTurnSend';
import type { ThreadContextSnapshot } from '../lib/contextUsage';
import type { AgentState } from '../types/agent';
import type {
  ComposerModelId,
  DesktopRouteIntentOption,
  DesktopRunModeId,
  DesktopTaskTypePreference,
  DesktopTaskTypeResolved,
} from '../types/desktop';
import type { RuntimeConnectionState } from '../api/client';
import type { SystemSettings } from '../api/client';
import type { Theme } from '../lib/appPreferences';
import type { InspectorNavActivity } from '../lib/inspectorUnread';
import { useHarnessFloatStack } from '../hooks/useHarnessFloatStack';
import type { HarnessGridDataSnapshot } from '../lib/useHarnessGridData';

export type AppShellProps = {
  desktopHost: boolean;
  storagePauseTurns?: boolean;
  storageSnapshot?: StoragePressureSnapshot | null;
  storageLevel?: 'ok' | 'warn' | 'critical';
  selectedWorkspace: string;
  approval: ApprovalState | null;
  approvalBusy: boolean;
  onApproveDecision: (decision: 'approve' | 'deny', rememberForSession?: boolean) => void;
  modelParamsOpen: boolean;
  modelParams: ModelParams;
  onModelParamsOpenChange: (open: boolean) => void;
  onModelParamsApply: (params: ModelParams) => void;
  editDraft: { messageId: string; content: string } | null;
  onEditDraftChange: (draft: { messageId: string; content: string } | null) => void;
  onConfirmEdit: () => void;
  backtrackDraft: {
    messageId: string;
    content: string;
    depthFromTail: number;
  } | null;
  backtrackBusy: boolean;
  onBacktrackDraftChange: (
    draft: { messageId: string; content: string; depthFromTail: number } | null,
  ) => void;
  onConfirmBacktrack: () => void;
  rewindDraft: {
    messageId: string;
    content: string;
    depthFromTail: number;
    turnOffset: number;
  } | null;
  rewindBusy: boolean;
  onRewindDraftChange: (
    draft: {
      messageId: string;
      content: string;
      depthFromTail: number;
      turnOffset: number;
    } | null,
  ) => void;
  onConfirmRewindWorkspace: () => void;
  visibleSessions: SessionStripSession[];
  showAllSessions: boolean;
  onToggleShowAllSessions: () => void;
  activeSessionId: string | null;
  streamingSessionIds?: Set<string>;
  onNewSession: () => void;
  onSelectSession: (sessionId: string) => void;
  onDeleteSession: (sessionId: string) => void;
  runtimeConn: RuntimeConnectionState;
  streaming: boolean;
  runtimeSessionEstablished: boolean;
  desktopApiKeyConfigured: boolean | null;
  activeInspector: RightPanelView;
  onInspectorChange: (view: RightPanelView) => void;
  sessionStripOpen: boolean;
  onToggleSessionStrip: () => void;
  harnessGridData: HarnessGridDataSnapshot;
  userDismissedHarness: boolean;
  onShowHarnessStack: () => void;
  focusMode?: boolean;
  officeSession: boolean;
  checklistActivity: InspectorNavActivity;
  auditActivity: InspectorNavActivity;
  taskActivity: InspectorNavActivity;
  agentActivity: InspectorNavActivity;
  onSend: (message: ComposerOutboundMessage, opts?: { editFromMessageId?: string }) => void;
  onCancelStream: () => void;
  autoApprove: boolean;
  approvalPolicy: Parameters<typeof Composer>[0]['approvalPolicy'];
  onAutoApproveChange: (value: boolean) => void;
  runMode: DesktopRunModeId;
  onRunModeChange: (mode: DesktopRunModeId) => void;
  taskTypePreference: DesktopTaskTypePreference;
  lockedThreadTaskType: DesktopTaskTypeResolved | null;
  onTaskTypePreferenceChange: (next: DesktopTaskTypePreference) => void;
  routeIntent: DesktopRouteIntentOption;
  onOpenRouting: () => void;
  onExportSessionJson: () => void;
  onExportThreadJson: () => void;
  onExportTraceReport: () => void;
  onExportTraceCompare: () => void;
  selectedModel: ComposerModelId;
  onModelChange: (model: ComposerModelId) => void;
  composerModelOptions: string[];
  onComposerWorkspaceChange: (next: string) => Promise<void>;
  useWorktree: boolean;
  onUseWorktreeChange: (next: boolean) => void;
  activeWorktreeName?: string | null;
  resumedThreadId: string | null;
  contextUsagePct: number;
  contextUsedTokens: number;
  contextWindowTokens: number;
  threadContextSnapshot: ThreadContextSnapshot | null;
  lastTurnOutputTokens: number | null;
  lastCacheHitPercent: number | null;
  lhtChip?: import('../lib/lhtChip').LhtChipState | null;
  composerMention?: {
    relPath: string;
    isDirectory: boolean;
    nonce: number;
  };
  composerPrefill?: { text: string; nonce: number };
  onOfficeQuickStart?: (prefill: string) => void;
  messages: TurnChatMessage[];
  sessionRestoreLoading?: boolean;
  sessionRestoreSource?: SessionRestoreSource;
  onRetrySessionRestore?: () => void;
  agentStates: AgentState[];
  onChatOpenWorkspacePath: (relPath: string) => void;
  onRevealWorkspacePath: (relPath: string) => void;
  onOpenDiffInPanel: () => void;
  onEditMessage?: (messageId: string, content: string) => void;
  onBacktrackFromMessage?: (messageId: string, content: string) => void;
  onRewindWorkspaceFromMessage?: (messageId: string, content: string) => void;
  rightPanelCollapsed: boolean;
  onExpandRightPanel: () => void;
  onCollapseRightPanel: () => void;
  theme: Theme;
  onToggleTheme: () => void;
  onThemeChange: (theme: Theme) => void;
  platform: string;
  threadTrustMode: boolean;
  onEnableTrust: () => Promise<void>;
  panelPreview: PreviewState | null;
  onClosePreview: () => void;
  openWorkspaceFile: (relPath: string, title?: string) => Promise<void>;
  revealWorkspaceFile: (relPath: string) => void;
  addWorkspaceFileToChat: (relPath: string, isDirectory?: boolean) => void;
  focusFilesNonce: number;
  focusFilesRelPath: string | null;
  filesRefreshNonce?: number;
  focusDiffNonce: number;
  onRequestChecklist: () => void;
  onRequestAudit: () => void;
  auditGridVisible: boolean;
  auditGridAvailable: boolean;
  onToggleAuditGrid: () => void;
  subagentActiveCount: number;
  narrativeSpawnSuspected: boolean;
  onRequestMermaid: () => void;
  onRequestDiff: () => void;
  onSystemSettingsSaved: (settings: SystemSettings) => void;
  onRouteIntentChange: (intent: DesktopRouteIntentOption) => void;
  refreshApiKeyStatus: () => void;
  onModelProvidersSaved: () => void;
  onOpenTasks?: (taskId?: string) => void;
  onOpenTaskThread?: (threadId: string) => void;
  highlightTaskId?: string | null;
};

export default function AppShell({
  desktopHost,
  storagePauseTurns = false,
  storageSnapshot = null,
  storageLevel = 'ok',
  selectedWorkspace,
  approval,
  approvalBusy,
  onApproveDecision,
  modelParamsOpen,
  modelParams,
  onModelParamsOpenChange,
  onModelParamsApply,
  editDraft,
  onEditDraftChange,
  onConfirmEdit,
  backtrackDraft,
  backtrackBusy,
  onBacktrackDraftChange,
  onConfirmBacktrack,
  rewindDraft,
  rewindBusy,
  onRewindDraftChange,
  onConfirmRewindWorkspace,
  visibleSessions,
  showAllSessions,
  onToggleShowAllSessions,
  activeSessionId,
  streamingSessionIds,
  onNewSession,
  onSelectSession,
  onDeleteSession,
  runtimeConn,
  streaming,
  runtimeSessionEstablished,
  desktopApiKeyConfigured,
  activeInspector,
  onInspectorChange,
  sessionStripOpen,
  onToggleSessionStrip,
  harnessGridData,
  userDismissedHarness,
  onShowHarnessStack,
  focusMode = false,
  officeSession,
  checklistActivity,
  auditActivity,
  taskActivity,
  agentActivity,
  onSend,
  onCancelStream,
  autoApprove,
  approvalPolicy,
  onAutoApproveChange,
  runMode,
  onRunModeChange,
  taskTypePreference,
  lockedThreadTaskType,
  onTaskTypePreferenceChange,
  routeIntent,
  onOpenRouting,
  onExportSessionJson,
  onExportThreadJson,
  onExportTraceReport,
  onExportTraceCompare,
  selectedModel,
  onModelChange,
  composerModelOptions,
  onComposerWorkspaceChange,
  useWorktree,
  onUseWorktreeChange,
  activeWorktreeName = null,
  resumedThreadId,
  contextUsagePct,
  contextUsedTokens,
  contextWindowTokens,
  threadContextSnapshot,
  lastTurnOutputTokens,
  lastCacheHitPercent,
  lhtChip,
  composerMention,
  composerPrefill,
  onOfficeQuickStart,
  messages,
  sessionRestoreLoading = false,
  sessionRestoreSource = null,
  onRetrySessionRestore,
  agentStates,
  onChatOpenWorkspacePath,
  onRevealWorkspacePath,
  onOpenDiffInPanel,
  onEditMessage,
  onBacktrackFromMessage,
  onRewindWorkspaceFromMessage,
  rightPanelCollapsed,
  onExpandRightPanel,
  onCollapseRightPanel,
  theme,
  onToggleTheme,
  onThemeChange,
  platform,
  threadTrustMode,
  onEnableTrust,
  panelPreview,
  onClosePreview,
  openWorkspaceFile,
  revealWorkspaceFile,
  addWorkspaceFileToChat,
  focusFilesNonce,
  focusFilesRelPath,
  filesRefreshNonce,
  focusDiffNonce,
  onRequestChecklist,
  onRequestAudit,
  auditGridVisible,
  auditGridAvailable,
  onToggleAuditGrid,
  subagentActiveCount,
  narrativeSpawnSuspected,
  onRequestMermaid,
  onRequestDiff,
  onSystemSettingsSaved,
  onRouteIntentChange,
  refreshApiKeyStatus,
  onModelProvidersSaved,
  onOpenTasks,
  onOpenTaskThread,
  highlightTaskId = null,
}: AppShellProps) {
  const { t } = useT();
  const { visible: harnessStackVisible, flashCardId, openAndScrollTo } = useHarnessFloatStack({
    harnessData: harnessGridData,
    userDismissed: userDismissedHarness,
    focusMode,
  });

  const handleSavedApiKey = useCallback(() => {
    onModelProvidersSaved();
    toast.dismissAll();
  }, [onModelProvidersSaved]);

  const harnessCardHasData = (cardId: HarnessCardId): boolean => {
    switch (cardId) {
      case 'checklist':
        return harnessGridData.hasChecklist;
      case 'audit':
        return harnessGridData.hasAudit;
      case 'lht':
        return harnessGridData.hasLongHorizon;
      case 'agents':
        return harnessGridData.hasAgents;
      default:
        return false;
    }
  };

  const handleHarnessNavigate = (cardId: HarnessCardId) => {
    if (!harnessCardHasData(cardId)) {
      toast.info(t('iconRail.harnessNoData'));
      return;
    }
    if (userDismissedHarness) {
      onShowHarnessStack();
    }
    onExpandRightPanel();
    onInspectorChange(HARNESS_CARD_VIEWS[cardId]);
    openAndScrollTo(cardId);
  };

  const handleHarnessHeadClick = (cardId: HarnessCardId, view: RightPanelView) => {
    onExpandRightPanel();
    onInspectorChange(view);
    openAndScrollTo(cardId);
  };

  return (
    <div className="flex flex-col h-screen w-screen bg-canvas">
      <SkipToMainLink />
      <StoragePressureBanner snapshot={storageSnapshot} level={storageLevel} />
      <SidecarRestartPendingBanner />
      <TitleBar
        desktopHost={desktopHost}
        onNewWindow={() => {
          void createAgentWindow(selectedWorkspace).catch((e) => {
            toast.error((e as Error).message);
          });
        }}
        auditGridAvailable={auditGridAvailable && !officeSession}
        auditGridVisible={harnessStackVisible}
        onToggleAuditGrid={onToggleAuditGrid}
      />
      <div className="flex flex-1 min-h-0 bg-canvas">
        <ApprovalDialog
          open={approval != null}
          toolName={approval?.toolName ?? ''}
          description={approval?.description ?? ''}
          busy={approvalBusy}
          onApprove={(rememberForSession) => void onApproveDecision('approve', rememberForSession)}
          onDeny={() => void onApproveDecision('deny')}
        />
        <ModelParamsDialog
          open={modelParamsOpen}
          initial={modelParams}
          modelId={selectedModel}
          onClose={() => onModelParamsOpenChange(false)}
          onApply={(params) => {
            onModelParamsApply(params);
            onModelParamsOpenChange(false);
          }}
        />
        <ChatActionDialogs
          editDraft={editDraft}
          onEditDraftChange={onEditDraftChange}
          onConfirmEdit={onConfirmEdit}
          backtrackDraft={backtrackDraft}
          backtrackBusy={backtrackBusy}
          onBacktrackDraftChange={onBacktrackDraftChange}
          onConfirmBacktrack={onConfirmBacktrack}
          rewindDraft={rewindDraft}
          rewindBusy={rewindBusy}
          onRewindDraftChange={onRewindDraftChange}
          onConfirmRewindWorkspace={onConfirmRewindWorkspace}
        />
        <div className="chrome-sidebar group flex shrink-0">
          <IconRail
            sessionStripOpen={sessionStripOpen && !focusMode}
            onToggleSessionStrip={onToggleSessionStrip}
            onNewSession={onNewSession}
            activeInspector={activeInspector}
            onInspectorChange={onInspectorChange}
            onExpandRightPanel={onExpandRightPanel}
            onHarnessNavigate={handleHarnessNavigate}
            harnessFlashId={flashCardId}
            desktopHost={desktopHost}
            officeSession={officeSession}
            runtimeConn={runtimeConn}
            streaming={streaming}
            runtimeSessionEstablished={runtimeSessionEstablished}
            apiKeyConfigured={desktopApiKeyConfigured}
            checklistActivity={checklistActivity}
            auditActivity={auditActivity}
            taskActivity={taskActivity}
            agentActivity={agentActivity}
            theme={theme}
            onThemeChange={onThemeChange}
          />
          <SessionStrip
            open={sessionStripOpen && !focusMode}
            sessions={visibleSessions}
            showAllSessions={showAllSessions}
            onToggleShowAllSessions={onToggleShowAllSessions}
            activeSessionId={activeSessionId}
            streamingSessionIds={streamingSessionIds}
            onSelectSession={onSelectSession}
            onDeleteSession={onDeleteSession}
          />
        </div>
        <main
          id="main-content"
          tabIndex={-1}
          className="flex min-h-0 flex-1 flex-col min-w-0 bg-canvas outline-none"
        >
          <div className="chat-stage flex min-h-0 min-w-0 flex-1">
            <div className="chat-col flex min-h-0 min-w-0 flex-1 flex-col">
              <ChatView
                messages={messages}
                workspaceRoot={selectedWorkspace}
                desktopHost={desktopHost}
                agentStates={agentStates}
                onOpenWorkspacePath={onChatOpenWorkspacePath}
                onRevealWorkspacePath={onRevealWorkspacePath}
                onOpenDiffInPanel={onOpenDiffInPanel}
                onRetryMessage={(content) =>
                  onSend({ displayContent: content, apiPrompt: content })
                }
                onEditMessage={onEditMessage}
                onBacktrackFromMessage={onBacktrackFromMessage}
                onRewindWorkspaceFromMessage={onRewindWorkspaceFromMessage}
                officeSession={officeSession}
                onOfficeQuickStart={officeSession ? onOfficeQuickStart : undefined}
                sessionRestoreLoading={sessionRestoreLoading}
                sessionRestoreSource={sessionRestoreSource}
                onRetrySessionRestore={onRetrySessionRestore}
              />
              <section className="shrink-0" aria-label={t('a11y.composerRegion')}>
                <Composer
                  onSend={onSend}
                  onCancel={onCancelStream}
              disabled={streaming || storagePauseTurns}
              autoApprove={autoApprove}
              approvalPolicy={approvalPolicy}
              onAutoApproveChange={onAutoApproveChange}
              runMode={runMode}
              onRunModeChange={onRunModeChange}
              taskTypePreference={taskTypePreference}
              lockedThreadTaskType={lockedThreadTaskType}
              onTaskTypePreferenceChange={onTaskTypePreferenceChange}
              routeIntent={routeIntent}
              onOpenRouting={officeSession ? undefined : onOpenRouting}
              sessionExportEnabled={Boolean(activeSessionId)}
              threadExportEnabled={Boolean(resumedThreadId)}
              onExportSessionJson={() => void onExportSessionJson()}
              onExportThreadJson={() => void onExportThreadJson()}
              onExportTraceReport={() => void onExportTraceReport()}
              onExportTraceCompare={() => void onExportTraceCompare()}
              model={selectedModel}
              onModelChange={onModelChange}
              modelOptions={composerModelOptions}
              onOpenModelParams={() => onModelParamsOpenChange(true)}
              workspace={selectedWorkspace}
              onWorkspaceChange={onComposerWorkspaceChange}
              useWorktree={useWorktree}
              onUseWorktreeChange={onUseWorktreeChange}
              activeWorktreeName={activeWorktreeName}
              resumedThreadActive={resumedThreadId != null && resumedThreadId.length > 0}
              threadId={resumedThreadId}
              contextUsagePct={contextUsagePct}
              contextUsedTokens={contextUsedTokens}
              contextWindowTokens={contextWindowTokens}
              contextSource={threadContextSnapshot?.source}
              compactionThresholdTokens={threadContextSnapshot?.compaction_threshold_tokens}
              lastApiInputTokens={threadContextSnapshot?.last_api_input_tokens ?? null}
              lastTurnOutputTokens={lastTurnOutputTokens}
              lastCacheHitPercent={lastCacheHitPercent}
              lhtChip={lhtChip}
              officeSession={officeSession}
              workspaceMention={composerMention}
                  composerPrefill={composerPrefill}
                />
              </section>
            </div>
            {!officeSession ? (
              <HarnessFloatStack
                visible={harnessStackVisible}
                harnessData={harnessGridData}
                agentStates={agentStates}
                flashCardId={flashCardId}
                onHeadClick={handleHarnessHeadClick}
              />
            ) : null}
          </div>
        </main>
        {!harnessStackVisible && !rightPanelCollapsed && !focusMode && (
          <RightPanel
            view={activeInspector}
            officeSession={officeSession}
            desktopHost={desktopHost}
            runtimeConn={runtimeConn}
            runtimeSessionEstablished={runtimeSessionEstablished}
            apiKeyConfigured={desktopApiKeyConfigured}
            onSavedApiKey={handleSavedApiKey}
            theme={theme}
            onToggleTheme={onToggleTheme}
            platform={platform}
            workspaceRoot={selectedWorkspace}
            resumedThreadId={resumedThreadId}
            threadTrustMode={threadTrustMode}
            onEnableTrust={onEnableTrust}
            preview={panelPreview}
            onClosePreview={onClosePreview}
            openWorkspaceFile={openWorkspaceFile}
            revealWorkspaceFile={revealWorkspaceFile}
            addWorkspaceFileToChat={addWorkspaceFileToChat}
            focusFilesNonce={focusFilesNonce}
            focusFilesRelPath={focusFilesRelPath}
            filesRefreshNonce={filesRefreshNonce}
            focusDiffNonce={focusDiffNonce}
            agentStates={agentStates}
            onRequestChecklist={onRequestChecklist}
            onRequestAudit={onRequestAudit}
            subagentActiveCount={subagentActiveCount}
            narrativeSpawnSuspected={narrativeSpawnSuspected}
            streaming={streaming}
            messages={messages}
            onRequestMermaid={onRequestMermaid}
            onRequestDiff={onRequestDiff}
            onCollapse={onCollapseRightPanel}
            onSystemSettingsSaved={onSystemSettingsSaved}
            routeIntent={routeIntent}
            onRouteIntentChange={onRouteIntentChange}
            onOpenTasks={onOpenTasks}
            onOpenTaskThread={onOpenTaskThread}
            highlightTaskId={highlightTaskId}
          />
        )}
        {!harnessStackVisible && rightPanelCollapsed && (
          <button
            type="button"
            onClick={onExpandRightPanel}
            className="chrome-seam-l shrink-0 w-8 bg-canvas hover:bg-hover transition-colors flex items-center justify-center group"
            title={t('rightPanel.expand')}
            aria-label={t('rightPanel.expand')}
          >
            <svg
              className="w-3.5 h-3.5 text-t-text-muted group-hover:text-t-text transition-colors"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              aria-hidden
            >
              <path d="M11 3.5v9" strokeLinecap="round" />
              <path d="M8 8L5 5v6l3-3z" strokeLinejoin="round" />
            </svg>
          </button>
        )}
      </div>
    </div>
  );
}
