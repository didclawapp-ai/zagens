# Zagens website

Static marketing and download hub for [Zagens](https://zagens.com) preview releases.

Built with **Astro 5** + **Tailwind CSS**. Deploy output is fully static (`website/dist/`).

## Pages

| Route | Purpose |
|-------|---------|
| `/` | Home (English default) |
| `/zh-Hans/` | Home (简体中文) |
| `/download` | Windows download + SHA-256 |
| `/install` | SmartScreen / zip unblock guide |
| `/privacy` | Privacy policy (draft) |
| `/terms` | Terms of use (draft) |

Localized routes mirror under `/zh-Hans/*`.

## URLs consumed by the desktop app

Keep these paths stable:

| URL | Consumer |
|-----|----------|
| `https://zagens.com/download/latest.json` | Tauri updater (`tauri.conf.json`, `updateConfig.ts`) |
| `https://zagens.com/download/*` | OTA download base |
| `https://zagens.com` | Installer README / help link |

## Development

```bash
cd website
npm install
npm run dev
```

Open http://localhost:4321

## Build

```bash
npm run build
npm run preview
```

## Sync download links after a GitHub Release

After tagging `zagens-v0.6.0-preview.x` and publishing assets:

```bash
cd website
GITHUB_REPO=owner/repo RELEASE_TAG=zagens-v0.6.0-preview.1 npm run sync:manifest
npm run build
```

With `gh` CLI authenticated, the script pulls asset URLs and computes SHA-256. Without `gh`, it writes GitHub download URL templates.

`public/download/latest.json` is regenerated for the Tauri updater. **Updater signatures are empty until `pubkey` is configured** in the desktop crate — manual download remains the primary path for preview.

## Deploy (GitHub Actions → VPS)

Push to **`main`** with changes under `website/` triggers [`.github/workflows/website.yml`](../.github/workflows/website.yml): `npm ci && npm run build`, then **rsync** `dist/` to your server.

**First-time server setup, SSH secrets, and Nginx:** [`docs/website/DEPLOY.md`](../docs/website/DEPLOY.md) and [`deploy/nginx-zagens.conf.example`](deploy/nginx-zagens.conf.example).

Optional: run `npm run sync:manifest` in CI before `build` when `RELEASE_TAG` is set (add a workflow step if needed).

## Assets

- `public/icon.png`, `favicon.ico` — copied from `crates/desktop/icons/`
- `public/screenshots/hero.png` — from repo `assets/screenshot.png` (replace with real Zagens UI shot before PH launch)

## Before public launch

- [ ] Replace privacy/terms placeholders (support email, legal review)
- [ ] Run `sync:manifest` against real GitHub Release
- [ ] Swap hero screenshot for current Zagens desktop UI
- [ ] Point DNS to static host
- [ ] Add PH-sized OG image (`public/og-image.png`, 1200×630)
