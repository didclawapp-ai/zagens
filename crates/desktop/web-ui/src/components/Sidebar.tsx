import type { RightPanelView } from './RightPanel';
import type { RuntimeConnectionState } from '../api/client';

interface SessionInfo {
  id: string;
  name: string;
  created_at?: number;
  updated_at?: number;
}

interface Props {
  sessions: SessionInfo[];
  activeSessionId: string | null;
  onNewSession: () => void;
  onSelectSession?: (id: string) => void;
  onDeleteSession?: (id: string) => void;
  desktopHost: boolean;
  runtimeConn: RuntimeConnectionState;
  apiKeyConfigured: boolean | null;
  activeInspector: RightPanelView;
  onInspectorChange: (view: RightPanelView) => void;
}

const navBtn = (active: boolean) =>
  `w-full text-left px-3 py-2 rounded-lg text-sm transition-colors ${
    active
      ? 'bg-gray-800 text-indigo-300 border border-indigo-500/30'
      : 'text-gray-400 hover:bg-gray-800/80 hover:text-gray-200'
  }`;

export default function Sidebar({
  sessions,
  activeSessionId,
  onNewSession,
  onSelectSession,
  onDeleteSession,
  desktopHost,
  runtimeConn,
  apiKeyConfigured,
  activeInspector,
  onInspectorChange,
}: Props) {
  return (
    <div
      className="flex w-60 shrink-0 flex-col border-r border-gray-800 bg-gray-900"
      aria-label="会话与导航"
    >
      <div className="shrink-0 border-b border-gray-800 px-3 py-3">
        <span className="text-sm font-semibold text-gray-200">会话</span>
      </div>

      <button
        type="button"
        onClick={onNewSession}
        className="mx-3 mt-3 shrink-0 px-3 py-2 text-sm text-indigo-400 hover:bg-gray-800 rounded-lg text-left border border-transparent hover:border-gray-700"
      >
        + 新会话
      </button>

      <div className="flex-1 min-h-0 overflow-y-auto px-2 py-2">
        {sessions.length === 0 && (
          <p className="text-xs text-gray-600 px-2 py-4 text-center">暂无会话</p>
        )}
        {sessions.map((s) => {
          const isActive = activeSessionId != null && s.id === activeSessionId;
          return (
          <div
            key={s.id}
            className={`flex items-center gap-1 rounded-lg group ${
              isActive ? 'bg-gray-800/80 ring-1 ring-indigo-500/40' : 'hover:bg-gray-800'
            }`}
          >
            <button
              type="button"
              onClick={() => onSelectSession?.(s.id)}
              className={`flex-1 min-w-0 px-3 py-2 text-sm text-left truncate ${
                isActive ? 'text-indigo-200' : 'text-gray-400'
              }`}
            >
              {s.name || s.id.slice(0, 8)}
            </button>
            {onDeleteSession && (
              <button
                type="button"
                title="删除会话"
                onClick={(e) => {
                  e.stopPropagation();
                  onDeleteSession(s.id);
                }}
                className="shrink-0 px-2 py-2 text-gray-600 hover:text-red-400 opacity-60 group-hover:opacity-100"
              >
                ×
              </button>
            )}
          </div>
          );
        })}
      </div>

      <div className="shrink-0 border-t border-gray-800 px-2 py-2 space-y-1">
        <div
          className="flex items-center gap-2 px-2 py-1.5 text-xs text-gray-500 mb-1"
          title="与本地 deepseek-tui 运行时 (127.0.0.1:7878) 的连接状态"
        >
          <span
            className={`shrink-0 inline-block w-2 h-2 rounded-full ${
              runtimeConn === 'connected'
                ? 'bg-emerald-500'
                : runtimeConn === 'auth_mismatch'
                  ? 'bg-amber-400'
                  : runtimeConn === 'offline'
                    ? 'bg-red-500'
                    : 'bg-gray-500'
            }`}
          />
          <span className="truncate">
            {runtimeConn === 'checking' && '检测运行时…'}
            {runtimeConn === 'connected' && '本地运行时已连接'}
            {runtimeConn === 'offline' && '运行时离线'}
            {runtimeConn === 'auth_mismatch' && '令牌不一致'}
          </span>
        </div>
        {desktopHost && apiKeyConfigured === false && (
          <p className="px-2 text-[10px] text-amber-600/90 leading-snug">未配置 API Key</p>
        )}

        <p className="px-2 pt-1 text-[10px] font-medium uppercase tracking-wide text-gray-600">
          侧栏
        </p>
        <button
          type="button"
          className={navBtn(activeInspector === 'workspace')}
          onClick={() => onInspectorChange('workspace')}
        >
          预览 / 项目文件
        </button>
        {desktopHost && (
          <button
            type="button"
            className={navBtn(activeInspector === 'api-key')}
            onClick={() => onInspectorChange('api-key')}
          >
            API Key
          </button>
        )}
        <button
          type="button"
          className={navBtn(activeInspector === 'settings')}
          onClick={() => onInspectorChange('settings')}
        >
          设置
        </button>
      </div>

      <div className="shrink-0 px-3 py-2 border-t border-gray-800">
        <p className="text-[10px] text-gray-600">DeepSeek Desktop v0.1</p>
      </div>
    </div>
  );
}
