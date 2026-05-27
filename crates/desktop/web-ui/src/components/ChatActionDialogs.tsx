import { useT } from '../i18n';

type EditDraft = { messageId: string; content: string };

type BacktrackDraft = {
  messageId: string;
  content: string;
  depthFromTail: number;
};

type Props = {
  editDraft: EditDraft | null;
  onEditDraftChange: (draft: EditDraft | null) => void;
  onConfirmEdit: () => void;
  backtrackDraft: BacktrackDraft | null;
  backtrackBusy: boolean;
  onBacktrackDraftChange: (draft: BacktrackDraft | null) => void;
  onConfirmBacktrack: () => void;
};

export default function ChatActionDialogs({
  editDraft,
  onEditDraftChange,
  onConfirmEdit,
  backtrackDraft,
  backtrackBusy,
  onBacktrackDraftChange,
  onConfirmBacktrack,
}: Props) {
  const { t } = useT();

  return (
    <>
      {editDraft ? (
        <div
          className="fixed inset-0 z-[10050] flex items-center justify-center bg-overlay"
          onClick={(e) => {
            if (e.target === e.currentTarget) onEditDraftChange(null);
          }}
        >
          <div
            className="w-full max-w-lg rounded-2xl border border-card-border bg-card p-5 shadow-lg"
            role="dialog"
            aria-modal="true"
            aria-labelledby="edit-message-title"
          >
            <h3 id="edit-message-title" className="mb-3 text-base font-semibold text-t-text">
              {t('chat.editTitle')}
            </h3>
            <textarea
              className="min-h-[120px] w-full resize-y rounded-lg border border-input-border bg-input-bg px-3 py-2 text-sm text-t-text outline-none focus:border-accent"
              value={editDraft.content}
              onChange={(e) =>
                onEditDraftChange(
                  editDraft ? { ...editDraft, content: e.target.value } : editDraft,
                )
              }
              autoFocus
            />
            <div className="mt-4 flex justify-end gap-2">
              <button
                type="button"
                className="rounded-lg px-4 py-2 text-sm text-t-text-secondary hover:bg-hover"
                onClick={() => onEditDraftChange(null)}
              >
                {t('modelParams.cancel')}
              </button>
              <button
                type="button"
                className="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-accent-text hover:opacity-90"
                onClick={onConfirmEdit}
              >
                {t('chat.editSubmit')}
              </button>
            </div>
          </div>
        </div>
      ) : null}
      {backtrackDraft ? (
        <div
          className="fixed inset-0 z-[10050] flex items-center justify-center bg-overlay"
          onClick={(e) => {
            if (e.target === e.currentTarget && !backtrackBusy) onBacktrackDraftChange(null);
          }}
        >
          <div
            className="w-full max-w-lg rounded-2xl border border-card-border bg-card p-5 shadow-lg"
            role="dialog"
            aria-modal="true"
            aria-labelledby="backtrack-message-title"
          >
            <h3 id="backtrack-message-title" className="mb-2 text-base font-semibold text-t-text">
              {t('chat.backtrackTitle')}
            </h3>
            <p className="mb-3 text-sm text-t-text-secondary">{t('chat.backtrackBody')}</p>
            <div className="mb-4 rounded-lg border border-card-border bg-canvas-alt px-3 py-2 text-sm text-t-text-secondary line-clamp-4 whitespace-pre-wrap">
              {backtrackDraft.content}
            </div>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                className="rounded-lg px-4 py-2 text-sm text-t-text-secondary hover:bg-hover disabled:opacity-50"
                disabled={backtrackBusy}
                onClick={() => onBacktrackDraftChange(null)}
              >
                {t('modelParams.cancel')}
              </button>
              <button
                type="button"
                className="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-accent-text hover:opacity-90 disabled:opacity-50"
                disabled={backtrackBusy}
                onClick={() => void onConfirmBacktrack()}
              >
                {backtrackBusy ? t('chat.backtrackWorking') : t('chat.backtrackConfirm')}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </>
  );
}
