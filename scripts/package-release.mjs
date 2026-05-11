/**
 * Builds a standalone deepseek-tui release package (`.tar.gz` / `.zip`)
 * that includes a bundled Python runtime (python-build-standalone).
 *
 * Usage:
 *   node scripts/package-release.mjs
 *
 * Output:
 *   ./deepseek-tui-<target-triple>.tar.gz   (macOS / Linux)
 *   ./deepseek-tui-<target-triple>.zip       (Windows)
 */

import { execSync } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = join(__dirname, '..');

function rustcHostTriple() {
    const out = execSync('rustc -vV', { encoding: 'utf8' });
    const m = out.match(/^host:\s*(\S+)/m);
    if (!m) throw new Error('Could not parse host triple');
    return m[1];
}

async function main() {
    const triple = rustcHostTriple();
    const ext = process.platform === 'win32' ? '.exe' : '';
    const releaseDir = join(workspaceRoot, 'target', 'release');

    // 1. Build deepseek-tui
    console.log('[release] Building deepseek-tui (release)…');
    execSync('cargo build --release -p deepseek-tui', { cwd: workspaceRoot, stdio: 'inherit' });

    // 2. Prepare Python runtime
    console.log('[release] Preparing Python runtime…');
    const { preparePythonRuntime } = await import('../crates/desktop/scripts/prepare-python.mjs');
    await preparePythonRuntime(releaseDir, triple);

    // 3. Package
    const archiveName = `deepseek-tui-${triple}`;
    if (process.platform === 'win32') {
        const zipPath = `${archiveName}.zip`;
        execSync(
            `powershell -NoProfile -Command "Compress-Archive -Path '${releaseDir}\\deepseek-tui.exe','${releaseDir}\\python-standalone' -DestinationPath '${zipPath}'"`,
            { cwd: workspaceRoot, stdio: 'inherit' }
        );
        console.log(`[release] Done: ${zipPath}`);
    } else {
        const tarball = `${archiveName}.tar.gz`;
        execSync(
            `tar czf "${tarball}" -C "${releaseDir}" deepseek-tui python-standalone`,
            { cwd: workspaceRoot, stdio: 'inherit' }
        );
        console.log(`[release] Done: ${tarball}`);
    }
}

main().catch(e => { console.error(e); process.exit(1); });
