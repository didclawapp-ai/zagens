# Install

Zagens **Windows x64** installers are distributed from this site. They are not yet code-signed; Windows may show SmartScreen warnings.

## Recommended path

1. Download the `.exe` or `.zip` from the [download page](/download).
2. If SmartScreen appears, choose **More info → Run anyway** (see [FAQ](/docs/faq#smartscreen)).
3. Run the installer and launch Zagens from the Start menu.

## Verify the download (optional)

Copy the SHA-256 hash from the download page and compare:

```powershell
Get-FileHash .\Zagens_*_x64-setup.exe -Algorithm SHA256
```

## System requirements

- Windows 10 version 1903+ or Windows 11
- 64-bit (x64) CPU
- Network access for DeepSeek API calls

For the full walkthrough with screenshots, see the [install guide](/install) page.
