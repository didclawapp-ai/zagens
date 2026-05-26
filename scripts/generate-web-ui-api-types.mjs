#!/usr/bin/env node
/**
 * Generate TypeScript types from zagens-runtime-v1.openapi.json (D8).
 * Run from repo root after scripts/export-runtime-openapi.ps1.
 */
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const spec = path.join(root, "docs/tech/openapi/zagens-runtime-v1.openapi.json");
const out = path.join(
  root,
  "crates/desktop/web-ui/src/api/generated/runtime-api.ts",
);

const npx = process.platform === "win32" ? "npx.cmd" : "npx";
const r = spawnSync(
  npx,
  [
    "openapi-typescript",
    spec,
    "-o",
    out,
    "--export-type",
    "--immutable",
  ],
  { cwd: root, stdio: "inherit", shell: process.platform === "win32" },
);

if (r.status !== 0) {
  process.exit(r.status ?? 1);
}
console.log(`OK: ${path.relative(root, out)}`);
