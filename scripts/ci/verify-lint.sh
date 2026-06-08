#!/usr/bin/env bash
# Mirror the CI "Lint" job locally before push. Exits non-zero on fmt/clippy drift.
set -euo pipefail

cd "$(dirname "$0")/../.."

export CARGO_TERM_COLOR=always
export RUSTFLAGS=-Dwarnings

expected_channel="$(grep -E '^channel = ' rust-toolchain.toml | sed -E 's/^channel = "([^"]+)".*/\1/')"
actual_version="$(rustc --version | awk '{print $2}')"
if [[ "${actual_version}" != "${expected_channel}"* ]]; then
  echo "::error::rustc ${actual_version} does not match rust-toolchain.toml (${expected_channel})." >&2
  echo "Run: rustup toolchain install ${expected_channel} && rustup default ${expected_channel}" >&2
  exit 1
fi

echo "==> Toolchain: $(rustc --version)"
bash scripts/ci/ensure-web-ui-dist.sh
echo "==> Pre-build runtime sidecar (desktop build.rs)"
cargo build -p zagens-cli --locked
echo "==> cargo fmt --all -- --check"
cargo fmt --all -- --check
echo "==> cargo clippy --workspace --all-targets --all-features --locked -- -D warnings"
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
echo "verify-lint: OK"
