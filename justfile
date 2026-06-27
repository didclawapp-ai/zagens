# Zagens unified test & verify orchestration.
# Install: cargo install just  |  scoop install just  |  winget install Casey.Just
# List all recipes: just --list
#
# Tier guide (lightest → heaviest):
#   L0  fmt / fmt-check / web-*     — single concern, fast iteration
#   L1  verify / lint / test-*      — CI Lint mirror or scoped tests (lint/test-all run prebuild)
#   L2  check                       — PR gate: verify + workspace tests + full web-ui quality
#   L3  verify-all                  — push gate: check superset + multi-session + lockfile drift
#   L4  contract / openapi / harness / gate / … — contracts, regression, release (`l4-*` bundles)

default:
    @just --list

# ── Prebuild (desktop clippy/tests need web-ui dist + runtime sidecar) ───────

[unix]
prebuild:
    bash scripts/ci/ensure-web-ui-dist.sh
    cargo build -p zagens-cli --bin zagens-runtime --locked

[windows]
prebuild:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ci/ensure-web-ui-dist.ps1
    cargo build -p zagens-cli --bin zagens-runtime --locked

# ── Local CI mirrors ────────────────────────────────────────────────────────

# Mirror CI Lint job (toolchain check + prebuild + fmt + clippy).
[unix]
verify:
    bash scripts/ci/verify-lint.sh

[windows]
verify:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ci/verify-lint.ps1

# Full pre-push gate: verify + workspace tests + multi-session + lockfile drift.
[unix]
verify-all:
    bash scripts/ci/verify-workspace.sh

[windows]
verify-all:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ci/verify-workspace.ps1

# ── Rust ────────────────────────────────────────────────────────────────────

# Run tests for one crate (package name, e.g. zagens-cli).
# zagens-desktop tests may need `just prebuild` first on a clean tree.
test crate:
    cargo test -p {{crate}} --all-features --locked

# Run all workspace tests (runs prebuild so desktop tests work on clean clones).
test-all: prebuild
    cargo test --workspace --all-features --locked

# cargo clippy with CI flags (runs prebuild; does not run fmt).
lint: prebuild
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

# Format all Rust sources.
fmt:
    cargo fmt --all

# Check formatting without writing.
fmt-check:
    cargo fmt --all -- --check

# Build release zagens binary (needed for gate/harness).
build-cli:
    cargo build -p zagens-cli --release --locked --bin zagens

# ── Frontend (web-ui) ─────────────────────────────────────────────────────

web-dir := "crates/desktop/web-ui"

# Vitest unit tests.
web-test:
    cd {{web-dir}} && npm test

# ESLint.
web-lint:
    cd {{web-dir}} && npm run lint

# Typecheck + Vite production build.
web-build:
    cd {{web-dir}} && npm run build

# Typecheck only (tsc -b).
web-typecheck:
    cd {{web-dir}} && npm run typecheck

# All web-ui quality gates (typecheck + ESLint + Vitest).
web-check: web-typecheck web-lint web-test

# Ensure web-ui dist exists (desktop clippy/tests).
web-dist:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "$(uname -s)" = "Linux" ] || [ "$(uname -s)" = "Darwin" ]; then
      bash scripts/ci/ensure-web-ui-dist.sh
    else
      powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ci/ensure-web-ui-dist.ps1
    fi

# ── L4: Contracts & architecture ─────────────────────────────────────────────

# Full D17 freeze gate: architecture tests + OpenAPI/api-types drift.
[unix]
contract:
    bash scripts/check-architecture-freeze.sh

[windows]
contract:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-architecture-freeze.ps1

# Architecture boundary tests only (no OpenAPI).
architecture: prebuild
    cargo test -p zagens-cli --test architecture_invariants --locked
    cargo test -p zagens-desktop --test architecture_boundary --locked

# OpenAPI JSON + generated web-ui TS drift check.
[unix]
openapi:
    bash scripts/check-openapi-contract.sh

[windows]
openapi:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-openapi-contract.ps1

# Regenerate checked-in OpenAPI JSON (then run api-types or openapi-sync).
[unix]
openapi-export:
    bash scripts/export-runtime-openapi.sh

[windows]
openapi-export:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/export-runtime-openapi.ps1

# Regenerate web-ui TS types from checked-in OpenAPI.
api-types:
    cd {{web-dir}} && npm run generate:api-types

# Export OpenAPI + regenerate TS types (after runtime API edits).
openapi-sync: openapi-export api-types

# Build debug zagens CLI (headless contract tests use CARGO_BIN_EXE_zagens).
build-zagens:
    cargo build -p zagens-cli --bin zagens --locked

# Sidecar + headless CLI contract tests need dist, sidecar, and zagens binaries.
prebuild-contracts: prebuild build-zagens

# ── L4: Runtime / CLI contracts ─────────────────────────────────────────────

sidecar-contract: prebuild-contracts
    cargo test -p zagens-cli --lib sidecar_contract_full_lifecycle --locked

