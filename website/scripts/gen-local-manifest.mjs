#!/usr/bin/env node
/**
 * Generate latest.json (OTA) and release.json (website download page)
 * from a LOCAL Tauri NSIS bundle directory — no GitHub Release required.
 *
 * Usage:
 *   node scripts/gen-local-manifest.mjs
 *   node scripts/gen-local-manifest.mjs --bundle-dir ../../target/release/bundle/nsis
 *   node scripts/gen-local-manifest.mjs --bundle-dir D:/some/path/nsis
 *
 * The script will:
 *   1. Find Zagens_*_x64-setup.exe in the bundle dir (picks the highest semver)
 *   2. Compute SHA-256 for exe and zip
 *   3. Read the .sig file (Tauri minisign) for the OTA signature
 *   4. Write  website/src/data/release.json  (download page metadata)
 *   5. Write  website/public/download/latest.json  (Tauri OTA endpoint)
 *   6. Copy   exe / exe.zip / exe.sha256 / exe.zip.sha256 / exe.sig
 *             → website/public/download/  (unless --no-copy)
 *
 * Env overrides:
 *   BUNDLE_DIR          same as --bundle-dir
 *   UPDATER_SIGNATURE   paste .sig content directly (skips file read)
 *   DOWNLOAD_BASE_URL   default https://zagens.com/download
 *   NO_COPY=1           skip copying files to public/download/
 */

import { createHash } from 'node:crypto';
import { createReadStream, existsSync } from 'node:fs';
import { copyFile, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const websiteRoot = join(__dirname, '..');
const downloadDir = join(websiteRoot, 'public', 'download');
const releasePath = join(websiteRoot, 'src', 'data', 'release.json');
const latestJsonPath = join(downloadDir, 'latest.json');

// ── CLI arg parse ──────────────────────────────────────────────────────────
function parseArgs() {
  const args = process.argv.slice(2);
  let bundleDir = null;
  let noCopy = process.env.NO_COPY === '1';
  for (let i = 0; i < args.length; i++) {
    if ((args[i] === '--bundle-dir' || args[i] === '-d') && args[i + 1]) {
      bundleDir = resolve(args[++i]);
    } else if (args[i] === '--no-copy') {
      noCopy = true;
    }
  }
  return { bundleDir, noCopy };
}

// ── Semver compare (basic, for x.y.z[-pre] strings) ───────────────────────
function semverParts(v) {
  const m = v.match(/^(\d+)\.(\d+)\.(\d+)(?:-(.+))?$/);
  if (!m) return [0, 0, 0, 'z'];
  return [+m[1], +m[2], +m[3], m[4] ?? ''];
}
function semverGt(a, b) {
  const pa = semverParts(a), pb = semverParts(b);
  for (let i = 0; i < 3; i++) {
    if (pa[i] !== pb[i]) return pa[i] > pb[i];
  }
  // pre-release: empty > non-empty (release > preview)
  if (pa[3] === '' && pb[3] !== '') return true;
  if (pa[3] !== '' && pb[3] === '') return false;
  return pa[3] > pb[3];
}

// ── SHA-256 of a local file ────────────────────────────────────────────────
function sha256File(filePath) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    createReadStream(filePath)
      .on('error', reject)
      .on('data', (c) => hash.update(c))
      .on('end', () => resolve(hash.digest('hex')));
  });
}

