import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { initRuntimeConfig } from './api/client';
import './styles/globals.css';

async function bootstrap() {
  await initRuntimeConfig();
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

bootstrap().catch((e) => {
  console.error(e);
  document.getElementById('root')!.textContent = `Failed to start: ${e}`;
});
