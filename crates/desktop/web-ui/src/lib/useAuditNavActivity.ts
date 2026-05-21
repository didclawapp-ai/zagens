import { useEffect, useMemo, useState } from 'react';
import { fetchThreadScratchpadStatus, type ScratchpadStatus } from '../api/client';
import type { RightPanelView } from '../components/RightPanel';
import type { InspectorNavActivity } from './inspectorUnread';
import {
  SCRATCHPAD_STATUS_POLL_IDLE_MS,
  SCRATCHPAD_STATUS_POLL_STREAMING_MS,
} from '../lib/runtimePoll';
import { PANEL_SCRATCHPAD_EVENT } from './panelChannel';

export function useAuditNavActivity({
  threadId,
  activeInspector,
  streaming,
  runtimeSessionEstablished,
  narrativeSpawnSuspected,
}: {
  threadId: string | null;
  activeInspector: RightPanelView;
  streaming: boolean;
  runtimeSessionEstablished: boolean;
  narrativeSpawnSuspected: boolean;
}): InspectorNavActivity {
  const [status, setStatus] = useState<ScratchpadStatus | null>(null);

  useEffect(() => {
    if (!runtimeSessionEstablished || !threadId) {
      setStatus(null);
      return;
    }
    let cancelled = false;
    const apply = (data: ScratchpadStatus | null) => {
      if (!cancelled) {
        setStatus(data);
      }
    };
    const load = async () => {
      try {
        apply(await fetchThreadScratchpadStatus(threadId));
      } catch {
        /* keep snapshot */
      }
    };
    const onPanelPush = (ev: Event) => {
      const detail = (ev as CustomEvent<ScratchpadStatus | null>).detail;
      if (detail && typeof detail === 'object') {
        apply(detail);
      }
    };
    void load();
    window.addEventListener(PANEL_SCRATCHPAD_EVENT, onPanelPush);
    const ms = streaming ? SCRATCHPAD_STATUS_POLL_STREAMING_MS : SCRATCHPAD_STATUS_POLL_IDLE_MS;
    const id = window.setInterval(() => void load(), ms);
    return () => {
      cancelled = true;
      window.removeEventListener(PANEL_SCRATCHPAD_EVENT, onPanelPush);
      window.clearInterval(id);
    };
  }, [threadId, runtimeSessionEstablished, streaming]);

  return useMemo((): InspectorNavActivity => {
    if (activeInspector === 'audit' || !status?.run_id) {
      return { active: false, pulse: false };
    }
    const warnings = status.contract_warnings ?? [];
    const notesTotal = status.notes_total ?? 0;
    const accounted =
      (status.areas_done ?? 0) + (status.areas_deferred ?? 0) + (status.areas_in_progress ?? 0);
    const contractViolation =
      warnings.includes('notes_without_accounted') ||
      (notesTotal > 0 && accounted === 0);
    const hasAttention =
      contractViolation ||
      warnings.includes('checklist_inventory_mismatch') ||
      narrativeSpawnSuspected ||
      warnings.length > 0;
    return { active: true, pulse: hasAttention || streaming };
  }, [activeInspector, status, narrativeSpawnSuspected, streaming]);
}
