# Runtime 长跑基准（R-015）

> **状态：** RSS 全量已填（2026-05-22）；p99 磁盘读代理见 dry-run（HTTP 跑时隔离目录多为 SQLite，读代理可为 0）  
> **SSOT 流程：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §12.6、§14.1 R-015  
> **修订：** 数值变更须 [CHANGELOG.md](../../../CHANGELOG.md) `[Unreleased]`

## 场景（A1.6）

| 参数 | 值 |
|------|-----|
| 轮数 N | 50 |
| 大 tool 输出 | 至少 1 次 ≥ 1MB（全量跑时 best-effort prompt） |
| 采样 | 同一 commit/tag 跑 **3 次**，取中位数 |

## 基线 commit

| 字段 | 值 |
|------|-----|
| Git ref | `ab4c3c4`（dry-run 采样；全量 RSS 须在同一 ref 重跑） |
| 日期 | 2026-05-22 |
| 平台 | Windows 10 x64（维护者机） |

## 指标（首版）

| 指标 | 中位数 | 单位 | 备注 |
|------|--------|------|------|
| 进程 RSS 峰值 | **26.6** | MB | 全量 HTTP @ `ab4c3c4`，`deepseek-v4-pro`，50 turn×3 run 中位数；A1 过渡门禁：不得劣化 **>10%** |
| 落盘 p99 | **0.16** | ms | **dry-run** 磁盘读代理（20× synthetic JSON @ `ab4c3c4`）；全量 HTTP 隔离目录多为 SQLite → 读代理 **0**（见历史表） |

## 脚本与复现

| 项 | 路径 / 命令 |
|----|-------------|
| 基准脚本 | [`scripts/runtime-longrun-baseline.ps1`](../../../scripts/runtime-longrun-baseline.ps1) |
| 全量（RSS + HTTP turns） | `$env:DEEPSEEK_API_KEY = '…'; .\scripts\runtime-longrun-baseline.ps1 -Runs 3`（或 `api_key` 写在 `~/.deepseek/config.toml` 时脚本自动读取） |
| 无 API key（仅磁盘代理） | `.\scripts\runtime-longrun-baseline.ps1 -DryRun -Runs 3` |
| 环境变量 | `DEEPSEEK_RUNTIME_TOKEN`（脚本随机生成）、`DEEPSEEK_RUNTIME_DIR`（隔离 data dir）、`DEEPSEEK_MODEL`（可选） |

## 历史修订

| 日期 | Ref | RSS 峰值 | 落盘 p99 | 说明 |
|------|-----|----------|----------|------|
| 2026-05-22 | ab4c3c4 | **26.6** MB | 0 ms (HTTP) / 0.16 ms (dry) | 全量 3×50 turn，`DEEPSEEK_RUNTIME_DIR` 隔离；模型 `deepseek-v4-pro`；脚本 release + turn 轮询等待 |
| 2026-05-22 | ab4c3c4 | — | 0.16 ms | dry-run @ ab4c3c4（首版 dry 填数） |
| 2026-05-22 | 5d566a3 | — | 0.27 ms | dry-run 首值（pre–ab4c3c4） |
| — | — | — | — | 首版占位 |
