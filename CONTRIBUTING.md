# Contributing to Zagens

Thanks for your interest in Zagens. This repository is [MIT licensed](LICENSE).

## Before you open a PR

1. Read the design specs you are touching under [`docs/`](docs/README.md) (architecture, API, LHT, CRAFT, prompts).
2. Run local verification — see **[LOCAL_DEV_VERIFY.md](LOCAL_DEV_VERIFY.md)** for toolchain pin, lint scripts, and optional git hooks.
3. For **code or behavior changes**, add an `[Unreleased]` entry in [CHANGELOG.md](CHANGELOG.md) when practical. Skip transactional edits (doc moves, README-only, license/repo hygiene) unless a maintainer asks.

## Quick verify

```bash
bash scripts/ci/verify-lint.sh          # mirrors CI Lint (fmt + clippy)
bash scripts/ci/verify-workspace.sh     # lint + full workspace tests
```

Windows: `pwsh -File scripts/ci/verify-lint.ps1`

Optional hooks (once per clone): `bash scripts/ci/install-git-hooks.sh`

## Push without full CI (maintainers)

Routine doc edits and small landings should not burn CI minutes on every `git push`.
The shared gate is [`scripts/ci/ci-push-gate.sh`](scripts/ci/ci-push-gate.sh) (remote + local pre-push).

| Push kind | Remote CI | CD (Release) | Local pre-push lint |
|-----------|-----------|--------------|---------------------|
| **Release tag** `zagens-v*` / `ds-pick-v*` | Full matrix | Yes (after CI green) | Full `verify-lint` |
| **Pull request** | Full matrix | No | N/A |
| **Docs / housekeeping only** on `master` / `main` | Skipped | No | Skipped |
| **Code change** on `master` / `main` | Full matrix | No | Full `verify-lint` |
| **`[skip ci]` or `[ci skip]`** in commit message | Skipped | No (unless tag) | Skipped |

**Housekeeping paths** (no code CI when *all* changed files match): `*.md`, `docs/`, `doc_Private/`, `deliverables/`, `assets/`, `producthunt/`, `.cursor/`, root policy files (`LICENSE`, `NOTICE.md`, …).

**Examples:**

```bash
# README / CHANGELOG / docs only — auto-skipped
git commit -m "docs(readme): clarify DeepSeek V4 positioning"
git push origin master

# Land WIP on master without remote CI (use sparingly; run verify before release)
git commit -m "chore: sync maintainer notes [skip ci]"
git push origin master

# Release — always full CI + CD
git tag -a zagens-v0.7.5 -m "Zagens v0.7.5"
git push origin zagens-v0.7.5
```

Emergency local bypass (does not affect remote): `SKIP_VERIFY=1 git push`.

Before a release tag, run `bash scripts/ci/verify-workspace.sh` even if recent pushes used `[skip ci]`.

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
