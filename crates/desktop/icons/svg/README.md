# Zagens icon source

Canonical mark: **Neural Ring** (`zagens-neural-ring.svg`).

## Regenerate bundle icons

From `crates/desktop`:

```bash
# Optional: re-rasterize master PNG (needs @resvg/resvg-js)
# npx --yes --package=@resvg/resvg-js node -e "..."  # or use existing *-1024.png

cargo tauri icon icons/svg/zagens-neural-ring-1024.png
cp icons/icon.png web-ui/public/app-icon.png
```

Tauri writes `icons/32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png`, `icon.ico`, `icon.icns`, plus Store / iOS / Android derivatives.
