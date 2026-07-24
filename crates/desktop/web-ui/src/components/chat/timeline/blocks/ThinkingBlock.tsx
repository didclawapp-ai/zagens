import { memo, useEffect, useRef, useState } from 'react';
import { MessageMetaBar } from '../../MessageMetaBar';
import { IconSparkle } from '../../../icons/FlatIcons';
import { useT } from '../../../../i18n';
import type { TurnBlock } from '../../../../lib/chat/timeline/turnBlockTypes';

/** ~3–4 lines of text-sm / leading-relaxed (P4.5). */
const REASONING_PREVIEW_MAX_CLASS = 'max-h-[4.5rem]';

export const ThinkingBlock = memo(function ThinkingBlock({
  block,
  isTurnStreaming,
}: {
  block: Extract<TurnBlock, { kind: 'thinking' }>;
  isTurnStreaming: boolean;
}) {
  const { t } = useT();
  const active = isTurnStreaming && block.streaming !== false;
  const [expanded, setExpanded] = useState(active);
  const [viewportExpanded, setViewportExpanded] = useState(false);
  const userToggledRef = useRef(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickBottomRef = useRef(true);

  useEffect(() => {
    if (userToggledRef.current) return;
    if (active) setExpanded(true);
  }, [block.id, active]);

  useEffect(() => {
    if (active || userToggledRef.current || isTurnStreaming) return;
    setExpanded(false);
  }, [active, isTurnStreaming]);

  useEffect(() => {
    if (active) return;
    setViewportExpanded(false);
  }, [active]);

  useEffect(() => {
    if (!active || !expanded) return;
    const el = scrollRef.current;
    if (!el || !stickBottomRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [block.text, active, expanded, viewportExpanded]);

  const hint =
    active && !block.text.trim()
      ? t('message.reasoningStreaming')
      : t('message.reasoningCollapsed');

  const panelClass = active
    ? `timeline-reasoning-panel${
        viewportExpanded ? ' timeline-reasoning-panel--expanded' : ''
      }`
    : REASONING_PREVIEW_MAX_CLASS;

  return (
    <MessageMetaBar
      icon={<IconSparkle className="size-3.5" />}
      label={t('message.reasoning')}
      hint={hint}
      expanded={expanded}
      panelOpen={expanded}
      chevronOpen={active ? viewportExpanded : expanded}
      hintVisible={active ? Boolean(hint) && !viewportExpanded : undefined}
      onToggle={() => {
        userToggledRef.current = true;
        if (active) {
          if (!expanded) {
            setExpanded(true);
            setViewportExpanded(false);
          } else {
            setViewportExpanded((v) => !v);
          }
          return;
        }
        setExpanded((v) => !v);
      }}
      copyText={block.text.trim()}
      copyTitle={t('chatMarkdown.copyReasoning')}
      copyDisabled={!block.text.trim()}
    >
      <div
        ref={scrollRef}
        onScroll={() => {
          const el = scrollRef.current;
          if (!el) return;
          stickBottomRef.current =
            el.scrollHeight - el.scrollTop - el.clientHeight <= 48;
        }}
        className={`${panelClass} overflow-y-auto whitespace-pre-wrap text-sm leading-relaxed`}
      >
        {block.text ||
          (active ? t('message.reasoningStreamingPlaceholder') : '')}
      </div>
    </MessageMetaBar>
  );
});
