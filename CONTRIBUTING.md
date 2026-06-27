# Contributing to Zagens

Thanks for your interest in Zagens. This repository is [MIT licensed](LICENSE).

## Before you open a PR

1. Read the design specs you are touching under [`docs/`](docs/README.md) (architecture, API, LHT, CRAFT, prompts).
2. Run local verification — see **[LOCAL_DEV_VERIFY.md](LOCAL_DEV_VERIFY.md)** for toolchain pin, lint scripts, and optional git hooks.
3. For **code or behavior changes**, add an `[Unreleased]` entry in [CHANGELOG.md](CHANGELOG.md) when practical. Skip transactional edits (doc moves, README-only, license/repo hygiene) unless a maintainer asks.

## Quick verify

**Recommended:** install [just](https://github.com/casey/just) (`cargo install just` / `scoop install just` / `winget install Casey.Just`), then from the repo root:

```bash
just verify          # L1: CI Lint mirror (toolchain + prebuild + fmt + clippy)
just check           # L2: PR gate — verify + workspace tests + web-check
just verify-all      # L3: push gate — verify + tests + multi-session + lockfile
just web-check       # Frontend: tsc + ESLint + Vitest
just l4-contracts    # L4: versions + architecture + OpenAPI + runtime contracts
just l4-ci-smoke     # L4: mirror CI ubuntu smoke extras
just l4-full         # L4: contracts + harness + trace-report + docs
just --list          # all recipes (tier guide in justfile header)
```

Cursor/VS Code: **Terminal → Run Task…** → pick a **Zagens:** task (see [`.vscode/tasks.json`](.vscode/tasks.json)).

Direct scripts (still supported):

```bash
bash scripts/ci/verify-lint.sh          # mirrors CI Lint (fmt + clippy)
bash scripts/ci/verify-workspace.sh     # lint + full workspace tests
```

Windows: `pwsh -File scripts/ci/verify-lint.ps1` or `just verify`

Web UI only: `just web-check` or `cd crates/desktop/web-ui && npm run lint && npm test`

Optional hooks (once per clone): `just hooks` or `bash scripts/ci/install-git-hooks.sh`

## CI when you push (PR-first)

This repo follows the usual open-source layout: **CI runs on pull requests**, not on every
merge push to `master` / `main`. Release tags still run the full matrix and trigger CD.

| Event | Remote CI | CD (Release) | Local pre-push lint |
|-------|-----------|--------------|---------------------|
| **Pull request** → `master` / `main` | Full matrix | No | N/A |
| **Merge / push** to `master` / `main` | **None** (already checked on the PR) | No | See below |
| **Release tag** `zagens-v*` / `ds-pick-v*` | Full matrix | Yes (after CI green) | Full `verify-lint` |
| **Weekly schedule** / **workflow_dispatch** | Full matrix | No | N/A |

**Contributor flow:**

```bash
git checkout -b feat/my-change
# … edit …
bash scripts/ci/verify-lint.sh    # optional but recommended before opening PR
git push -u origin feat/my-change
gh pr create                      # CI runs on the PR
```

**Maintainer direct push to `master`** (doc hotfix, merge button, etc.) does **not** start
GitHub Actions. Local pre-push still uses [`scripts/ci/ci-push-gate.sh`](scripts/ci/ci-push-gate.sh):
docs/housekeeping-only diffs or `[skip ci]` skip `verify-lint`; code changes run lint locally.

```bash
# Doc-only hotfix — no remote CI, local lint skipped
git commit -m "docs(readme): …"
git push origin master

# Release — always remote CI + CD
git tag -a zagens-v0.7.5 -m "Zagens v0.7.5"
git push origin zagens-v0.7.5
```

Before tagging, run `bash scripts/ci/verify-workspace.sh` if changes did not go through a PR.
Emergency local bypass: `SKIP_VERIFY=1 git push`.

## Security

See [SECURITY.md](SECURITY.md) for vulnerability reporting — please do not file public issues for security bugs.

## User documentation

End-user guides are maintained on [zagens.com/docs](https://zagens.com/docs), not in this repo.

## Maintainer-only material

Internal runbooks, release ops, eval notebooks, and session handoffs live in local `doc_Private/` (not published).

## Dependency updates (maintainers)

This repo does **not** use GitHub Dependabot (aligned with [CodeWhale](https://github.com/Hmbown/CodeWhale)). Cargo, npm (`crates/desktop/web-ui`), and GitHub Actions pin bumps are **manual** during release prep or security maintenance:

- Run `bash scripts/release/pre-publish-check.sh` before crates.io publish.
- After `cargo update` / npm lockfile changes, run `bash scripts/ci/verify-workspace.sh`.
- Pin third-party Actions to commit SHAs in `.github/workflows/` when upgrading.

Close any stale open Dependabot PRs on GitHub after removing Dependabot config.
