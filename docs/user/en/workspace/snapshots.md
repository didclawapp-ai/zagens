# Workspace snapshots

Zagens can capture **workspace snapshots** (side-git under `~/.zagens/snapshots/`) separate from your repository `.git`.

## Purpose

Snapshots let you roll back file state after agent edits without relying on your own commits — useful during exploratory refactors.

## Restore

Use runtime restore actions (e.g. revert turn / restore snapshot commands exposed in the UI or CLI) to return to a captured point.

Snapshots are **not** a replacement for git — use both for serious projects.

## Scope

- Snapshots cover the workspace directory configured for the session
- They do not include API keys or global `~/.zagens/config.toml`

## Configuration

Retention and behavior follow `[snapshots]` in `~/.zagens/config.toml` (see `config.example.toml` in the repo).

## Tips

- Commit to git before large agent tasks when you want permanent history.
- Office `deliverables/` can be copied aside before asking for a full rewrite.

Related: [Workspace overview](/docs/workspace/overview) · [Diff review](/docs/workspace/diff)
