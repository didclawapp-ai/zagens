/**
 * Builds a standalone deepseek-tui / zagens-runtime style release zip/tarball
 * (binary only — no bundled Python).
 *
 * Usage:
 *   node scripts/package-release.mjs
 *
 * Output:
 *   ./deepseek-tui-<target-triple>.tar.gz   (macOS / Linux)
 *   ./deepseek-tui-<target-triple>.zip       (Windows)
 */

import { execSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const workspaceRoot = join(__dirname, '..');

function rustcHostTriple() {
    const out = execSync('rustc -vV', { encoding: 'utf8' });
    const m = out.match(/^host:\s*(\S+)/m);
    if (!m) throw new Error('Could not parse host triple');
    return m[1];
}

function main() {
    const triple = rustcHostTriple();
    const ext = process.platform === 'win32' ? '.exe' : '';
    const releaseDir = join(workspaceRoot, 'target', 'release');

    console.log('[release] Building deepseek-tui (release)…');
    execSync('cargo build --release -p deepseek-tui', { cwd: workspaceRoot, stdio: 'inherit' });

    const archiveName = `deepseek-tui-${triple}`;
    if (process.platform === 'win32') {
        const zipPath = `${archiveName}.zip`;
        execSync(
            `powershell -NoProfile -Command "Compress-Archive -Path '${releaseDir}\\deepseek-tui.exe' -DestinationPath '${zipPath}' -Force"`,
            { cwd: workspaceRoot, stdio: 'inherit' },
        );
        console.log(`[release] Done: ${zipPath}`);
    } else {
        const tarball = `${archiveName}.tar.gz`;
        execSync(
            `tar czf "${tarball}" -C "${releaseDir}" deepseek-tui`,
            { cwd: workspaceRoot, stdio: 'inherit' },
        );
        console.log(`[release] Done: ${tarball}`);
    }
}

main();
