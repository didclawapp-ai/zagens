---
name: desktop-draft-test
description: 桌面手测
---

# 桌面手测流程

1. **确认环境** — 检查 `crates/desktop/` 目录结构是否完整，确认 Tauri 侧车配置就绪。
2. **构建并启动** — 在 `crates/desktop/web-ui` 执行 `npm run build`，然后 `cargo build -p desktop` 编译 Rust 侧，启动桌面应用。
3. **验证输出** — 确认 `deliverables/` 下生成了至少一个输出文件，退出即完成。

---

## 内置浏览器（Browser pane）

对照维护者 plan：`doc_Private/docs/desktop/BUILTIN_BROWSER_PLAN.md` §8。  
**主路径回归：** 2026-07-17 **通过** — 样本 `.zagens/deepseek-thread-thr_829e.json`（`thr_829e426d`）。

| # | 项 | 期望 |
|---|-----|------|
| 1 | 先打开 Browser 视图 | 未开时 `browser_*` → `browser_host_missing` + hint（预期，非失败） |
| 2 | 回环导航 | `browser_navigate` → `http://127.0.0.1:…` 可见页 |
| 3 | 结构化观测 | `browser_snapshot` 含 a11y + `role:slug:nth` ref；`browser_get_text` 中文可读 |
| 4 | 写路径 | `browser_type` / `click` / `scroll` 按 ref 返回 `ok:true`（无 JSON.parse SyntaxError） |
| 5 | 等待 | `browser_wait`（text / ref / load）`ok:true` |
| 6 | Preview | `.zagens/preview.json` + `browser_start_preview`（或人手起服）后可导航 |

边角（未纳入 thr_829e 通过判据）：聊天「应用内打开」、外站 ask / `nav_blocked`、关 `browser_yolo` 审批、多窗 candidates、B 侧栏 resize/焦点。

---

## 交错时间线（Streaming Timeline）

对照 `doc_Private/docs/desktop/STREAMING_TIMELINE_UX_PLAN.md`。样本：`.zagens/deepseek-thread-thr_0c02.json`、`thr_2be0`。

| # | 项 | 期望 |
|---|-----|------|
| 1 | Live：Thought → Tool → Text **交错**增长 | DOM 为 `AssistantTurnFrame` blocks，非三桶分区 |
| 2 | 长 turn（>20 tool） | 滚动跟随 active；Step 分组 / 紧凑工具行可读 |
| 3 | Turn 完成 | thinking/tool 默认折叠，正文展开 |
| 4 | 切会话再切回 | block 顺序保持 |
| 5 | 停止生成 | interrupted 落在 block `status` |
| 6 | 复制 reasoning / tools | 内容完整（`blocksToLegacyFields`） |
| 7 | 多窗口 | 非 owner 无 ghost（D10） |
| 8 | 无重复 prose | Step 标题 ≠ 卡片正文双显 |
| 9 | 刷新 / 冷加载 | 回放顺序与 live 一致；仅有 items、无 events 时出现「推理未持久化」提示 |
| 10 | Fork / 回溯 | 新线程 transcript 仍为交错 `blocks` |
| 11 | 去碎片化 `thr_662d` / `thr_8da4` / `thr_9409` | 短旁白不刷屏；explore/shell/write/plan 可折叠；推理与长用户提示限高 |
