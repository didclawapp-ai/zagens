/**
 * Runs AFTER `tauri build`: wraps each Windows installer (NSIS `*-setup.exe`,
 * MSI) in a `.zip` and emits SHA-256 checksums for both the zip and the raw
 * installer.
 *
 * Why the zip: unsigned installers carry SmartScreen friction once a browser
 * stamps them with the "Mark of the Web". Distributing the installer inside a
 * zip lets users *unblock once* at the zip level (right-click → Properties →
 * Unblock) so the extracted installer runs without the SmartScreen prompt — and
 * the full installer inside still auto-installs WebView2 (unlike a portable
 * build). See docs/desktop/SMARTSCREEN.md.
 *
 * Usage: `npm run package:release` (from crates/desktop) after a build, or run
 * automatically in the release workflow. Idempotent: re-running overwrites.
 */
import { execFileSync } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import {
  createReadStream,
  readdirSync,
  existsSync,
  mkdtempSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname, basename } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const desktopRoot = join(__dirname, '..');
const workspaceRoot = join(desktopRoot, '..', '..');
const bundleDir = join(workspaceRoot, 'target', 'release', 'bundle');

const HELP_URL = 'https://zagens.com';

/** Collect installer files from the NSIS and MSI bundle sub-directories. */
function findInstallers() {
  const found = [];
  for (const [sub, exts] of [
    ['nsis', ['.exe']],
    ['msi', ['.msi']],
  ]) {
    const dir = join(bundleDir, sub);
    if (!existsSync(dir)) continue;
    for (const name of readdirSync(dir)) {
      const lower = name.toLowerCase();
      if (exts.some((e) => lower.endsWith(e))) {
        found.push(join(dir, name));
      }
    }
  }
  return found;
}

function sha256File(path) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    const stream = createReadStream(path);
    stream.on('error', reject);
    stream.on('data', (chunk) => hash.update(chunk));
    stream.on('end', () => resolve(hash.digest('hex')));
  });
}

/** Write a `<file>.sha256` next to the artifact in `sha256sum`-compatible form. */
async function writeChecksum(path) {
  const hex = await sha256File(path);
  const line = `${hex} *${basename(path)}\n`;
  writeFileSync(`${path}.sha256`, line);
  console.log(`[package] sha256 ${basename(path)}  ${hex}`);
  return hex;
}

/** Plain-text install guide bundled inside each zip (CRLF for Notepad). */
function readmeText(installerName, installerSha256) {
  return [
    'Zagens — 安装说明 / Install Guide',
    '====================================',
    '',
    `文件 / File : ${installerName}`,
    `SHA-256     : ${installerSha256}`,
    '',
    '【安装步骤 / How to install】',
    '1. 若本 zip 还带"网络来源"标记,请先解锁,再解压:',
    '   右键 zip → 属性(Properties)→ 勾选"解除锁定(Unblock)"→ 确定。',
    '   Right-click the .zip → Properties → tick "Unblock" → OK, THEN extract.',
    '   (先解锁再解压,安装时就不会弹 Windows SmartScreen 提示。)',
    `2. 双击 ${installerName} 完成安装。`,
    `   Double-click ${installerName} to install.`,
    '',
    '【若仍弹出 SmartScreen 蓝框 / If SmartScreen still appears】',
    '点"更多信息(More info)"→"仍要运行(Run anyway)"。',
    '这不是病毒告警,只是该安装包尚未做代码签名。',
    'This is not a virus warning — the installer is simply not code-signed yet.',
    '',
    '【校验完整性 / Verify integrity (optional)】',
    'PowerShell:',
    `  Get-FileHash .\\${installerName} -Algorithm SHA256`,
    '输出应与上面的 SHA-256 一致 / The output should match the SHA-256 above.',
    '',
    '【系统要求 / Requirements】',
    '- Windows 10/11 (x64)',
    '- 需要 WebView2 运行时;缺失时安装器会自动联网安装。',
    '  WebView2 runtime auto-installs during setup if missing.',
    '',
    `更多帮助 / More help: ${HELP_URL}`,
    '',
  ].join('\r\n');
}

/**
 * Zip a single installer into `<name>.zip` (flat: installer + README.txt at the
 * archive root). The README is generated in a temp dir so the bundle stays clean.
 */
function zipInstaller(installerPath, installerSha256) {
  const zipPath = `${installerPath}.zip`;
  const tmp = mkdtempSync(join(tmpdir(), `zagens-pkg-${randomUUID().slice(0, 8)}-`));
  const readmePath = join(tmp, 'README.txt');
  writeFileSync(readmePath, readmeText(basename(installerPath), installerSha256));
  try {
    if (process.platform === 'win32') {
      // Compress-Archive is built into Windows PowerShell — zero extra deps.
      execFileSync(
        'powershell',
        [
          '-NoProfile',
          '-NonInteractive',
          '-Command',
          `Compress-Archive -Path '${installerPath}','${readmePath}' -DestinationPath '${zipPath}' -Force`,
        ],
        { stdio: 'inherit' },
      );
    } else {
      // Non-Windows fallback (requires `zip` on PATH); release CI is Windows-only.
      execFileSync('zip', ['-j', '-q', zipPath, installerPath, readmePath], { stdio: 'inherit' });
    }
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
  return zipPath;
}

async function main() {
  if (!existsSync(bundleDir)) {
    throw new Error(`Missing bundle dir (run a build first): ${bundleDir}`);
  }
  const installers = findInstallers();
  if (installers.length === 0) {
    throw new Error(`No installers (.exe/.msi) found under ${bundleDir}`);
  }

  for (const installer of installers) {
    const sizeMb = (statSync(installer).size / (1024 * 1024)).toFixed(1);
    console.log(`[package] ${basename(installer)} (${sizeMb} MB)`);
    const installerHash = await writeChecksum(installer);
    const zipPath = zipInstaller(installer, installerHash);
    await writeChecksum(zipPath);
  }

  console.log(`[package] done — ${installers.length} installer(s) zipped + checksummed.`);
}

main().catch((err) => {
  console.error(`[package] failed: ${err.message}`);
  process.exit(1);
});
