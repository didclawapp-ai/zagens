import { useMemo } from 'react';
import { useT } from '../../i18n';
import type { HarnessGridDataSnapshot } from '../../lib/useHarnessGridData';
import type { SessionFileChangeRow } from '../../lib/diff/sessionFileChanges';
import {
  harnessCardLineLabel,
  harnessFileChangeLineLabel,
  mapAgentsCardSummary,
  mapAuditCardSummary,
  mapChecklistCardSummary,
  mapLhtCardSummary,
} from '../../lib/harnessCardMappers';
import { HARNESS_CARD_VIEWS } from '../../lib/harnessCardViews';
import type { AgentState } from '../../types/agent';
import type { RightPanelView } from '../RightPanel';
import HarnessCard, { type HarnessCardId } from './HarnessCard';
import ProgressScrollViewport from './ProgressScrollViewport';
import { IconRailSvg } from './IconRailButton';

export type HarnessFloatStackProps = {
  visible: boolean;
  harnessData: HarnessGridDataSnapshot;
  sessionFileChanges: SessionFileChangeRow[];
  agentStates: AgentState[];
  flashCardId?: HarnessCardId | null;
  onHeadClick?: (cardId: HarnessCardId, view: RightPanelView) => void;
  onOpenDiffInPanel?: (relPath?: string) => void;
};

function MiniProgressBar({ pct }: { pct: number }) {
  return (
    <div className="harness-card__progress" aria-hidden>
      <span style={{ width: `${Math.max(0, Math.min(100, pct))}%` }} />
    </div>
  );
}

function FileChangeStats({ added, removed, running }: { added: number; removed: number; running: boolean }) {
  if (running && added === 0 && removed === 0) {
    return <span className="harness-file-change__stats">…</span>;
  }
  return (
    <span className="harness-file-change__stats">
      <span className="text-success">+{added}</span>{' '}
      <span className="text-t-error">−{removed}</span>
    </span>
  );
}

