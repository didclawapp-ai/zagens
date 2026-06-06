# SmartScreen & safe install

Windows installers are **not Authenticode-signed** yet. SmartScreen may show **"Windows protected your PC"** — this is normal for unsigned publishers, not a malware verdict.

## Recommended: zip + Unblock

1. Download `Zagens_<version>_x64-setup.exe.zip` from [Download](/download).
2. Right-click the **zip** → **Properties** → check **Unblock** → OK.
3. Extract the zip.
4. Run the `*-setup.exe` inside.

Unblocking the zip **before** extract avoids Mark of the Web on the installer — often **no SmartScreen prompt**.

## Alternative: Run anyway

If you run the installer directly:

1. SmartScreen blue screen → **More info**
2. **Run anyway**

## Verify integrity

Compare SHA-256 on the [download](/download) page before running.

## In-app updates

OTA installs use signed builds when available; see [In-app updates](/docs/desktop/updates). Website zip path remains best for first install.

Related: [Install](/docs/install) · [FAQ](/docs/faq#smartscreen)
