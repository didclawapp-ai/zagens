import { useCallback, useEffect, useMemo, useState } from 'react';
import { useT } from '../i18n';
import {
  applyLhtPreset,
  deleteThreadConfigField,
  fetchLhtComposerMode,
  fetchLhtSettings,
  fetchThreadConfig,
  previewLhtPreset,
  putThreadConfig,
  saveLhtComposerMode,
  saveLhtSettings,
  type LhtGateMode,
  type LhtPresetId,
  type LhtSettings,
} from '../api/client';
import { confirmDialog } from '../lib/confirmDialog';
import {
  lhtSettingsFromEffectiveOverlay,
  lhtSettingsToOverlay,
  overlayHasSessionOverrides,
  type ThreadConfigResponse,
} from '../lib/threadConfigOverlay';
import {
  COMPOSER_MODE_FOR_PRESET,
  effectiveLhtEnabled,
  effectiveLhtMode,
  matchPresetFromSettings,
  rememberLastPreset,
  summarizeGateModes,
} from '../lib/lhtPresetMatch';
import LhtSettingsAdvancedSections from './LhtSettingsAdvancedSections';
import { LHT_COMPOSER_MODE_CHANGED_EVENT, type LhtComposerMode } from './LhtModeToggle';

interface Props {
  desktopHost: boolean;
  streaming?: boolean;
  /** When set, LHT writes go to per-session overlay (zero sidecar restart). */
  threadId?: string | null;
}

const LHT_PRESETS: { id: LhtPresetId; labelKey: string; descKey: string }[] = [
  { id: 'code-default', labelKey: 'lhtSettings.presetCodeDefault', descKey: 'lhtSettings.presetCodeDefaultDesc' },
  { id: 'long-refactor', labelKey: 'lhtSettings.presetLongRefactor', descKey: 'lhtSettings.presetLongRefactorDesc' },
  { id: 'long-fix', labelKey: 'lhtSettings.presetLongFix', descKey: 'lhtSettings.presetLongFixDesc' },
  { id: 'craft-audit', labelKey: 'lhtSettings.presetCraftAudit', descKey: 'lhtSettings.presetCraftAuditDesc' },
];

const COMPOSER_MODES: { id: LhtComposerMode; labelKey: string }[] = [
  { id: 'auto', labelKey: 'lhtSettings.composerModeAuto' },
  { id: 'strict', labelKey: 'lhtSettings.composerModeStrict' },
  { id: 'off', labelKey: 'lhtSettings.composerModeOff' },
];

function presetLabelKey(presetId: LhtPresetId): string {
  const row = LHT_PRESETS.find((p) => p.id === presetId);
  return row?.labelKey ?? 'lhtSettings.presetCodeDefault';
}

type WriteScope = 'session' | 'global';

