import ApiKeyForm from './ApiKeyForm';
import MarkdownPreview from './MarkdownPreview';
import type { RuntimeConnectionState } from '../api/client';

export type RightPanelView = 'workspace' | 'api-key' | 'settings';

type Theme = 'light' | 'dark';

interface Props {
  view: RightPanelView;
  desktopHost: boolean;
  runtimeConn: RuntimeConnectionState;
  apiKeyConfigured: boolean | null;
  onSavedApiKey: () => void;
  theme: Theme;
  onToggleTheme: () => void;
}

const titles: Record<RightPanelView, string> = {
  workspace: '预览与项目文件',
  'api-key': 'API Key',
  settings: '设置',
};

const sampleMarkdown = `# deepseek-desktop-overview

*右侧预览面板 — 支持 Markdown 渲染与代码高亮。*

---

## 技术栈

- Tauri 壳 + \`web-ui\`（Vite / React）
- Sidecar：\`deepseek-tui serve --http\`
- 流式：\`POST /v1/stream\`（SSE）

## 项目结构

\`\`\`rust
fn main() {
    println!("DeepSeek Desktop");
}
\`\`\`

> 这是一个桌面端的 AI 编码助手，可以管理会话、流式对话、审批工具调用。
`;

export default function RightPanel({
  view,
  desktopHost,
  runtimeConn,
  apiKeyConfigured,
  onSavedApiKey,
  theme,
  onToggleTheme,
}: Props) {
  return (
    <aside
      className="flex w-80 shrink-0 flex-col border-l border-divider bg-card"
      aria-label="侧栏面板"
    >
      <div className="flex shrink-0 items-center border-b border-divider px-4 py-3">
        <h2 className="flex-1 text-sm font-semibold text-t-text">{titles[view]}</h2>
      </div>
      <div className="flex-1 overflow-y-auto text-sm text-t-text">
        {view === 'workspace' && (
          <MarkdownPreview content={sampleMarkdown} fileName="overview.md" language="markdown" />
        )}
        {view === 'api-key' && (
          <div className="p-4">
            {!desktopHost && (
              <p className="mb-3 text-xs text-amber-text/90 leading-relaxed">
                当前未在 Tauri 桌面壳中运行，无法通过此处写入密钥；请在配置文件中手动设置或使用 CLI。
              </p>
            )}
            {desktopHost && apiKeyConfigured === false && (
              <p className="mb-3 text-xs text-amber-text/90 leading-relaxed">
                未检测到已保存的 DeepSeek API Key。
              </p>
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
          <div className="p-4 space-y-4">
            <p className="text-xs text-t-text-muted leading-relaxed">
              通用设置（主题、语言、默认模型等）将在此扩展。当前连接状态：
            </p>
            <dl className="space-y-2 text-xs">
              <div className="flex justify-between gap-2 py-1.5 border-b border-divider">
                <dt className="text-t-text-muted">本地运行时</dt>
                <dd className="text-t-text">
                  {runtimeConn === 'connected' && '已连接'}
                  {runtimeConn === 'checking' && '检测中…'}
                  {runtimeConn === 'offline' && '离线'}
                  {runtimeConn === 'auth_mismatch' && '令牌不一致'}
                </dd>
              </div>
              <div className="flex justify-between gap-2 py-1.5 border-b border-divider">
                <dt className="text-t-text-muted">主题</dt>
                <dd>
                  <button
                    type="button"
                    onClick={onToggleTheme}
                    className="text-accent hover:underline"
                  >
                    {theme === 'light' ? '浅色' : '暗色'}（点击切换）
                  </button>
                </dd>
              </div>
              <div className="flex justify-between gap-2 py-1.5">
                <dt className="text-t-text-muted">Tauri 桌面</dt>
                <dd className="text-t-text">{desktopHost ? '是' : '否（浏览器模式）'}</dd>
              </div>
            </dl>
          </div>
        )}
      </div>
    </aside>
  );
}
