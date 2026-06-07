# `/v2` HTTP API Versioning Strategy (Draft)

| Field | Value |
|-------|-------|
| Status | **Draft** (D8 doc delivery) |
| Current production | **`/v1/*` only** |

## Principles

1. **Breaking changes** require a new path prefix (`/v2/...`) or explicit `Accept-Version` negotiation; no silent break of published `/v1` fields.
2. **After D8**, any `/v1` shape change must sync [`zagens-runtime-v1.openapi.json`](../openapi/zagens-runtime-v1.openapi.json) and `web-ui` generated types.
3. **Desktop shell** and sidecar ship same version; old Zagens + new sidecar not in first-version support matrix.

## When to Introduce `/v2`

| Trigger | Example |
|---------|---------|
| Delete or rename JSON field | Remove `title` from `SessionMetadata` |
| Enum value semantic change | Merge `RuntimeTurnStatus` states |
| Error body unified reshape | `ErrorEnvelope` replaces `{ error: string }` |
| SSE event name breaking | Rename `thinking.delta` |

Non-breaking changes (new optional fields, new endpoints) stay on `/v1` and update OpenAPI.

## Migration Flow (Future)

1. Expose `/v2` subset in parallel + document diff.
2. `web-ui` generates `runtime-api-v2.ts` (or single spec with multiple server URLs).
3. At least **one minor** Zagens version supports v1+v2 simultaneously.
4. After telemetry/logs confirm zero v1 traffic, deprecate v1 (CHANGELOG + Assessment).

## Harness Proposal

maintainer: `doc_Private/docs/harness/HARNESS_INTEGRATION_PROPOSAL.md` Phase 2 depends on D8-generated TS; new harness endpoints prefer `/v1` expansion until v2 strategy is signed off, then batch upgrade.
