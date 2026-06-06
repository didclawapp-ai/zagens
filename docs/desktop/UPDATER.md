# Zagens 应用内更新（Tauri Updater）

桌面端通过 [Tauri updater 插件](https://v2.tauri.app/plugin/updater/) 从官网清单检查并安装更新。

| 配置 | 位置 |
|------|------|
| 清单 URL | `https://zagens.com/download/latest.json` |
| 公钥 | `crates/desktop/tauri.conf.json` → `plugins.updater.pubkey`（与 `updater.key.pub` 一致） |
| 关于页 | 检查更新 / 下载并安装 |
| 启动提示 | 有新版本时 toast（`useDesktopShell`） |

官网发版与 OTA 清单由私有仓库 [zagens_website](https://github.com/jjlin0603-svg/zagens_website) 维护。见 [REPO_SPLIT.md](../REPO_SPLIT.md)。

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

### 3. 发布 tag（推荐）

```bash
git tag zagens-v0.7.0
git push origin zagens-v0.7.0
```

CI [`.github/workflows/release.yml`](../../.github/workflows/release.yml) 会：

1. 构建并上传 GitHub Release 附件  
2. `repository_dispatch` 触发官网仓 `sync-release.yml` 更新 `zagens.com/download/` 与 `latest.json`

### 4. 本地同步官网（可选）

若需在本机验证 OTA，克隆官网仓并在产品构建后执行：

```bash
cd zagens_website
npm run release:local -- --bundle-dir ../zagens/crates/desktop/target/release/bundle/nsis
npm run build
```

## 本地验证 OTA

1. 构建并签名较低版本的安装包，安装运行。  
2. 在官网仓 `frontend/public/download/` 放置更高版本的 **已签名** `*-setup.exe` + `.sig`，运行 `npm run release:from-artifacts -- --artifact-dir <dir>` 或 `npm run release:local`。  
3. `cd frontend && npm run dev`，临时把 `tauri.conf.json` 的 endpoint 改为 `http://127.0.0.1:5173/download/latest.json`（仅 dev；生产必须 HTTPS）。

## 故障排查

- **签名失败** — 检查 `TAURI_SIGNING_PRIVATE_KEY` 与密码环境变量。  
- **No update found** — `latest.json` 中 `version` 须高于已安装版本；`signature` 须与 `.sig` 一致。  
- **下载 404** — 确认官网仓 `sync-release` 已 rsync 到 VPS `/var/www/zagens/download/`。
