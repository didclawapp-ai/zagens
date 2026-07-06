# H4 · `draft_skill` security checklist (Phase 4.2)

**Status:** v0 · human-in-loop only  
**Scope:** Model may **stage** skills; only a maintainer may **promote** into the catalogue.

---

## Threat model

| Risk | Mitigation |
|------|------------|
| Model installs skills without review | `draft_skill` writes only to `.zagens/skill-drafts/`; `zagens skill promote` required |
| Path traversal / escape | Draft id validated (`[a-z0-9-]{1,64}`); writes confined under workspace meta |
| Arbitrary code in tarball | v0 accepts only `SKILL.md` + optional `harness.toml` via tool args (no companion upload) |
| Invalid harness oracle | `HarnessContract::validate()` before draft write and before promote |
| Symlink / oversized bundle | Promote uses `import_local_directory` size cap + symlink rejection |
| Auto script execution | Promote does **not** write `.trusted`; existing skill trust flow unchanged |
| Dynamic runtime registration | **Not in scope** — no hot-load into tool registry without release |

---

## Maintainer review checklist (before `zagens skill promote`)

- [ ] Read full `SKILL.md` body — no hidden instructions targeting exfiltration or policy bypass
- [ ] If `harness.toml` present: run `zagens gate validate --file .zagens/skill-drafts/<id>/harness.toml`
- [ ] Stage tools in `[[stages]]` are minimal (principle of least exposure)
- [ ] Verify predicates reference registered names only (`zagens gate list` / predicates.md)
- [ ] No bash-only gates as sole path on Windows targets (prefer `command_output_matches` / `exit_code`)
- [ ] Skill id does not collide with bundled system skills
- [ ] After promote: smoke `load_skill name=<id>` and exercise one real task

---

## Artifacts

| Path | Meaning |
|------|---------|
| `.zagens/skill-drafts/<id>/` | Model staging (not in catalogue) |
| `.agents/skills/<id>/` or `~/.agents/skills/<id>/` | Promoted install target |
| `.human-reviewed` | JSON audit record (promote timestamp, draft path) |
| `.installed-from` | Provenance marker (update/uninstall compatible) |

---

## Related

- [skill-manifest-schema.md](../skill-manifest-schema.md)
- [gates/README.md](../gates/README.md)
- [HARNESS_LOOP_ITERATION.md](../../doc_Private/docs/HARNESS_LOOP_ITERATION.md) § H4
