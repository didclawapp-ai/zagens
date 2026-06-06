# 安装

Zagens 提供 **Windows x64** 安装包，托管在本站。当前构建尚未代码签名，Windows 可能弹出 SmartScreen 警告。

## 推荐步骤

1. 在[下载页](/zh-Hans/download)下载 `.exe` 或 `.zip`。
2. 若出现 SmartScreen，选择**更多信息 → 仍要运行**（详见[常见问题](/zh-Hans/docs/faq#smartscreen)）。
3. 运行安装程序，从开始菜单启动 Zagens。

## 校验下载（可选）

从下载页复制 SHA-256，在 PowerShell 中对比：

```powershell
Get-FileHash .\Zagens_*_x64-setup.exe -Algorithm SHA256
```

## 系统要求

- Windows 10 1903+ 或 Windows 11
- 64 位 (x64) CPU
- 可访问 DeepSeek API 的网络

完整图文步骤见[安装指引](/zh-Hans/install)页面。
