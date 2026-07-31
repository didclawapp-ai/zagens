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
  const zagensLicenseSrc = join(workspaceRoot, 'LICENSE');
  const runtimeLicenseSrc = join(workspaceRoot, 'third-party', 'deepseek-tui', 'LICENSE');
  const zagensLicenseDest = join(outDir, 'zagens-LICENSE.txt');
  const runtimeLicenseDest = join(outDir, 'deepseek-tui-runtime-LICENSE.txt');

  mkdirSync(outDir, { recursive: true });
  copyFileSync(zagensLicenseSrc, zagensLicenseDest);
  copyFileSync(runtimeLicenseSrc, runtimeLicenseDest);

  const notices = `Zagens — License & Third-Party Notices
==========================================

This folder ships with the Zagens desktop application.

Zagens (desktop shell + embedded runtime)
-----------------------------------------
License:  MIT — full text in zagens-LICENSE.txt
Copyright (c) 2024-2026 Zagens Contributors

Embedded agent runtime lineage (deepseek-tui / CodeWhale)
---------------------------------------------------------
Version:  ${runtimeVersion} (embedded Rust workspace crates)
License:  MIT — full text in deepseek-tui-runtime-LICENSE.txt
Copyright (c) 2024-2025 DeepSeek CLI Contributors

Per the MIT License, the copyright and permission notices above must be
retained in copies or substantial portions of the corresponding components.

Other dependencies (Rust crates, npm packages, Tauri, React, etc.)
are subject to their respective upstream licenses.
`;

  writeFileSync(join(outDir, 'THIRD-PARTY-NOTICES.txt'), notices, 'utf8');
  console.log(`[legal] Staged runtime MIT license + notices (runtime ${runtimeVersion}) → ${outDir}`);
}
