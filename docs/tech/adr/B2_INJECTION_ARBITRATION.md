# B2.1 — Context injection arbitration (SSOT)

**Status:** Accepted (2026-05-24)  
**Related:** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §9.2 B2.1, [topic-memory-rust-plan.md](../../topic-memory-rust-plan.md)

## Problem

Multiple subsystems inject text into the model context:

| Source | Typical block | Writer |
|--------|---------------|--------|
| Tool results | `tool` role messages / `<tool_result>` | Engine turn loop |
| CRAFT blackboard | Assignment / sub-agent prompt section | `tools/subagent` (`read_blackboard_section`) |
| Topic memory graph | `<topic_memory>` in system prompt | `tui/topic_memory.rs` → `cycle_hooks::refresh_system_prompt` |
| User memory | `<user_memory>` | `memory.rs` |
| Compaction summary | Merged system prompt tail | `compaction` / `merge_compaction_summary` |

Without a fixed precedence, slow consumers and capacity pressure can produce **contradictory or duplicated** context (e.g. stale topic graph vs fresh tool output).

## Arbitration order (strict)

When context is assembled or trimmed, **higher rows win** over lower rows. Lower sources may be dropped or truncated first; never the reverse.

| Priority | Source | Rationale |
|:--------:|--------|-----------|
| **1** | **Tool results** (in-thread messages) | Ground truth from the environment; must not be summarized away before auxiliary memory |
| **2** | **CRAFT blackboard** (per-task partition) | Task-scoped working state for multi-agent runs; sub-agent assignments depend on it |
| **3** | **Topic memory graph** (`<topic_memory>`) | Heuristic, auto-extracted; safe to skip a cycle or omit under pressure |
| **4** | User memory (`<user_memory>`) | User-declared prefs; lower than live task state |
| **5** | Compaction / scratchpad summaries | Already lossy; trim last |

### Same-turn rules

- **Tool output always stays** in `session.messages` until compaction policy removes it (pin rules apply).
- **Blackboard** is injected into **sub-agent** prompts only (not main-thread system prompt every turn).
- **Topic memory** injects on interval (default every N completed turns); if `should_inject_memory` is false, omit entirely — do not partial-inject.
- When **capacity_policy** or manual trim runs, drop **topic_memory_block** before dropping blackboard sections in sub-agent prompts or before trimming unpinned tool results.

## Code anchors

| Concern | Location |
|---------|----------|
| Topic block assembly | `crates/tui/src/topic_memory.rs` — `TopicMemoryRuntime::compose_block` |
| System prompt merge | `crates/tui/src/core/engine/cycle_hooks.rs` — `refresh_system_prompt` |
| Blackboard read | `crates/tui/src/tools/subagent/blackboard.rs` — `read_blackboard_section` |
| Sub-agent prompt | `crates/tui/src/tools/subagent/mod.rs` — `build_assignment_prompt` |
| Capacity trim | `crates/tui/src/core/engine/capacity_flow/interventions.rs` — `refresh_system_prompt_for_turn_mode_under_capacity` |
| Arbitration flags | `crates/tui/src/topic_memory.rs` — `PromptInjectionArbitration` |
| Constant (short) | `topic_memory::INJECTION_ARBITRATION` |

## Operational notes

- **Do not** paste the full topic graph every turn (B2.3 k-hop only).
- **Do not** inject topic memory into sub-agent prompts unless explicitly added later via ADR.
- Zagens **TopicMemoryPanel** is read-only visualization; toggles live in system settings.
- Metrics for B2.5 (`metrics.json`) track `clarification_rounds` / `repeat_topic_turns` — see `scripts/topic-memory-eval.ps1`.

## Change control

Any new injectable source (e.g. RAG chunk store) must be assigned a priority row here **before** shipping. Bump [API_DESIGN.md](../API_DESIGN.md) only if HTTP/SSE exposes new blocks.
