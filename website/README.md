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
| `https://zagens.com/download/stats.json` | Public download counter (updated on VPS from Nginx logs) |
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

## Hosting installers (private repo)

Installers live under **`public/download/`** (served as `https://zagens.com/download/*`). Update `src/data/release.json` when bumping version — no public GitHub Release required.

Optional: `npm run sync:manifest` still works if you later publish GitHub Releases (needs `gh` CLI).

## Download counter

The home and download pages fetch **`/download/stats.json`** (client-side). On the VPS, Nginx logs `.exe` / `.zip` hits to `zagens-download.log`; cron runs `deploy/update-download-stats.sh` (or `npm run stats:aggregate`) to refresh the JSON. CI **does not** overwrite `stats.json` on deploy (`--exclude='download/stats.json'`). See [`deploy/nginx-zagens.conf.example`](deploy/nginx-zagens.conf.example) and [`docs/website/DEPLOY.md`](../docs/website/DEPLOY.md).

## Deploy (GitHub Actions → VPS)

Push to **`main`** with changes under `website/` triggers [`.github/workflows/website.yml`](../.github/workflows/website.yml): `npm ci && npm run build`, then **rsync** `dist/` to your server.

**First-time server setup, SSH secrets, and Nginx:** [`docs/website/DEPLOY.md`](../docs/website/DEPLOY.md) and [`deploy/nginx-zagens.conf.example`](deploy/nginx-zagens.conf.example).

Optional: run `npm run sync:manifest` in CI before `build` when `RELEASE_TAG` is set (add a workflow step if needed).

## Assets

- `public/icon.png`, `favicon.ico` — copied from `crates/desktop/icons/`
- `public/screenshots/hero.png` — from repo `assets/screenshot.png` (replace with real Zagens UI shot before PH launch)

## Launch checklist

- [x] Support email: `didclawapp@gmail.com` (Privacy / Terms / Footer)
- [x] Hero screenshot — Zagens desktop UI
- [x] DNS → zagens.com
- [ ] Product Hunt assets (1270×760 gallery, OG 1200×630) — in progress
- [ ] Legal review of Privacy/Terms (optional)
- [x] `public/download/latest.json` + updater pubkey — see [`docs/desktop/UPDATER.md`](../docs/desktop/UPDATER.md)
- [ ] Upload signed `*-setup.exe` + `.sig` to `public/download/` and run `npm run sync:manifest`