export default function LhtSettingsPanel({ desktopHost, streaming = false, threadId = null }: Props) {
  const { t } = useT();
  const [settings, setSettings] = useState<LhtSettings | null>(null);
  const [composerMode, setComposerMode] = useState<LhtComposerMode>('auto');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [activePreset, setActivePreset] = useState<LhtPresetId | 'custom'>('code-default');
  const [writeScope, setWriteScope] = useState<WriteScope>(threadId?.trim() ? 'session' : 'global');
  const [threadConfig, setThreadConfig] = useState<ThreadConfigResponse | null>(null);

  const hasThread = Boolean(threadId?.trim());
  const sessionScoped = hasThread && writeScope === 'session';
  const hasSessionOverlay = overlayHasSessionOverrides(threadConfig?.overlay);

  const loadPanelState = useCallback(async () => {
    const globalSettings = await fetchLhtSettings();
    let settings = globalSettings;
    let mode: LhtComposerMode = 'auto';
    let cfg: ThreadConfigResponse | null = null;
    if (sessionScoped && threadId?.trim()) {
      cfg = await fetchThreadConfig(threadId.trim());
      const mapped = lhtSettingsFromEffectiveOverlay(cfg.effective, globalSettings);
      settings = mapped.settings;
      mode = mapped.composerMode;
    } else {
      mode = await fetchLhtComposerMode();
    }
    setThreadConfig(cfg);
    setSettings(settings);
    setComposerMode(mode);
    setActivePreset(matchPresetFromSettings(settings));
  }, [sessionScoped, threadId]);

  useEffect(() => {
    if (!desktopHost) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        await loadPanelState();
      } catch {
        /* keep defaults */
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [desktopHost, loadPanelState]);

  useEffect(() => {
    if (threadId?.trim()) {
      setWriteScope((prev) => (prev === 'global' ? prev : 'session'));
    } else {
      setWriteScope('global');
    }
  }, [threadId]);

  useEffect(() => {
    if (!desktopHost) return;
    const onComposerModeChanged = (event: Event) => {
      const detail = (event as CustomEvent<LhtComposerMode>).detail;
      if (detail === 'strict' || detail === 'off' || detail === 'auto') {
        setComposerMode(detail);
      }
      void fetchLhtSettings()
        .then((s) => {
          setSettings(s);
          setActivePreset(matchPresetFromSettings(s));
        })
        .catch(() => {});
    };
    window.addEventListener(LHT_COMPOSER_MODE_CHANGED_EVENT, onComposerModeChanged);
    return () => window.removeEventListener(LHT_COMPOSER_MODE_CHANGED_EVENT, onComposerModeChanged);
  }, [desktopHost]);

  const handleScopeChange = useCallback(
    async (next: WriteScope) => {
      if (next === writeScope) return;
      setWriteScope(next);
      setLoading(true);
      try {
        const globalSettings = await fetchLhtSettings();
        if (next === 'session' && threadId?.trim()) {
          const cfg = await fetchThreadConfig(threadId.trim());
          const mapped = lhtSettingsFromEffectiveOverlay(cfg.effective, globalSettings);
          setThreadConfig(cfg);
          setSettings(mapped.settings);
          setComposerMode(mapped.composerMode);
          setActivePreset(matchPresetFromSettings(mapped.settings));
        } else {
          setThreadConfig(null);
          setSettings(globalSettings);
          setComposerMode(await fetchLhtComposerMode());
          setActivePreset(matchPresetFromSettings(globalSettings));
        }
      } finally {
        setLoading(false);
      }
    },
    [writeScope, threadId],
  );

  const handleClearSessionOverride = useCallback(async () => {
    const id = threadId?.trim();
    if (!id || !hasSessionOverlay) return;
    if (!(await confirmDialog(t('lhtSettings.clearSessionOverrideConfirm')))) return;
    setSaving(true);
    try {
      await deleteThreadConfigField(id, 'long_horizon');
      await deleteThreadConfigField(id, 'lht_composer_mode');
      const globalSettings = await fetchLhtSettings();
      const cfg = await fetchThreadConfig(id);
      const mapped = lhtSettingsFromEffectiveOverlay(cfg.effective, globalSettings);
      setThreadConfig(cfg);
      setSettings(mapped.settings);
      setComposerMode(mapped.composerMode);
      setActivePreset(matchPresetFromSettings(mapped.settings));
      window.dispatchEvent(
        new CustomEvent(LHT_COMPOSER_MODE_CHANGED_EVENT, { detail: mapped.composerMode }),
      );
    } finally {
      setSaving(false);
    }
  }, [threadId, hasSessionOverlay, t]);

  const update = useCallback(<K extends keyof LhtSettings>(key: K, value: LhtSettings[K]) => {
    setSettings((prev) => {
      if (!prev) return prev;
      const next = { ...prev, [key]: value };
      setActivePreset(matchPresetFromSettings(next));
      return next;
    });
  }, []);

  const syncComposerMode = useCallback(
    async (mode: LhtComposerMode) => {
      if (sessionScoped && threadId?.trim()) {
        const cfg = await putThreadConfig(threadId.trim(), { lht_composer_mode: mode });
        setThreadConfig(cfg);
      } else {
        await saveLhtComposerMode(mode);
      }
      setComposerMode(mode);
      window.dispatchEvent(new CustomEvent(LHT_COMPOSER_MODE_CHANGED_EVENT, { detail: mode }));
      if (!sessionScoped) {
        const refreshed = await fetchLhtSettings();
        setSettings(refreshed);
        setActivePreset(matchPresetFromSettings(refreshed));
      }
    },
    [sessionScoped, threadId],
  );

  const handleApplyPreset = useCallback(
    async (presetId: LhtPresetId) => {
      if (!desktopHost) return;
      if (!sessionScoped && streaming && !(await confirmDialog(t('settings.saveRestartsSidecar')))) {
        return;
      }
      setSaving(true);
      try {
        const pairedComposer = COMPOSER_MODE_FOR_PRESET[presetId];
        if (sessionScoped && threadId?.trim()) {
          const preview = await previewLhtPreset(presetId);
          const cfg = await putThreadConfig(
            threadId.trim(),
            lhtSettingsToOverlay(preview, pairedComposer),
          );
          setThreadConfig(cfg);
          setSettings(preview);
          setComposerMode(pairedComposer);
          window.dispatchEvent(
            new CustomEvent(LHT_COMPOSER_MODE_CHANGED_EVENT, { detail: pairedComposer }),
          );
        } else {
          const next = await applyLhtPreset(presetId);
          setSettings(next);
          if (pairedComposer !== composerMode) {
            await syncComposerMode(pairedComposer);
          }
        }
        rememberLastPreset(presetId);
        setActivePreset(presetId);
      } finally {
        setSaving(false);
      }
    },
    [desktopHost, sessionScoped, streaming, t, threadId, composerMode, syncComposerMode],
  );

  const handleComposerModeChange = useCallback(
    async (mode: LhtComposerMode) => {
      if (!desktopHost || mode === composerMode) return;
      if (!sessionScoped && streaming && !(await confirmDialog(t('settings.saveRestartsSidecar')))) {
        return;
      }
      setSaving(true);
      try {
        await syncComposerMode(mode);
      } finally {
        setSaving(false);
      }
    },
    [desktopHost, composerMode, sessionScoped, streaming, t, syncComposerMode],
  );

  const handleSave = useCallback(async () => {
    if (!settings || !desktopHost) return;
    const enforcing =
      settings.auto_verify_replay === 'enforce' ||
      settings.toolchain_gate === 'enforce' ||
      settings.stub_gate === 'enforce';
    if (enforcing && !(await confirmDialog(t('lhtSettings.enforceConfirm')))) {
      return;
    }
    if (!sessionScoped && streaming && !(await confirmDialog(t('settings.saveRestartsSidecar')))) {
      return;
    }
    setSaving(true);
    try {
      if (sessionScoped && threadId?.trim()) {
        const cfg = await putThreadConfig(threadId.trim(), lhtSettingsToOverlay(settings, composerMode));
        setThreadConfig(cfg);
      } else {
        await saveLhtSettings(settings);
      }
      setActivePreset(matchPresetFromSettings(settings));
    } finally {
      setSaving(false);
    }
  }, [settings, desktopHost, sessionScoped, streaming, threadId, composerMode, t]);

  const selectCls =
    'w-full rounded-lg border border-divider bg-canvas px-3 py-2 text-xs text-t-text focus:outline-none focus:ring-1 focus:ring-accent';
  const labelCls = 'text-[11px] font-medium text-t-text-secondary';
  const descCls = 'text-[10px] text-t-text-muted';
  const sectionCls = 'text-[11px] font-semibold uppercase tracking-wider text-t-text-muted';

  const gateLabel = (mode: LhtGateMode) => t(`lhtSettings.gate_${mode}` as 'lhtSettings.gate_off');

  const summaryLine = useMemo(() => {
    if (!settings) return '';
    const lhtOn = effectiveLhtEnabled(settings, composerMode);
    const lhtMode = effectiveLhtMode(settings, composerMode);
    const gate = summarizeGateModes(settings);
    const gateKey =
      gate === 'mixed' ? 'lhtSettings.summaryGateMixed' : (`lhtSettings.summaryGate_${gate}` as const);
    return t('lhtSettings.currentSummary', {
      mode:
        activePreset === 'custom'
          ? t('lhtSettings.customPreset')
          : t(presetLabelKey(activePreset)),
      composer: t(`lhtSettings.composerModeShort_${composerMode}` as 'lhtSettings.composerModeShort_auto'),
      lht: lhtOn ? t(`lhtSettings.summaryLht_${lhtMode}` as 'lhtSettings.summaryLht_auto') : t('lhtSettings.summaryLht_off'),
      macro: settings.macro_loop_enabled ? t('lhtSettings.summaryMacro_on') : t('lhtSettings.summaryMacro_off'),
      gate: t(gateKey),
    });
  }, [settings, composerMode, activePreset, t]);

  return (
    <div className="p-4 space-y-5 overflow-y-auto h-full">
      {hasThread && (
        <section className="space-y-2 border-b border-divider pb-3">
          <p className={sectionCls}>{t('lhtSettings.scopeSection')}</p>
          <div className="flex flex-wrap gap-1.5">
            {(['session', 'global'] as const).map((scope) => (
              <button
                key={scope}
                type="button"
                disabled={saving}
                onClick={() => void handleScopeChange(scope)}
                className={`rounded-full px-3 py-1 text-[11px] font-medium transition-colors disabled:opacity-50 ${
                  writeScope === scope
                    ? 'bg-accent text-white'
                    : 'border border-divider bg-canvas text-t-text-secondary hover:border-accent/40'
                }`}
              >
                {t(
                  scope === 'session'
                    ? 'lhtSettings.scopeSession'
                    : 'lhtSettings.scopeGlobal',
                )}
              </button>
            ))}
          </div>
          <p className={descCls}>
            {sessionScoped ? t('lhtSettings.scopeSessionHint') : t('lhtSettings.scopeGlobalHint')}
          </p>
          {hasSessionOverlay && (
            <div className="space-y-1.5">
              <p className="text-[10px] text-accent leading-relaxed">
                {sessionScoped
                  ? t('lhtSettings.sessionOverrideBadge')
                  : t('lhtSettings.sessionOverrideActiveWhileGlobal')}
              </p>
              <button
                type="button"
                disabled={saving}
                onClick={() => void handleClearSessionOverride()}
                className="rounded-full border border-divider bg-canvas px-3 py-1 text-[11px] font-medium text-t-text-secondary transition-colors hover:border-accent/40 disabled:opacity-50"
              >
                {t('lhtSettings.clearSessionOverride')}
              </button>
            </div>
          )}
        </section>
      )}

      <p className="text-xs text-t-text-muted leading-relaxed border-b border-divider pb-3">
        {sessionScoped ? t('lhtSettings.introSession') : t('lhtSettings.introShort')}
      </p>

      {!desktopHost && (
        <p className="text-xs text-t-text-muted leading-relaxed">{t('settings.notAvailable')}</p>
      )}

      {loading && <p className="text-xs text-t-text-muted">{t('settings.loadingSettings')}</p>}

      {settings && (
        <>
          <section className="space-y-3">
            <p className={sectionCls}>{t('lhtSettings.sectionWorkMode')}</p>
            <p className={descCls}>{t('lhtSettings.workModeIntro')}</p>
            <div className="grid gap-2 sm:grid-cols-2">
              {LHT_PRESETS.map(({ id, labelKey, descKey }) => {
                const selected = activePreset === id;
                return (
                  <button
                    key={id}
                    type="button"
                    disabled={saving}
                    onClick={() => void handleApplyPreset(id)}
                    className={`rounded-lg border px-3 py-2.5 text-left transition-colors disabled:opacity-50 ${
                      selected
                        ? 'border-accent bg-accent/10 ring-1 ring-accent/40'
                        : 'border-divider bg-canvas hover:border-accent/50 hover:bg-canvas-alt'
                    }`}
                  >
                    <div className="flex items-start justify-between gap-2">
                      <span className={`${labelCls} block`}>{t(labelKey)}</span>
                      {selected && (
                        <span className="shrink-0 text-[9px] font-semibold uppercase tracking-wide text-accent">
                          {t('lhtSettings.presetActive')}
                        </span>
                      )}
                    </div>
                    <span className={`${descCls} block mt-1`}>{t(descKey)}</span>
                  </button>
                );
              })}
            </div>
            <p className="text-[10px] text-t-text-muted leading-relaxed rounded-lg border border-divider bg-canvas-alt/60 px-3 py-2">
              {summaryLine}
            </p>
          </section>

          <section className="space-y-2">
            <p className={sectionCls}>{t('lhtSettings.sectionComposerOverride')}</p>
            <p className={descCls}>{t('lhtSettings.composerOverrideIntro')}</p>
            <div className="flex flex-wrap gap-1.5">
              {COMPOSER_MODES.map(({ id, labelKey }) => (
                <button
                  key={id}
                  type="button"
                  disabled={saving}
                  onClick={() => void handleComposerModeChange(id)}
                  className={`rounded-full px-3 py-1 text-[11px] font-medium transition-colors disabled:opacity-50 ${
                    composerMode === id
                      ? 'bg-accent text-white'
                      : 'border border-divider bg-canvas text-t-text-secondary hover:border-accent/40'
                  }`}
                >
                  {t(labelKey)}
                </button>
              ))}
            </div>
            {composerMode === 'off' && (
              <p className="text-[10px] text-amber-600 dark:text-amber-400 leading-relaxed">
                {t('lhtSettings.composerOverrideOffShort')}
              </p>
            )}
            {composerMode === 'strict' && (
              <p className="text-[10px] text-accent leading-relaxed">
                {t('lhtSettings.composerOverrideStrictShort')}
              </p>
            )}
          </section>

          <section className="border-t border-divider pt-3">
            <button
              type="button"
              onClick={() => setAdvancedOpen((open) => !open)}
              className="flex w-full items-center justify-between gap-2 rounded-lg px-1 py-1 text-left hover:bg-canvas-alt/80 transition-colors"
              aria-expanded={advancedOpen}
            >
              <span className={`${labelCls} text-t-text`}>{t('lhtSettings.advancedSettings')}</span>
              <span className="text-t-text-muted text-xs" aria-hidden>
                {advancedOpen ? '▾' : '▸'}
              </span>
            </button>
            {!advancedOpen && (
              <p className={`${descCls} mt-1 px-1`}>{t('lhtSettings.advancedSettingsHint')}</p>
            )}
            {advancedOpen && (
              <div className="mt-4 space-y-5">
                <LhtSettingsAdvancedSections
                  settings={settings}
                  composerMode={composerMode}
                  update={update}
                  selectCls={selectCls}
                  labelCls={labelCls}
                  descCls={descCls}
                  sectionCls={sectionCls}
                  gateLabel={gateLabel}
                  t={t}
                />
                <div className="pt-3 border-t border-divider">
                  <button
                    type="button"
                    onClick={() => void handleSave()}
                    disabled={saving}
                    className="w-full py-2 rounded-lg bg-accent text-white text-sm font-medium hover:opacity-90 disabled:opacity-50 transition-colors"
                  >
                    {saving ? t('settings.saving') : t('lhtSettings.saveAdvanced')}
                  </button>
                  <p className="text-[10px] text-t-text-muted mt-1.5 text-center">
                    {sessionScoped
                      ? t('lhtSettings.saveAdvancedHintSession')
                      : t('lhtSettings.saveAdvancedHint')}
                  </p>
                </div>
              </div>
            )}
          </section>
        </>
      )}
    </div>
  );
}
