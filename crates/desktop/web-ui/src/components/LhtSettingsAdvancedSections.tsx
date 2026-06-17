import type { LhtComposerMode } from '../api/client';
import type { LhtGateMode, LhtSettings } from '../api/client';

const GATE_OPTIONS: LhtGateMode[] = ['off', 'observe', 'enforce'];

interface Props {
  settings: LhtSettings;
  composerMode: LhtComposerMode;
  update: <K extends keyof LhtSettings>(key: K, value: LhtSettings[K]) => void;
  selectCls: string;
  labelCls: string;
  descCls: string;
  sectionCls: string;
  gateLabel: (mode: LhtGateMode) => string;
  t: (key: string, params?: Record<string, string>) => string;
}

export default function LhtSettingsAdvancedSections({
  settings,
  composerMode,
  update,
  selectCls,
  labelCls,
  descCls,
  sectionCls,
  gateLabel,
  t,
}: Props) {
  const harnessFieldsDisabled = composerMode === 'off' || composerMode === 'strict';
  const macroFieldsDisabled =
    composerMode === 'off' ||
    !settings.macro_loop_enabled ||
    (composerMode !== 'strict' && settings.mode !== 'strict');

  return (
    <>
      <section className="space-y-3">
        <p className={sectionCls}>{t('lhtSettings.sectionHarness')}</p>

        <label className="flex items-center justify-between gap-2 py-1">
          <div className="flex-1 min-w-0">
            <span className={labelCls}>{t('lhtSettings.enabled')}</span>
            <p className={descCls}>{t('lhtSettings.enabledDesc')}</p>
          </div>
          <input
            type="checkbox"
            className="shrink-0 w-4 h-4 accent-accent rounded"
            checked={settings.enabled}
            disabled={composerMode === 'off'}
            onChange={(e) => update('enabled', e.target.checked)}
          />
        </label>

        <label className="block space-y-1">
          <span className={labelCls}>{t('lhtSettings.mode')}</span>
          <p className={descCls}>{t('lhtSettings.modeDesc')}</p>
          <select
            className={selectCls}
            value={settings.mode}
            disabled={harnessFieldsDisabled}
            onChange={(e) => update('mode', e.target.value as LhtSettings['mode'])}
          >
            <option value="auto">{t('lhtSettings.modeAuto')}</option>
            <option value="strict">{t('lhtSettings.modeStrict')}</option>
          </select>
        </label>

        <label className="flex items-center justify-between gap-2 py-1">
          <div className="flex-1 min-w-0">
            <span className={labelCls}>{t('lhtSettings.progressViaGit')}</span>
            <p className={descCls}>{t('lhtSettings.progressViaGitDesc')}</p>
          </div>
          <input
            type="checkbox"
            className="shrink-0 w-4 h-4 accent-accent rounded"
            checked={settings.progress_via_git}
            onChange={(e) => update('progress_via_git', e.target.checked)}
          />
        </label>

        <label className="block space-y-1">
          <span className={labelCls}>{t('lhtSettings.maxNudges')}</span>
          <input
            type="number"
            min={1}
            max={20}
            className={selectCls}
            value={settings.max_nudges_per_item}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (v >= 1 && v <= 20) update('max_nudges_per_item', v);
            }}
          />
        </label>

        <label className="block space-y-1">
          <span className={labelCls}>{t('lhtSettings.blockedNudges')}</span>
          <input
            type="number"
            min={1}
            max={10}
            className={selectCls}
            value={settings.blocked_nudges_without_progress}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (v >= 1 && v <= 10) update('blocked_nudges_without_progress', v);
            }}
          />
        </label>

        <label className="flex items-center justify-between gap-2 py-1">
          <div className="flex-1 min-w-0">
            <span className={labelCls}>{t('lhtSettings.autoContinue')}</span>
            <p className={descCls}>{t('lhtSettings.autoContinueDesc')}</p>
          </div>
          <input
            type="checkbox"
            className="shrink-0 w-4 h-4 accent-accent rounded"
            checked={settings.auto_continue}
            onChange={(e) => update('auto_continue', e.target.checked)}
          />
        </label>

        {settings.auto_continue && (
          <label className="block space-y-1">
            <span className={labelCls}>{t('lhtSettings.maxAutoContinue')}</span>
            <input
              type="number"
              min={1}
              max={64}
              className={selectCls}
              value={settings.max_auto_continue_rounds}
              onChange={(e) => {
                const v = Number(e.target.value);
                if (v >= 1 && v <= 64) update('max_auto_continue_rounds', v);
              }}
            />
          </label>
        )}
      </section>

      <section className="space-y-3">
        <p className={sectionCls}>{t('lhtSettings.sectionCompletionGate')}</p>
        <p className={descCls}>{t('lhtSettings.completionGateIntro')}</p>

        {(
          [
            ['auto_verify_replay', 'autoVerifyReplay', 'autoVerifyReplayDesc'],
            ['toolchain_gate', 'toolchainGate', 'toolchainGateDesc'],
            ['stub_gate', 'stubGate', 'stubGateDesc'],
          ] as const
        ).map(([key, titleKey, descKey]) => (
          <label key={key} className="block space-y-1">
            <span className={labelCls}>{t(`lhtSettings.${titleKey}`)}</span>
            <p className={descCls}>{t(`lhtSettings.${descKey}`)}</p>
            <select
              className={selectCls}
              value={settings[key]}
              onChange={(e) => update(key, e.target.value as LhtGateMode)}
            >
              {GATE_OPTIONS.map((opt) => (
                <option key={opt} value={opt}>
                  {gateLabel(opt)}
                </option>
              ))}
            </select>
          </label>
        ))}

        <label className="block space-y-1">
          <span className={labelCls}>{t('lhtSettings.maxManifestRounds')}</span>
          <input
            type="number"
            min={1}
            max={32}
            className={selectCls}
            value={settings.max_manifest_rounds}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (v >= 1 && v <= 32) update('max_manifest_rounds', v);
            }}
          />
        </label>

        <label className="block space-y-1">
          <span className={labelCls}>{t('lhtSettings.maxAuditRounds')}</span>
          <input
            type="number"
            min={1}
            max={32}
            className={selectCls}
            value={settings.max_audit_rounds}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (v >= 1 && v <= 32) update('max_audit_rounds', v);
            }}
          />
        </label>

        <label className="block space-y-1">
          <span className={labelCls}>{t('lhtSettings.maxInfraStrikes')}</span>
          <input
            type="number"
            min={1}
            max={16}
            className={selectCls}
            value={settings.max_infra_strikes}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (v >= 1 && v <= 16) update('max_infra_strikes', v);
            }}
          />
        </label>

        {(settings.custom_verify_count > 0 || settings.custom_deliverable_count > 0) && (
          <p className="text-[10px] text-amber-600 dark:text-amber-400 leading-relaxed">
            {t('lhtSettings.customManifestHint', {
              verify: String(settings.custom_verify_count),
              deliverable: String(settings.custom_deliverable_count),
            })}
          </p>
        )}
      </section>

      <section className="space-y-3">
        <p className={sectionCls}>{t('lhtSettings.sectionMacroLoop')}</p>
        <p className={descCls}>{t('lhtSettings.macroLoopIntro')}</p>
        {composerMode !== 'strict' && settings.mode !== 'strict' && (
          <p className="text-[10px] text-amber-600 dark:text-amber-400 leading-relaxed">
            {t('lhtSettings.macroLoopStrictHint')}
          </p>
        )}

        <label className="flex items-center justify-between gap-2 py-1">
          <div className="flex-1 min-w-0">
            <span className={labelCls}>{t('lhtSettings.macroLoopEnabled')}</span>
            <p className={descCls}>{t('lhtSettings.macroLoopEnabledDesc')}</p>
          </div>
          <input
            type="checkbox"
            className="shrink-0 w-4 h-4 accent-accent rounded"
            checked={settings.macro_loop_enabled}
            disabled={composerMode === 'off' || (composerMode !== 'strict' && settings.mode !== 'strict')}
            onChange={(e) => update('macro_loop_enabled', e.target.checked)}
          />
        </label>

        <label className="block space-y-1">
          <span className={labelCls}>{t('lhtSettings.macroLoopAutoEnter')}</span>
          <p className={descCls}>{t('lhtSettings.macroLoopAutoEnterDesc')}</p>
          <select
            className={selectCls}
            value={settings.macro_loop_auto_enter_craft}
            disabled={macroFieldsDisabled}
            onChange={(e) =>
              update(
                'macro_loop_auto_enter_craft',
                e.target.value as LhtSettings['macro_loop_auto_enter_craft'],
              )
            }
          >
            <option value="user_confirm">{t('lhtSettings.macroLoopAutoUserConfirm')}</option>
            <option value="on_graph_complete">{t('lhtSettings.macroLoopAutoOnGraphComplete')}</option>
            <option value="on_manifest_exhausted">
              {t('lhtSettings.macroLoopAutoOnManifestExhausted')}
            </option>
            <option value="on_micro_pass">{t('lhtSettings.macroLoopAutoOnMicroPass')}</option>
            <option value="off">{t('lhtSettings.macroLoopAutoOff')}</option>
          </select>
        </label>

        {settings.macro_loop_enabled && (
          <p className="text-[10px] text-amber-600 dark:text-amber-400 leading-relaxed">
            {t('lhtSettings.macroLoopCostWarning')}
          </p>
        )}

        <label className="block space-y-1">
          <span className={labelCls}>{t('lhtSettings.macroLoopMaxCycles')}</span>
          <input
            type="number"
            min={1}
            max={8}
            className={selectCls}
            value={settings.macro_loop_max_cycles}
            disabled={macroFieldsDisabled}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (v >= 1 && v <= 8) update('macro_loop_max_cycles', v);
            }}
          />
        </label>

        <label className="block space-y-1">
          <span className={labelCls}>{t('lhtSettings.macroLoopMaxCraftRounds')}</span>
          <input
            type="number"
            min={1}
            max={4}
            className={selectCls}
            value={settings.macro_loop_max_craft_rounds}
            disabled={macroFieldsDisabled}
            onChange={(e) => {
              const v = Number(e.target.value);
              if (v >= 1 && v <= 4) update('macro_loop_max_craft_rounds', v);
            }}
          />
        </label>

        <label className="flex items-center justify-between gap-2 py-1">
          <div className="flex-1 min-w-0">
            <span className={labelCls}>{t('lhtSettings.macroLoopSmallTasks')}</span>
            <p className={descCls}>{t('lhtSettings.macroLoopSmallTasksDesc')}</p>
          </div>
          <input
            type="checkbox"
            className="shrink-0 w-4 h-4 accent-accent rounded"
            checked={settings.macro_loop_craft_on_small_tasks}
            disabled={macroFieldsDisabled}
            onChange={(e) => update('macro_loop_craft_on_small_tasks', e.target.checked)}
          />
        </label>

        {!settings.macro_loop_craft_on_small_tasks && (
          <label className="block space-y-1">
            <span className={labelCls}>{t('lhtSettings.macroLoopMinChecklist')}</span>
            <input
              type="number"
              min={1}
              max={32}
              className={selectCls}
              value={settings.macro_loop_min_checklist_items}
              disabled={macroFieldsDisabled}
              onChange={(e) => {
                const v = Number(e.target.value);
                if (v >= 1 && v <= 32) update('macro_loop_min_checklist_items', v);
              }}
            />
          </label>
        )}
      </section>
    </>
  );
}
