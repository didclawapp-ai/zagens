import { useCallback } from 'react';
import { useT } from '../../../i18n';
import type { TurnBlock } from '../../../lib/chat/timeline/turnBlockTypes';
import { blocksToLegacyFields } from '../../../lib/chat/timeline/legacyMessageAdapter';
import CopyTextButton from '../../CopyTextButton';

/** Footer icon actions shown after an assistant turn finishes streaming. */
export function AssistantTurnActions({
  blocks,
  legacyContent,
}: {
  blocks: TurnBlock[];
  legacyContent?: string;
}) {
  const { t } = useT();
  const getText = useCallback(() => {
    const fromBlocks = blocksToLegacyFields(blocks).content.trim();
    if (fromBlocks) return fromBlocks;
    return (legacyContent ?? '').trim();
  }, [blocks, legacyContent]);

  const text = getText();
  if (!text) return null;

  return (
    <div
      className="message-assistant-actions mt-2 flex items-center gap-0.5 border-t border-t-border/40 pt-2"
      role="toolbar"
      aria-label={t('message.assistantActions')}
    >
      <CopyTextButton getText={getText} title={t('message.copyMessage')} />
    </div>
  );
}
