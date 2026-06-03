import { useT } from '../i18n';
import {
  formatStorageBytes,
  type StoragePressureSnapshot,
} from '../lib/storagePressure';

type Props = {
  snapshot: StoragePressureSnapshot | null;
  level: 'ok' | 'warn' | 'critical';
};

export default function StoragePressureBanner({ snapshot, level }: Props) {
  const { t } = useT();
  if (!snapshot || level === 'ok') return null;

  const volumes = [snapshot.user_data, snapshot.workspace].filter(
    (v): v is NonNullable<typeof v> => v != null,
  );
  const isCritical = level === 'critical';

  return (
    <div
      role="alert"
      className={
        isCritical
          ? 'border-b border-red-500/40 bg-red-950/90 px-4 py-2.5 text-sm text-red-50'
          : 'border-b border-amber-500/30 bg-amber-950/70 px-4 py-2.5 text-sm text-amber-50'
      }
    >
      <p className="font-medium">
        {isCritical ? t('storage.criticalTitle') : t('storage.warnTitle')}
      </p>
      <p className="mt-0.5 text-[13px] opacity-90">
        {isCritical ? t('storage.criticalBody') : t('storage.warnBody')}
      </p>
      <ul className="mt-1.5 list-inside list-disc text-[12px] opacity-85">
        {volumes.map((v) => (
          <li key={v.path}>
            {t('storage.volumeFree', {
              path: v.path,
              free: formatStorageBytes(v.free_bytes),
            })}
          </li>
        ))}
      </ul>
    </div>
  );
}
