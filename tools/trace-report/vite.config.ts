import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';

export default defineConfig({
  plugins: [preact()],
  build: {
    outDir: 'dist',
    cssCodeSplit: false,
    rollupOptions: {
      output: {
        entryFileNames: 'assets/report.js',
        assetFileNames: 'assets/report.[ext]',
      },
    },
  },
});
