# Diff review

When the agent edits code, Zagens shows a **side-by-side diff** (diff2html) in the workspace panel.

## When diffs appear

Typical triggers:

- `edit_file` / `apply_patch` tool results
- Patch proposals awaiting your review

## How to use it

1. Open the changed file from the workspace tree or tool card link.
2. Review additions and deletions in the diff view.
3. Apply or reject per your workflow (some flows auto-apply after approval).

## Office mode

Diff view targets **code** edits. Office deliverables are usually new files under `deliverables/` — preview those as documents instead.

## Tips

- Pair diff review with git status in the terminal for a second opinion.
- Large patches: ask the agent to split changes into smaller commits.

Related: [File preview](/docs/workspace/preview) · [Code mode](/docs/code-mode)
