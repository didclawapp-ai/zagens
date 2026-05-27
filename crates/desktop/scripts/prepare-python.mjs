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
    readFileSync,
    statSync,
    unlinkSync,
    rmSync,
    createWriteStream,
} from 'node:fs';
import { join, dirname, basename } from 'node:path';
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
    "reportlab==4.2.5",
    "matplotlib==3.9.3",
    "numpy==2.1.3",
    "Pillow>=10.0.0",
];

/** Bump when PIP_DEPS change so CI/dev rebuilds wheels into the installer bundle. */
const DEPS_LOCK_ID = "office-py-v2-reportlab";

function depsMarkerUpToDate(markerPath) {
    if (!existsSync(markerPath)) return false;
    try {
        return readFileSync(markerPath, "utf8").trim() === DEPS_LOCK_ID;
    } catch {
        return false;
    }
}

function formatBytes(n) {
    if (n >= 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    if (n >= 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${n} B`;
}

async function fetchArchiveSize(url) {
    const resp = await fetch(url, { method: "HEAD" });
    if (!resp.ok) {
        throw new Error(`Failed to probe PBS archive: ${resp.status} ${resp.statusText}`);
    }
    const len = Number(resp.headers.get("content-length"));
    return Number.isFinite(len) && len > 0 ? len : null;
}

function archiveSizeOk(archivePath, expectedBytes) {
    if (!existsSync(archivePath)) return false;
    const size = statSync(archivePath).size;
    if (expectedBytes == null) return size > 0;
    return size === expectedBytes;
}

async function downloadPbsArchive(url, archivePath, expectedBytes) {
    console.log(
        `[python] downloading PBS ${PBS_VERSION}…` +
        (expectedBytes ? ` (${formatBytes(expectedBytes)})` : ""),
    );
    const resp = await fetch(url);
    if (!resp.ok) {
        throw new Error(`Failed to download PBS: ${resp.status} ${resp.statusText}`);
    }

    mkdirSync(dirname(archivePath), { recursive: true });
    const writer = createWriteStream(archivePath);
    let downloaded = 0;
    let lastLogPct = -1;

    const body = Readable.fromWeb(resp.body);
    body.on("data", (chunk) => {
        downloaded += chunk.length;
        if (expectedBytes == null) return;
        const pct = Math.floor((downloaded / expectedBytes) * 100);
        if (pct >= lastLogPct + 10 || pct === 100) {
            console.log(
                `[python] download ${pct}% (${formatBytes(downloaded)} / ${formatBytes(expectedBytes)})`,
            );
            lastLogPct = pct;
        }
    });

    await pipeline(body, writer);

    if (expectedBytes != null && downloaded !== expectedBytes) {
        try { unlinkSync(archivePath); } catch { /* best-effort */ }
        throw new Error(
            `PBS download incomplete (${formatBytes(downloaded)} / ${formatBytes(expectedBytes)}). Retry bundle:prepare.`,
        );
    }

    console.log(`[python] downloaded ${basename(archivePath)} (${formatBytes(downloaded)})`);
}

function installPipDeps(pyExe, pyDir) {
    console.log(`[python] installing pip dependencies (${DEPS_LOCK_ID})…`);
    const pipArgs = [
        "-m", "pip", "install",
        "--no-cache-dir", "--disable-pip-version-check", "--quiet",
        ...PIP_DEPS,
    ];
    execSync(`"${pyExe}" ${pipArgs.join(" ")}`, {
        stdio: "inherit",
        env: { ...process.env, PYTHONUNBUFFERED: "1" },
    });

    const verifyScript = [
        "import pptx, docx, reportlab, matplotlib, PIL",
        "matplotlib.use('Agg')",
        "import matplotlib.pyplot as plt",
        "plt.plot([1,2,3], [4,5,6])",
        `plt.savefig('${join(pyDir, "_verify.png").replace(/\\/g, "\\\\")}')`,
        'print("OK")',
    ].join("\n");
    execSync(`"${pyExe}" -c "${verifyScript}"`, { stdio: "inherit" });

    const depsMarker = join(pyDir, ".deps-installed");
    const escapedMarker = depsMarker.replace(/\\/g, "\\\\");
    execSync(
        `"${pyExe}" -c "open('${escapedMarker}','w').write('${DEPS_LOCK_ID}')"`,
        { stdio: "inherit" },
    );
}

export async function preparePythonRuntime(binariesDir, triple) {
    const info = PBS_ARCHIVE_MAP[triple];
    if (!info) {
        console.warn(`[python] No PBS archive for ${triple} — Python tools will need system install`);
        return;
    }

    const pyDir = join(binariesDir, "python-standalone");
    const pyExe = join(pyDir, "python-install", info.pythonExe);
    const depsMarker = join(pyDir, ".deps-installed");

    // Python tree exists but lock predates reportlab (or other PIP_DEPS edits).
    if (existsSync(pyExe) && !depsMarkerUpToDate(depsMarker)) {
        console.log(`[python] refreshing bundled office deps → ${DEPS_LOCK_ID}`);
        installPipDeps(pyExe, pyDir);
        console.log(`[python] runtime ready at ${pyDir}`);
        return;
    }

    if (depsMarkerUpToDate(depsMarker)) {
        console.log(`[python] runtime already prepared (${DEPS_LOCK_ID})`);
        return;
    }

    // 1. Download PBS archive (re-fetch if a prior run left a truncated file)
    const archiveName = `python-pbs-${triple}${info.ext}`;
    const archivePath = join(binariesDir, archiveName);
    const expectedBytes = await fetchArchiveSize(info.url);

    if (!archiveSizeOk(archivePath, expectedBytes)) {
        if (existsSync(archivePath)) {
            const got = statSync(archivePath).size;
            console.warn(
                `[python] removing incomplete PBS archive (${formatBytes(got)}` +
                (expectedBytes ? ` / ${formatBytes(expectedBytes)}` : "") +
                `)`,
            );
            unlinkSync(archivePath);
        }
        if (existsSync(pyDir)) {
            rmSync(pyDir, { recursive: true, force: true });
        }
        await downloadPbsArchive(info.url, archivePath, expectedBytes);
    }

    // 2. Extract — requires `tar` (built-in on Win10 1803+, macOS, Linux)
    if (!existsSync(pyExe)) {
        console.log(`[python] extracting PBS…`);
        mkdirSync(pyDir, { recursive: true });
        if (info.ext === ".tar.gz") {
            try {
                execSync(`tar -xzf "${archivePath}" -C "${pyDir}"`, { stdio: "inherit" });
            } catch (err) {
                rmSync(pyDir, { recursive: true, force: true });
                try { unlinkSync(archivePath); } catch { /* best-effort */ }
                throw new Error(
                    "PBS extract failed (archive may be corrupt). Removed partial files — rerun bundle:prepare.",
                    { cause: err },
                );
            }
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

    // 3. pip install + verify (offline-capable installer bundle)
    installPipDeps(pyExe, pyDir);

    // 4. Clean up archive (keep extracted files only)
    try {
        if (existsSync(archivePath)) {
            unlinkSync(archivePath);
        }
    } catch {
        /* best-effort */
    }

    console.log(`[python] runtime ready at ${pyDir}`);
}
