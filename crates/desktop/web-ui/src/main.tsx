import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { I18nProvider } from './i18n';
import { ToastProvider } from './lib/toast';
import { initRuntimeConfig } from './api/client';
import './styles/fonts.css';
import './styles/globals.css';
import 'highlight.js/styles/github.css';

function bootstrap() {
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <I18nProvider>
        <ToastProvider>
          <App />
        </ToastProvider>
      </I18nProvider>
    </React.StrictMode>,
  );

  // Start port discovery before first paint; all runtime HTTP awaits this promise in client.ts.
  void initRuntimeConfig();

  // Show the shell as soon as React mounts — do not wait for sidecar boot or session list.
  void import('@tauri-apps/api/window')
    .then(({ getCurrentWindow }) => getCurrentWindow().show())
    .catch(() => {});
}

try {
  bootstrap();
} catch (e) {
  console.error(e);
  const fb = document.getElementById('zagens-boot-fallback');
  const root = document.getElementById('root');
  const msg = `界面初始化失败：${e instanceof Error ? e.message : String(e)}。请释放系统盘与 ~/.zagens 所在盘的磁盘空间后重启。`;
  if (fb) {
    const p = document.getElementById('zagens-boot-fallback-msg');
    if (p) p.textContent = msg;
    fb.style.display = 'block';
    if (root) root.style.display = 'none';
  } else if (root) {
    root.textContent = msg;
  }
}
