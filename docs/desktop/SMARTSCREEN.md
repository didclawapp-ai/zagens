# Windows SmartScreen & 安装指引 / Install Guide

Zagens 当前**尚未做代码签名**(EV/OV 证书或云签名待后续)。未签名的 Windows
安装包在通过浏览器下载后会被打上"网络来源标记"(Mark of the Web, MOTW),
首次运行可能弹出蓝色的 **"Windows 已保护你的电脑 / Windows protected your PC"**
提示。这是**正常现象,不代表软件有问题**。本页给出 0 成本的安全安装方式。

> 这不是病毒告警。SmartScreen 只是因为这个文件还没积累足够的"发布者信誉"。
> This is **not** a virus warning — SmartScreen only flags the file because it
> has not yet accrued publisher reputation (it is unsigned).

---

## 推荐方式:下载 zip → 先"解除锁定"→ 再解压安装

**Recommended: download the zip, Unblock it first, then extract & install.**

我们把完整安装器放进了 `Zagens_<版本>_x64-setup.exe.zip`。在 zip 这一层
**解锁一次**,解压出来的安装器就不带网络标记,安装全程**不会弹 SmartScreen**。

1. 下载 `Zagens_<version>_x64-setup.exe.zip`。
   Download `Zagens_<version>_x64-setup.exe.zip`.
2. **右键这个 zip → 属性(Properties)→ 勾选底部「解除锁定 / Unblock」→ 确定。**
   Right-click the zip → **Properties** → tick **"Unblock"** at the bottom → OK.
3. 解压 zip(右键 → 全部解压)。
   Extract the zip (right-click → Extract All).
4. 双击里面的 `Zagens_<version>_x64-setup.exe` 安装。
   Double-click the `*-setup.exe` inside to install.

> 重要:**一定要先在第 2 步解锁 zip,再解压**。如果直接解压,Windows 自带的
> 解压器会把网络标记传染给里面的安装器,安装时仍会弹窗。
> Important: unblock the zip **before** extracting — Windows Explorer otherwise
> propagates the Mark of the Web onto the extracted installer.

---

## 备选:直接运行安装器并手动放行

**Alternative: run the installer and click through the prompt.**

如果你直接下载了 `Zagens_<version>_x64-setup.exe`:

1. 双击运行,出现 "Windows 已保护你的电脑" 蓝框。
2. 点击 **「更多信息 / More info」**。
3. 点击 **「仍要运行 / Run anyway」**。

---

## 校验文件完整性(可选但推荐)

**Verify the download (optional but recommended).**

每个下载文件都附带一个 `.sha256` 校验文件。在 PowerShell 中核对:

```powershell
# 计算你下载文件的哈希
Get-FileHash .\Zagens_<version>_x64-setup.exe.zip -Algorithm SHA256

# 与 .sha256 文件中的值比对(应完全一致)
Get-Content .\Zagens_<version>_x64-setup.exe.zip.sha256
```

哈希一致即说明文件未被篡改、下载完整。
A matching hash confirms the file is intact and untampered.

---

## 系统要求 / Requirements

- **Windows 10 / 11 (x64)。**
- **WebView2 运行时**:Win11 默认自带;Win10 若缺失,安装器会**自动静默下载安装**
  (需要安装时有网络)。WebView2 runtime auto-installs during setup if missing.
- Zagens 自带运行时 sidecar,**无需额外安装** Python 等依赖。
  The runtime sidecar is bundled — no separate Python/runtime install needed.

---

## 为什么不签名? / Why unsigned?

代码签名证书(尤其能立即免警告的 EV 证书)费用较高。在尚未签名阶段,我们先以
"zip 解锁 + 校验值"的零成本方式发布;后续计划接入 **Microsoft Store** 或
**Azure Trusted Signing** 以彻底消除该提示。

Code-signing certificates are costly. As an early preview we ship with the
zero-cost "unblock + checksum" flow; **Microsoft Store** distribution or
**Azure Trusted Signing** are planned to remove the prompt entirely.
