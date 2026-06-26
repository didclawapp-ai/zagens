import type { ReactNode } from 'react';
import type { RightPanelView } from '../RightPanel';
import type { RuntimeConnectionState } from '../../api/client';
import { useT } from '../../i18n';
import type { InspectorNavActivity } from '../../lib/inspectorUnread';
import IconRailButton, { IconRailSvg } from './IconRailButton';
import RuntimeConnIndicator from './RuntimeConnIndicator';
import SettingsRailMenu from './SettingsRailMenu';
import ThemeRailMenu from './ThemeRailMenu';
import type { HarnessCardId } from './HarnessCard';
import type { Theme } from '../../lib/appPreferences';

export type IconRailProps = {
  sessionStripOpen: boolean;
  onToggleSessionStrip: () => void;
  onNewSession: () => void;
  activeInspector: RightPanelView;
  onInspectorChange: (view: RightPanelView) => void;
  onExpandRightPanel?: () => void;
  onHarnessNavigate?: (cardId: HarnessCardId) => void;
  harnessFlashId?: HarnessCardId | null;
  desktopHost: boolean;
  officeSession?: boolean;
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
  apiKeyConfigured?: boolean | null;
  checklistActivity?: InspectorNavActivity;
  auditActivity?: InspectorNavActivity;
  taskActivity?: InspectorNavActivity;
  agentActivity?: InspectorNavActivity;
  sessionStripControlsId?: string;
  theme: Theme;
  onThemeChange: (theme: Theme) => void;
};

function HarnessRailButton({
  label,
  cardId,
  flash,
  activity,
  onClick,
  children,
}: {
  label: string;
  cardId: HarnessCardId;
  flash?: boolean;
  activity?: InspectorNavActivity;
  onClick?: (cardId: HarnessCardId) => void;
  children: ReactNode;
}) {
  return (
    <IconRailButton
      label={label}
      highlight={flash}
      onClick={() => onClick?.(cardId)}
      className="icon-rail-btn--harness"
    >
      {children}
      {activity?.active ? (
        <span
          className={`icon-rail-activity-dot ${activity.pulse ? 'animate-pulse' : ''}`}
          aria-hidden
        />
      ) : null}
    </IconRailButton>
  );
}

