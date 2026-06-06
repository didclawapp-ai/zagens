# 应用内更新

Zagens 从 **zagens.com** 检查已签名的 Windows 构建，可在**关于**页安装更新。

## 检查更新

1. 侧栏打开**关于**。
2. 点击**检查更新**。
3. 若有新版本，选择**下载并安装**。
4. 按提示重启。

启动时也可能 Toast 提示有新版本。

## 更新清单

应用读取 `https://zagens.com/download/latest.json`（版本、下载 URL、签名）。与[下载页](/zh-Hans/download)手动下的 **zip** 不同 — OTA 使用已签名的 `.exe`。

## 首次安装 vs 升级

| 渠道 | 包 | 用途 |
|------|-----|------|
| 官网 zip | `*-setup.exe.zip` | 首次安装（解压后减 SmartScreen 干扰） |
| 应用内 OTA | 已签名 `*-setup.exe` | 已有安装的升级 |

## 排错

- **签名校验失败** — 改从官网下载安装；见[常见问题](/zh-Hans/docs/faq#smartscreen)。
- **已是最新** — 对照[下载页](/zh-Hans/download)版本号。
- 企业代理 — 需能 HTTPS 访问 `zagens.com`。

相关：[安装](/zh-Hans/docs/install) · [系统托盘](/zh-Hans/docs/desktop/tray)
