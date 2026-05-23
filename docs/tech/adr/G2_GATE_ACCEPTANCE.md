# G2 门控验收记录（A+ / §12.2）

> **日期：** 2026-05-23  
> **执行：** Cursor agent（本仓库 `cargo test`，无需 release 打包）  
> **SSOT：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §12.2、§8.3

## 验收结论（自动化部分）

| # | 标准 | 状态 | 证据 |
|---|------|------|------|
| 1 | `event_schema_version` 代码字段 | ✅ | `CURRENT_EVENT_SCHEMA_VERSION`（`runtime_threads/mod.rs`）；`GET /health` 返回 `event_schema_version`；SSE `runtime_event_payload` 含同名字段 |
| 2 | Sidecar 契约测 CI 绿 | ✅ | `sidecar_contract_full_lifecycle`（`runtime_api/tests.rs`）；`.github/workflows/ci.yml` |
| 3 | 桌面全链路冒烟 | ⏸ 人工 | 见下方「维护者待办」 |
| 4 | A+.7 审批回归 | ✅ | `resolve_approval_sends_decision_to_engine_when_auto_approve_off`（approve → turn completed）；`resolve_approval_deny_sends_denial_to_engine`；`resolve_approval_rejects_invalid_decision` |
| — | A5.5 回放 10–20 步 | ✅ | `tests/fixtures/runtime_turn_replay.jsonl`（15 事件）+ `runtime_turn_replay_fixture_covers_full_turn_lifecycle` |
| — | A5.5 最小回放 | ✅ | `runtime_turn_minimal.jsonl` + `runtime_turn_minimal_fixture_*` |
| — | 稳定 SSE 子集 v1 映射 | ✅ | `runtime_api/stream.rs` 单测（每文档化 `event:` 一条） |

## 验证命令与结果（2026-05-23 本机）

```bash
cargo test -p deepseek-tui --lib
# ok. 2339 passed; 0 failed; 2 ignored

cargo test -p deepseek-tui --test runtime_event_replay_fixture
# ok. 2 passed (minimal + 15-step replay)

cargo test -p deepseek-tui --lib sidecar_contract_full_lifecycle --all-features --locked
# CI 同款；含于 --lib 全量
```

## PR5 后续（G2 之后）

| 项 | 状态 |
|----|------|
| sidecar 双 thread 并行 turn 测 | ✅ `sidecar_parallel_turns_on_two_threads` |
| `core::Runtime::handle_thread(Message)` 真 turn | ⏸ app-server 路径仍 `queued` |
| DS Pick 多窗口手测 | ⏸ 维护者 ~15min |

---

## 仍未满足（非本自动化范围）

| 门 | 项 | 说明 |
|----|-----|------|
| **G3** | §11.0 ADR 维护者签收 | 需维护者书面确认 |
| **G1** | §12.1 A 完成线 | 长跑/R-015 基线、A1–A3 全量等 |
| **§12.3** | P2 完成线 | `Engine` 仍在 tui；多窗口抽样 |
| **§12.2 #3** | DS Pick 全链路 | 本机 `tauri dev` + sidecar 一轮对话 |

## 维护者待办（可选，~15 分钟）

1. 启动 sidecar + DS Pick：一条需审批的 tool → resolve → turn 完成。  
2. 在本文或 `P2_MIGRATION_SPIKE.md` 勾选 G3。  
3. 决议 §12.3 #1：`Engine` L2 终态 vs 继续迁入 core。
