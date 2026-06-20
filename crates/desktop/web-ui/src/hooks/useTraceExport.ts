import { useCallback } from 'react';
import { toast } from '../lib/toast';

type TFn = (key: string, params?: Record<string, string>) => string;

export function useTraceExport(resumedThreadId: string | null, t: TFn) {
  const handleExportTraceReport = useCallback(async () => {
    if (!resumedThreadId) {
      toast.warning(t('banner.exportThreadNoId'));
      return;
    }
    const threadId = resumedThreadId;
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const { invoke } = await import('@tauri-apps/api/core');
      const savePath = await save({
        title: t('longHorizon.exportTraceReportTitle'),
        defaultPath: `zagens-trace-${threadId.slice(0, 8)}.html`,
        filters: [{ name: 'HTML', extensions: ['html'] }],
      });
      if (!savePath) return;
      await invoke('export_thread_trace_report', { threadId, savePath });
      toast.success(t('longHorizon.exportTraceReportDone'));
      try {
        await invoke('open_with_system_app', { path: savePath });
      } catch {
        // saved even if open fails
      }
    } catch (e) {
      toast.error(
        t('longHorizon.exportTraceReportFailed', {
          message: e instanceof Error ? e.message : String(e),
        }),
      );
    }
  }, [resumedThreadId, t]);

  const handleExportTraceCompare = useCallback(async () => {
    if (!resumedThreadId) {
      toast.warning(t('banner.exportThreadNoId'));
      return;
    }
    const threadId = resumedThreadId;
    const peer = window.prompt(t('longHorizon.exportTraceCompareVs'), '')?.trim() ?? '';
    if (!peer) {
      toast.warning(t('longHorizon.exportTraceCompareNeedPeer'));
      return;
    }
    if (peer === threadId) {
      toast.warning(t('longHorizon.exportTraceCompareSameThread'));
      return;
    }
    try {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const { invoke } = await import('@tauri-apps/api/core');
      const savePath = await save({
        title: t('longHorizon.exportTraceCompareTitle'),
        defaultPath: `zagens-compare-${threadId.slice(0, 6)}_vs_${peer.slice(0, 6)}.html`,
        filters: [{ name: 'HTML', extensions: ['html'] }],
      });
      if (!savePath) return;
      await invoke('export_thread_trace_compare', {
        leftThreadId: threadId,
        rightThreadId: peer,
        savePath,
      });
      toast.success(t('longHorizon.exportTraceCompareDone'));
      try {
        await invoke('open_with_system_app', { path: savePath });
      } catch {
        // optional
      }
    } catch (e) {
      toast.error(
        t('longHorizon.exportTraceCompareFailed', {
          message: e instanceof Error ? e.message : String(e),
        }),
      );
    }
  }, [resumedThreadId, t]);

  return { handleExportTraceReport, handleExportTraceCompare };
}
