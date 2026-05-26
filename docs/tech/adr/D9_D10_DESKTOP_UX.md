# D9 + D10 — 取消两层契约 & 多窗口 SSE 过滤

**Status:** Landed (phase B — 2026-05-26)  
**Related:** [ARCHITECTURE_ASSESSMENT_2026-05-25.md](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) §5.1 阶段 B · [API_DESIGN.md](../API_DESIGN.md) §2.1.1–2.1.2

## Context

- **D9:** Stop / Escape 曾只调 HTTP interrupt 或只 abort 本地流，两层语义混用；`runtime_cancel_sse` 与 `POST …/interrupt` 职责未在 API 文档中单列。
- **D10:** 多 Agent 窗口可 `register_window_thread` 抢同一 thread；非 owner 窗口仍消费 live SSE，出现「幽灵渲染」（delta / tool / approval 出现在错误窗口）。

## Decision

### D9 — Two-layer stop

1. 新增 `web-ui/src/api/turnControl.ts`：
   - `disconnectThreadEventStream` — Layer 1（`AbortSignal` + `runtime_cancel_sse`）
   - `stopThreadTurn` — 用户 Stop：Layer 2 HTTP interrupt，再 Layer 1（409 忽略）
2. `App.tsx` `handleCancelStream` 统一走 `stopThreadTurn`。
3. [API_DESIGN.md](../API_DESIGN.md) **§2.1.1** 为 SSOT；交叉引用 [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) §8。

### D10 — Thread owner SSE filter

1. `windowBridge.ts`：`windowOwnsThreadForStream` + 250ms TTL 缓存；`registerWindowThread` 成功后 `markThreadRegisteredLocally`。
2. `client.ts`：`filterThreadStreamEvents` + `threadIdFromSseEvent`。
3. `App.tsx` live SSE 管道（`postStreamTurn` / `pollThreadTurnEvents`）经 owner 过滤；`approval_required` 仍用 `threadOwnedByWindow`（现委托同一缓存路径）。

## Acceptance

- [x] Stop / Escape → `stopThreadTurn`（interrupt + disconnect）
- [x] API_DESIGN §2.1.1 / §2.1.2 文档化
- [x] 非 owner 窗口忽略 live SSE delta（`filterThreadStreamEvents`）
- [ ] 手工多窗口 E2E（抢 thread + 并行 streaming）— 维护者 sign-off

## Non-goals

- 不改 runtime broadcast 语义
- 不对历史 `replayThreadEvents`（加载会话）加 owner 过滤
- §1 勾选数不变（体验债；进度仍 **7/10**）
