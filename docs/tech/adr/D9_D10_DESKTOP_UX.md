# D9 + D10 — Cancel Two-layer Contract & Multi-window SSE Filtering

**Status:** Landed (phase B — 2026-05-26)  
**Related:** maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` §5.1 Phase B · [API_DESIGN.md](../API_DESIGN.md) §2.1.1–2.1.2

## Context

- **D9:** Stop / Escape previously called only HTTP interrupt or only aborted local stream; two-layer semantics mixed; `runtime_cancel_sse` vs `POST …/interrupt` responsibilities not listed separately in API docs.
- **D10:** Multiple Agent windows could `register_window_thread` on the same thread; non-owner windows still consumed live SSE, causing "ghost rendering" (delta / tool / approval appearing in wrong window).

## Decision

### D9 — Two-layer stop

1. Add `web-ui/src/api/turnControl.ts`:
   - `disconnectThreadEventStream` — Layer 1 (`AbortSignal` + `runtime_cancel_sse`)
   - `stopThreadTurn` — user Stop: Layer 2 HTTP interrupt, then Layer 1 (409 ignored)
2. `App.tsx` `handleCancelStream` unified through `stopThreadTurn`.
3. [API_DESIGN.md](../API_DESIGN.md) **§2.1.1** is SSOT; cross-ref [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) §8.

### D10 — Thread owner SSE filter

1. `windowBridge.ts`: `windowOwnsThreadForStream` + 250ms TTL cache; `registerWindowThread` success calls `markThreadRegisteredLocally`.
2. `client.ts`: `filterThreadStreamEvents` + `threadIdFromSseEvent`.
3. `App.tsx` live SSE pipeline (`postStreamTurn` / `pollThreadTurnEvents`) goes through owner filter; `approval_required` still uses `threadOwnedByWindow` (now delegates to same cache path).

## Acceptance

- [x] Stop / Escape → `stopThreadTurn` (interrupt + disconnect)
- [x] API_DESIGN §2.1.1 / §2.1.2 documented
- [x] Non-owner windows ignore live SSE delta (`filterThreadStreamEvents`)
- [ ] Manual multi-window E2E (thread steal + parallel streaming) — maintainer sign-off

## Non-goals

- Do not change runtime broadcast semantics
- Do not add owner filter to historical `replayThreadEvents` (session load)
- maintainer: `doc_Private/docs/tech/adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md` §1 check count unchanged (UX debt; progress still **7/10**)
