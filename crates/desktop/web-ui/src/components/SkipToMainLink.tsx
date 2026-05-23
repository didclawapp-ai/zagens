import { useT } from '../i18n';

/** Visible on keyboard focus — jumps past chrome to `#main-content` (F3 a11y). */
export default function SkipToMainLink() {
  const { t } = useT();
  return (
    <a href="#main-content" className="skip-to-main">
      {t('a11y.skipToMain')}
    </a>
  );
}
