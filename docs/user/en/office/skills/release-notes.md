# Release notes

**Skill:** `office-release-notes` · **Output:** DOCX

## What it does

Turns a changelog, PR list, or bullet notes into customer- or developer-facing **release notes** DOCX.

## Before you start

- Task type: **Office**
- Optional: `CHANGELOG.md`, release ticket export, or notes in `inbox/`
- In a code repo workspace, the agent can `read_file` the project changelog

## How to run

1. Tap **Release notes** or ask:
   > Write release notes for v0.7.0 for external customers.
2. Confirm: product name, version, release date, audience (internal / customer / developer).
3. DOCX in `deliverables/`.

## Sections (typical)

**Version summary** · new features · improvements · fixes · known issues · upgrade guide

## Verify

- Version number and date are correct
- Feature list matches your source changelog (no hallucinated items)

## Tips

- Point the workspace at your product repo so `read_file` can ingest `CHANGELOG.md`.
- Ask for a shorter "email blast" section inside the same DOCX.

Related: [Project report PPT](/docs/office/skills/project-report) · [File tools](/docs/tools/files)
