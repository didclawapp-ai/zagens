# G2 门控验收记录（A+ / §12.2）

> **日期：** 2026-05-23  
> **执行：** Cursor agent（本仓库 `cargo test`，无需 release 打包）  
> **SSOT：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §12.2、§8.3

## 验收结论（自动化部分）

| # | 标准 | 状态 | 证据 |
|---|------|------|------|
| 1 | `event_schema_version` 代码字段 | ✅ | `CURRENT_EVENT_SCHEMA_VERSION`（`runtime_threads/mod.rs`）；`GET /health` 返回 `event_schema_version`；SSE `runtime_event_payload` 含同名字段 |
| 2 | Sidecar 契约测 CI 绿 | ✅ | `sidecar_contract_full_lifecycle`（`runtime_api/tests.rs`）；`.github/workflows/ci.yml` |
| 3 | 桌面全链路冒烟 | ✅ 人工 | §1 单窗对话 + SSE（2026-05-23 维护者手测） |
| 4 | A+.7 审批回归 | ✅ 自动化 / ⏸ UI 手测 | 单测绿；**桌面弹窗手测暂缓**（`approval_policy` ↔ `auto_approve` 接线待修） |
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
| DS Pick 多窗口手测 | ✅ | §3 双窗并行 + 终端不串窗（2026-05-23） |
| §0.4 `/health` | ✅ | `event_schema_version: 2` |
| Stop 中途取消 | ✅ | §5.1 |

## 人工手测摘要（2026-05-23）

| 项 | 结果 |
|----|------|
| §0.4 `GET /health` → `event_schema_version: 2` | ✅ |
| §1 单窗全链路 | ✅ |
| §3 PR5 双窗并行 turn + T4 终端 | ✅ |
| §5.1 Stop | ✅ |
| §2 A+.7 审批弹窗 | ⏸ 暂缓（Auto 路径已验证；非 Auto 待产品接线后复测） |

---

## 仍未满足（非本自动化范围）

| 门 | 项 | 说明 |
|----|-----|------|
| **G3** | §11.0 ADR 维护者签收 | 需维护者书面确认 |
| **G1** | §12.1 A 完成线 | 长跑/R-015 基线、A1–A3 全量等 |
| **§12.3** | P2 完成线 | `Engine` 仍在 tui；多窗口抽样 |
| **§12.2 #3** | DS Pick 全链路 | ✅ 单窗（§1）；审批 UI 暂缓 |
| **§12.3 #2** | 多窗口抽样 | ✅ §3 手测（2026-05-23） |

## 维护者待办（剩余）

**手测清单（可打印/勾选）：** [G2_PR5_MANUAL_SMOKE_CHECKLIST.md](./G2_PR5_MANUAL_SMOKE_CHECKLIST.md)

1. **§2 审批 UI** — `approval_policy` ↔ Composer `auto_approve` 接线后复测。  
2. **G3** — §11.0 ADR 签收。  
3. **§12.3 #1** — `Engine` L2 终态 vs 继续迁 core 决议。
