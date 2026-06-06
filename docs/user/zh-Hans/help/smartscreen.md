# SmartScreen 与安全安装

Windows 安装包**尚未 Authenticode 签名**。SmartScreen 可能提示 **「Windows 已保护你的电脑」** — 这是未签名发布者的常见情况，**不是**恶意软件判定。

## 推荐：zip + 解除锁定

1. 从[下载页](/zh-Hans/download)获取 `Zagens_<版本>_x64-setup.exe.zip`。
2. **右键 zip → 属性 → 勾选「解除锁定」→ 确定。**
3. 解压 zip。
4. 运行其中的 `*-setup.exe`。

**先解锁 zip 再解压**，可避免安装器携带 MOTW，往往**不再弹 SmartScreen**。

## 备选：仍要运行

若直接运行安装器：

1. 蓝屏 SmartScreen → **更多信息**
2. **仍要运行**

## 校验完整性

运行前对照[下载页](/zh-Hans/download) SHA-256。

## 应用内更新

OTA 在可用时使用签名构建；见[应用内更新](/zh-Hans/docs/desktop/updates)。首次安装仍推荐官网 zip 路径。

相关：[安装](/zh-Hans/docs/install) · [常见问题](/zh-Hans/docs/faq#smartscreen)