export default function IconRail({
  sessionStripOpen,
  onToggleSessionStrip,
  onNewSession,
  activeInspector,
  onInspectorChange,
  onExpandRightPanel,
  onHarnessNavigate,
  harnessFlashId = null,
  desktopHost,
  officeSession = false,
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
  apiKeyConfigured = null,
  checklistActivity,
  auditActivity,
  taskActivity,
  agentActivity,
  sessionStripControlsId = 'session-strip',
  theme,
  onThemeChange,
}: IconRailProps) {
  const { t } = useT();

  return (
    <nav className="icon-rail" aria-label={t('a11y.sidebarNav')}>
      <div className="icon-rail-logo" title={t('app.title')}>
        <img src="/app-icon.png" alt="" width={32} height={32} className="icon-rail-logo__img" />
      </div>

      <div className="icon-rail-group">
        <IconRailButton label={t('sidebar.newSession')} onClick={onNewSession}>
          <IconRailSvg>
            <path d="M12 5v14M5 12h14" />
          </IconRailSvg>
        </IconRailButton>
        <IconRailButton
          label={t('iconRail.sessionList')}
          active={sessionStripOpen}
          expanded={sessionStripOpen}
          controls={sessionStripControlsId}
          onClick={onToggleSessionStrip}
        >
          <IconRailSvg>
            <rect x="3" y="4" width="18" height="16" rx="2" />
            <path d="M8 4v16M11 9h7M11 13h5M11 17h6" />
          </IconRailSvg>
        </IconRailButton>
        <IconRailButton
          label={t('sidebar.workspace')}
          active={activeInspector === 'workspace'}
          onClick={() => {
            onExpandRightPanel?.();
            onInspectorChange('workspace');
          }}
        >
          <IconRailSvg>
            <path d="M4 6h16v12H4zM8 6V4h8v2" />
          </IconRailSvg>
        </IconRailButton>
        <IconRailButton
          label={t('sidebar.usage')}
          active={activeInspector === 'usage'}
          onClick={() => {
            onExpandRightPanel?.();
            onInspectorChange('usage');
          }}
        >
          <IconRailSvg>
            <path d="M4 19h16M6 16l3-5 3 3 4-7 4 9" />
          </IconRailSvg>
        </IconRailButton>
      </div>

      <div className="icon-rail-divider" aria-hidden />

      {!officeSession ? (
        <div className="icon-rail-group" aria-label={t('auditGrid.panelAria')}>
          <HarnessRailButton
            label={t('sidebar.checklist')}
            cardId="checklist"
            flash={harnessFlashId === 'checklist'}
            activity={checklistActivity}
            onClick={onHarnessNavigate}
          >
            <IconRailSvg>
              <path d="M9 6h11M9 12h11M9 18h11M5 6h.01M5 12h.01M5 18h.01" />
            </IconRailSvg>
          </HarnessRailButton>
          <HarnessRailButton
            label={t('sidebar.audit')}
            cardId="audit"
            flash={harnessFlashId === 'audit'}
            activity={auditActivity}
            onClick={onHarnessNavigate}
          >
            <IconRailSvg>
              <path d="M4 6h16v12H4zM8 6V4h8v2M9 10h6M9 14h4" />
            </IconRailSvg>
          </HarnessRailButton>
          <HarnessRailButton
            label={t('auditGrid.longHorizon')}
            cardId="lht"
            flash={harnessFlashId === 'lht'}
            activity={taskActivity}
            onClick={onHarnessNavigate}
          >
            <IconRailSvg>
              <path d="M12 3l7 4v6c0 4-3 7-7 8-4-1-7-4-7-8V7l7-4z" />
              <path d="M9 12l2 2 4-4" />
            </IconRailSvg>
          </HarnessRailButton>
          <HarnessRailButton
            label={t('sidebar.agents')}
            cardId="agents"
            flash={harnessFlashId === 'agents'}
            activity={agentActivity}
            onClick={onHarnessNavigate}
          >
            <IconRailSvg>
              <path d="M12 3a4 4 0 014 4v1h2a2 2 0 012 2v10a2 2 0 01-2 2H6a2 2 0 01-2-2V10a2 2 0 012-2h2V7a4 4 0 014-4z" />
            </IconRailSvg>
          </HarnessRailButton>
        </div>
      ) : (
        <div className="icon-rail-group">
          <IconRailButton
            label={t('sidebar.tasks')}
            active={activeInspector === 'tasks'}
            onClick={() => {
              onExpandRightPanel?.();
              onInspectorChange('tasks');
            }}
          >
            <IconRailSvg>
              <path d="M9 6h11M9 12h11M9 18h7M5 6h.01M5 12h.01M5 18h.01" />
            </IconRailSvg>
          </IconRailButton>
        </div>
      )}

      <div className="icon-rail-spacer" aria-hidden />

      <div className="icon-rail-group icon-rail-group--bottom">
        <ThemeRailMenu theme={theme} onThemeChange={onThemeChange} />
        <SettingsRailMenu
          activeInspector={activeInspector}
          onInspectorChange={onInspectorChange}
          desktopHost={desktopHost}
          officeSession={officeSession}
          onExpandRightPanel={onExpandRightPanel}
        />
        {desktopHost && apiKeyConfigured === false ? (
          <p className="icon-rail-api-hint">{t('sidebar.apiKeyNotConfigured')}</p>
        ) : null}
        <RuntimeConnIndicator
          runtimeConn={runtimeConn}
          streaming={streaming}
          runtimeSessionEstablished={runtimeSessionEstablished}
          labels={{
            connected: t('common.connectionNormal'),
            disconnected: t('common.connectionDisconnected'),
            busy: t('common.connectionBusy'),
            authMismatch: t('common.runtimeAuthMismatch'),
            checking: t('common.runtimeChecking'),
          }}
        />
      </div>
    </nav>
  );
}
