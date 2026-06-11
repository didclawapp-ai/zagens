# 本地开发校验（Rust 工具链 + Lint）

本文档说明 Zagens 产品仓如何在**本地**复现 CI 的 Lint 门槛，避免「本地能编过、推上去才红」。

**相关文件：**

| 路径 | 作用 |
|------|------|
| [`rust-toolchain.toml`](rust-toolchain.toml) | 开发/CI 钉死的 Rust 版本（当前 **1.96.0**） |
| [`scripts/ci/verify-lint.sh`](scripts/ci/verify-lint.sh) | 镜像 CI **Lint** job（bash） |
| [`scripts/ci/verify-lint.ps1`](scripts/ci/verify-lint.ps1) | 同上（PowerShell / Windows） |
| [`scripts/ci/verify-lint-linux.ps1`](scripts/ci/verify-lint-linux.ps1) | 在 WSL/Docker 里跑 Linux 版 Lint（**仅手动**，覆盖 `#[cfg(unix)]` 盲区） |
| [`scripts/ci/verify-workspace.sh`](scripts/ci/verify-workspace.sh) | Lint + 全 workspace 测试 + lockfile 漂移 |
| [`scripts/ci/install-git-hooks.sh`](scripts/ci/install-git-hooks.sh) | 安装 pre-commit / pre-push |
| [`.github/workflows/ci.yml`](.github/workflows/ci.yml) | 远程 CI 定义 |

根 `Cargo.toml` 里的 `rust-version = "1.88"` 是 **MSRV**（下游兼容下限）；**贡献者与 CI 实际使用** `rust-toolchain.toml` 中的版本。

---

## 1. 要不要每次手动跑脚本？

**不必。** 校验分三层，按场景自动或手动触发：

| 层级 | 何时运行 | 检查内容 | 你需要做什么 |
|------|----------|----------|--------------|
| **日常编译** | `cargo build` / `cargo test` | 能编过、测试过 | 正常开发即可 |
| **Git hooks** | `git commit` / `git push` | commit：fmt；push：按 [`ci-push-gate.sh`](scripts/ci/ci-push-gate.sh) 决定是否跑 `verify-lint` | 克隆后执行一次 `install-git-hooks` |
| **手动脚本** | 大改前、发版前、没装 hook 的机器 | 同 CI 或更严（含 test） | 主动跑 `verify-lint` / `verify-workspace` |

**结论：** 装好 hook 后，`git push` 会自动跑 Lint 镜像；**不需要**每次推送前记得敲脚本。手动脚本用于提前发现问题，或 hook 未安装的环境。

---

## 2. 为什么没绑在 `cargo build` 上？

全 workspace 的 `cargo clippy … -D warnings` 通常需要 **几十秒到数分钟**，而开发时 `cargo build` 一天可能跑很多次。常见分工是：

- **迭代：** `cargo build` / `cargo test -p <crate>` — 快
- **提交：** `cargo fmt --check` — 快（pre-commit）
- **推送 / CI：** 完整 clippy — 严

若希望「一条命令 = lint + 构建」，用下面的 **`verify-workspace`** 或自行在发版脚本前加 `verify-lint`，而不是改默认 `cargo build` 行为。

---

## 3. 首次环境准备

### 3.1 安装钉死的工具链

进入仓库根目录后，`rustup` 会读取 `rust-toolchain.toml`：

```bash
cd /path/to/zagens
rustup toolchain install 1.96.0 --component clippy --component rustfmt
rustc --version   # 应显示 1.96.0
```

### 3.2 安装 Git hooks（推荐）

**Linux / macOS / Git Bash：**

```bash
bash scripts/ci/install-git-hooks.sh
```

**Windows（PowerShell）：**

```powershell
pwsh scripts/ci/install-git-hooks.ps1
```

安装后：

- **pre-commit** → `cargo fmt --all -- --check`
- **pre-push** → [`ci-push-gate.sh`](scripts/ci/ci-push-gate.sh) 判断；仅代码变更时跑 `verify-lint`（与 CI Lint 一致）

