# System tray & notifications

Zagens runs as a normal desktop app with a **system tray** icon on Windows.

## Tray behavior

- Minimize or close the window — the app can stay running in the tray (per your OS settings)
- Click the tray icon to restore the main window
- Quit fully from the tray menu when you want to stop the runtime sidecar

## Turn-complete notifications

When a long agent turn finishes while Zagens is **in the background**, a native **toast notification** can alert you (see desktop shell settings).

Useful for Office DOCX generation or Code test runs that take several minutes.

## Runtime sidecar

The tray reflects app process health; agent work still flows through the local **runtime sidecar** on `127.0.0.1`. Check the sidebar connection badge if turns fail silently.

## Tips

- Allow notifications for Zagens in Windows Settings if toasts never appear.
- For unattended runs, keep the machine awake and API key configured.

Related: [Updates](/docs/desktop/updates) · [UI tour](/docs/ui-tour)
