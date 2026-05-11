/**
 * Downloads python-build-standalone (PBS) and pre-installs office + matplotlib deps.
 *
 * Called by `prepare-bundle.mjs` during `npm run bundle:prepare`.
 * The resulting `binaries/python-standalone/python-install/` tree is
 * bundled as Tauri resources (mapped to `python/` in the app bundle).
 *
 * Requires `tar` command (built-in on Win10 1803+, macOS, Linux).
 * CI environments with older Windows may need 7-Zip fallback.
 */

import { execSync } from 'node:child_process';
import {
    existsSync,
    mkdirSync,
    renameSync,
    readdirSync,
    unlinkSync,
    createWriteStream,
} from 'node:fs';
import { join } from 'node:path';
import { pipeline } from 'node:stream/promises';
import { Readable } from 'node:stream';

const PBS_VERSION = "20251014";  // pin a known-good release — Python 3.12.12

const PBS_ARCHIVE_MAP = {
    "x86_64-pc-windows-msvc": {
        url: `https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_VERSION}/cpython-3.12.12+${PBS_VERSION}-x86_64-pc-windows-msvc-install_only.tar.gz`,
        ext: ".tar.gz",
        pythonExe: "python.exe",
    },
    "x86_64-apple-darwin": {
        url: `https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_VERSION}/cpython-3.12.12+${PBS_VERSION}-x86_64-apple-darwin-install_only.tar.gz`,
        ext: ".tar.gz",
        pythonExe: "python3.12",  // macOS PBS ships as "python3.12"
    },
    "aarch64-apple-darwin": {
        url: `https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_VERSION}/cpython-3.12.12+${PBS_VERSION}-aarch64-apple-darwin-install_only.tar.gz`,
        ext: ".tar.gz",
        pythonExe: "python3.12",
    },
    "x86_64-unknown-linux-gnu": {
        url: `https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_VERSION}/cpython-3.12.12+${PBS_VERSION}-x86_64-unknown-linux-gnu-install_only.tar.gz`,
        ext: ".tar.gz",
        pythonExe: "python3",
    },
    "aarch64-unknown-linux-gnu": {
        url: `https://github.com/astral-sh/python-build-standalone/releases/download/${PBS_VERSION}/cpython-3.12.12+${PBS_VERSION}-aarch64-unknown-linux-gnu-install_only.tar.gz`,
        ext: ".tar.gz",
        pythonExe: "python3",
    },
};

const PIP_DEPS = [
    "python-pptx==1.0.2",
    "python-docx==1.1.2",
    "matplotlib==3.9.3",
    "numpy==2.1.3",
    "Pillow>=10.0.0",
];

export async function preparePythonRuntime(binariesDir, triple) {
    const info = PBS_ARCHIVE_MAP[triple];
    if (!info) {
        console.warn(`[python] No PBS archive for ${triple} — Python tools will need system install`);
        return;
    }

    const pyDir = join(binariesDir, "python-standalone");
    const pyExe = join(pyDir, "python-install", info.pythonExe);
    const depsMarker = join(pyDir, ".deps-installed");

    // Already prepared?
    if (existsSync(depsMarker)) {
        console.log("[python] runtime already prepared");
        return;
    }

    // 1. Download PBS archive
    const archiveName = `python-pbs-${triple}${info.ext}`;
    const archivePath = join(binariesDir, archiveName);

    if (!existsSync(archivePath)) {
        console.log(`[python] downloading PBS ${PBS_VERSION} for ${triple}…`);
        const resp = await fetch(info.url);
        if (!resp.ok) {
            throw new Error(`Failed to download PBS: ${resp.status} ${resp.statusText}`);
        }
        mkdirSync(binariesDir, { recursive: true });
        const writer = createWriteStream(archivePath);
        await pipeline(Readable.fromWeb(resp.body), writer);
        console.log(`[python] downloaded ${archiveName}`);
    }

    // 2. Extract — requires `tar` (built-in on Win10 1803+, macOS, Linux)
    if (!existsSync(pyDir)) {
        console.log(`[python] extracting PBS…`);
        mkdirSync(pyDir, { recursive: true });
        if (info.ext === ".tar.gz") {
            execSync(`tar -xzf "${archivePath}" -C "${pyDir}"`, { stdio: "inherit" });
        }
    }

    // Verify python — PBS extracts to a versioned subdir
    if (!existsSync(pyExe)) {
        const dirs = readdirSync(pyDir, { withFileTypes: true })
            .filter(d => d.isDirectory() && d.name.startsWith('python'));
        const installDir = dirs.find(d => existsSync(join(pyDir, d.name, info.pythonExe)));
        if (!installDir) {
            throw new Error(`Cannot locate python in extracted PBS at ${pyDir}`);
        }
        const srcDir = join(pyDir, installDir.name);
        const dstDir = join(pyDir, "python-install");
        renameSync(srcDir, dstDir);
    }

    // 3. pip install deps
    console.log(`[python] installing pip dependencies…`);
    const pipArgs = [
        "-m", "pip", "install",
        "--no-cache-dir", "--disable-pip-version-check", "--quiet",
        ...PIP_DEPS,
    ];
    execSync(`"${pyExe}" ${pipArgs.join(" ")}`, {
        stdio: "inherit",
        env: { ...process.env, PYTHONUNBUFFERED: "1" },
    });

    // 4. Verify critical imports and matplotlib native rendering
    const verifyScript = [
        'import pptx, docx, matplotlib, PIL',
        "matplotlib.use('Agg')",
        'import matplotlib.pyplot as plt',
        'plt.plot([1,2,3], [4,5,6])',
        `plt.savefig('${join(pyDir, "_verify.png").replace(/\\/g, "\\\\")}')`,
        'print("OK")',
    ].join('\n');
    execSync(`"${pyExe}" -c "${verifyScript}"`, { stdio: "inherit" });

    // 5. Write marker
    const escapedMarker = depsMarker.replace(/\\/g, "\\\\");
    execSync(`"${pyExe}" -c "open('${escapedMarker}','w').write('installed')"`, { stdio: "inherit" });

    // 6. Clean up archive (keep extracted files only)
    try {
        if (existsSync(archivePath)) {
            unlinkSync(archivePath);
        }
    } catch {
        /* best-effort */
    }

    console.log(`[python] runtime ready at ${pyDir}`);
}
