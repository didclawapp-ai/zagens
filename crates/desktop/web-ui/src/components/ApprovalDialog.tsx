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

  const forcePush =
    /FORCE PUSH/i.test(description) ||
    /git\s+push\b[\s\S]*(\s-f\b|--force(?:-with-lease)?|\s\+[^\s:]+:)/i.test(description);

  return (
    <div className="fixed inset-0 z-[10100] flex items-center justify-center px-4" style={{ background: 'var(--color-overlay)' }}>
      <div
        className={`w-full max-w-lg rounded-xl border bg-card shadow-md p-6 ${
          forcePush ? 'border-red-500/50' : 'border-amber/30'
        }`}
        role="dialog"
        aria-modal="true"
        aria-labelledby="approval-title"
      >
        <div className="flex items-center gap-2 mb-1">
          <span className="text-xl">⚠️</span>
          <h2
            id="approval-title"
            className={`text-lg font-semibold ${forcePush ? 'text-red-400' : 'text-amber-text'}`}
          >
            {forcePush ? t('approval.forcePushTitle') : t('approval.title')}
          </h2>
        </div>
        <p className="mt-1 text-sm text-t-text-secondary">{t('approval.toolLabel', { toolName })}</p>
        {forcePush ? (
          <p className="mt-2 text-xs text-red-300/90 leading-relaxed">{t('approval.forcePushHint')}</p>
        ) : null}
        <div
          className={`mt-4 rounded-lg bg-canvas-alt p-3 text-sm text-t-text max-h-48 overflow-y-auto whitespace-pre-wrap leading-relaxed border ${
            forcePush ? 'border-red-500/40' : 'border-card-border'
          }`}
        >
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
