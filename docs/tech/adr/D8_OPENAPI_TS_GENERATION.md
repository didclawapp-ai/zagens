# D8 — OpenAPI 导出 + web-ui TS 类型自动生成

| 字段 | 值 |
|------|-----|
| Status | **Landed** (2026-05-26) |
| Assessment §1 | **#9 勾选 → 9/10** |
| 前置 | D6 sidecar、D7 持久化稳定 |

## 目标

消除 `web-ui` 手写 HTTP interface 与 `crates/tui/src/runtime_api/router.rs` 的飘移；新增端点必须先更新 OpenAPI 再生成 TS。

## 产物

| 路径 | 说明 |
|------|------|
| [`docs/tech/openapi/zagens-runtime-v1.openapi.json`](../openapi/zagens-runtime-v1.openapi.json) | 检入的 OpenAPI 3.1 契约（paths + components） |
| `crates/tui/src/runtime_api/openapi/` | Rust 导出：`schemars` → components，`paths.rs` 对齐 router |
| `crates/tui` binary `export-runtime-openapi` | feature `openapi-export` |
| `crates/desktop/web-ui/src/api/generated/runtime-api.ts` | `openapi-typescript` 生成 |
| `crates/desktop/web-ui/src/api/runtimeTypes.ts` | 稳定 re-export 层 |

## 本地再生

```powershell
# 1) OpenAPI JSON
.\scripts\export-runtime-openapi.ps1

# 2) TypeScript（需 devDependencies：npm install --include=dev）
cd crates\desktop\web-ui
npm run generate:api-types
npm run build
```

Linux/macOS：`scripts/export-runtime-openapi.sh` + 同上 `generate:api-types`。

## 设计选择

1. **`schemars` + 手写 paths**（非 `utoipa` 全量注解）— 避免给 50+ handler 加宏；paths 表与 `router.rs` 用单元测试 `EXPECTED_PATH_TEMPLATES`（54）防漂移。
2. **组件扁平化** — 导出时将各 schema 的 `$defs` 提升为 `components.schemas` 并重写 `$ref`，供 `openapi-typescript` 解析。
3. **渐进迁移** — `client.ts` 已接入 `StreamTurnRequest` / `TurnRecord` 等；UI 专用别名（如 `SessionInfo.name`）保留映射层，见 `runtimeTypes.ts`。

## 纪律（延续 §7）

- ⛔ 禁止无 schema 的新 `/v1/*` 路径（Assessment §7.1）。
- 改 Rust 请求/响应类型 → 重跑 export + `generate:api-types` → 同 PR 更新检入文件。

## 后续

- 将 `types/automation.ts`、`types/mcp.ts` 等逐步改为从 `runtimeTypes` / `generated` 引用。
- CI 可加 `git diff` 守卫（见 `.github/workflows/ci.yml` 可选步骤）。
- `/v2` 版本策略：[`V2_API_VERSIONING.md`](./V2_API_VERSIONING.md)。
