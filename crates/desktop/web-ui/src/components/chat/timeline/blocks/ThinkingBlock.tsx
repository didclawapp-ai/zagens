import { useEffect, useRef, useState } from 'react';
import { MessageMetaBar } from '../../MessageMetaBar';
import { IconSparkle } from '../../../icons/FlatIcons';
import { useT } from '../../../../i18n';
import type { TurnBlock } from '../../../../lib/chat/timeline/turnBlockTypes';

export function ThinkingBlock({
  block,
  isTurnStreaming,
}: {
  block: Extract<TurnBlock, { kind: 'thinking' }>;
  isTurnStreaming: boolean;
}) {
  const { t } = useT();
  const active = isTurnStreaming && block.streaming !== false;
  const [expanded, setExpanded] = useState(active);
  const userToggledRef = useRef(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const stickBottomRef = useRef(true);

  useEffect(() => {
    userToggledRef.current = false;
    setExpanded(active);
  }, [block.id, active]);

  useEffect(() => {
    if (active || userToggledRef.current) return;
    setExpanded(false);
  }, [active]);

  useEffect(() => {
    if (!active || !expanded || userToggledRef.current) return;
    const el = scrollRef.current;
    if (!el || !stickBottomRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [block.text, active, expanded]);

  const hint =
    active && !block.text.trim()
      ? t('message.reasoningStreaming')
      : t('message.reasoningCollapsed');

  return (
    <MessageMetaBar
      icon={<IconSparkle className="size-3.5" />}
      label={t('message.reasoning')}
      hint={hint}
      expanded={expanded}
      onToggle={() => {
        userToggledRef.current = true;
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
        className="max-h-[40vh] overflow-y-auto whitespace-pre-wrap"
      >
        {block.text ||
          (active ? t('message.reasoningStreamingPlaceholder') : '')}
      </div>
    </MessageMetaBar>
  );
}
