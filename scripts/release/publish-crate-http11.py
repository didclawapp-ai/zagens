#!/usr/bin/env python3
"""Publish a packaged crate to crates.io using HTTP/1.1 (avoids cargo's HTTP/2 upload flakes).

Usage:
  python scripts/release/publish-crate-http11.py zagens-cli 0.8.6

Requires:
  - cargo package already run (target/package/<name>-<ver>.crate + unpacked dir)
  - ~/.cargo/credentials.toml with crates-io token
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
import tomllib
import urllib.error
import urllib.request
from pathlib import Path


def load_token() -> str:
    cred = Path.home() / ".cargo" / "credentials.toml"
    raw = cred.read_text(encoding="utf-8")
    data = tomllib.loads(raw)
    # [registry] or [registries.crates-io]
    if "registry" in data and "token" in data["registry"]:
        return data["registry"]["token"]
    regs = data.get("registries", {})
    crates_io = regs.get("crates-io", {})
    if "token" in crates_io:
        return crates_io["token"]
    raise SystemExit(f"no crates-io token in {cred}")


def parse_dep(name: str, spec: dict | str, kind: str, target: str | None) -> dict:
    if isinstance(spec, str):
        version_req = spec
        features: list[str] = []
        optional = False
        default_features = True
        package = None
    else:
        version_req = spec.get("version", "*")
        features = list(spec.get("features", []) or [])
        optional = bool(spec.get("optional", False))
        default_features = bool(spec.get("default-features", True))
        package = spec.get("package")
        # Skip path-only (should already be rewritten)
        if "path" in spec and "version" not in spec:
            raise SystemExit(f"unrewritten path dep: {name}")
    dep = {
        "name": package or name,
        "version_req": version_req,
        "features": features,
        "optional": optional,
        "default_features": default_features,
        "target": target,
        "kind": kind,
        "registry": None,
        "explicit_name_in_toml": name if package else None,
    }
    return dep


def collect_deps(toml: dict) -> list[dict]:
    deps: list[dict] = []
    for kind, key in (
        ("normal", "dependencies"),
        ("dev", "dev-dependencies"),
        ("build", "build-dependencies"),
    ):
        section = toml.get(key, {}) or {}
        for name, spec in section.items():
            deps.append(parse_dep(name, spec, kind, None))

    for target, tables in (toml.get("target") or {}).items():
        for kind, key in (
            ("normal", "dependencies"),
            ("dev", "dev-dependencies"),
            ("build", "build-dependencies"),
        ):
            section = (tables or {}).get(key, {}) or {}
            for name, spec in section.items():
                deps.append(parse_dep(name, spec, kind, target))
    return deps


def build_metadata(pkg_dir: Path) -> dict:
    cargo_toml = tomllib.loads((pkg_dir / "Cargo.toml").read_text(encoding="utf-8"))
    pkg = cargo_toml["package"]
    readme_file = pkg.get("readme")
    readme = None
    if readme_file:
        rp = pkg_dir / readme_file
        if rp.is_file():
            readme = rp.read_text(encoding="utf-8", errors="replace")

    features = cargo_toml.get("features") or {}
    # Ensure feature values are lists of strings
    features = {k: list(v) for k, v in features.items()}

    return {
        "name": pkg["name"],
        "vers": pkg["version"],
        "deps": collect_deps(cargo_toml),
        "features": features,
        "authors": list(pkg.get("authors") or []),
        "description": pkg.get("description"),
        "documentation": pkg.get("documentation"),
        "homepage": pkg.get("homepage"),
        "readme": readme,
        "readme_file": readme_file,
        "keywords": list(pkg.get("keywords") or []),
        "categories": list(pkg.get("categories") or []),
        "license": pkg.get("license"),
        "license_file": pkg.get("license-file"),
        "repository": pkg.get("repository"),
        "badges": pkg.get("badges") or {},
        "links": pkg.get("links"),
        "rust_version": pkg.get("rust-version"),
    }


def publish(name: str, version: str, root: Path) -> None:
    pkg_dir = root / "target" / "package" / f"{name}-{version}"
    crate_path = root / "target" / "package" / f"{name}-{version}.crate"
    if not pkg_dir.is_dir() or not crate_path.is_file():
        raise SystemExit(
            f"missing package artifacts under target/package/ — run:\n"
            f"  cargo package -p {name} --no-verify"
        )

    meta = build_metadata(pkg_dir)
    meta_bytes = json.dumps(meta, separators=(",", ":"), ensure_ascii=False).encode("utf-8")
    crate_bytes = crate_path.read_bytes()

    body = (
        struct.pack("<I", len(meta_bytes))
        + meta_bytes
        + struct.pack("<I", len(crate_bytes))
        + crate_bytes
    )

    token = load_token()
    req = urllib.request.Request(
        "https://crates.io/api/v1/crates/new",
        data=body,
        method="PUT",
        headers={
            "Content-Type": "application/octet-stream",
            "Accept": "application/json",
            "Authorization": token,
            "User-Agent": "zagens-publish-http11/0.1 (urllib)",
        },
    )

    print(f"Uploading {name} {version} ({len(crate_bytes)} bytes crate, {len(body)} total) via HTTP/1.1…")
    try:
        with urllib.request.urlopen(req, timeout=600) as resp:
            payload = resp.read().decode("utf-8", errors="replace")
            print(f"HTTP {resp.status}: {payload}")
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf-8", errors="replace")
        print(f"HTTP {e.code}: {detail}", file=sys.stderr)
        raise SystemExit(1) from e


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("name")
    ap.add_argument("version")
    ap.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
    )
    args = ap.parse_args()
    publish(args.name, args.version, args.root)


if __name__ == "__main__":
    main()