export default function HarnessFloatStack({
  visible,
  harnessData,
  sessionFileChanges,
  agentStates,
  flashCardId = null,
  onHeadClick,
  onOpenDiffInPanel,
}: HarnessFloatStackProps) {
  const { t } = useT();

  const sources = useMemo(
    () => ({
      checklist: harnessData.checklist,
      scratchpad: harnessData.scratchpad,
      taskGraph: harnessData.taskGraph,
      agents: agentStates,
    }),
    [harnessData.checklist, harnessData.scratchpad, harnessData.taskGraph, agentStates],
  );

  const checklist = mapChecklistCardSummary(harnessData.checklist);
  const audit = mapAuditCardSummary(harnessData.scratchpad);
  const lht = mapLhtCardSummary(harnessData.taskGraph);
  const agents = mapAgentsCardSummary(agentStates);

  if (!visible) {
    return null;
  }

  return (
    <aside
      className="harness-floats harness-floats--visible"
      aria-label={t('harnessCard.stackAria')}
    >
      <HarnessCard
        cardId="checklist"
        label={t('sidebar.checklist')}
        hasData={harnessData.hasChecklist}
        stat={checklist?.stat ?? '0/0'}
        className={flashCardId === 'checklist' ? 'harness-card--flash' : ''}
        onHeadClick={onHeadClick ? () => onHeadClick('checklist', HARNESS_CARD_VIEWS.checklist) : undefined}
        icon={
          <IconRailSvg>
            <path d="M9 6h11M9 12h11M9 18h11M5 6h.01M5 12h.01M5 18h.01" />
          </IconRailSvg>
        }
      >
        {checklist ? (
          <>
            {checklist.progressPct != null ? <MiniProgressBar pct={checklist.progressPct} /> : null}
            <ProgressScrollViewport
              items={checklist.items}
              maxRows={2}
              renderItem={(item) => harnessCardLineLabel('checklist', item.id, sources)}
            />
          </>
        ) : null}
      </HarnessCard>

      <HarnessCard
        cardId="audit"
        label={t('sidebar.audit')}
        hasData={harnessData.hasAudit}
        stat={audit ? t('harnessCard.openCount', { count: audit.stat }) : t('harnessCard.openCount', { count: '0' })}
        className={flashCardId === 'audit' ? 'harness-card--flash' : ''}
        onHeadClick={onHeadClick ? () => onHeadClick('audit', HARNESS_CARD_VIEWS.audit) : undefined}
        icon={
          <IconRailSvg>
            <path d="M4 6h16v12H4zM8 6V4h8v2M9 10h6M9 14h4" />
          </IconRailSvg>
        }
      >
        {audit ? (
          <ProgressScrollViewport
            items={audit.items}
            maxRows={2}
            renderItem={(item) => harnessCardLineLabel('audit', item.id, sources)}
          />
        ) : null}
      </HarnessCard>

      <HarnessCard
        cardId="lht"
        label={t('auditGrid.longHorizon')}
        hasData={harnessData.hasLongHorizon}
        stat={lht?.stat ?? '0%'}
        className={flashCardId === 'lht' ? 'harness-card--flash' : ''}
        onHeadClick={onHeadClick ? () => onHeadClick('lht', HARNESS_CARD_VIEWS.lht) : undefined}
        icon={
          <IconRailSvg>
            <path d="M12 3l7 4v6c0 4-3 7-7 8-4-1-7-4-7-8V7l7-4z" />
            <path d="M9 12l2 2 4-4" />
          </IconRailSvg>
        }
      >
        {lht ? (
          <>
            {lht.progressPct != null ? <MiniProgressBar pct={lht.progressPct} /> : null}
            <ProgressScrollViewport
              items={lht.items}
              maxRows={2}
              renderItem={(item) => harnessCardLineLabel('lht', item.id, sources)}
            />
          </>
        ) : null}
      </HarnessCard>

      <HarnessCard
        cardId="changes"
        label={t('harnessCard.fileChanges')}
        hasData={sessionFileChanges.length > 0}
        stat={String(sessionFileChanges.length)}
        className={flashCardId === 'changes' ? 'harness-card--flash' : ''}
        onHeadClick={
          onOpenDiffInPanel
            ? () =>
                onOpenDiffInPanel(
                  sessionFileChanges[sessionFileChanges.length - 1]?.path,
                )
            : undefined
        }
        icon={
          <IconRailSvg>
            <path d="M4 7h16M4 12h10M4 17h14" />
            <path d="M15 10l3 3-3 3" />
          </IconRailSvg>
        }
      >
        {sessionFileChanges.length > 0 ? (
          <div
            className="harness-file-changes-scroll"
            onClick={(event) => event.stopPropagation()}
            onWheel={(event) => event.stopPropagation()}
          >
            {sessionFileChanges.map((row) => (
              <button
                key={row.path}
                type="button"
                className="harness-file-change"
                onClick={(event) => {
                  event.stopPropagation();
                  onOpenDiffInPanel?.(row.path);
                }}
              >
                <span className="harness-file-change__name" title={row.path}>
                  {harnessFileChangeLineLabel(row)}
                </span>
                <FileChangeStats
                  added={row.added}
                  removed={row.removed}
                  running={row.status === 'running'}
                />
              </button>
            ))}
          </div>
        ) : null}
      </HarnessCard>

      <HarnessCard
        cardId="agents"
        label={t('sidebar.agents')}
        hasData={harnessData.hasAgents}
        stat={agents?.stat ?? '0/0'}
        className={flashCardId === 'agents' ? 'harness-card--flash' : ''}
        onHeadClick={onHeadClick ? () => onHeadClick('agents', HARNESS_CARD_VIEWS.agents) : undefined}
        icon={
          <IconRailSvg>
            <path d="M12 3a4 4 0 014 4v1h2a2 2 0 012 2v10a2 2 0 01-2 2H6a2 2 0 01-2-2V10a2 2 0 012-2h2V7a4 4 0 014-4z" />
          </IconRailSvg>
        }
      >
        {agents ? (
          <>
            {agents.progressPct != null ? <MiniProgressBar pct={agents.progressPct} /> : null}
            <ProgressScrollViewport
              items={agents.items}
              maxRows={2}
              renderItem={(item) => harnessCardLineLabel('agents', item.id, sources)}
            />
          </>
        ) : null}
      </HarnessCard>
    </aside>
  );
}
