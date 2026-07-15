import { useEffect } from 'react';

type ShortcutHandler = () => void;

export interface ShortcutDef {
  /** Logical key (`e.key`), case-insensitive. Prefer `code` for layout-stable chords. */
  key?: string;
  /** Physical key (`e.code`), e.g. `Backquote` for Ctrl+` / Ctrl+Shift+`. */
  code?: string;
  ctrl?: boolean;
  shift?: boolean;
  /** When set, also fires while an input/textarea/select is focused (default skips most shortcuts there). */
  global?: boolean;
  handler: ShortcutHandler;
  description: string;
}

export default function useKeyboardShortcuts(shortcuts: ShortcutDef[]) {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Don't capture when an input/textarea is focused (Ctrl+K / Ctrl+N are meta shortcuts)
      const tag = (e.target as HTMLElement)?.tagName?.toLowerCase();
      const isInput = tag === 'input' || tag === 'textarea' || tag === 'select';

      for (const s of shortcuts) {
        const ctrlMatch = s.ctrl ? (e.ctrlKey || e.metaKey) : !(e.ctrlKey || e.metaKey);
        const shiftMatch = s.shift ? e.shiftKey : !e.shiftKey;
        const keyMatch = s.code
          ? e.code === s.code
          : s.key != null && e.key.toLowerCase() === s.key.toLowerCase();
        if (!keyMatch || !ctrlMatch || !shiftMatch) {
          continue;
        }
        const allowInInput =
          s.global ||
          (s.ctrl &&
            (s.key === 'k' ||
              s.key === 'n' ||
              s.code === 'Backquote'));
        if (isInput && !allowInInput) {
          continue;
        }
        e.preventDefault();
        s.handler();
        return;
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
