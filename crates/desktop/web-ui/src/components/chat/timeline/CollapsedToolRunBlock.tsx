import { useLayoutEffect, useRef, useState } from 'react';
import { MessageMetaBar } from '../MessageMetaBar';
import { IconSparkle, IconWrench } from '../../icons/FlatIcons';
import { useT } from '../../../i18n';
import type { TurnBlock } from '../../../lib/chat/timeline/turnBlockTypes';
import type { TimelineCollapsedCategory } from '../../../lib/chat/timeline/timelinePresentationTypes';
import { summarizeToolCalls } from '../../../lib/chat/summarizeToolCalls';
import { scrollTopToPinElementTop } from '../../../lib/chat/chatScrollAnchor';
import { ToolBlock } from './blocks/ToolBlock';
import { toolBlockToCardModel } from './blocks/toolBlockRouter';
import {
  shouldPreferActivityExpanded,
  shouldUseLiveHoldPanel,
} from './collapsedToolRunExpand';
import type { AgentState } from '../../../types/agent';

const LIVE_PANEL_STICK_THRESHOLD_PX = 48;

function findChatScroller(from: HTMLElement | null): HTMLElement | null {
  if (!from) return null;
  return from.closest('[role="log"]');
}

export function CollapsedToolRunBlock({
  blocks,
  category,
  absorbedThinking,
  absorbedCaptions,
  onOpenDiffInPanel,
  agentStates,
  isTurnStreaming = false,
  isTrailingActivity = false,
}: {
  blocks: Extract<TurnBlock, { kind: 'tool' }>[];
  category: TimelineCollapsedCategory;
  absorbedThinking?: Extract<TurnBlock, { kind: 'thinking' }>[];
  absorbedCaptions?: Extract<TurnBlock, { kind: 'text' }>[];
  onOpenDiffInPanel?: () => void;
  agentStates?: AgentState[];
  isTurnStreaming?: boolean;
  /** True when this is the last activity row in the current presentation slice. */
  isTrailingActivity?: boolean;
}) {
  const { t } = useT();
  const rootRef = useRef<HTMLDivElement>(null);
  const livePanelRef = useRef<HTMLDivElement>(null);
  const livePanelStickRef = useRef(true);
  const pinTopRef = useRef<number | null>(null);
  const tools = blocks.map(toolBlockToCardModel);
  const label = summarizeToolCalls(tools, t, {
    captions: absorbedCaptions?.map((c) => c.content),
  });
  const thinkingCount = absorbedThinking?.length ?? 0;
  const runningCount = tools.filter((tool) => tool.status === 'running').length;

  const useLiveHoldPanel = shouldUseLiveHoldPanel({
    isTurnStreaming,
    isTrailingActivity,
  });

  const preferExpanded = shouldPreferActivityExpanded({
    isTurnStreaming,
    runningCount,
    isTrailingActivity,
  });

  const [expanded, setExpanded] = useState(preferExpanded);
  const [thinkingOpen, setThinkingOpen] = useState(
    preferExpanded && thinkingCount > 0,
  );
  const [userToggledExpand, setUserToggledExpand] = useState(false);
  const [userToggledThinking, setUserToggledThinking] = useState(false);
  const [livePanelViewportExpanded, setLivePanelViewportExpanded] = useState(false);

  const panelExpanded = useLiveHoldPanel || expanded;
  const showThinkingBody =
    thinkingCount > 0 &&
    absorbedThinking &&
    (useLiveHoldPanel || thinkingOpen);

  useLayoutEffect(() => {
    if (useLiveHoldPanel || userToggledExpand) return;
    if (preferExpanded === expanded) return;
    const top = rootRef.current?.getBoundingClientRect().top;
    if (typeof top === 'number' && !preferExpanded) {
      pinTopRef.current = top;
    } else {
      pinTopRef.current = null;
    }
    setExpanded(preferExpanded);
  }, [preferExpanded, expanded, userToggledExpand, useLiveHoldPanel]);

  useLayoutEffect(() => {
    if (useLiveHoldPanel) return;
    const pinTop = pinTopRef.current;
    if (pinTop == null) return;
    pinTopRef.current = null;
    const root = rootRef.current;
    const scroller = findChatScroller(root);
    if (!root || !scroller) return;
    const topAfter = root.getBoundingClientRect().top;
    scroller.scrollTop = scrollTopToPinElementTop(scroller, pinTop, topAfter);
  }, [expanded, useLiveHoldPanel]);

  useLayoutEffect(() => {
    if (useLiveHoldPanel || userToggledThinking) return;
    setThinkingOpen(preferExpanded && thinkingCount > 0);
  }, [preferExpanded, thinkingCount, userToggledThinking, useLiveHoldPanel]);

  useLayoutEffect(() => {
    if (isTurnStreaming) return;
    setUserToggledExpand(false);
    setUserToggledThinking(false);
    setLivePanelViewportExpanded(false);
  }, [isTurnStreaming]);

  const thinkingSignature =
    absorbedThinking?.map((th) => `${th.id}:${th.text.length}`).join('|') ?? '';
  const toolSignature = blocks
    .map((b) => `${b.id}:${b.status}:${(b.output ?? '').length}`)
    .join('|');

  useLayoutEffect(() => {
    if (!useLiveHoldPanel || !panelExpanded) return;
    const el = livePanelRef.current;
    if (!el || !livePanelStickRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [useLiveHoldPanel, panelExpanded, thinkingSignature, toolSignature]);

  const hint = useLiveHoldPanel
    ? runningCount > 0
      ? t('message.toolsRunning', { count: String(runningCount) })
      : t('message.timelineLiveHoldHint')
    : runningCount > 0
      ? t('message.toolsRunning', { count: String(runningCount) })
      : undefined;

  const useCompact =
    category === 'explore' ||
    category === 'plan' ||
    category === 'write' ||
    category === 'shell' ||
    category === 'office' ||
    category === 'workflow' ||
    category === 'agent' ||
    category === 'browser' ||
    category === 'mixed';

  const liveToolId = tools.find((tool) => tool.status === 'running')?.id;
  const showLiveToolDetails = useLiveHoldPanel && livePanelViewportExpanded;

  const body = (
    <div className="space-y-1.5">
      {thinkingCount > 0 && absorbedThinking ? (
        <div
          className={
            useLiveHoldPanel
              ? 'rounded-md border border-card-border/60 bg-canvas-alt/40 px-2 py-1.5'
              : 'rounded-md border border-card-border/60 bg-canvas-alt/40'
          }
        >
          {!useLiveHoldPanel ? (
            <button
              type="button"
              className="flex w-full items-center gap-1.5 px-2 py-1.5 text-left text-[11px] text-t-text-muted hover:text-t-text"
              onClick={() => {
                setUserToggledThinking(true);
                setThinkingOpen((v) => !v);
              }}
              aria-expanded={thinkingOpen}
            >
              <IconSparkle className="size-3 shrink-0" />
              <span className="min-w-0 flex-1 truncate">
                {t('message.timelineAbsorbedReasoning', {
                  count: String(thinkingCount),
                })}
              </span>
              <span>{thinkingOpen ? '▾' : '▸'}</span>
            </button>
          ) : (
            <div className="mb-1.5 flex items-center gap-1.5 text-[11px] text-t-text-muted">
              <IconSparkle className="size-3 shrink-0" />
              <span>
                {t('message.timelineAbsorbedReasoning', {
                  count: String(thinkingCount),
                })}
              </span>
            </div>
          )}
          {showThinkingBody ? (
            <div
              className={
                useLiveHoldPanel
                  ? 'space-y-2 text-[11px] leading-relaxed whitespace-pre-wrap text-t-text-secondary'
                  : 'max-h-[4.5rem] space-y-2 overflow-y-auto border-t border-card-border/40 px-2 py-1.5 text-[11px] leading-relaxed whitespace-pre-wrap text-t-text-secondary'
              }
            >
              {absorbedThinking.map((th) => (
                <div key={th.id}>{th.text.trim() || '…'}</div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}
      {blocks.map((block) => (
        <ToolBlock
          key={block.id}
          block={block}
          onOpenDiffInPanel={onOpenDiffInPanel}
          agentStates={agentStates}
          compact={useCompact && !showLiveToolDetails}
        />
      ))}
    </div>
  );

  return (
    <div
      ref={rootRef}
      data-timeline-live-hold={useLiveHoldPanel ? 'true' : undefined}
      data-timeline-live-hold-expanded={
        useLiveHoldPanel && livePanelViewportExpanded ? 'true' : undefined
      }
      data-timeline-activity={runningCount > 0 ? 'live' : isTrailingActivity ? 'hold' : 'done'}
      {...(liveToolId ? { 'data-timeline-block': liveToolId } : {})}
    >
      <MessageMetaBar
        icon={<IconWrench className="size-3.5" />}
        label={label}
        hint={hint}
        expanded={panelExpanded}
        panelOpen={useLiveHoldPanel ? true : panelExpanded}
        chevronOpen={useLiveHoldPanel ? livePanelViewportExpanded : panelExpanded}
        hintVisible={
          useLiveHoldPanel
            ? Boolean(hint) && !livePanelViewportExpanded
            : undefined
        }
        onToggle={() => {
          if (useLiveHoldPanel) {
            setLivePanelViewportExpanded((v) => !v);
            return;
          }
          setUserToggledExpand(true);
          setExpanded((v) => !v);
        }}
      >
        {useLiveHoldPanel ? (
          <div
            ref={livePanelRef}
            className={`timeline-activity-live-panel${
              livePanelViewportExpanded
                ? ' timeline-activity-live-panel--expanded'
                : ''
            }`}
            onScroll={() => {
              const el = livePanelRef.current;
              if (!el) return;
              livePanelStickRef.current =
                el.scrollHeight - el.scrollTop - el.clientHeight <=
                LIVE_PANEL_STICK_THRESHOLD_PX;
            }}
          >
            {body}
          </div>
        ) : (
          body
        )}
      </MessageMetaBar>
    </div>
  );
}
