---
name: desktop-draft-test
description: 桌面手测
---

# 桌面手测流程

1. **确认环境** — 检查 `crates/desktop/` 目录结构是否完整，确认 Tauri 侧车配置就绪。
2. **构建并启动** — 在 `crates/desktop/web-ui` 执行 `npm run build`，然后 `cargo build -p desktop` 编译 Rust 侧，启动桌面应用。
3. **验证输出** — 确认 `deliverables/` 下生成了至少一个输出文件，退出即完成。
