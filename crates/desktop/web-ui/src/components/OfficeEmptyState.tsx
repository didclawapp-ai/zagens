import { useT } from '../i18n';

const CARD_IDS = [
  'executiveDailyBrief',
  'customerQuote',
  'productionDailyReport',
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
      <p className="font-display text-[1.75rem] font-semibold tracking-tight text-t-text">
        {t('officeEmpty.title')}
      </p>
      <p className="mt-3 text-[15px] leading-relaxed text-t-text-secondary">{t('officeEmpty.subtitle')}</p>
      <p className="mt-1 max-w-lg text-sm text-t-text-muted">{t('officeEmpty.hint')}</p>
      <div className="mt-6 grid w-full max-w-3xl gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {CARD_IDS.map((id) => (
          <button
            key={id}
            type="button"
            className="chat-empty-card"
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
