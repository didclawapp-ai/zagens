# Repository split: product vs website

Zagens uses a **two-repo** layout: this repository ships the desktop app and runtime; the website repo hosts user docs, downloads, and platform services.

| Repository | Role | In this repo? |
|------------|------|---------------|
| **zagens** (this repo) | Desktop shell, embedded runtime, harness fixtures & contributor docs | Yes |
| **zagens_website** | Official site, CMS, installers on zagens.com, user documentation | No — separate private repo |

## Directory mapping

| Content | Location |
|---------|----------|
| `crates/`, public `docs/` | This repo |
| User guides (`content/docs/`) | Website repo |
| `website/`, `docs/website/`, `scripts/website/` | Website repo only (ignored here — see `.gitignore`) |

## Local development

**Product (this repo):**

```bash
cd crates/desktop
cargo tauri dev
```

**Website:** clone and run the website repository separately (not part of this workspace).

## Releases

1. Tag `zagens-v*` in this repo → [`.github/workflows/release.yml`](../.github/workflows/release.yml) builds the Windows installer and publishes a GitHub Release.
2. The website repo syncs release assets and updates the public download manifest (maintainer workflow).

Installer binaries are **not** committed to this repository.

## User documentation

- **Edit:** website repo `content/docs/{en,zh-Hans}/`
- **Live:** [zagens.com/docs](https://zagens.com/docs)
- **Pointer in this repo:** [`docs/user/README.md`](./user/README.md)
