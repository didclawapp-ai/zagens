# Runtime 长跑基准（R-015）

> **状态：** 部分填数 — p99 磁盘读代理（dry-run）；RSS 须带 `DEEPSEEK_API_KEY` 全量跑  
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
| Git ref | `5d566a3`（dry-run 采样；全量 RSS 须在同一 ref 重跑） |
| 日期 | 2026-05-22 |
| 平台 | Windows 10 x64（维护者机） |

## 指标（首版）

| 指标 | 中位数 | 单位 | 备注 |
|------|--------|------|------|
| 进程 RSS 峰值 | _待填（全量）_ | MB | A1 过渡门禁：不得比本 commit 劣化 **>10%**；CI/本地须 `DEEPSEEK_API_KEY` |
| 落盘 p99 | **0.27** | ms | **dry-run** 磁盘读代理（20× synthetic JSON）；全量 HTTP 跑见脚本默认模式 |

## 脚本与复现

| 项 | 路径 / 命令 |
|----|-------------|
| 基准脚本 | [`scripts/runtime-longrun-baseline.ps1`](../../../scripts/runtime-longrun-baseline.ps1) |
| 全量（RSS + HTTP turns） | `$env:DEEPSEEK_API_KEY = '…'; .\scripts\runtime-longrun-baseline.ps1 -Runs 3` |
| 无 API key（仅磁盘代理） | `.\scripts\runtime-longrun-baseline.ps1 -DryRun -Runs 3` |
| 环境变量 | `DEEPSEEK_RUNTIME_TOKEN`（脚本随机生成）、`DEEPSEEK_RUNTIME_DIR`（隔离 data dir）、`DEEPSEEK_MODEL`（可选） |

## 历史修订

| 日期 | Ref | RSS 峰值 | 落盘 p99 | 说明 |
|------|-----|----------|----------|------|
| 2026-05-22 | 5d566a3 | — | 0.27 ms | dry-run 首值；RSS 待全量 3 次中位数 |
| — | — | — | — | 首版占位 |
