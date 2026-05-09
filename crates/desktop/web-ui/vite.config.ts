import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { visualizer } from 'rollup-plugin-visualizer';

// `npm run build:analyze` — open dist/bundle-stats.html after build (treemap + gzip/brotli estimates).
export default defineConfig(({ mode }) => ({
  plugins: [
    react(),
    mode === 'analyze' &&
      visualizer({
        filename: 'dist/bundle-stats.html',
        gzipSize: true,
        brotliSize: true,
        template: 'treemap',
      }),
  ].filter(Boolean),
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: 'esnext',
    /** Later: split heavy routes (e.g. preview) with `manualChunks` or `import()` — check bundle-stats first */
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('node_modules/react-dom') || id.includes('node_modules/react/')) {
            return 'react-vendor';
          }
          if (id.includes('node_modules/markdown-it') || id.includes('node_modules/dompurify')) {
            return 'markdown-vendor';
          }
          if (id.includes('node_modules/highlight.js')) {
            return 'hljs-vendor';
          }
        },
      },
    },
  },
}));
