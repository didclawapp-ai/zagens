import { useEffect, useId, useState } from 'react';
import { useT } from '../../i18n';
import type { TranslationKey } from '../../i18n/keys';
import type {
  DesktopRunModeId,
  DesktopTaskTypePreference,
  DesktopTaskTypeResolved,
} from '../../types/desktop';
import { DESKTOP_RUN_MODE_LABELS } from '../../types/desktop';
import { approvalPolicySettingsKey } from '../../lib/approvalPolicy';
import type { LhtChipState } from '../../lib/lhtChip';
import LhtModeToggle from '../LhtModeToggle';
import OverflowMenu from '../chrome/OverflowMenu';
import { IconBolt } from '../icons/FlatIcons';
import ComposerOverflowSection from './ComposerOverflowSection';

const TASK_TYPE_LABEL_KEYS: Record<
  DesktopTaskTypePreference | DesktopTaskTypeResolved,
  TranslationKey
> = {
  auto: 'composer.taskTypeAuto',
  office: 'composer.taskTypeOffice',
  code: 'composer.taskTypeCode',
};

const TASK_TYPE_HINT_KEYS: Record<DesktopTaskTypePreference, TranslationKey> = {
  auto: 'composer.taskTypeAutoHint',
  office: 'composer.taskTypeOfficeHint',
  code: 'composer.taskTypeCodeHint',
};

const RUN_MODE_HINT_KEYS: Record<DesktopRunModeId, TranslationKey> = {
  plan: 'composer.planModeHint',
  agent: 'composer.agentModeHint',
  yolo: 'composer.runModeYoloHint',
};

type SectionId = 'autoApprove' | 'runMode' | 'taskType' | 'lht';

type Props = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  disabled: boolean;
  officeSession: boolean;
  showAutoApprove: boolean;
  autoApprove: boolean;
  autoApproveToggleEnabled: boolean;
  approvalPolicy: string;
  onAutoApproveChange: (value: boolean) => void;
  runMode: DesktopRunModeId;
  availableRunModes: DesktopRunModeId[];
  runModePickerDisabled: boolean;
  onRunModeChange: (mode: DesktopRunModeId) => void;
  taskTypePreference: DesktopTaskTypePreference;
  lockedThreadTaskType: DesktopTaskTypeResolved | null;
  onTaskTypePreferenceChange: (value: DesktopTaskTypePreference) => void;
  lhtChip: LhtChipState | null;
  sessionExportEnabled: boolean;
  threadExportEnabled: boolean;
  onExportSessionJson: () => void;
  onExportThreadJson: () => void;
  onExportTraceReport: () => void;
  onExportTraceCompare: () => void;
  onOpenRouting?: () => void;
};

function optionBtnClass(selected: boolean) {
  return `flex w-full flex-col gap-0.5 rounded-md px-3 py-2 text-left text-sm transition-colors ${
    selected ? 'bg-accent-soft text-accent' : 'text-t-text hover:bg-hover'
  }`;
}

function readLhtModeSummary(t: ReturnType<typeof useT>['t']): string {
  try {
    const mode = localStorage.getItem('zagens-lht-composer-mode');
    if (mode === 'strict') {
      return t('composer.lhtModeStrictLabel');
    }
    if (mode === 'off') {
      return t('composer.lhtModeDisabledLabel');
    }
  } catch {
    /* ignore */
  }
  return t('composer.lhtModeLabel');
}

