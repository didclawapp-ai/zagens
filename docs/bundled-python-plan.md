# 零依赖 Python 运行时打包方案

> 目标：用户安装 DS Pick / `deepseek` 后，`write_office` 和 PPTX 引擎立即可用，无需安装 Python 或任何 pip 包。

---

## 1. 核心方案：python-build-standalone + 预装依赖

使用 [python-build-standalone](https://github.com/astral-sh/python-build-standalone) (PBS)，一个可再发行的自包含 Python 发行版。

**为什么不用 venv + `pip install`：**
- PBS 的 `site-packages` 对内部 Python 可写，不需要单独的 venv
- 所有依赖在构建时通过 `pip install --target` 注入，运行时无需网络
- 单目录结构，relocatable（移动目录不影响 import）

---

## 2. 构建时准备（新增 `prepare-python.mjs`）

### 2.1 执行时机

`prepare-bundle.mjs` 中新增一步，在 `bundle:prepare` 时调用：

```js
// prepare-bundle.mjs 末尾新增
import { preparePythonRuntime } from './prepare-python.mjs';
await preparePythonRuntime(binariesDir, triple);
```

### 2.2 `prepare-python.mjs` 脚本

```js
import { execSync } from 'node:child_process';
import { existsSync, mkdirSync, renameSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { createWriteStream } from 'node:fs';
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
        pythonExe: "python3.12",
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

    // 2. Extract
    if (!existsSync(pyDir)) {
        console.log(`[python] extracting PBS…`);
        mkdirSync(pyDir, { recursive: true });
        if (info.ext === ".tar.gz") {
            // Requires `tar` command (built-in on Win10 1803+, macOS, Linux).
            // CI environments with older Windows may need 7-Zip fallback.
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
    const verifyScript = `
import pptx, docx, matplotlib, PIL
matplotlib.use('Agg')
import matplotlib.pyplot as plt
plt.plot([1,2,3], [4,5,6])
plt.savefig('${join(pyDir, "_verify.png").replace(/\\/g, "\\\\")}')
print("OK")
`.trim();
    execSync(`"${pyExe}" -c "${verifyScript}"`, { stdio: "inherit" });

    // 5. Write marker
    execSync(`echo "installed" > "${depsMarker}"`, { stdio: "inherit" });

    // 6. Clean up archive (keep extracted files only)
    try { execSync(`rm "${archivePath}"`); } catch {}

    console.log(`[python] runtime ready at ${pyDir}`);
}
```

---

## 3. Tauri 打包配置

### 3.1 `tauri.conf.json` 新增 resources

```json
{
  "bundle": {
    "resources": {
      "binaries/python-standalone/python-install/**": "python/"
    }
  }
}
```

安装后的目录结构：

```
DS Pick.app/Contents/Resources/  (macOS)
  ├── python/
  │   ├── python3              ← PBS 解释器
  │   ├── lib/
  │   │   └── python3.12/
  │   │       └── site-packages/
  │   │           ├── pptx/
  │   │           ├── docx/
  │   │           ├── matplotlib/
  │   │           └── numpy/
  │   └── ...
  └── deepseek-tui             ← sidecar
```

### 3.2 `build.rs` 无需改动

PBS 下载由 `prepare-bundle.mjs` → `prepare-python.mjs` 完成，`build.rs` 只验证 `externalBin`，不校验 `resources`。

---

## 4. 运行时集成：`python_env.rs` 改造

### 4.1 新增 `find_bundled_python()`

```rust
/// Try to locate a bundled Python runtime shipped alongside the binary.
///
/// DS Pick (Tauri):
///   <app_dir>/Resources/python/python3  (macOS/Linux)
///   <app_dir>/python/python.exe         (Windows)
///
/// CLI/TUI:
///   <exe_dir>/python-standalone/python-install/python(.exe)
///
/// Returns the path to the python executable, or None.
pub fn find_bundled_python() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();

    let candidates = [
        // Tauri resource path on macOS
        exe_dir.join("../../Resources/python").join(python_bin_name()),
        // Tauri resource path on Windows/Linux
        exe_dir.join("python").join(python_bin_name()),
        // Local dev / CLI sidecar
        exe_dir.join("python-standalone/python-install").join(python_bin_name()),
    ];

    for path in candidates {
        if path.is_file() {
            // Verify it works
            if crate::python_env::probe_python(
                &path.to_string_lossy(),
                &[],
            ).is_some()
            {
                return Some(path);
            }
        }
    }
    None
}

fn python_bin_name() -> &'static str {
    #[cfg(windows)] { "python.exe" }
    #[cfg(target_os = "macos")] { "python3.12" }  // PBS macOS ships as "python3.12"
    #[cfg(all(unix, not(target_os = "macos")))] { "python3" }
}
```

### 4.2 改造 `ensure_office_venv()`

```rust
pub fn ensure_office_venv() -> Result<PathBuf, String> {
    // ── Path 1: Bundled Python (zero-dependency) ──
    if let Some(bundled_py) = find_bundled_python() {
        // Dependencies are already in site-packages of the bundled Python.
        // No venv creation or pip install needed.
        return Ok(bundled_py);
    }

    // ── Path 2: System Python (fallback) ──
    // ... existing venv creation logic unchanged ...
}
```

### 4.3 Python 脚本部署

`install_embedded_scripts` 保持不变——脚本通过 `include_str!` 落盘到 `~/.deepseek/office-py/scripts/`，与用哪个 Python 无关。

---

## 5. CLI/TUI 独立分发策略

对于非 Tauri 场景（`deepseek` / `deepseek-tui` 独立二进制）：

| 分发方式 | Python 策略 |
|---------|------------|
| `cargo install` | 无 Python — 走 `find_python()` PATH 扫描（现有行为） |
| 正式发布 `.tar.gz` / `.zip` | 包含 `python-standalone/` 目录，与 `deepseek-tui` 二进制同层级 |
| 首次下载 | 如果二进制同目录无 `python-standalone/`，尝试 `find_python()`；失败时输出友好安装指引 |

### 5.1 独立分发打包脚本（新增 `scripts/package-release.mjs`）

```js
import { execSync } from 'node:child_process';
import { copySync, existsSync, mkdirSync } from 'fs-extra'; // or manual recursive copy
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { preparePythonRuntime } from '../crates/desktop/scripts/prepare-python.mjs';

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
    const pythonDir = join(releaseDir, 'python-standalone');
    await preparePythonRuntime(releaseDir, triple);

    // 3. Package
    const archiveName = `deepseek-tui-${triple}`;
    if (process.platform === 'win32') {
        execSync(`powershell Compress-Archive -Path "${releaseDir}\\deepseek-tui.exe","${releaseDir}\\python-standalone" -DestinationPath "${archiveName}.zip"`, { stdio: 'inherit' });
    } else {
        execSync(`tar czf "${archiveName}.tar.gz" -C "${releaseDir}" deepseek-tui python-standalone`, { stdio: 'inherit' });
    }
    console.log(`[release] Done: ${archiveName}`);
}

main().catch(e => { console.error(e); process.exit(1); });
```

---

## 6. 体积估算

| 组件 | 提取后 | 压缩后 (.tar.gz / NSIS) |
|------|--------|------------------------|
| PBS (Python 3.12 基础) | ~90 MB | ~30 MB |
| numpy + .libs/ (.dll/.dylib) | ~50 MB | ~15 MB |
| matplotlib + 字体 | ~30 MB | ~8 MB |
| python-pptx | ~2 MB | ~0.5 MB |
| python-docx | ~1 MB | ~0.3 MB |
| Pillow | ~5 MB | ~1.5 MB |
| **合计** | **~180 MB** | **~55 MB** |

> 实际体积：numpy 的 `.libs/` 目录含大量 `.dll`/`.dylib`（OpenBLAS 等），matplotlib 自带字体缓存，合计提取后可能接近 200 MB。压缩后约 55-60 MB。

对 DS Pick 安装包（当前约 50 MB），增量约 **55 MB**，总包体 ~105 MB——对桌面应用可接受。发布 notes 中需标注体积变化。

---

## 7. 改动清单

```
新建:
  crates/desktop/scripts/prepare-python.mjs      PBS 下载 + deps 预装 (~100 行)
  scripts/package-release.mjs                    CLI/TUI 独立分发打包脚本 (~50 行)

改动:
  crates/desktop/scripts/prepare-bundle.mjs      +1 行导入 preparePythonRuntime()
  crates/desktop/tauri.conf.json                 + "resources": { "binaries/python-standalone/python-install/**": "python/" }
  crates/tui/src/python_env.rs                   新增 find_bundled_python() (~40 行)
  crates/tui/src/python_env.rs                   改造 ensure_office_venv() (~5 行，加 bundle 优先分支)
  crates/tui/src/python_env.rs                   OFFICE_REQUIREMENTS 标注：build-time only (bundle 路径); runtime fallback 仍使用
  crates/desktop/binaries/.gitignore             新增 python-standalone/ 排除
```

---

## 8. 回退保证

如果 bundled Python 不可用（构建时跳过了 PBS 下载、或用户从源码 `cargo build`）：

1. `find_bundled_python()` → `None`
2. `ensure_office_venv()` 走原有路径：`find_python()` → `python3 -m venv` → `pip install`
3. 与当前行为完全一致，零破坏

---

## 9. 实施步骤

1. **先行：** 本地跑通 `prepare-python.mjs`，验证 PBS 下载 + pip install + `import pptx; import matplotlib` 通过
2. **Rust 侧：** 加 `find_bundled_python()` + 改造 `ensure_office_venv()`
3. **Tauri 打包：** 加 `tauri.conf.json` resources 映射，`tauri build` 全平台验证
4. **CI：** 构建矩阵确保 3 平台都能正确打包 Python
5. **测试：** 解压安装包后 `write_office("pptx", ...)` 零报错
