import { useT } from '../i18n';

/** Visible on keyboard focus — skip past chrome to main content or composer (F3 a11y). */
export default function SkipToMainLink() {
  const { t } = useT();
  return (
    <>
      <a href="#main-content" className="skip-to-main">
        {t('a11y.skipToMain')}
      </a>
      <a href="#composer-input" className="skip-to-main skip-to-composer">
        {t('a11y.skipToComposer')}
      </a>
    </>
  );
}
