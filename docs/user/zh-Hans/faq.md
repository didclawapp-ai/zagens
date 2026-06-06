# 常见问题

## 代码模式 vs 办公模式

| | **代码** | **办公** |
|---|----------|----------|
| 目标 | 仓库/工作区内工程任务 | DOCX/XLSX/PPTX 交付物 |
| Shell | 有（启用时） | 无 |
| 终端 | 有 | 无 |
| 典型产出 | 修改后的源码 | `deliverables/` 文档 |

新会话前切换任务类型。见[任务类型](/zh-Hans/docs/task-types)、[代码模式](/zh-Hans/docs/code-mode)与[办公概览](/zh-Hans/docs/office/overview)。

## Runtime 未连接 / 文档 API 500

官网文档 API 需前后端一起跑。在 [zagens_website](https://github.com/jjlin0603-svg/zagens_website) 仓库根目录执行 `npm run sync:docs` 后 `npm run dev`。仅跑 frontend 会连接失败。

桌面应用请查看侧栏**连接**状态 — runtime 侧车须在 localhost 运行。

## SmartScreen 与未签名安装包 {#smartscreen}

安装包尚未 Authenticode 签名。**推荐：** 下载 zip → 属性**解除锁定** → 解压安装，通常可避开 SmartScreen。

详见 [SmartScreen 安装指引](/zh-Hans/docs/help/smartscreen)。

## 更新

应用内更新读取 CDN 的 `latest.json`。见[应用内更新](/zh-Hans/docs/desktop/updates)与[下载页](/zh-Hans/download)。

## API Key 存储

密钥保存在本机。**Zagens 默认使用 `~/.zagens/config.toml`**；从旧版 deepseek-tui 迁移时也可能读取 `~/.deepseek/config.toml`。见[隐私摘要](/zh-Hans/docs/help/privacy)与[隐私政策](/zh-Hans/privacy)。

## 工作区与交付物

- **代码：** 工作区指向 git 仓库根。
- **办公：** 使用 `inbox/`、`data/`、`deliverables/` — [办公工作区](/zh-Hans/docs/office/workspace)。

## 支持

反馈请发邮件至 [didclawapp@gmail.com](mailto:didclawapp@gmail.com)。
