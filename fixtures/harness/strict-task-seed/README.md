# Strict-mode harness task seed

Copied into the ephemeral workspace before each `lht-harness-run` for tasks with `workspace_seed`.

- `scripts/lht_strict_oracle.sh` — authoritative pass/fail (exit code)
- `go.mod` — empty module; agent fills `main.go`, packages, tests, `scripts/conformance.sh`