export default function ComposerOverflowMenu({
  open,
  onOpenChange,
  disabled,
  officeSession,
  showAutoApprove,
  autoApprove,
  autoApproveToggleEnabled,
  approvalPolicy,
  onAutoApproveChange,
  runMode,
  availableRunModes,
  runModePickerDisabled,
  onRunModeChange,
  taskTypePreference,
  lockedThreadTaskType,
  onTaskTypePreferenceChange,
  lhtChip,
  sessionExportEnabled,
  threadExportEnabled,
  onExportSessionJson,
  onExportThreadJson,
  onExportTraceReport,
  onExportTraceCompare,
  onOpenRouting,
}: Props) {
  const { t } = useT();
  const sectionPrefix = useId();
  const [expanded, setExpanded] = useState<SectionId | null>(null);

  useEffect(() => {
    if (!open) {
      setExpanded(null);
    }
  }, [open]);

  const close = () => onOpenChange(false);

  const toggleSection = (id: SectionId) => {
    setExpanded((current) => (current === id ? null : id));
  };

  const sectionPanelId = (id: SectionId) => `${sectionPrefix}-${id}`;

  const taskTypeResolved: DesktopTaskTypePreference | DesktopTaskTypeResolved =
    lockedThreadTaskType ?? taskTypePreference;

  const taskTypeChipHint =
    lockedThreadTaskType != null
      ? t('composer.taskTypeLocked', { type: t(TASK_TYPE_LABEL_KEYS[lockedThreadTaskType]) })
      : t(TASK_TYPE_HINT_KEYS[taskTypePreference]);

  const selectRunMode = (mode: DesktopRunModeId) => {
    onRunModeChange(mode);
    close();
  };

  const selectTaskType = (value: DesktopTaskTypePreference) => {
    onTaskTypePreferenceChange(value);
    close();
  };

  const autoApproveSummary = autoApproveToggleEnabled
    ? autoApprove
      ? t('composer.autoApproveShort')
      : t('composer.overflowOff')
    : t(
        `settings.${approvalPolicySettingsKey(approvalPolicy)}` as 'settings.approvalOnRequest',
      );

  return (
    <OverflowMenu
      open={open}
      onOpenChange={onOpenChange}
      disabled={disabled}
      triggerTitle={t('composer.moreMenu')}
      triggerAriaLabel={t('composer.moreMenu')}
      menuAriaLabel={t('a11y.composerOptionsToolbar')}
      align="start"
      panelClassName="composer-overflow-panel w-[min(100vw-2rem,18rem)]"
    >
      {showAutoApprove ? (
        <ComposerOverflowSection
          title={t('composer.autoApproveShort')}
          summary={autoApproveSummary}
          expanded={expanded === 'autoApprove'}
          onToggle={() => toggleSection('autoApprove')}
          panelId={sectionPanelId('autoApprove')}
        >
          {autoApproveToggleEnabled ? (
            <button
              type="button"
              role="menuitemcheckbox"
              aria-checked={autoApprove}
              disabled={disabled}
              onClick={() => onAutoApproveChange(!autoApprove)}
              className={`mx-1 flex w-[calc(100%-0.5rem)] items-center gap-2 rounded-md px-2 py-2 text-left text-sm transition-colors ${
                autoApprove ? 'bg-accent-soft text-accent' : 'text-t-text hover:bg-hover'
              }`}
            >
              <IconBolt className="size-4 shrink-0" />
              <span>{t('composer.autoApprove')}</span>
            </button>
          ) : (
            <p className="px-3 py-1 text-[11px] leading-snug text-t-text-muted">
              {t('composer.approvalFromSettings', {
                policy: t(
                  `settings.${approvalPolicySettingsKey(approvalPolicy)}` as 'settings.approvalOnRequest',
                ),
              })}
              {' — '}
              {t('composer.approvalFromSettingsHint')}
            </p>
          )}
        </ComposerOverflowSection>
      ) : null}

      <ComposerOverflowSection
        title={t('composer.selectMode')}
        summary={DESKTOP_RUN_MODE_LABELS[runMode]}
        expanded={expanded === 'runMode'}
        onToggle={() => toggleSection('runMode')}
        panelId={sectionPanelId('runMode')}
      >
        {runModePickerDisabled ? (
          <p
            className="px-3 py-1 text-[11px] leading-snug text-t-text-muted"
            title={officeSession ? t('composer.officeRunModeHint') : t(RUN_MODE_HINT_KEYS[runMode])}
          >
            {officeSession ? t('composer.officeRunModeHint') : t(RUN_MODE_HINT_KEYS[runMode])}
          </p>
        ) : (
          availableRunModes.map((id) => (
            <button
              key={id}
              type="button"
              role="menuitemradio"
              aria-checked={id === runMode}
              disabled={disabled}
              title={t(RUN_MODE_HINT_KEYS[id])}
              onClick={() => selectRunMode(id)}
              className={optionBtnClass(id === runMode)}
            >
              <span className="font-medium">{DESKTOP_RUN_MODE_LABELS[id]}</span>
              <span className="text-[11px] leading-snug text-t-text-muted">{t(RUN_MODE_HINT_KEYS[id])}</span>
            </button>
          ))
        )}
      </ComposerOverflowSection>

      <ComposerOverflowSection
        title={t('composer.selectTaskType')}
        summary={t(TASK_TYPE_LABEL_KEYS[taskTypeResolved])}
        expanded={expanded === 'taskType'}
        onToggle={() => toggleSection('taskType')}
        panelId={sectionPanelId('taskType')}
      >
        <p className="px-3 pb-1 text-[11px] leading-snug text-t-text-muted">{taskTypeChipHint}</p>
        {(['auto', 'office', 'code'] as DesktopTaskTypePreference[]).map((id) => {
          const selected =
            lockedThreadTaskType == null
              ? id === taskTypePreference
              : id === lockedThreadTaskType;
          return (
            <button
              key={id}
              type="button"
              role="menuitemradio"
              aria-checked={selected}
              disabled={disabled}
              title={t(TASK_TYPE_HINT_KEYS[id])}
              onClick={() => selectTaskType(id)}
              className={optionBtnClass(selected)}
            >
              <span className="font-medium">{t(TASK_TYPE_LABEL_KEYS[id])}</span>
              <span className="text-[11px] leading-snug text-t-text-muted">{t(TASK_TYPE_HINT_KEYS[id])}</span>
            </button>
          );
        })}
      </ComposerOverflowSection>

      {!officeSession ? (
        <ComposerOverflowSection
          title={t('composer.lhtModeLabel')}
          summary={readLhtModeSummary(t)}
          expanded={expanded === 'lht'}
          onToggle={() => toggleSection('lht')}
          panelId={sectionPanelId('lht')}
        >
          <div className="px-2 py-1.5">
            <LhtModeToggle disabled={disabled} />
          </div>
          {lhtChip ? (
            <p
              className={`px-3 pb-2 text-[11px] leading-snug ${
                lhtChip.kind === 'blocked'
                  ? 'text-amber-700 dark:text-amber-300'
                  : lhtChip.kind === 'warning'
                    ? 'text-amber-600 dark:text-amber-400'
                    : 'text-t-text-muted'
              }`}
            >
              {lhtChip.kind === 'continue'
                ? t('composer.lhtContinue', { detail: lhtChip.detail ?? '' })
                : lhtChip.kind === 'blocked'
                  ? t('composer.lhtBlocked', { detail: lhtChip.detail ?? '' })
                  : t('composer.lhtWarning', { detail: lhtChip.detail ?? '' })}
            </p>
          ) : null}
        </ComposerOverflowSection>
      ) : null}

      <div className="composer-overflow-actions" role="group" aria-label={t('composer.overflowActions')}>
        <button
          type="button"
          role="menuitem"
          disabled={!sessionExportEnabled}
          onClick={() => {
            close();
            onExportSessionJson();
          }}
          className="flex w-full rounded-md px-3 py-2 text-left text-sm text-t-text hover:bg-hover disabled:opacity-40"
        >
          {t('composer.exportSession')}
        </button>
        <button
          type="button"
          role="menuitem"
          disabled={!threadExportEnabled}
          onClick={() => {
            close();
            onExportThreadJson();
          }}
          className="flex w-full rounded-md px-3 py-2 text-left text-sm text-t-text hover:bg-hover disabled:opacity-40"
        >
          {t('composer.exportThread')}
        </button>
        <button
          type="button"
          role="menuitem"
          disabled={!threadExportEnabled}
          onClick={() => {
            close();
            onExportTraceReport();
          }}
          className="flex w-full rounded-md px-3 py-2 text-left text-sm text-t-text hover:bg-hover disabled:opacity-40"
          title={t('longHorizon.exportTraceReportHint')}
        >
          {t('longHorizon.exportTraceReport')}
        </button>
        <button
          type="button"
          role="menuitem"
          disabled={!threadExportEnabled}
          onClick={() => {
            close();
            onExportTraceCompare();
          }}
          className="flex w-full rounded-md px-3 py-2 text-left text-sm text-t-text hover:bg-hover disabled:opacity-40"
          title={t('longHorizon.exportTraceCompareHint')}
        >
          {t('longHorizon.exportTraceCompare')}
        </button>
        {onOpenRouting && !officeSession ? (
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              close();
              onOpenRouting();
            }}
            className="flex w-full rounded-md px-3 py-2 text-left text-sm text-t-text hover:bg-hover"
          >
            {t('composer.openRouting')}
          </button>
        ) : null}
      </div>
    </OverflowMenu>
  );
}
