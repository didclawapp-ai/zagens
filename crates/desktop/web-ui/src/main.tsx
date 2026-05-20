import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { I18nProvider } from './i18n';
import { ToastProvider } from './lib/toast';
import { initRuntimeConfig } from './api/client';
import './styles/fonts.css';
import './styles/globals.css';
import 'highlight.js/styles/github.css';

async function bootstrap() {
  await initRuntimeConfig();
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <I18nProvider defaultLocale="zh-Hans">
        <ToastProvider>
          <App />
        </ToastProvider>
      </I18nProvider>
    </React.StrictMode>,
  );
}

bootstrap().catch((e) => {
  console.error(e);
  document.getElementById('root')!.textContent = `Failed to start: ${e}`;
});
