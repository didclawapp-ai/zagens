import { useT } from '../i18n';

const APP_VERSION = '0.5.0';

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
          <h3 className="text-base font-semibold text-t-text">{t('app.title')}</h3>
          <p className="text-xs text-t-text-muted">
            {t('app.subtitle')} · v{APP_VERSION}
          </p>
        </div>
      </div>
      <p className="text-sm leading-relaxed text-t-text-secondary">{t('about.description')}</p>
      <div className="mt-6">
        <h4 className="text-xs font-medium text-t-text">{t('about.techStackTitle')}</h4>
        <ul className="mt-2 space-y-1 text-xs leading-relaxed text-t-text-muted">
          <li>{t('about.techStackDeepseekTui')}</li>
          <li>{t('about.techStackTauri')}</li>
          <li>{t('about.techStackReact')}</li>
        </ul>
      </div>
    </div>
  );
}
