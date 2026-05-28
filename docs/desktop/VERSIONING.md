# Zagens 版本与发布渠道

Zagens 桌面端（`crates/desktop/`）使用**独立 SemVer**，与根 `Cargo.toml` 中嵌入式 runtime workspace 版本（如 `0.8.15`）无关。

**SSOT：** 本文件 + [`CHANGELOG.md`](../../CHANGELOG.md) 头部说明。

---

## 1. 版本格式

### 1.1 稳定段（pre-1.0）

在 **1.0.0** 之前，主版本固定为 **0**：

```text
0.MINOR.PATCH[-<prerelease>][+<build>]
```

示例：`0.5.0`、`0.6.0`。

### 1.2 预发布（对外发布默认）

公开发布、官网下载、GitHub Release 在达到 **1.0.0 GA** 之前，使用 **SemVer 预发布标识**：

```text
0.MINOR.PATCH-<channel>.<N>
```

| 字段 | 约定 |
|------|------|
| `<channel>` | 默认 **`preview`**（产品文案：**预览版** / Early Access）。可选 `beta`、`alpha`（更早期、更小范围；不用于当前主线）。 |
| `<N>` | 从 **1** 起的整数；同一 `0.MINOR.PATCH` 基线上仅修 bug / 重打包时递增（`preview.1` → `preview.2`）。 |

**当前示例：** `0.6.0-preview.1`

**显示：** UI / README 可写 `v0.6.0-preview.1`（前缀 `v` 仅展示用，写入 manifest 时不带 `v`）。

### 1.3 何时 bump 哪一段

| 变更 | 版本动作 |
|------|----------|
| 同一次预览线的 bugfix / 安装包重发 | `0.6.0-preview.1` → `0.6.0-preview.2` |
| 新一批用户可见功能（仍非 GA） | `0.6.0-preview.x` → `0.7.0-preview.1`（升 **MINOR**） |
| 破坏性配置或 `/v1` 行为变更（pre-1.0 仍可能发生） | 升 **MINOR** 或 **PATCH**，并在 CHANGELOG 标明 |
| 对外承诺 GA、Updater/支持策略就绪 | `1.0.0`（去掉 `-preview`） |

**1.0.0** 保留给「正式版」：行为与支持预期冻结，不再默认带 `-preview`。

### 1.4 Windows MSI（WiX）版本映射

WiX/MSI 只接受纯数字 `major.minor.patch[.build]`，**不能**直接使用 `-preview.N` 等非数字预发布标识。Tauri 会在未覆盖时从 `"version"` 推导并失败。

在 [`tauri.conf.json`](../../crates/desktop/tauri.conf.json) 中单独设置 `bundle.windows.wix.version`：

| SemVer（对外 / UI） | MSI `wix.version` |
|---------------------|-------------------|
| `0.6.0-preview.1` | `0.6.0.1` |
| `0.6.0-preview.2` | `0.6.0.2` |
| `1.0.0`（GA，无后缀） | `1.0.0` |

规则：`0.M.P-<channel>.N` → `0.M.P.N`（第四段为预发布序号）。NSIS（`-setup.exe`）仍使用完整 SemVer，不受此限制。

---

## 2. 必须同步的文件

发布或 bump 版本时，**四处一致**（CI：`scripts/release/check-versions.sh`）：

| 文件 | 字段 |
|------|------|
| [`crates/desktop/Cargo.toml`](../../crates/desktop/Cargo.toml) | `version = "…"` |
| [`crates/desktop/tauri.conf.json`](../../crates/desktop/tauri.conf.json) | `"version"` |
| [`crates/desktop/tauri.conf.json`](../../crates/desktop/tauri.conf.json) | `bundle.windows.wix.version`（仅 Windows MSI，见 §1.4） |
| [`crates/desktop/web-ui/package.json`](../../crates/desktop/web-ui/package.json) | `"version"` |
| [`crates/desktop/web-ui/src/components/AboutPanel.tsx`](../../crates/desktop/web-ui/src/components/AboutPanel.tsx) | `APP_VERSION` |

然后：

1. `cd crates/desktop/web-ui && npm install`（刷新 `package-lock.json` 顶层 version，若需）
2. [`CHANGELOG.md`](../../CHANGELOG.md)：将 `[Unreleased]` 归档为 `## [0.6.0-preview.1] - YYYY-MM-DD`
3. Git 标签：`zagens-v0.6.0-preview.1`（见 §3）

---

## 3. Git 标签与 GitHub Release

- **标签格式：** `zagens-v<semver>`，例如 `zagens-v0.6.0-preview.1`
- **遗留：** `ds-pick-v*` 仍触发 [`.github/workflows/release.yml`](../../.github/workflows/release.yml)，新发布请用 `zagens-v*`
- **GitHub Release：** 版本号含 `-preview`、`-beta`、`-alpha` 时，workflow 将 `prerelease: true`

```bash
git tag zagens-v0.6.0-preview.1
git push origin zagens-v0.6.0-preview.1
```

---

## 4. CHANGELOG 标题

与 [Keep a Changelog](https://keepachangelog.com/) 一致，预发布版本作为独立章节：

```markdown
## [0.6.0-preview.1] - 2026-05-28
```

`[Unreleased]` 保留给尚未打 tag 的变更。

---

## 5. 与 runtime workspace 的关系

| 产品线 | 版本线 | 示例 |
|--------|--------|------|
| **Zagens 桌面** | 独立 SemVer + 预发布 | `0.6.0-preview.1` |
| **嵌入式 runtime crates** | 根 `[workspace.package] version` | `0.8.15` |

不要在 Zagens 发布说明中混用两条版本线；对外以 **Zagens `0.x-preview.n`** 为准。

---

## 6. 发布渠道用语（对外）

| SemVer 后缀 | 英文 | 中文（推荐） |
|-------------|------|----------------|
| `-preview.N` | Preview / Early Access | **预览版** |
| `-beta.N` | Beta | 公测版 |
| `-alpha.N` | Alpha | 内测版 |
| （无后缀，≥1.0.0） | Stable / GA | **正式版** |

当前主线：**预览版** → `0.x.y-preview.n`。
