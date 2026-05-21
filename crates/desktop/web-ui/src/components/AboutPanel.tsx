import { useT } from '../i18n';

const APP_VERSION = '0.4.1';

export default function AboutPanel() {
  const { t } = useT();

  return (
    <div className="flex h-full flex-col overflow-y-auto p-4">
      <div className="flex items-center gap-3 pb-4">
        <img
          src="/app-icon.png"
          alt=""
          className="size-12 shrink-0 rounded-xl object-cover shadow-sm"
          width={48}
          height={48}
        />
        <div>
          <h3 className="text-base font-semibold text-t-text">DS Pick</h3>
          <p className="text-xs text-t-text-muted">v{APP_VERSION}</p>
        </div>
      </div>
      <p className="text-sm leading-relaxed text-t-text-secondary">{t('about.description')}</p>
      <p className="mt-4 text-xs leading-relaxed text-t-text-muted">{t('about.runtimeLine')}</p>
    </div>
  );
}
