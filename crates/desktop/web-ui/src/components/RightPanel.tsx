import ApiKeyForm from './ApiKeyForm';
import type { RuntimeConnectionState } from '../api/client';

export type RightPanelView = 'workspace' | 'api-key' | 'settings';

interface Props {
  view: RightPanelView;
  desktopHost: boolean;
  runtimeConn: RuntimeConnectionState;
  apiKeyConfigured: boolean | null;
  onSavedApiKey: () => void;
}

const titles: Record<RightPanelView, string> = {
  workspace: '预览与项目文件',
  'api-key': 'API Key',
  settings: '设置',
};

export default function RightPanel({
  view,
  desktopHost,
  runtimeConn,
  apiKeyConfigured,
  onSavedApiKey,
}: Props) {
  return (
    <aside
      className="flex w-80 shrink-0 flex-col border-l border-gray-800 bg-gray-900/95"
      aria-label="侧栏面板"
    >
      <div className="shrink-0 border-b border-gray-800 px-4 py-3">
        <h2 className="text-sm font-semibold text-gray-200">{titles[view]}</h2>
      </div>
      <div className="flex-1 overflow-y-auto p-4 text-sm text-gray-300">
        {view === 'workspace' && (
          <div className="space-y-3">
            <p className="text-xs text-gray-500 leading-relaxed">
              此栏预留用于预览生成文件、工作区树、差异对比等。后续版本将接入运行时工作区状态与本地项目目录。
            </p>
            <div className="rounded-lg border border-dashed border-gray-700 bg-gray-950/50 px-3 py-6 text-center text-xs text-gray-600">
              暂无预览内容
            </div>
          </div>
        )}
        {view === 'api-key' && (
          <div>
            {!desktopHost && (
              <p className="mb-3 text-xs text-amber-600/90">
                当前未在 Tauri 桌面壳中运行，无法通过此处写入密钥；请在配置文件中手动设置或使用 CLI。
              </p>
            )}
            {desktopHost && apiKeyConfigured === false && (
              <p className="mb-3 text-xs text-amber-500/90">未检测到已保存的 DeepSeek API Key。</p>
            )}
            {desktopHost && (
              <ApiKeyForm
                onSaved={onSavedApiKey}
                className={!desktopHost ? 'pointer-events-none opacity-50' : ''}
              />
            )}
          </div>
        )}
        {view === 'settings' && (
          <div className="space-y-4">
            <p className="text-xs text-gray-500 leading-relaxed">
              通用设置（主题、语言、默认模型等）将在此扩展。当前连接状态：
            </p>
            <dl className="space-y-2 text-xs">
              <div className="flex justify-between gap-2">
                <dt className="text-gray-500">本地运行时</dt>
                <dd className="text-gray-300">
                  {runtimeConn === 'connected' && '已连接'}
                  {runtimeConn === 'checking' && '检测中…'}
                  {runtimeConn === 'offline' && '离线'}
                  {runtimeConn === 'auth_mismatch' && '令牌不一致'}
                </dd>
              </div>
            </dl>
          </div>
        )}
      </div>
    </aside>
  );
}
