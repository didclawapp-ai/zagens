# In-app updates

Zagens checks **zagens.com** for signed Windows builds and can install them from **About**.

## Check for updates

1. Open **About** from the sidebar.
2. Click **Check for updates**.
3. If a newer version is available, choose **Download and install**.
4. Restart when prompted.

On startup, a toast may hint that a newer version exists.

## Update manifest

The app reads `https://zagens.com/download/latest.json` for version, download URL, and signature. This is separate from the **zip** you download manually on the [download page](/download) — OTA uses the signed `.exe`.

## First install vs upgrade

| Channel | Package | Use |
|---------|---------|-----|
| Website zip | `*-setup.exe.zip` | First install (unblock zip, fewer SmartScreen issues) |
| In-app OTA | Signed `*-setup.exe` | Upgrades from an existing install |

## Troubleshooting

- **Signature errors** — install the build from the website instead; see [FAQ](/docs/faq#smartscreen).
- **No update found** — you may already be on the latest release listed on [download](/download).
- Corporate proxies — ensure HTTPS access to `zagens.com`.

Related: [Install](/docs/install) · [System tray](/docs/desktop/tray)
