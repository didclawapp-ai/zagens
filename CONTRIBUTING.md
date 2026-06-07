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

## Security

See [SECURITY.md](SECURITY.md) for vulnerability reporting — please do not file public issues for security bugs.

## User documentation

End-user guides are maintained on [zagens.com/docs](https://zagens.com/docs), not in this repo.

## Maintainer-only material

Internal runbooks, release ops, eval notebooks, and session handoffs live in local `doc_Private/` (not published).
