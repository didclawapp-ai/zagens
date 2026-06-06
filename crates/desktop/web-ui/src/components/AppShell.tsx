import { useT } from '../i18n';
import ChatView from './ChatView';
import ChatActionDialogs from './ChatActionDialogs';
import Composer, { type ComposerOutboundMessage } from './Composer';
import ModelParamsDialog, { type ModelParams } from './ModelParamsDialog';
import Sidebar from './Sidebar';
import ApprovalDialog from './ApprovalDialog';
import RightPanel, { type RightPanelView } from './RightPanel';
import AuditGridPanel from './AuditGridPanel';
import TitleBar from './TitleBar';
import StoragePressureBanner from './StoragePressureBanner';
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
  DesktopModelId,
  DesktopRouteIntentOption,
  DesktopRunModeId,
  DesktopTaskTypePreference,
  DesktopTaskTypeResolved,
} from '../types/desktop';
import type { RuntimeConnectionState } from '../api/client';
import type { SystemSettings } from '../api/client';
import type { Theme } from '../lib/appPreferences';
import type { InspectorNavActivity } from '../lib/inspectorUnread';

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
  visibleSessions: Parameters<typeof Sidebar>[0]['sessions'];
  showAllSessions: boolean;
  onToggleShowAllSessions: () => void;
  activeSessionId: string | null;
  onNewSession: () => void;
  onSelectSession: (sessionId: string) => void;
  onDeleteSession: (sessionId: string) => void;
  runtimeConn: RuntimeConnectionState;
  streaming: boolean;
  runtimeSessionEstablished: boolean;
  desktopApiKeyConfigured: boolean | null;
  activeInspector: RightPanelView;
  onInspectorChange: (view: RightPanelView) => void;
  sidebarCollapsed: boolean;
  onToggleSidebarCollapse: () => void;
  onExpandSidebar: () => void;
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
  selectedModel: DesktopModelId;
  onModelChange: (model: DesktopModelId) => void;
  onComposerWorkspaceChange: (next: string) => Promise<void>;
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
  agentStates: AgentState[];
  onChatOpenWorkspacePath: (relPath: string) => void;
  onRevealWorkspacePath: (relPath: string) => void;
  onOpenDiffInPanel: () => void;
  onEditMessage?: (messageId: string, content: string) => void;
  onBacktrackFromMessage?: (messageId: string, content: string) => void;
  rightPanelCollapsed: boolean;
  onExpandRightPanel: () => void;
  onCollapseRightPanel: () => void;
  theme: Theme;
  onToggleTheme: () => void;
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
  onDismissAuditGrid: () => void;
  subagentActiveCount: number;
  narrativeSpawnSuspected: boolean;
  onRequestMermaid: () => void;
  onRequestDiff: () => void;
  onSystemSettingsSaved: (settings: SystemSettings) => void;
  onRouteIntentChange: (intent: DesktopRouteIntentOption) => void;
  refreshApiKeyStatus: () => void;
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
  visibleSessions,
  showAllSessions,
  onToggleShowAllSessions,
  activeSessionId,
  onNewSession,
  onSelectSession,
  onDeleteSession,
  runtimeConn,
  streaming,
  runtimeSessionEstablished,
  desktopApiKeyConfigured,
  activeInspector,
  onInspectorChange,
  sidebarCollapsed,
  onToggleSidebarCollapse,
  onExpandSidebar,
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
  selectedModel,
  onModelChange,
  onComposerWorkspaceChange,
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
  agentStates,
  onChatOpenWorkspacePath,
  onRevealWorkspacePath,
  onOpenDiffInPanel,
  onEditMessage,
  onBacktrackFromMessage,
  rightPanelCollapsed,
  onExpandRightPanel,
  onCollapseRightPanel,
  theme,
  onToggleTheme,
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
  onDismissAuditGrid,
  subagentActiveCount,
  narrativeSpawnSuspected,
  onRequestMermaid,
  onRequestDiff,
  onSystemSettingsSaved,
  onRouteIntentChange,
  refreshApiKeyStatus,
}: AppShellProps) {
  const { t } = useT();

  return (
    <div className="flex flex-col h-screen w-screen bg-canvas">
      <SkipToMainLink />
      <StoragePressureBanner snapshot={storageSnapshot} level={storageLevel} />
      <TitleBar
        desktopHost={desktopHost}
        onNewWindow={() => {
          void createAgentWindow(selectedWorkspace).catch((e) => {
            toast.error((e as Error).message);
          });
        }}
        auditGridAvailable={auditGridAvailable}
        auditGridVisible={auditGridVisible}
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
        />
        <Sidebar
          sessions={visibleSessions}
          showAllSessions={showAllSessions}
          onToggleShowAllSessions={onToggleShowAllSessions}
          activeSessionId={activeSessionId}
          onNewSession={onNewSession}
          onSelectSession={onSelectSession}
          onDeleteSession={onDeleteSession}
          desktopHost={desktopHost}
          runtimeConn={runtimeConn}
          streaming={streaming}
          runtimeSessionEstablished={runtimeSessionEstablished}
          apiKeyConfigured={desktopApiKeyConfigured}
          activeInspector={activeInspector}
          onInspectorChange={onInspectorChange}
          collapsed={sidebarCollapsed}
          onToggleCollapse={onToggleSidebarCollapse}
          officeSession={officeSession}
          checklistActivity={checklistActivity}
          auditActivity={auditActivity}
          taskActivity={taskActivity}
          agentActivity={agentActivity}
        />
        {sidebarCollapsed && (
          <button
            type="button"
            onClick={onExpandSidebar}
            className="chrome-seam-r shrink-0 w-8 bg-canvas hover:bg-hover transition-colors flex items-center justify-center group"
            title={t('sidebar.expand')}
            aria-label={t('sidebar.expand')}
          >
            <svg
              className="w-3.5 h-3.5 text-t-text-muted group-hover:text-t-text transition-colors"
              viewBox="0 0 16 16"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              aria-hidden
            >
              <path d="M5 3.5v9" strokeLinecap="round" />
              <path d="M8 8l3-3v6l-3-3z" strokeLinejoin="round" />
            </svg>
          </button>
        )}
        <main
          id="main-content"
          tabIndex={-1}
          className="flex min-h-0 flex-1 flex-col min-w-0 bg-card outline-none"
        >
          <section className="order-2 shrink-0" aria-label={t('a11y.composerRegion')}>
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
              model={selectedModel}
              onModelChange={onModelChange}
              onOpenModelParams={() => onModelParamsOpenChange(true)}
              workspace={selectedWorkspace}
              onWorkspaceChange={onComposerWorkspaceChange}
              resumedThreadActive={resumedThreadId != null && resumedThreadId.length > 0}
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
          <section
            className="order-1 flex min-h-0 min-w-0 flex-1 flex-col"
            aria-label={t('a11y.chatLog')}
          >
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
              officeSession={officeSession}
              onOfficeQuickStart={officeSession ? onOfficeQuickStart : undefined}
            />
          </section>
        </main>
        {!auditGridVisible && !rightPanelCollapsed && (
          <RightPanel
            view={activeInspector}
            officeSession={officeSession}
            desktopHost={desktopHost}
            runtimeConn={runtimeConn}
            runtimeSessionEstablished={runtimeSessionEstablished}
            apiKeyConfigured={desktopApiKeyConfigured}
            onSavedApiKey={() => {
              refreshApiKeyStatus();
              toast.dismissAll();
            }}
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
          />
        )}
        {auditGridVisible && resumedThreadId && (
          <AuditGridPanel
            workspaceRoot={selectedWorkspace}
            resumedThreadId={resumedThreadId}
            streaming={streaming}
            runtimeConn={runtimeConn}
            runtimeSessionEstablished={runtimeSessionEstablished}
            agentStates={agentStates}
            subagentActiveCount={subagentActiveCount}
            narrativeSpawnSuspected={narrativeSpawnSuspected}
            openWorkspaceFile={openWorkspaceFile}
            onDismiss={onDismissAuditGrid}
          />
        )}
        {!auditGridVisible && rightPanelCollapsed && (
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
