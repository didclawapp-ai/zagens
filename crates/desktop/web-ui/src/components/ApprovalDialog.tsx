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
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 px-4">
      <div
        className="w-full max-w-lg rounded-xl border border-amber-700/50 bg-gray-900 shadow-2xl p-6"
        role="dialog"
        aria-modal="true"
        aria-labelledby="approval-title"
      >
        <h2 id="approval-title" className="text-lg font-semibold text-amber-200">
          需要你的审批
        </h2>
        <p className="mt-1 text-sm text-gray-400">工具：{toolName}</p>
        <div className="mt-4 rounded-lg bg-gray-950 border border-gray-700 p-3 text-sm text-gray-200 max-h-48 overflow-y-auto whitespace-pre-wrap">
          {description || '（无描述）'}
        </div>
        <div className="mt-6 flex justify-end gap-3">
          <button
            type="button"
            disabled={busy}
            onClick={onDeny}
            className="px-4 py-2 rounded-lg border border-gray-600 text-gray-200 hover:bg-gray-800 disabled:opacity-50"
          >
            拒绝
          </button>
          <button
            type="button"
            disabled={busy}
            onClick={onApprove}
            className="px-4 py-2 rounded-lg bg-amber-600 text-white hover:bg-amber-500 disabled:opacity-50"
          >
            批准
          </button>
        </div>
      </div>
    </div>
  );
}