// ── Main ───────────────────────────────────────────────────────────────────
async function main() {
  const { bundleDir: argBundleDir, noCopy } = parseArgs();

  // Resolve bundle directory
  const defaultBundleDir = resolve(websiteRoot, '..', 'target', 'release', 'bundle', 'nsis');
  const bundleDir =
    argBundleDir ??
    (process.env.BUNDLE_DIR ? resolve(process.env.BUNDLE_DIR) : defaultBundleDir);

  if (!existsSync(bundleDir)) {
    console.error(`[gen] Bundle dir not found: ${bundleDir}`);
    console.error('[gen] Run: cargo tauri build  or pass --bundle-dir <path>');
    process.exit(1);
  }
  console.log(`[gen] Bundle dir: ${bundleDir}`);

  // Find all exe installers
  const files = await readdir(bundleDir);
  const exeFiles = files.filter((f) => /^Zagens_.*_x64-setup\.exe$/.test(f));
  if (exeFiles.length === 0) {
    console.error('[gen] No Zagens_*_x64-setup.exe found in bundle dir');
    process.exit(1);
  }

  // Pick highest version
  const exeName = exeFiles.sort((a, b) => {
    const verA = a.match(/Zagens_([\d.\-\w]+)_x64/)?.[1] ?? '';
    const verB = b.match(/Zagens_([\d.\-\w]+)_x64/)?.[1] ?? '';
    return semverGt(verA, verB) ? -1 : 1;
  })[0];

  const version = exeName.match(/Zagens_([\d.\-\w]+)_x64/)?.[1];
  if (!version) {
    console.error(`[gen] Could not parse version from filename: ${exeName}`);
    process.exit(1);
  }
  console.log(`[gen] Version: ${version}  (file: ${exeName})`);

  const zipName = `${exeName}.zip`;
  const sigName = `${exeName}.sig`;
  const exeSha256Name = `${exeName}.sha256`;
  const zipSha256Name = `${zipName}.sha256`;

  const exePath = join(bundleDir, exeName);
  const zipPath = join(bundleDir, zipName);
  const sigPath = join(bundleDir, sigName);

  // Validate required files exist
  for (const [label, p] of [['exe', exePath], ['zip', zipPath], ['sig', sigPath]]) {
    if (!existsSync(p)) {
      console.error(`[gen] Required file missing (${label}): ${p}`);
      process.exit(1);
    }
  }

  // Compute SHA-256
  console.log('[gen] Computing SHA-256...');
  const [exeSha, zipSha] = await Promise.all([sha256File(exePath), sha256File(zipPath)]);
  console.log(`[gen]   exe: ${exeSha}`);
  console.log(`[gen]   zip: ${zipSha}`);

  // Read updater signature
  const updaterSig =
    process.env.UPDATER_SIGNATURE?.trim() ||
    (await readFile(sigPath, 'utf8')).trim();
  if (!updaterSig) {
    console.error('[gen] .sig file is empty — build with TAURI_SIGNING_PRIVATE_KEY set');
    process.exit(1);
  }

  const baseUrl = (process.env.DOWNLOAD_BASE_URL ?? 'https://zagens.com/download').replace(/\/$/, '');
  const today = new Date().toISOString().slice(0, 10);

  // ── Copy files to public/download/ ──────────────────────────────────────
  if (!noCopy) {
    await mkdir(downloadDir, { recursive: true });
    const toCopy = [exeName, zipName, sigName, exeSha256Name, zipSha256Name];
    // Write sha256 files fresh from computed values (more reliable than copying)
    await writeFile(join(downloadDir, exeSha256Name), `${exeSha}  ${exeName}\n`);
    await writeFile(join(downloadDir, zipSha256Name), `${zipSha}  ${zipName}\n`);
    // Copy binary files
    for (const f of [exeName, zipName, sigName]) {
      const src = join(bundleDir, f);
      const dst = join(downloadDir, f);
      if (existsSync(src)) {
        await copyFile(src, dst);
        console.log(`[gen] copied → public/download/${f}`);
      }
    }
    console.log(`[gen] wrote  → public/download/${exeSha256Name}`);
    console.log(`[gen] wrote  → public/download/${zipSha256Name}`);
  } else {
    console.log('[gen] --no-copy: skipping file copy to public/download/');
  }

  // ── Write release.json ───────────────────────────────────────────────────
  const release = {
    version,
    publishedAt: today,
    platforms: {
      'windows-x64': {
        zip: {
          filename: zipName,
          url: `${baseUrl}/${zipName}`,
          sha256: zipSha,
        },
        exe: {
          filename: exeName,
          url: `${baseUrl}/${exeName}`,
          sha256: exeSha,
        },
      },
    },
    notes: `Local signed build ${new Date().toISOString()}.`,
  };
  await writeFile(releasePath, `${JSON.stringify(release, null, 2)}\n`);
  console.log(`[gen] wrote  → src/data/release.json  (v${version})`);

  // ── Write latest.json (OTA) ──────────────────────────────────────────────
  const latest = {
    version,
    notes: `Zagens ${version} preview`,
    pub_date: new Date().toISOString(),
    platforms: {
      'windows-x86_64': {
        signature: updaterSig,
        url: `${baseUrl}/${exeName}`,
      },
    },
  };
  await writeFile(latestJsonPath, `${JSON.stringify(latest, null, 2)}\n`);
  console.log(`[gen] wrote  → public/download/latest.json  (OTA)`);

  console.log('\n[gen] Done. Next steps:');
  console.log('  1. git add website/public/download/ website/src/data/release.json');
  console.log('  2. git commit && git push   # triggers website.yml deploy');
}

main().catch((err) => {
  console.error(`[gen] Error: ${err.message}`);
  process.exit(1);
});
