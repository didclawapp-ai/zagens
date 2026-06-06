# Task types: Code vs Office

Zagens separates engineering and document workflows with a **task type**. When you start a session, choosing **Code** or **Office** loads a different system prompt and tool surface.

## Which to pick

| Type | Best for |
|------|----------|
| **Code** | Repo edits, tests, terminal, diff, symbol index, LHT / CRAFT, sub-agents |
| **Office** | Spreadsheets, web research, DOCX / XLSX / PPTX / PDF deliverables in `deliverables/` |

Casual chat and document work belong in **Office**; refactors, debugging, and repo tooling belong in **Code**.

## Switching rules

**Changing task type starts a new session** — do not mix types in one chat, so the model context prefix stays stable.

Pick the type again in the composer or new-session flow.

## Tool differences (summary)

**Code** tools include `grep_files`, `exec_shell`, Git, `edit_file` / `apply_patch`, symbol index, sub-agents, and more.

**Office** keeps `read_office`, `write_office`, `load_office_payload`, `glob_files`, `file_search`, `load_skill`, optional web tools, and `describe_image`. **No** shell or patch-style engineering tools.

See [Agent tools](/docs/tools/files) and [Office I/O](/docs/tools/office-io).

## Settings differences

Office sessions hide **routing**, **topic memory**, **symbol index**, and **LHT settings** in the sidebar. Usage, MCP, skills, and API key remain available.

## Next steps

| Goal | Docs |
|------|------|
| Engineering | [Code mode](/docs/code-mode) · [Workspace](/docs/workspace/overview) |
| Documents | [Office mode](/docs/office/overview) · [Office workspace](/docs/office/workspace) |
| UI map | [UI tour](/docs/ui-tour) |
