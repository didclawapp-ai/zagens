/**
 * Stage third-party license texts for Tauri `bundle.resources` (MIT compliance).
 * Output: crates/desktop/bundle-legal/ → installed as legal/ next to the app binary.
 */
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const desktopRoot = join(__dirname, '..');
const workspaceRoot = join(desktopRoot, '..', '..');
const outDir = join(desktopRoot, 'bundle-legal');

function readWorkspaceRuntimeVersion() {
  const cargoToml = readFileSync(join(workspaceRoot, 'Cargo.toml'), 'utf8');
  const m = cargoToml.match(/^\[workspace\.package\][\s\S]*?^version = "([^"]+)"/m);
  if (!m) {
    throw new Error('Could not read [workspace.package] version from root Cargo.toml');
  }
  return m[1];
}

export function prepareLegalBundle() {
  const runtimeVersion = readWorkspaceRuntimeVersion();
  const runtimeLicenseSrc = join(workspaceRoot, 'third-party', 'deepseek-tui', 'LICENSE');
  const runtimeLicenseDest = join(outDir, 'deepseek-tui-runtime-LICENSE.txt');

  mkdirSync(outDir, { recursive: true });
  copyFileSync(runtimeLicenseSrc, runtimeLicenseDest);

  const notices = `Zagens — Third-Party Notices
================================

This folder ships with the Zagens desktop application. Zagens itself is
proprietary; see the repository root LICENSE for the product terms.

Embedded agent runtime (zagens-runtime sidecar)
------------------------------------------------
Lineage:  deepseek-tui / CodeWhale (third-party, MIT)
Version:  ${runtimeVersion} (embedded Rust workspace crates)
License:  MIT — full text in deepseek-tui-runtime-LICENSE.txt

Per the MIT License, the copyright and permission notice in
deepseek-tui-runtime-LICENSE.txt must be retained in copies or
substantial portions of the runtime components.

Other dependencies (Rust crates, npm packages, bundled Python, Tauri,
React, etc.) are subject to their respective upstream licenses.
`;

  writeFileSync(join(outDir, 'THIRD-PARTY-NOTICES.txt'), notices, 'utf8');
  console.log(`[legal] Staged runtime MIT license + notices (runtime ${runtimeVersion}) → ${outDir}`);
}
