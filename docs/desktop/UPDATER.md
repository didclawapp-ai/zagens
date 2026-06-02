# Zagens 应用内更新（Tauri Updater）

桌面端通过 [Tauri updater 插件](https://v2.tauri.app/plugin/updater/) 从官网清单检查并安装更新。

| 配置 | 位置 |
|------|------|
| 清单 URL | `https://zagens.com/download/latest.json` |
| 公钥 | `crates/desktop/tauri.conf.json` → `plugins.updater.pubkey`（与 `updater.key.pub` 一致） |
| 关于页 | 检查更新 / 下载并安装 |
| 启动提示 | 有新版本时 toast（`useDesktopShell`） |

## 发布前（维护者）

### 1. 签名密钥（一次性）

```powershell
cd crates/desktop
cargo tauri signer generate --ci -p "" -w updater.key -f
```

- **提交：** `updater.key.pub`、`tauri.conf.json` 中的 `pubkey` 字符串  
- **勿提交：** `updater.key`（已在根 `.gitignore`）  
- **CI：** 将 `updater.key` 全文存入 GitHub Secret `TAURI_SIGNING_PRIVATE_KEY`（或 `TAURI_SIGNING_PRIVATE_KEY_PATH`）

### 2. 签名构建 Windows 安装包

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw crates/desktop/updater.key
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""   # 无密码密钥必填，否则会卡在解密提示
cd crates/desktop
cargo tauri build
npm run package:release
```

产物含 `target/release/bundle/nsis/*-setup.exe` 与同名的 `*.sig`（`createUpdaterArtifacts: true`）。

### 3. 同步官网清单

将安装包与 `.sig` 放入 `website/public/download/`，然后：

```bash
cd website
# 可选：UPDATER_SIGNATURE="$(cat public/download/Zagens_x64-setup.exe.sig)"
npm run sync:manifest
npm run build
```

`sync-download-manifest.mjs` 会：

- `release.json` — 下载页 zip/exe + SHA-256  
- `latest.json` — OTA：`url` 指向 **setup.exe**（非 zip），`signature` 为 `.sig` 文件全文  

推 `main` 后 [website.yml](../../.github/workflows/website.yml) 部署到 VPS。

### 4. 打 Git 标签（可选 GitHub Release）

```bash
git tag zagens-v0.6.0-preview.2
git push origin zagens-v0.6.0-preview.2
```

[release.yml](../../.github/workflows/release.yml) 在设置 `TAURI_SIGNING_PRIVATE_KEY` 后会产出已签名的 NSIS 包。

## 本地测试 OTA

1. 本机安装 **较低版本**（例如 `preview.1`）。  
2. 在 `website/public/download/` 放置更高版本的 **已签名** `*-setup.exe` + `.sig`，并更新 `latest.json`（`version` 更高、`signature` 非空）。  
3. `cd website && npm run preview`，临时把 `tauri.conf.json` 的 endpoint 改为 `http://127.0.0.1:4321/download/latest.json`（仅 dev；生产必须 HTTPS）。  
4. 打开 Zagens → **关于** → **检查更新** → **下载并安装**。  

未签名或 `signature` 为空时，检查会返回明确错误（见 `update.rs` 的 `humanize_update_error`）。

## 与「官网 zip 下载」的关系

| 渠道 | 包 | 用途 |
|------|-----|------|
| 官网 zip | `*-setup.exe.zip` | 推荐首次安装（Unblock zip，减 SmartScreen） |
| OTA | `*-setup.exe` + minisign | 应用内增量升级 |

两者版本号应对齐；仅 bump 预览线 patch 时递增 `0.x.y-preview.N` 与 WiX 第四段（见 [VERSIONING.md](VERSIONING.md)）。
