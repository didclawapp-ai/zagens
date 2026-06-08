# Mirror the CI "Lint" job locally before push (PowerShell).
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")

$env:CARGO_TERM_COLOR = "always"
$env:RUSTFLAGS = "-Dwarnings"

$toolchainFile = Join-Path (Get-Location) "rust-toolchain.toml"
$expected = (Select-String -Path $toolchainFile -Pattern '^channel = "([^"]+)"').Matches[0].Groups[1].Value
$actual = (rustc --version).Split()[1]
if (-not $actual.StartsWith($expected)) {
    Write-Error "rustc $actual does not match rust-toolchain.toml ($expected). Run: rustup toolchain install $expected"
}

Write-Host "==> Toolchain: $(rustc --version)"
& (Join-Path $PSScriptRoot "ensure-web-ui-dist.ps1")
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> Pre-build runtime sidecar (desktop build.rs)"
cargo build -p zagens-cli --locked
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> cargo fmt --all -- --check"
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "==> cargo clippy --workspace --all-targets --all-features --locked -- -D warnings"
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "verify-lint: OK"
