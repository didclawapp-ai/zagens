# FAQ

## Code vs Office mode

| | **Code** | **Office** |
|---|----------|------------|
| Goal | Engineering in a repo/workspace | DOCX/XLSX/PPTX deliverables |
| Shell | Yes (when enabled) | No |
| Terminal | Yes | No |
| Typical output | Patched source files | `deliverables/` documents |

Switch task type before starting a new thread. See [Task types](/docs/task-types), [Code mode](/docs/code-mode), and [Office overview](/docs/office/overview).

## Runtime not connected / API 500 on docs

The website docs API runs with the full stack. In the [zagens_website](https://github.com/jjlin0603-svg/zagens_website) repo, run `npm run sync:docs` then `npm run dev` (frontend **and** backend). Frontend-only dev yields connection errors.

In the desktop app, check the sidebar **connection** badge — the runtime sidecar must be running on localhost.

## SmartScreen and unsigned builds {#smartscreen}

Installers are not Authenticode-signed yet. **Recommended:** download the zip, **Unblock** it in Properties, then extract and install — often avoids SmartScreen entirely.

Details: [SmartScreen install guide](/docs/help/smartscreen).

## Updates

In-app updates use `latest.json` from the download CDN. See [In-app updates](/docs/desktop/updates) and the [download page](/download) for version history.

## API key storage

Your API key is stored locally. **Zagens defaults to `~/.zagens/config.toml`**; legacy deepseek-tui installs may still use `~/.deepseek/config.toml`. See [Privacy summary](/docs/help/privacy) and [Privacy Policy](/privacy).

## Workspace and deliverables

- **Code:** point workspace at your git repo root.
- **Office:** use `inbox/`, `data/`, `deliverables/` — [Office workspace](/docs/office/workspace).

## Support

Email [didclawapp@gmail.com](mailto:didclawapp@gmail.com) for feedback.
