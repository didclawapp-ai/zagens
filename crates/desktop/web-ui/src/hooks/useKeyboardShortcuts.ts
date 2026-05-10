import { useEffect } from 'react';

type ShortcutHandler = () => void;

interface ShortcutDef {
  key: string;
  ctrl?: boolean;
  shift?: boolean;
  handler: ShortcutHandler;
  description: string;
}

interface Props {
  shortcuts: ShortcutDef[];
}

export default function useKeyboardShortcuts(shortcuts: ShortcutDef[]) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Don't capture when an input/textarea is focused (Ctrl+K / Ctrl+N are meta shortcuts)
      const tag = (e.target as HTMLElement)?.tagName?.toLowerCase();
      const isInput = tag === 'input' || tag === 'textarea' || tag === 'select';

      for (const s of shortcuts) {
        const ctrlMatch = s.ctrl ? (e.ctrlKey || e.metaKey) : true;
        const shiftMatch = s.shift ? e.shiftKey : !e.shiftKey;
        if (e.key.toLowerCase() === s.key.toLowerCase() && ctrlMatch && shiftMatch) {
          // Allow Ctrl+K/N even when in input fields
          if (isInput && !(s.ctrl && (s.key === 'k' || s.key === 'n'))) {
            continue;
          }
          e.preventDefault();
          s.handler();
          return;
        }
      }

      // Escape in input fields: blur
      if (e.key === 'Escape' && isInput) {
        (e.target as HTMLElement).blur();
        return;
      }
    };

    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [shortcuts]);
}
