# DS Pick 开发笔记

零散想法、后续方向与非正式排期；需要落地时再拆 issue / 写入 IMPLEMENTATION_STEPS。

---

## 2026-05-09 — 会话持久化与崩溃恢复（后续大块）

**背景：** 当前桌面端对 `~/.deepseek/sessions/*.json` 的更新大致在 **turn 完成** 后通过 `persist-session` 写入；进程异常退出、WebView 整页重载或最后一轮未结束时，UI 与磁盘快照可能脱节，表现为「上文对模型不可见」或侧栏历史不完整。

**后续可做方向（另一块开发）：**

1. **周期性 / 流式 checkpoint 写 session** — 在流式生成过程中按间隔或关键事件增量持久化（需权衡 IO、与 runtime 导出的一致性、以及与现有 `turn.completed` 语义的关系）。
2. **崩溃恢复时从 runtime JSONL 尽力回填 UI** — 侧写事件已落在 `RuntimeThreadStore`（threads/turns/items/events）；重连后可尝试用服务端事件重放或合并，补全尚未写入 session 文件的进行中部份（需产品定义「可恢复边界」与冲突处理）。

---

*（有新条目时按日期追加在本文件顶部或本条之后。）*