远程 CI 仅在 **Pull Request** 与 **发版标签** 上运行；合并到 `master` 不会重复跑 Actions。本地 pre-push 仍用 gate 决定是否跑 lint。详见 [CONTRIBUTING.md — CI when you push](CONTRIBUTING.md#ci-when-you-push-pr-first)。

紧急跳过（不推荐）：`SKIP_VERIFY=1 git push`

### 3.3 Desktop / Tauri 额外依赖

`verify-lint` 会通过 [`ensure-web-ui-dist`](scripts/ci/ensure-web-ui-dist.sh) 在缺少 `crates/desktop/web-ui/dist` 时自动 `npm ci && npm run build`（desktop 的 clippy 需要 `frontendDist`）。首次可能较慢，之后 dist 存在则跳过。

---

## 4. 脚本说明

### 4.1 `verify-lint` — 镜像 CI Lint

与 [`.github/workflows/ci.yml`](.github/workflows/ci.yml) 中 **Lint** job 对齐：

1. 断言 `rustc` 版本与 `rust-toolchain.toml` 一致  
2. 确保 `web-ui/dist` 存在（必要时构建）  
3. 预构建 `zagens-cli`（`crates/desktop/build.rs` 编译期需要 sidecar 二进制）  
4. `cargo fmt --all -- --check`  
5. `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`  

```bash
bash scripts/ci/verify-lint.sh
# Windows:
pwsh scripts/ci/verify-lint.ps1
```

### 4.2 `verify-workspace` — 发版前全量门控

在 `verify-lint` 之后追加：

- `cargo test --workspace --all-features --locked`
- 检查 `Cargo.lock` 未被测试过程意外改写

```bash
bash scripts/ci/verify-workspace.sh
pwsh scripts/ci/verify-workspace.ps1
```

### 4.3 `verify-lint-linux` — 跨平台 cfg 盲区（仅手动）

> **何时用：** 改动了带 `#[cfg(unix)]` / `#[cfg(target_os = "…")]` 等条件编译的代码（典型为 `crates/desktop/src/` 下的 `disk_guard.rs`、`sidecar.rs`、`terminal.rs` 等），且你在 **Windows** 上开发时。

**为什么需要它：** clippy/rustc 只分析**当前目标平台实际会编译的代码**。被 `#[cfg(...)]` 门控的其它平台分支会被**整段跳过**，根本不参与 Lint。所以在 Windows 上 `verify-lint.ps1` 跑过，并不代表 Linux 分支没问题——CI 的 Lint job 跑在 `ubuntu-latest`，会编译 unix 分支，于是「本地过、推上去 Linux 才红」。

本脚本在**真实 Linux 环境**里跑与 CI 完全相同的 [`verify-lint.sh`](scripts/ci/verify-lint.sh)，把这层盲区在推送前补上。**它是手动触发的**——不绑定到 `git push` hook，因为完整 Linux 构建较慢，只在你确实改了跨平台代码时按需运行。

```powershell
# 自动选择引擎：有 WSL 用 WSL（快、复用增量缓存），否则用 Docker（隔离、与 CI 一致）
pwsh scripts/ci/verify-lint-linux.ps1

# 首次使用：装齐 Linux 依赖（apt 系统库 + Node 20 + 钉死的 rust 工具链）
pwsh scripts/ci/verify-lint-linux.ps1 -Bootstrap

# 强制指定引擎
pwsh scripts/ci/verify-lint-linux.ps1 -Engine wsl
pwsh scripts/ci/verify-lint-linux.ps1 -Engine docker
```

- **WSL**：需要 Ubuntu/Debian 发行版与 `rustup`（工具链按 `rust-toolchain.toml` 自动安装）；首次用 `-Bootstrap` 补 apt + Node 20。
- **Docker**：需 Docker Desktop 运行；镜像用 `rust:<pinned>-bookworm`，cargo registry 与 `target` 走命名卷缓存，重复运行更快，且不污染 Windows 的 `target/`。

> 提示：发版前 CI 会在 tag 上跑完整 Lint + Test 矩阵（见 `cd.yml` 的 `workflow_run` 门禁）；本脚本只是把 Linux clippy 盲区提前到推送前。

### 4.4 与 CI 的对应关系

| CI Job | 本地等价 |
|--------|----------|
| Lint（fmt + clippy，**当前 OS**） | `verify-lint` 或 pre-push hook |
| Lint（**Linux 分支 / `cfg(unix)`**） | `verify-lint-linux`（仅手动；Windows 开发改了跨平台代码时） |
| Test（三平台 test） | `verify-workspace`（仅当前 OS；完整矩阵仍在 CI；**CI 上需 Lint 通过后才启动**） |
| Version drift | `bash scripts/release/check-versions.sh` |

---

## 5. 推荐工作流

```mermaid
flowchart LR
  dev["日常改代码\ncargo build / test"]
  commit["git commit\npre-commit: fmt"]
  push["git push\npre-push: verify-lint"]
  ci["GitHub Actions\nLint + Test"]
  dev --> commit --> push --> ci
```

1. **平时：** `cargo build`、`cargo test -p …` 快速迭代。  
2. **提交：** hook 自动 fmt 检查；失败则 `cargo fmt --all` 后重试。  
3. **推送：** hook 自动 `verify-lint`；通过后再到 GitHub。  
4. **大功能 / 合并前：** 主动跑 `verify-workspace`。  
5. **改 OpenAPI / 发版：** 另见 CI 中的 OpenAPI drift 步骤与 [`scripts/export-runtime-openapi.ps1`](scripts/export-runtime-openapi.ps1)。发版打 `zagens-v*` tag 后，CI 全绿会自动触发 [`.github/workflows/cd.yml`](.github/workflows/cd.yml) 构建安装包并同步官网。

---

## 6. 常见问题

### 本地 1.94，CI 1.96，为什么之前会挂？

CI 曾使用浮动 `stable`，与本地工具链漂移时 Clippy 规则不一致。现已用 `rust-toolchain.toml` 钉死 **1.96.0**，本地与 CI 应对齐。

### `cargo build` 过了，push 仍失败？

`build` 不跑 clippy。装 hook 或手动 `verify-lint` 可提前暴露 `-D warnings` 问题。

### 本地 Windows 的 `verify-lint` 过了，CI 在 Linux 才报 clippy 错？

这是**条件编译（`cfg`）的固有盲区**，不是配置 bug：clippy 只检查当前平台会编译的代码，`#[cfg(unix)]` 等其它平台分支在 Windows 上被整段跳过。改了这类跨平台代码时，推送前手动跑 `verify-lint-linux`（见 §4.3，WSL/Docker）即可在本地复现 CI 的 Linux Lint。反向同理：只在 `#[cfg(windows)]` 用到的符号，在 Linux 上会被判为「未使用」。

### Windows 上 pre-push 找不到 bash？

Hook 会回退到 `pwsh scripts/ci/verify-lint.ps1`。若两者都不可用，请手动跑 PowerShell 脚本。

### 官网仓也要装这些吗？

**否。** 本文档与脚本仅适用于 **Zagens 产品仓**（本仓库）。官网在独立仓 `zagens_website`，不参与此 Rust workspace Lint。

### `zagens-desktop` / Tauri 报 `frontendDist` 不存在

先构建 web-ui，或让 `verify-lint` 自动构建：

```bash
cd crates/desktop/web-ui && npm ci && npm run build
```

### Hook 未生效

确认已运行 `install-git-hooks` 且 `.git/hooks/pre-commit` / `pre-push` 存在并可执行。

---

## 7. 历史背景（2026-06）

主仓大推送后 CI 连续暴露：fmt 漂移、Unix 缺 `libc`、Rust 1.96 clippy 新规则等。随后引入工具链钉死 + 本地 `verify-lint` + pre-push hook，使推送前与 CI Lint 对齐。

此后又遇到一类「Windows 本地过、Linux CI 才红」的问题：`crates/desktop` 里 `#[cfg(unix)]` 分支的 clippy 错误（`needless_return`、`unnecessary_cast`、`collapsible_if`）以及 `cfg(windows)`-only 符号在 Linux 上的「未使用导入」。根因是 clippy 只检查当前平台编译的代码。为补这层盲区，新增手动脚本 [`verify-lint-linux.ps1`](scripts/ci/verify-lint-linux.ps1)（见 §4.3），并把发布流程加了 ubuntu 上的 `verify` 门禁。详见 [`CHANGELOG.md`](CHANGELOG.md) `[Unreleased]` 条目。

---

## 8. 维护说明

- 升级钉死版本：改 [`rust-toolchain.toml`](rust-toolchain.toml)，并在 CI lint job 的 toolchain 断言（`rustc --version | grep 1.96.0`）中同步。`verify-lint-linux.ps1` 与 `cd.yml` 构建 job 也用 `rust-toolchain.toml` 的版本，无需单独改。  
- 新增 Lint 步骤：先改 `verify-lint.sh` / `.ps1`，再改 `.github/workflows/ci.yml`，最后更新本文档；跨平台 `cfg` 代码记得用 `verify-lint-linux.ps1` 自查。  
- AI / 贡献者速查：[`project_rules.md`](project_rules.md)、[`.cursor/rules/rust-workspace.mdc`](.cursor/rules/rust-workspace.mdc)。

---

*文档版本：与 Zagens 产品仓 `rust-toolchain.toml` 1.96 线同步（2026-06）。*
