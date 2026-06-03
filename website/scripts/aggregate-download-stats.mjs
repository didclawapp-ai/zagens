#!/usr/bin/env node
/**
 * Aggregate installer downloads from an Nginx access log into public/download/stats.json.
 *
 * Usage (on VPS after enabling zagens-download.log — see deploy/nginx-zagens.conf.example):
 *   NGINX_DOWNLOAD_LOG=/var/log/nginx/zagens-download.log \
 *     node scripts/aggregate-download-stats.mjs --out /var/www/zagens/download/stats.json
 *
 * Options:
 *   --log <path>     Access log (default: NGINX_DOWNLOAD_LOG or /var/log/nginx/zagens-download.log)
 *   --out <path>     Output stats.json (default: website/public/download/stats.json)
 *   --offset <n>     Add baseline (e.g. historical downloads before logging was enabled)
 *   --dry-run        Print count without writing
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const websiteRoot = path.resolve(__dirname, '..');

function parseArgs(argv) {
  const opts = {
    log: process.env.NGINX_DOWNLOAD_LOG || '/var/log/nginx/zagens-download.log',
    out: path.join(websiteRoot, 'public/download/stats.json'),
    offset: 0,
    dryRun: false,
  };
  for (let i = 2; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--dry-run') opts.dryRun = true;
    else if (arg === '--log') opts.log = argv[++i];
    else if (arg === '--out') opts.out = argv[++i];
    else if (arg === '--offset') opts.offset = Number(argv[++i]) || 0;
    else if (arg === '--help' || arg === '-h') {
      console.log(fs.readFileSync(fileURLToPath(import.meta.url), 'utf8').split('\n').slice(0, 14).join('\n'));
      process.exit(0);
    }
  }
  return opts;
}

/** Count successful GET/HEAD of .zip / .exe under /download/ (not .sha256 / .sig / .json). */
export function countInstallerDownloads(logText) {
  let count = 0;
  for (const line of logText.split('\n')) {
    if (!line.includes(' /download/')) continue;
    if (!/"(?:GET|HEAD) \/download\/[^"]+\.(?:zip|exe) HTTP\//i.test(line)) continue;
    if (/\.(?:sha256|sig)(?:\s|")/i.test(line)) continue;
    if (!/\s(200|206)\s/.test(line)) continue;
    count++;
  }
  return count;
}

function main() {
  const opts = parseArgs(process.argv);

  if (!fs.existsSync(opts.log)) {
    console.error(`Log not found: ${opts.log}`);
    process.exit(1);
  }

  const logText = fs.readFileSync(opts.log, 'utf8');
  const fromLog = countInstallerDownloads(logText);
  const total = fromLog + opts.offset;
  const payload = {
    total,
    updatedAt: new Date().toISOString(),
    source: 'nginx-access-log',
  };

  if (opts.dryRun) {
    console.log(JSON.stringify({ fromLog, offset: opts.offset, ...payload }, null, 2));
    return;
  }

  fs.mkdirSync(path.dirname(opts.out), { recursive: true });
  fs.writeFileSync(opts.out, `${JSON.stringify(payload, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${opts.out} (total=${total}, fromLog=${fromLog}, offset=${opts.offset})`);
}

main();
