# Zagens 官网部署（私有仓库 → VPS）

官网源码在 `website/`。CI 在 **`main` 或 `master` 分支**且 `website/**` 有变更时构建静态文件，经 **SSH + rsync** 同步到服务器（默认规划 IP：`43.160.233.39`）。

工作流：[`.github/workflows/website.yml`](../../.github/workflows/website.yml)。

## 一、哪些内容不要推到远程

根目录 [`.gitignore`](../../.gitignore) 与 [`website/.gitignore`](../../website/.gitignore) 已覆盖常见产物。推送前可用：

```powershell
git status
git ls-files --others --exclude-standard
```

| 类别 | 路径 / 模式 | 说明 |
|------|-------------|------|
| Rust 构建 | `/target`, `*.pdb`, `*.exe` 等 | 本地编译产物 |
| Node | `node_modules/`, `website/dist/`, `website/.astro/` | 安装依赖与 Astro 构建缓存 |
| 密钥与环境 | `.env`, `.env.*`, `~/.deepseek/` 类 | **切勿**提交 API Key |
| 本地运行时 | `**/session_*.json`, `*.db`, `.deepseek/` | 会话与本地 DB |
| 评测输出 | `results/lht-eval/`, `results/lht-harness/`, `outputs/`, `tmp/` | Harness 跑出来的结果 |
| 内部材料 | `.private/`, `docs/*.pdf`, `apps/` | 贸易秘密 / 大文件 / 独立子应用 |
| 助手临时 | `.codex/`, `.context/`, `CLAUDE.md`, `AI_HANDOFF.md` 等 | 仅本机协作 |
| 桌面打包生成 | `crates/desktop/bundle-legal/` | 发布脚本生成 |

**应提交**的 `website/` 内容：源码（`src/`）、`public/`（含 `latest.json` 基线）、`package.json` / `package-lock.json`、配置与 `scripts/sync-download-manifest.mjs`。**不要**提交 `dist/`、`node_modules/`、`.astro/`。

## 二、服务器首次初始化（43.160.233.39）

以下以 **Ubuntu 22.04+** 为例；其他发行版请对照包名调整。

### 1. 登录并安装 Nginx

```bash
sudo apt update
sudo apt install -y nginx rsync
sudo systemctl enable nginx
```

### 2. 站点目录与专用部署用户

```bash
sudo useradd -m -s /bin/bash -d /home/zagens-deploy zagens-deploy
sudo mkdir -p /var/www/zagens
sudo chown zagens-deploy:zagens-deploy /var/www/zagens
sudo chmod 755 /var/www/zagens
```

Nginx 需要读静态文件，将目录组设为 `www-data` 并赋予组读：

```bash
sudo usermod -aG www-data zagens-deploy
sudo chgrp -R www-data /var/www/zagens
sudo chmod -R g+rX /var/www/zagens
```

### 3. 为 GitHub Actions 配置 SSH 密钥

在**本机**生成仅用于部署的密钥（不要复用个人密钥）：

```bash
ssh-keygen -t ed25519 -f ./zagens-website-deploy -N "" -C "github-actions-website"
```

- 公钥 `zagens-website-deploy.pub` → 服务器 `/home/zagens-deploy/.ssh/authorized_keys`
- 私钥 `zagens-website-deploy` → GitHub 仓库 Secret `WEBSITE_DEPLOY_KEY`（完整 PEM 内容）

服务器上：

```bash
sudo -u zagens-deploy mkdir -m 700 /home/zagens-deploy/.ssh
sudo -u zagens-deploy nano /home/zagens-deploy/.ssh/authorized_keys   # 粘贴公钥一行
sudo chmod 600 /home/zagens-deploy/.ssh/authorized_keys
```

验证（在本机）：

```bash
ssh -i ./zagens-website-deploy zagens-deploy@43.160.233.39 "echo ok"
```

### 4. Nginx 站点配置

复制仓库示例并按域名修改：

[`website/deploy/nginx-zagens.conf.example`](../../website/deploy/nginx-zagens.conf.example)

```bash
sudo cp nginx-zagens.conf.example /etc/nginx/sites-available/zagens
sudo ln -sf /etc/nginx/sites-available/zagens /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

### 5. 防火墙与 HTTPS（上线前）

```bash
sudo ufw allow OpenSSH
sudo ufw allow 'Nginx Full'
sudo ufw enable
```

域名 `zagens.com` 解析到该 IP 后，建议用 **Certbot** 申请 TLS：

```bash
sudo apt install -y certbot python3-certbot-nginx
sudo certbot --nginx -d zagens.com -d www.zagens.com
```

`website/astro.config.mjs` 中 `site: 'https://zagens.com'` 与桌面端 updater URL 依赖 HTTPS 与稳定路径，请在 DNS + 证书就绪后再对外宣传。

## 三、GitHub 私有仓库配置

在仓库 **Settings → Secrets and variables → Actions** 添加：

| Secret | 示例值 |
|--------|--------|
| `WEBSITE_DEPLOY_HOST` | `43.160.233.39` |
| `WEBSITE_DEPLOY_USER` | `zagens-deploy` |
| `WEBSITE_DEPLOY_KEY` | 私钥全文（`-----BEGIN ...`） |
| `WEBSITE_DEPLOY_PATH` | `/var/www/zagens` |
| `WEBSITE_DEPLOY_PORT` | （可选）`22` |

首次部署可手动触发：**Actions → Website → Run workflow**。

## 四、本地与发布流程

```bash
cd website
npm ci
npm run build
# 本地预览
npm run preview
```

发版后更新下载清单（需 `gh` 与 Release）：

```bash
GITHUB_REPO=owner/repo RELEASE_TAG=zagens-v0.6.0-preview.1 npm run sync:manifest
npm run build
git add public/download/latest.json src/data/release.json
```

再推 `main`，CI 会重新构建并 rsync 到 VPS。

## 五、与旧 GitHub Pages 工作流的区别

当前工作流**不再**使用 `deploy-pages`；静态文件只落在你控制的 VPS 上。若曾启用 GitHub Pages，请在仓库 Settings 中关闭，避免与自有 Nginx 混淆。
