import { useEffect, useRef } from 'react';
import type { TurnBlock } from '../../../lib/chat/timeline/turnBlockTypes';

function findActiveBlockId(blocks: TurnBlock[], isTurnStreaming: boolean): string | null {
  if (!isTurnStreaming) return null;
  for (let i = blocks.length - 1; i >= 0; i--) {
    const block = blocks[i];
    if (block.kind === 'thinking' && block.streaming !== false) return block.id;
    if (block.kind === 'text' && block.streaming !== false) return block.id;
    if (block.kind === 'tool' && block.status === 'running') return block.id;
  }
  return blocks[blocks.length - 1]?.id ?? null;
}

/** Scroll the chat container to keep the active timeline block in view while streaming. */
export function useTurnScroll(
  containerRef: React.RefObject<HTMLElement | null>,
  blocks: TurnBlock[],
  isTurnStreaming: boolean,
) {
  const activeId = findActiveBlockId(blocks, isTurnStreaming);
  const prevActiveRef = useRef<string | null>(null);

  useEffect(() => {
    if (!isTurnStreaming || !activeId || activeId === prevActiveRef.current) return;
    prevActiveRef.current = activeId;
    const container = containerRef.current;
    if (!container) return;
    const el = container.querySelector(`[data-timeline-block="${activeId}"]`);
    if (!(el instanceof HTMLElement)) return;
    const containerRect = container.getBoundingClientRect();
    const elRect = el.getBoundingClientRect();
    const below = elRect.bottom - containerRect.bottom;
    const above = containerRect.top - elRect.top;
    if (below > 48) {
      container.scrollTop += below + 24;
    } else if (above > 48) {
      container.scrollTop -= above + 24;
    }
  }, [activeId, blocks, containerRef, isTurnStreaming]);
}
