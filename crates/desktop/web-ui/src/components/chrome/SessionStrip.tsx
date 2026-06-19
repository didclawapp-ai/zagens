import { useT } from '../../i18n';

export interface SessionStripSession {
  id: string;
  name: string;
  created_at?: number;
  updated_at?: number;
}

export type SessionStripProps = {
  open: boolean;
  sessions: SessionStripSession[];
  showAllSessions?: boolean;
  onToggleShowAllSessions?: () => void;
  activeSessionId: string | null;
  onSelectSession?: (id: string) => void;
  onDeleteSession?: (id: string) => void;
  id?: string;
};

export default function SessionStrip({
  open,
  sessions,
  showAllSessions = false,
  onToggleShowAllSessions,
  activeSessionId,
  onSelectSession,
  onDeleteSession,
  id = 'session-strip',
}: SessionStripProps) {
  const { t } = useT();

  return (
    <aside
      id={id}
      className={`session-strip ${open ? 'session-strip--open' : 'session-strip--collapsed'}`}
      aria-label={t('iconRail.sessionList')}
      aria-hidden={!open}
    >
      <div className="session-strip-head">
        <span>{t('common.sessions')}</span>
        {onToggleShowAllSessions ? (
          <button
            type="button"
            className="session-strip-toggle-all"
            onClick={onToggleShowAllSessions}
            title={t('sidebar.showAllSessionsHint')}
          >
            {showAllSessions ? '✓ ' : ''}
            {t('sidebar.showAllSessions')}
          </button>
        ) : null}
      </div>
      <div className="session-strip-list">
        {sessions.length === 0 ? (
          <p className="session-strip-empty">{t('common.noSessions')}</p>
        ) : null}
        {sessions.map((session) => {
          const isActive = activeSessionId != null && session.id === activeSessionId;
          return (
            <div
              key={session.id}
              className={`session-row group ${isActive ? 'session-row--active' : ''}`}
            >
              <button
                type="button"
                onClick={() => onSelectSession?.(session.id)}
                className="session-row-btn flex-1 min-w-0 truncate"
              >
                {session.name || session.id.slice(0, 8)}
              </button>
              {onDeleteSession ? (
                <button
                  type="button"
                  title={t('sidebar.deleteSessionTitle')}
                  onClick={(event) => {
                    event.stopPropagation();
                    onDeleteSession(session.id);
                  }}
                  className="shrink-0 px-2 py-2 text-t-text-muted hover:text-t-error opacity-0 group-hover:opacity-100 transition-opacity"
                >
                  ×
                </button>
              ) : null}
            </div>
          );
        })}
      </div>
    </aside>
  );
}
