# Runtime 长跑基准（R-015）

> **状态：** 占位 — 首版数值待 R-015 脚本跑完后填入  
> **SSOT 流程：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §12.6、§14.1 R-015  
> **修订：** 数值变更须 [CHANGELOG.md](../../../CHANGELOG.md) `[Unreleased]`

## 场景（A1.6）

| 参数 | 值 |
|------|-----|
| 轮数 N | 50 |
| 大 tool 输出 | 至少 1 次 ≥ 1MB |
| 采样 | 同一 commit/tag 跑 **3 次**，取中位数 |

## 基线 commit

| 字段 | 值 |
|------|-----|
| Git ref | _（待填，例如 `main@<sha>` 或 release tag） |
| 日期 | _待填_ |
| 平台 | _待填（OS / arch）_ |

## 指标（首版）

| 指标 | 中位数 | 单位 | 备注 |
|------|--------|------|------|
| 进程 RSS 峰值 | _待填_ | MB | A1 过渡门禁：不得比本 commit 劣化 **>10%** |
| 落盘 p99 | _待填_ | ms | 同上 |

## 脚本与复现

| 项 | 路径 |
|----|------|
| 基准脚本 | [`scripts/runtime-longrun-baseline.ps1`](../../../scripts/runtime-longrun-baseline.ps1) |
| 环境变量 | `DEEPSEEK_RUNTIME_TOKEN`、随机 HTTP 端口（与 A+.4 契约测一致） |

## 历史修订

| 日期 | Ref | RSS 峰值 | 落盘 p99 | 说明 |
|------|-----|----------|----------|------|
| — | — | — | — | 首版待 R-015 |
