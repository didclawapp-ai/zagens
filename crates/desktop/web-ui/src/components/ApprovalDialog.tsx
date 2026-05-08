interface Props {
  open: boolean;
  toolName: string;
  description: string;
  busy?: boolean;
  onApprove: () => void;
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
            需要你的审批
          </h2>
        </div>
        <p className="mt-1 text-sm text-t-text-secondary">工具：{toolName}</p>
        <div className="mt-4 rounded-lg bg-canvas-alt border border-card-border p-3 text-sm text-t-text max-h-48 overflow-y-auto whitespace-pre-wrap leading-relaxed">
          {description || '（无描述）'}
        </div>
        <div className="mt-6 flex justify-end gap-3">
          <button
            type="button"
            disabled={busy}
            onClick={onDeny}
            className="px-4 py-2 rounded-lg border border-card-border text-t-text-secondary hover:bg-hover disabled:opacity-50 transition-colors"
          >
            拒绝
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={onApprove}
            className="px-4 py-2 rounded-lg bg-amber text-white hover:brightness-105 disabled:opacity-50 transition-all"
          >
            批准
          </button>
        </div>
      </div>
    </div>
  );
}
