#!/usr/bin/env node
/**
 * Inline Vite build output into a single-file HTML shell for `zagens trace export`.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const distDir = path.resolve(__dirname, '../tools/trace-report/dist');
const outPath = path.join(distDir, 'report.html');
const indexPath = path.join(distDir, 'index.html');

if (!fs.existsSync(indexPath)) {
  console.error('Missing dist/index.html — run vite build first');
  process.exit(1);
}

let html = fs.readFileSync(indexPath, 'utf8');

// Inline CSS
html = html.replace(
  /<link rel="stylesheet" crossorigin href="([^"]+)">/,
  (_match, href) => {
    const cssPath = path.join(distDir, href.replace(/^\//, ''));
    const css = fs.readFileSync(cssPath, 'utf8');
    return `<style>${css}</style>`;
  },
);

// Inline JS module
html = html.replace(
  /<script type="module" crossorigin src="([^"]+)"><\/script>/,
  (_match, href) => {
    const jsPath = path.join(distDir, href.replace(/^\//, ''));
    const js = fs.readFileSync(jsPath, 'utf8');
    return `<script type="module">${js}</script>`;
  },
);

// Remove dev-only module script if still present
html = html.replace(/<script type="module" src="\/src\/main\.tsx"><\/script>\s*/g, '');

if (!html.includes('__ZAGENS_TRACE_BUNDLE__')) {
  console.error('Bundle placeholder missing from HTML shell');
  process.exit(1);
}

fs.writeFileSync(outPath, html);
console.log(`Wrote ${outPath}`);

const assetDir = path.resolve(__dirname, '../crates/runtime-server/assets/trace-report');
fs.mkdirSync(assetDir, { recursive: true });
const assetPath = path.join(assetDir, 'report.html');
fs.copyFileSync(outPath, assetPath);
console.log(`Copied shell → ${assetPath}`);
