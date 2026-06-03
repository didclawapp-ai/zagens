import { useT } from '../i18n';

const CARD_IDS = [
  'weeklyReport',
  'meetingMinutes',
  'projectDeck',
  'dataReport',
  'competitiveAnalysis',
  'contractDraft',
  'resume',
  'releaseNotes',
] as const;

export type OfficeQuickStartId = (typeof CARD_IDS)[number];

type Props = {
  onPick: (prefill: string) => void;
};

export function OfficeEmptyState({ onPick }: Props) {
  const { t } = useT();

  return (
    <div className="flex min-h-[min(60vh,28rem)] flex-col items-center justify-center px-2 py-6">
      <h1 className="font-display text-3xl font-bold text-accent">{t('officeEmpty.title')}</h1>
      <p className="mt-2 text-lg text-t-text-secondary">{t('officeEmpty.subtitle')}</p>
      <p className="mt-1 max-w-lg text-sm text-t-text-muted">{t('officeEmpty.hint')}</p>
      <div className="mt-6 grid w-full max-w-3xl gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {CARD_IDS.map((id) => (
          <button
            key={id}
            type="button"
            className="rounded-xl border border-divider bg-card/80 px-4 py-3 text-left transition hover:border-accent/40 hover:bg-accent/5"
            onClick={() => onPick(t(`officeEmpty.prefill.${id}`))}
          >
            <span className="block text-sm font-semibold text-t-text">
              {t(`officeEmpty.cards.${id}.title`)}
            </span>
            <span className="mt-1 block text-xs text-t-text-muted">
              {t(`officeEmpty.cards.${id}.hint`)}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
