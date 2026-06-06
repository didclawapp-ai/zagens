import { useEffect, useState } from 'react';
import { useT } from '../i18n';

interface Props {
  open: boolean;
  toolName: string;
  description: string;
  busy?: boolean;
  onApprove: (rememberForSession: boolean) => void;
  onDeny: () => void;
}

export default function ApprovalDialog({
  open,
  toolName,
  description,
  busy = false,
  onApprove,
  onDeny,
}: Props) {
  const { t } = useT();
  const [rememberForSession, setRememberForSession] = useState(false);

  useEffect(() => {
    if (open) {
      setRememberForSession(false);
    }
  }, [open, toolName]);

  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-[10100] flex items-center justify-center px-4" style={{ background: 'var(--color-overlay)' }}>
      <div
        className="w-full max-w-lg rounded-xl border border-amber/30 bg-card shadow-md p-6"
        role="dialog"
        aria-modal="true"
        aria-labelledby="approval-title"
      >
        <div className="flex items-center gap-2 mb-1">
          <span className="text-xl">⚠️</span>
          <h2 id="approval-title" className="text-lg font-semibold text-amber-text">
            {t('approval.title')}
          </h2>
        </div>
        <p className="mt-1 text-sm text-t-text-secondary">{t('approval.toolLabel', { toolName })}</p>
        <div className="mt-4 rounded-lg bg-canvas-alt border border-card-border p-3 text-sm text-t-text max-h-48 overflow-y-auto whitespace-pre-wrap leading-relaxed">
          {description || t('approval.noDescription')}
        </div>
        <label className="mt-4 flex items-start gap-2 text-sm text-t-text-secondary cursor-pointer select-none">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={rememberForSession}
            disabled={busy}
            onChange={(e) => setRememberForSession(e.target.checked)}
          />
          <span>{t('approval.rememberForSession')}</span>
        </label>
        <div className="mt-6 flex justify-end gap-3">
          <button
            type="button"
            disabled={busy}
            onClick={onDeny}
            className="px-4 py-2 rounded-lg border border-card-border text-t-text-secondary hover:bg-hover disabled:opacity-50 transition-colors"
          >
            {t('approval.reject')}
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={() => onApprove(rememberForSession)}
            className="px-4 py-2 rounded-lg bg-amber text-white hover:brightness-105 disabled:opacity-50 transition-all"
          >
            {t('approval.approve')}
          </button>
        </div>
      </div>
    </div>
  );
}
