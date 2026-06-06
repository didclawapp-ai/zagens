# 仓库拆分：产品本体 vs 官网平台

Zagens 采用**双仓库**结构，为后续开源本体 + 闭源平台服务做准备。

| 仓库 | 地址 | 许可 | 职责 |
|------|------|------|------|
| **产品本体** | [jjlin0603-svg/zagens](https://github.com/jjlin0603-svg/zagens)（本仓库） | 计划 MIT | Desktop、Runtime、Harness、`docs/user/` |
| **官网平台** | [jjlin0603-svg/zagens_website](https://github.com/jjlin0603-svg/zagens_website) | 专有 | 官网 SPA、CMS、下载托管、未来计费/账号 |

## 目录映射

| 原 monorepo 路径 | 现归属 |
|------------------|--------|
| `crates/`、`docs/harness/`、`docs/desktop/` | 产品仓 |
| `docs/user/` | 产品仓（用户文档 SSOT） |
| 原 `website/`、`docs/website/`、`scripts/website/` | 官网仓（**不在产品仓**） |

## 本地开发

### 产品（本仓库）

```bash
cd crates/desktop
cargo tauri dev
```

### 官网

```bash
git clone https://github.com/jjlin0603-svg/zagens_website.git
cd zagens_website
npm run sync:docs -- --from ../zagens/docs/user
npm run dev
```

## 发版流程

1. 产品仓打 tag `zagens-v*` → [`.github/workflows/release.yml`](../.github/workflows/release.yml) 构建 Windows 安装包并发布 GitHub Release。
2. Release workflow 通过 `repository_dispatch` 触发官网仓 [`sync-release.yml`](https://github.com/jjlin0603-svg/zagens_website/blob/main/.github/workflows/sync-release.yml)。
3. 官网仓下载 Release assets → 生成 `latest.json` → rsync 到 VPS `/download/` → 触发 `deploy.yml`。

### 所需 GitHub Secrets

**产品仓 (`zagens`)**

| Secret | 用途 |
|--------|------|
| `TAURI_SIGNING_PRIVATE_KEY` | 签名安装包 |
| `WEBSITE_REPO_DISPATCH_TOKEN` | PAT，对 `zagens_website` 有 `actions:write` |

**官网仓 (`zagens_website`)**

| Secret | 用途 |
|--------|------|
| `WEBSITE_DEPLOY_*` | VPS rsync 部署 |
| `PRODUCT_REPO_READ_TOKEN` | 私有产品仓时拉取 `docs/user` 与 Release assets |

## 用户文档维护

- **编辑位置：** 本仓库 `docs/user/{en,zh-Hans}/`
- **官网展示：** 官网 CI 执行 `npm run sync:docs` 复制到 `content/docs/`
- **导航：** `docs/user/<locale>/_nav.json`

## 迁移说明

产品仓**不包含**任何 `website/` 路径；官网代码与运维文档均在 [zagens_website](https://github.com/jjlin0603-svg/zagens_website)。
