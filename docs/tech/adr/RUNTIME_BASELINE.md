# Runtime Long-run Baseline (R-015)

> **Status:** Full RSS populated (2026-05-22); p99 disk-read proxy see dry-run (HTTP run isolation dirs are mostly SQLite, read proxy may be 0)  
> **SSOT process:** maintainer: `doc_Private/docs/tech/RUNTIME_EVOLUTION_ROADMAP.md` §12.6, §14.1 R-015  
> **Revisions:** Numeric changes require [CHANGELOG.md](../../../CHANGELOG.md) `[Unreleased]`

## Scenario (A1.6)

| Parameter | Value |
|-----------|-------|
| Turn count N | 50 |
| Large tool output | At least 1 × ≥ 1MB (script writes **1.1 MB** fixture in thread workspace, deterministic `read_file`) |
| Sampling | Same commit/tag run **3 times**, take median |

## Baseline Commit

| Field | Value |
|-------|-------|
| Git ref | `3d7ab0d` (full 3×50 + 1.1 MB fixture; 2026-06-27) |
| Date | 2026-06-27 |
| Platform | Windows 10 x64 (maintainer machine) |

## Metrics (First Version)

| Metric | Median | Unit | Notes |
|--------|--------|------|-------|
| Process RSS peak | **35.4** | MB | Full HTTP @ `3d7ab0d`, `deepseek-v4-pro`, 50 turn×3 run median; includes 1.1 MB `read_file` fixture turn; `-Gate` PASS vs 29 MB @ `8b1538a` (+10% threshold 31.9 MB) |
| Persist p99 | **0.16** | ms | **dry-run** disk-read proxy (20× synthetic JSON @ `ab4c3c4`); full HTTP isolation dirs mostly SQLite → read proxy **0** (see history table) |

## Scripts and Reproduction

| Item | Path / Command |
|------|----------------|
| Baseline script | [`scripts/runtime-longrun-baseline.ps1`](../../../scripts/runtime-longrun-baseline.ps1) |
| Full (RSS + HTTP turns) | `$env:DEEPSEEK_API_KEY = '…'; .\scripts\runtime-longrun-baseline.ps1 -Runs 3` (or script auto-reads `api_key` from `~/.deepseek/config.toml`) |
| RSS regression gate (A1 transition) | `.\scripts\runtime-longrun-baseline.ps1 -Runs 3 -Gate` — median RSS must not exceed ADR baseline **+10%** (current baseline **35.4 MB**; override with `-BaselineRssMB` / `-MaxRegressionPct`) |
| No API key (disk proxy only) | `.\scripts\runtime-longrun-baseline.ps1 -DryRun -Runs 3` (CI ubuntu job also runs this step) |
| A1-full offline self-check (hot/cold trim) | `cargo test -p deepseek-tui trim_preserves_workshop_ref --lib` |
| Environment variables | `DEEPSEEK_RUNTIME_TOKEN` (script random), `DEEPSEEK_RUNTIME_DIR` (isolated data dir), `DEEPSEEK_MODEL` (optional) |

## Crash-safe Checkpoint (A1.3)

| Path | Strategy |
|------|----------|
| **TUI interactive** | `persistence_actor` — dedicated task + latest-wins merge checkpoint / session snapshot, avoids blocking event loop |
| **HTTP runtime thread store** | `RuntimeThreadStore::append_event` / checklist / scratchpad metadata — **`spawn_blocking`** + SQLite WAL (`journal_mode=WAL`, `synchronous=NORMAL`) |
| **HTTP session persist** | `runtime_api::threads` — `spawn_blocking` wraps `SessionManager::save_session` |
| **Atomicity** | JSON mode: `write_atomic` temp + rename; SQLite: single-transaction commit |

## Revision History

| Date | Ref | RSS peak | Persist p99 | Notes |
|------|-----|----------|-------------|-------|
| 2026-06-27 | 3d7ab0d | **35.4** MB | 0 ms (HTTP) | Full 3×50 + 1.1 MB fixture, `deepseek-v4-pro`; `-Gate` PASS vs 29 MB @ `8b1538a`; fixed `runtime-longrun-baseline.ps1` config path + ADR RSS parser |
| 2026-05-23 | 8b1538a | **29** MB | 0 ms (HTTP) | Full 3×50 + 1.1 MB fixture (after A1 hot/cold trim); `-Gate` PASS vs 28.5 MB @ `10972e4`; log `deliverables/runtime-baseline-full-run.log` |
| 2026-05-23 | 10972e4 | **28.5** MB | 0 ms (HTTP) | Full 3×50 + 1.1 MB fixture; `-Gate` PASS vs 26.6 MB @ ab4c3c4; log `deliverables/runtime-baseline-full-run.log` |
| 2026-05-22 | ab4c3c4 | **26.6** MB | 0 ms (HTTP) / 0.16 ms (dry) | Full 3×50 turn, `DEEPSEEK_RUNTIME_DIR` isolated; model `deepseek-v4-pro`; script release + turn poll wait |
| 2026-05-22 | ab4c3c4 | — | 0.16 ms | dry-run @ ab4c3c4 (first dry numbers) |
| 2026-05-22 | 5d566a3 | — | 0.27 ms | dry-run first value (pre–ab4c3c4) |
| — | — | — | — | First-version placeholder |