sidecar-binary: prebuild-contracts
    cargo test -p zagens-cli --test sidecar_binary_contract --locked

cli-contract: prebuild-contracts
    cargo test -p zagens-cli --test zagens_cli_contract --locked

exec-mock-e2e: prebuild-contracts
    cargo test -p zagens-cli --lib exec_agent_json_e2e_with_mock_llm --locked

# All headless runtime/CLI contract tests (mirrors CI ubuntu Test job extras).
runtime-contracts: sidecar-contract sidecar-binary cli-contract exec-mock-e2e

# ── L4: Release & lockfile ──────────────────────────────────────────────────

# Workspace + desktop version drift (CI Lint job).
[unix]
versions:
    bash scripts/release/check-versions.sh

[windows]
versions:
    bash scripts/release/check-versions.sh

# Cargo.lock must be committed and in sync (also runs at end of verify-all).
lockfile:
    git diff --exit-code -- Cargo.lock

# Pre-crates.io / tag publish checklist (bash; mirrors release CI gates).
[unix]
pre-publish *args:
    bash scripts/release/pre-publish-check.sh {{args}}

[windows]
pre-publish *args:
    bash scripts/release/pre-publish-check.sh {{args}}

# ── L4: Harness & coverage gate ─────────────────────────────────────────────

[unix]
harness *args:
    bash scripts/ci/harness-regression.sh {{args}}

# Windows: requires Git Bash or WSL on PATH (`bash`).
[windows]
harness *args:
    bash scripts/ci/harness-regression.sh {{args}}

# Harness + optional 35min+ R-015 baseline (needs DEEPSEEK_API_KEY).
harness-longrun:
    bash scripts/ci/run-harness-longrun.sh

zagens-bin := if os() == "windows" { "target/release/zagens.exe" } else { "target/release/zagens" }

# Layer-2 coverage gate (builds release binary if missing).
gate *args:
    just build-cli
    {{zagens-bin}} coverage-gate {{args}}

# CI smoke: report-only gate (never fails the recipe on checklist gaps).
gate-smoke:
    just build-cli
    {{zagens-bin}} coverage-gate --no-fail --json

# Gate with workspace tests enabled.
gate-tests:
    just build-cli
    {{zagens-bin}} coverage-gate --run-tests --json

# R-015 longrun baseline dry-run (CI ubuntu smoke; no API key).
longrun-dry:
    pwsh -File scripts/runtime-longrun-baseline.ps1 -DryRun -Runs 3

# R-015 longrun baseline with gate (needs DEEPSEEK_API_KEY).
longrun:
    pwsh -File scripts/runtime-longrun-baseline.ps1 -Runs 3 -Gate

# ── L4: Tooling & cross-platform ────────────────────────────────────────────

# Kernel trace report export smoke (fixtures → HTML).
[unix]
trace-report:
    bash scripts/ci/verify-trace-report.sh

[windows]
trace-report:
    bash scripts/ci/verify-trace-report.sh

# Workspace rustdoc with -D warnings (CI schedule job).
# Exclude desktop: both desktop and zagens-cli ship a `zagens` bin (rustdoc path collision).
[unix]
docs:
    #!/usr/bin/env bash
    set -euo pipefail
    export RUSTDOCFLAGS=-Dwarnings
    cargo doc --workspace --no-deps --locked --exclude zagens-desktop

[windows]
docs:
    set RUSTDOCFLAGS=-Dwarnings
    cargo doc --workspace --no-deps --locked --exclude zagens-desktop

# Linux Lint in WSL/Docker from Windows (cfg(unix) blind spot). Windows only.
[windows]
verify-linux *args:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ci/verify-lint-linux.ps1 {{args}}

# ── L4 aggregates ───────────────────────────────────────────────────────────

# Contract surface: versions + architecture + OpenAPI + runtime contracts.
l4-contracts: versions architecture openapi runtime-contracts

# Mirror CI ubuntu-only smoke extras (post Test job).
l4-ci-smoke: versions openapi runtime-contracts gate-smoke longrun-dry lockfile

# Headless harness regression (lib + contracts + gate).
l4-regression: harness

# Extended L4: contracts + harness + trace report + docs.
l4-full: l4-contracts harness trace-report docs

# Maintainer publish gate (versions + fmt + tests + leaf dry-runs).
release-check: pre-publish

# Multi-session parallel streaming (Rust integration + web-ui Vitest/ESLint).
[unix]
multi-session:
    bash scripts/ci/test-multi-session.sh

[windows]
multi-session:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ci/test-multi-session.ps1

# ── Aggregates ──────────────────────────────────────────────────────────────

# Pre-commit style: fmt check only.
pre-commit: fmt-check

# Pre-push style: lint mirror (matches git hook default).
pre-push: verify

# PR gate: CI lint mirror + workspace tests + full web-ui quality (no multi-session / lockfile).
check: verify test-all web-check

# Install git hooks (pre-commit fmt, pre-push verify).
[unix]
hooks:
    bash scripts/ci/install-git-hooks.sh

[windows]
hooks:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/ci/install-git-hooks.ps1
