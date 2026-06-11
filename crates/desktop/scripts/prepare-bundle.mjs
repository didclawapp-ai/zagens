/**
 * Runs before `tauri build`: Vite production build + release `zagens-runtime` +
 * copy into `binaries/` with the rustc host triple (Tauri `externalBin` layout).
 */
import { execSync } from 'node:child_process';
import { copyFileSync, mkdirSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const desktopRoot = join(__dirname, '..');
const workspaceRoot = join(desktopRoot, '..', '..');
const webUi = join(desktopRoot, 'web-ui');
const binariesDir = join(desktopRoot, 'binaries');

function rustcHostTriple() {
  const out = execSync('rustc -vV', { encoding: 'utf8' });
  const m = out.match(/^host:\s*(\S+)/m);
  if (!m) {
    throw new Error('Could not parse host triple from `rustc -vV`');
  }
  return m[1];
}

/** `npm run build` needs devDependencies (tsc, vite). Skip them when NODE_ENV=production. */
function ensureWebUiDevDependencies() {
  const tscBin = join(
    webUi,
    'node_modules',
    '.bin',
    process.platform === 'win32' ? 'tsc.cmd' : 'tsc',
  );
  if (existsSync(tscBin)) {
    return;
  }
  console.log(
    '[bundle] web-ui devDependencies missing (often NODE_ENV=production) — npm install --include=dev…',
  );
  execSync('npm install --include=dev', {
    cwd: webUi,
    stdio: 'inherit',
    env: { ...process.env, NODE_ENV: 'development' },
  });
}

ensureWebUiDevDependencies();
console.log('[bundle] Building web-ui…');
execSync('npm run build', { cwd: webUi, stdio: 'inherit' });

console.log('[bundle] Building zagens-runtime (release)…');
execSync('cargo build --release -p zagens-cli', {
  cwd: workspaceRoot,
  stdio: 'inherit',
});

const triple = rustcHostTriple();
const ext = process.platform === 'win32' ? '.exe' : '';
const src = join(workspaceRoot, 'target', 'release', `zagens-runtime${ext}`);
const dest = join(binariesDir, `zagens-runtime-${triple}${ext}`);

mkdirSync(binariesDir, { recursive: true });
if (!existsSync(src)) {
  throw new Error(`Missing sidecar binary: ${src}`);
}
copyFileSync(src, dest);
console.log(`[bundle] Sidecar ready: ${dest}`);

if (process.platform === 'win32') {
  console.log('[bundle] Building Windows sandbox helpers (release)…');
  execSync(
    'cargo build --release -p zagens-windows-sandbox --bin zagens-sandbox-setup --bin zagens-command-runner',
    { cwd: workspaceRoot, stdio: 'inherit' },
  );
  const resourcesDir = join(binariesDir, 'zagens-resources');
  mkdirSync(resourcesDir, { recursive: true });
  for (const name of ['zagens-sandbox-setup.exe', 'zagens-command-runner.exe']) {
    const helperSrc = join(workspaceRoot, 'target', 'release', name);
    const helperDest = join(resourcesDir, name);
    if (!existsSync(helperSrc)) {
      throw new Error(`Missing sandbox helper binary: ${helperSrc}`);
    }
    copyFileSync(helperSrc, helperDest);
    console.log(`[bundle] Sandbox helper ready: ${helperDest}`);
  }
}

// Stage third-party license texts (MIT compliance for embedded runtime).
const { prepareLegalBundle } = await import('./prepare-legal.mjs');
prepareLegalBundle();

// Prepare bundled Python runtime (python-build-standalone + office deps).
const { preparePythonRuntime } = await import('./prepare-python.mjs');
await preparePythonRuntime(binariesDir, triple);
