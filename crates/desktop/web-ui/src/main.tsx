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

  // Port discovery runs in the background; AppShell renders while runtimeConn is `checking`.
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
  document.getElementById('root')!.textContent = `Failed to start: ${e}`;
}
