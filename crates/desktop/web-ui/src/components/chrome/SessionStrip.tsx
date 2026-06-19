import { useEffect, useMemo, useState } from 'react';
import { useT } from '../../i18n';
import {
  groupSessionsByDate,
  SESSIONS_VISIBLE_PER_DAY,
  type SessionStripSession,
} from '../../lib/chat/sessionStripGrouping';

export type { SessionStripSession } from '../../lib/chat/sessionStripGrouping';

export type SessionStripProps = {
  open: boolean;
  sessions: SessionStripSession[];
  showAllSessions?: boolean;
  onToggleShowAllSessions?: () => void;
  activeSessionId: string | null;
  /** Session ids with an in-flight turn (multi-session spinner). */
  streamingSessionIds?: Set<string>;
  onSelectSession?: (id: string) => void;
  onDeleteSession?: (id: string) => void;
  id?: string;
};

function SessionStatusIcon({ streaming }: { streaming: boolean }) {
  const { t } = useT();
  if (streaming) {
    return (
      <span
        className="session-row-spinner"
        title={t('common.sessionStreaming')}
        aria-label={t('common.sessionStreaming')}
      />
    );
  }
  return (
    <svg
      className="session-row-check"
      viewBox="0 0 16 16"
      aria-hidden
      focusable="false"
    >
      <path
        d="M3.25 8.25 6.25 11.25 12.75 4.75"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export default function SessionStrip({
  open,
  sessions,
  showAllSessions = false,
  onToggleShowAllSessions,
  activeSessionId,
  streamingSessionIds,
  onSelectSession,
  onDeleteSession,
  id = 'session-strip',
}: SessionStripProps) {
  const { t } = useT();
  const [expandedDates, setExpandedDates] = useState<Set<string>>(() => new Set());

  const dateGroups = useMemo(() => groupSessionsByDate(sessions), [sessions]);

  useEffect(() => {
    if (!activeSessionId) return;
    for (const group of dateGroups) {
      if (group.sessions.some((s) => s.id === activeSessionId)) {
        setExpandedDates((prev) => {
          if (prev.has(group.dateKey)) return prev;
          const next = new Set(prev);
          next.add(group.dateKey);
          return next;
        });
      }
    }
  }, [activeSessionId, dateGroups]);

  const renderSessionRow = (session: SessionStripSession) => {
    const isActive = activeSessionId != null && session.id === activeSessionId;
    const isStreaming = streamingSessionIds?.has(session.id) ?? false;
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
          <SessionStatusIcon streaming={isStreaming} />
          <span className="session-row-title truncate">
            {session.name || session.id.slice(0, 8)}
          </span>
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
  };

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
        {dateGroups.map((group) => {
          const expanded = expandedDates.has(group.dateKey);
          const hasOverflow = group.sessions.length > SESSIONS_VISIBLE_PER_DAY;
          const visibleSessions =
            expanded || !hasOverflow
              ? group.sessions
              : group.sessions.slice(0, SESSIONS_VISIBLE_PER_DAY);

          return (
            <section key={group.dateKey} className="session-strip-group" aria-label={group.dateKey}>
              <h3 className="session-strip-date">{group.dateKey}</h3>
              {visibleSessions.map(renderSessionRow)}
              {!expanded && hasOverflow ? (
                <button
                  type="button"
                  className="session-strip-more"
                  onClick={() =>
                    setExpandedDates((prev) => new Set(prev).add(group.dateKey))
                  }
                >
                  {t('sidebar.sessionsMore')}
                </button>
              ) : null}
            </section>
          );
        })}
      </div>
    </aside>
  );
}
