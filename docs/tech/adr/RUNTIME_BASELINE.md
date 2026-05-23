# Runtime 长跑基准（R-015）

> **状态：** RSS 全量已填（2026-05-22）；p99 磁盘读代理见 dry-run（HTTP 跑时隔离目录多为 SQLite，读代理可为 0）  
> **SSOT 流程：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §12.6、§14.1 R-015  
> **修订：** 数值变更须 [CHANGELOG.md](../../../CHANGELOG.md) `[Unreleased]`

## 场景（A1.6）

| 参数 | 值 |
|------|-----|
| 轮数 N | 50 |
| 大 tool 输出 | 至少 1 次 ≥ 1MB（脚本在 thread workspace 写入 **1.1 MB** fixture，`read_file` 确定性读取） |
| 采样 | 同一 commit/tag 跑 **3 次**，取中位数 |

## 基线 commit

| 字段 | 值 |
|------|-----|
| Git ref | `10972e4`（全量 3×50 + 1.1 MB fixture；2026-05-23） |
| 日期 | 2026-05-23 |
| 平台 | Windows 10 x64（维护者机） |

## 指标（首版）

| 指标 | 中位数 | 单位 | 备注 |
|------|--------|------|------|
| 进程 RSS 峰值 | **28.5** | MB | 全量 HTTP @ `10972e4`，`deepseek-v4-pro`，50 turn×3 run 中位数；含 1.1 MB `read_file` fixture turn；A1 过渡门禁：不得劣化 **>10%**（对上一行基线） |
| 落盘 p99 | **0.16** | ms | **dry-run** 磁盘读代理（20× synthetic JSON @ `ab4c3c4`）；全量 HTTP 隔离目录多为 SQLite → 读代理 **0**（见历史表） |

## 脚本与复现

| 项 | 路径 / 命令 |
|----|-------------|
| 基准脚本 | [`scripts/runtime-longrun-baseline.ps1`](../../../scripts/runtime-longrun-baseline.ps1) |
| 全量（RSS + HTTP turns） | `$env:DEEPSEEK_API_KEY = '…'; .\scripts\runtime-longrun-baseline.ps1 -Runs 3`（或 `api_key` 写在 `~/.deepseek/config.toml` 时脚本自动读取） |
| RSS 回归门（A1 过渡） | `.\scripts\runtime-longrun-baseline.ps1 -Runs 3 -Gate` — 中位数 RSS 不得高于 ADR 基线 **+10%**（默认读上表 **26.6 MB**；可用 `-BaselineRssMB` / `-MaxRegressionPct` 覆盖） |
| 无 API key（仅磁盘代理） | `.\scripts\runtime-longrun-baseline.ps1 -DryRun -Runs 3`（CI ubuntu job 亦跑此步） |
| 环境变量 | `DEEPSEEK_RUNTIME_TOKEN`（脚本随机生成）、`DEEPSEEK_RUNTIME_DIR`（隔离 data dir）、`DEEPSEEK_MODEL`（可选） |

## Crash-safe checkpoint（A1.3）

| 路径 | 策略 |
|------|------|
| **TUI 交互** | `persistence_actor` — 专用 task + latest-wins 合并 checkpoint / session snapshot，避免阻塞 event loop |
| **HTTP runtime 线程库** | `RuntimeThreadStore::append_event` / checklist / scratchpad 元数据 — **`spawn_blocking`** + SQLite WAL（`journal_mode=WAL`, `synchronous=NORMAL`） |
| **HTTP session 落盘** | `runtime_api::threads` — `spawn_blocking` 包裹 `SessionManager::save_session` |
| **原子性** | JSON 模式：`write_atomic` temp + rename；SQLite：单事务 commit |

## 历史修订

| 日期 | Ref | RSS 峰值 | 落盘 p99 | 说明 |
|------|-----|----------|----------|------|
| 2026-05-23 | 10972e4 | **28.5** MB | 0 ms (HTTP) | 全量 3×50 + 1.1 MB fixture；`-Gate` PASS vs 26.6 MB @ ab4c3c4；log `deliverables/runtime-baseline-full-run.log` |
| 2026-05-22 | ab4c3c4 | **26.6** MB | 0 ms (HTTP) / 0.16 ms (dry) | 全量 3×50 turn，`DEEPSEEK_RUNTIME_DIR` 隔离；模型 `deepseek-v4-pro`；脚本 release + turn 轮询等待 |
| 2026-05-22 | ab4c3c4 | — | 0.16 ms | dry-run @ ab4c3c4（首版 dry 填数） |
| 2026-05-22 | 5d566a3 | — | 0.27 ms | dry-run 首值（pre–ab4c3c4） |
| — | — | — | — | 首版占位 |
