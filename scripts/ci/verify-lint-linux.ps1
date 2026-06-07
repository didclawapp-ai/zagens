# Run the CI "Lint" job in a real Linux environment from a Windows dev box.
#
# Why: clippy/rustc only lint code compiled for the *current* target, so
# `#[cfg(unix)]` / Linux-only branches are skipped entirely by the Windows
# `verify-lint.ps1`. Those branches are only checked on CI's ubuntu runner —
# this script reproduces that locally so OS-specific lints don't surprise you
# after push.
#
# Usage:
#   pwsh scripts/ci/verify-lint-linux.ps1                 # auto: WSL, else Docker
#   pwsh scripts/ci/verify-lint-linux.ps1 -Engine wsl     # force WSL
#   pwsh scripts/ci/verify-lint-linux.ps1 -Engine docker  # force Docker (hermetic, slower)
#   pwsh scripts/ci/verify-lint-linux.ps1 -Bootstrap      # one-time: install Linux deps first
#
# Prereqs:
#   WSL    : an Ubuntu/Debian distro with rustup (toolchain auto-installs from
#            rust-toolchain.toml). Run once with -Bootstrap to add apt + Node 20.
#   Docker : Docker Desktop running. Image + deps are provisioned automatically;
#            cargo/target caches use named volumes so repeat runs are faster.

[CmdletBinding()]
param(
    [ValidateSet("auto", "wsl", "docker")]
    [string]$Engine = "auto",
    # WSL distro to use. Default: auto-pick the first real Linux distro,
    # skipping Docker Desktop's `docker-desktop*` distros (no bash).
    [string]$Distro = "",
    [switch]$Bootstrap
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
# Make `wsl` emit UTF-8 instead of UTF-16LE so distro names parse cleanly.
$env:WSL_UTF8 = "1"

# Resolve a usable WSL distro (one that actually has bash). Returns $null if
# none found. Docker Desktop registers `docker-desktop` (often the default)
# which is a minimal distro without bash, so we skip it.
function Resolve-WslDistro {
    if (-not (Get-Command wsl -ErrorAction SilentlyContinue)) { return $null }
    if ($Distro) { return $Distro }
    $names = @()
    try {
        $names = (wsl -l -q 2>$null) |
            ForEach-Object { ($_ -replace "`0", "").Trim() } |
            Where-Object { $_ -and ($_ -notmatch '^docker-desktop') }
    } catch { return $null }
    foreach ($n in $names) {
        wsl -d $n -e bash -lc "true" 2>$null
        if ($LASTEXITCODE -eq 0) { return $n }
    }
    return $null
}

function Test-Wsl {
    return [bool](Resolve-WslDistro)
}

function Test-Docker {
    if (-not (Get-Command docker -ErrorAction SilentlyContinue)) { return $false }
    try { docker info *> $null; return ($LASTEXITCODE -eq 0) } catch { return $false }
}

# Pinned to rust-toolchain.toml so the container matches CI's compiler exactly.
$channel = (Select-String -Path (Join-Path $repoRoot "rust-toolchain.toml") `
        -Pattern '^channel = "([^"]+)"').Matches[0].Groups[1].Value

function Invoke-Wsl {
    $distro = Resolve-WslDistro
    if (-not $distro) { Write-Error "No usable WSL distro (with bash) found. Install Ubuntu, pass -Distro <name>, or use -Engine docker." }
    Write-Host "==> [WSL] distro: $distro"
    $wslPath = (wsl -d $distro wslpath -u "$repoRoot").Trim()
    if (-not $wslPath) { Write-Error "Could not translate '$repoRoot' to a WSL path." }

    if ($Bootstrap) {
        Write-Host "==> [WSL] Bootstrapping Linux deps (apt + Node 20 + rust $channel)"
        $boot = @"
set -euo pipefail
cd '$wslPath'
sudo bash scripts/ci/install-linux-deps.sh
if ! command -v node >/dev/null 2>&1 || [ "`$(node -v | cut -dv -f2 | cut -d. -f1)" -lt 20 ]; then
  curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
  sudo apt-get install -y nodejs
fi
if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  . "`$HOME/.cargo/env"
fi
rustup toolchain install $channel --component clippy,rustfmt
"@
        wsl -d $distro -e bash -lc $boot
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }

    Write-Host "==> [WSL] Running scripts/ci/verify-lint.sh"
    wsl -d $distro -e bash -lc "cd '$wslPath' && bash scripts/ci/verify-lint.sh"
    exit $LASTEXITCODE
}

function Invoke-Docker {
    $image = "rust:$channel-bookworm"
    Write-Host "==> [Docker] $image (cargo/target caches via named volumes)"
    # install-linux-deps.sh uses sudo; in the root container we expose a no-op
    # sudo shim so the same script works unmodified. Node 20 via NodeSource.
    $script = @"
set -euo pipefail
if ! command -v sudo >/dev/null 2>&1; then printf '#!/bin/sh\nexec "`$@"\n' > /usr/local/bin/sudo && chmod +x /usr/local/bin/sudo; fi
apt-get update && apt-get install -y curl
curl -fsSL https://deb.nodesource.com/setup_20.x | bash -
apt-get install -y nodejs
bash scripts/ci/install-linux-deps.sh
rustup component add clippy rustfmt
bash scripts/ci/verify-lint.sh
"@
    docker run --rm `
        -v "${repoRoot}:/work" `
        -v "zagens-cargo-registry:/usr/local/cargo/registry" `
        -v "zagens-linux-target:/work/target" `
        -w /work `
        $image bash -lc $script
    exit $LASTEXITCODE
}

switch ($Engine) {
    "wsl" {
        if (-not (Test-Wsl)) { Write-Error "WSL not available. Install a WSL distro or use -Engine docker." }
        Invoke-Wsl
    }
    "docker" {
        if (-not (Test-Docker)) { Write-Error "Docker not available/running. Start Docker Desktop or use -Engine wsl." }
        Invoke-Docker
    }
    default {
        if (Test-Wsl) { Invoke-Wsl }
        elseif (Test-Docker) { Invoke-Docker }
        else { Write-Error "Neither WSL nor Docker is available. Install one, or push and rely on the CI lint job." }
    }
}
