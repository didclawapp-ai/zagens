# 本地开发校验（Rust 工具链 + Lint）

本文档说明 Zagens 产品仓如何在**本地**复现 CI 的 Lint 门槛，避免「本地能编过、推上去才红」。

**相关文件：**

| 路径 | 作用 |
|------|------|
| [`rust-toolchain.toml`](../rust-toolchain.toml) | 开发/CI 钉死的 Rust 版本（当前 **1.96.0**） |
| [`scripts/ci/verify-lint.sh`](../scripts/ci/verify-lint.sh) | 镜像 CI **Lint** job（bash） |
| [`scripts/ci/verify-lint.ps1`](../scripts/ci/verify-lint.ps1) | 同上（PowerShell / Windows） |
| [`scripts/ci/verify-workspace.sh`](../scripts/ci/verify-workspace.sh) | Lint + 全 workspace 测试 + lockfile 漂移 |
| [`scripts/ci/install-git-hooks.sh`](../scripts/ci/install-git-hooks.sh) | 安装 pre-commit / pre-push |
| [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) | 远程 CI 定义 |

根 `Cargo.toml` 里的 `rust-version = "1.88"` 是 **MSRV**（下游兼容下限）；**贡献者与 CI 实际使用** `rust-toolchain.toml` 中的版本。

---

## 1. 要不要每次手动跑脚本？

**不必。** 校验分三层，按场景自动或手动触发：

| 层级 | 何时运行 | 检查内容 | 你需要做什么 |
|------|----------|----------|--------------|
| **日常编译** | `cargo build` / `cargo test` | 能编过、测试过 | 正常开发即可 |
| **Git hooks** | `git commit` / `git push` | commit：fmt；push：完整 `verify-lint` | 克隆后执行一次 `install-git-hooks` |
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
- **pre-push** → `scripts/ci/verify-lint`（与 CI Lint 一致）

紧急跳过（不推荐）：`SKIP_VERIFY=1 git push`

### 3.3 Desktop / Tauri 额外依赖

`verify-lint` 会通过 [`ensure-web-ui-dist`](../scripts/ci/ensure-web-ui-dist.sh) 在缺少 `crates/desktop/web-ui/dist` 时自动 `npm ci && npm run build`（desktop 的 clippy 需要 `frontendDist`）。首次可能较慢，之后 dist 存在则跳过。

---

## 4. 脚本说明

### 4.1 `verify-lint` — 镜像 CI Lint

与 [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) 中 **Lint** job 对齐：

1. 断言 `rustc` 版本与 `rust-toolchain.toml` 一致  
2. 确保 `web-ui/dist` 存在（必要时构建）  
3. 预构建 `deepseek-runtime-server`（`crates/desktop/build.rs` 编译期需要 sidecar 二进制）  
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

### 4.3 与 CI 的对应关系

| CI Job | 本地等价 |
|--------|----------|
| Lint（fmt + clippy） | `verify-lint` 或 pre-push hook |
| Test（三平台 test） | `verify-workspace`（仅当前 OS；完整矩阵仍在 CI） |
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
5. **改 OpenAPI / 发版：** 另见 CI 中的 OpenAPI drift 步骤与 [`scripts/export-runtime-openapi.ps1`](../scripts/export-runtime-openapi.ps1)。

---

## 6. 常见问题

### 本地 1.94，CI 1.96，为什么之前会挂？

CI 曾使用浮动 `stable`，与本地工具链漂移时 Clippy 规则不一致。现已用 `rust-toolchain.toml` 钉死 **1.96.0**，本地与 CI 应对齐。

### `cargo build` 过了，push 仍失败？

`build` 不跑 clippy。装 hook 或手动 `verify-lint` 可提前暴露 `-D warnings` 问题。

### Windows 上 pre-push 找不到 bash？

Hook 会回退到 `pwsh scripts/ci/verify-lint.ps1`。若两者都不可用，请手动跑 PowerShell 脚本。

### 官网仓也要装这些吗？

**否。** 本文档与脚本仅适用于 **Zagens 产品仓**（本仓库）。官网在独立仓 `zagens_website`，不参与此 Rust workspace Lint。

### `deepseek-desktop` / Tauri 报 `frontendDist` 不存在

先构建 web-ui，或让 `verify-lint` 自动构建：

```bash
cd crates/desktop/web-ui && npm ci && npm run build
```

### Hook 未生效

确认已运行 `install-git-hooks` 且 `.git/hooks/pre-commit` / `pre-push` 存在并可执行。

---

## 7. 历史背景（2026-06）

主仓大推送后 CI 连续暴露：fmt 漂移、Unix 缺 `libc`、Rust 1.96 clippy 新规则等。随后引入工具链钉死 + 本地 `verify-lint` + pre-push hook，使推送前与 CI Lint 对齐。详见 [`CHANGELOG.md`](../CHANGELOG.md) `[Unreleased]` Tooling 条目。

---

## 8. 维护说明

- 升级钉死版本：改 [`rust-toolchain.toml`](../rust-toolchain.toml)，并在 CI lint job 的 toolchain 断言（`rustc --version | grep 1.96.0`）中同步。  
- 新增 Lint 步骤：先改 `verify-lint.sh` / `.ps1`，再改 `.github/workflows/ci.yml`，最后更新本文档。  
- AI / 贡献者速查：[`project_rules.md`](../project_rules.md)、[`.cursor/rules/rust-workspace.mdc`](../.cursor/rules/rust-workspace.mdc)。

---

*文档版本：与 Zagens 产品仓 `rust-toolchain.toml` 1.96 线同步（2026-06）。*
