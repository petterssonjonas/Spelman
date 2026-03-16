#!/usr/bin/env python3
"""Sync version from Cargo.toml to all packaging files. Run from repo root.

Usage: ./scripts/sync-version.py
"""
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent


def get_version() -> str:
    cargo_toml = REPO_ROOT / "Cargo.toml"
    text = cargo_toml.read_text()
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
    if not m:
        sys.exit("Could not find version in Cargo.toml")
    return m.group(1)


def main():
    version = get_version()
    updated: list[str] = []

    # packaging/spelman.spec
    spec = REPO_ROOT / "packaging" / "spelman.spec"
    if spec.exists():
        s = spec.read_text()
        s = re.sub(r"^Version:\s*\S+", f"Version:        {version}", s, count=1, flags=re.MULTILINE)
        spec.write_text(s)
        updated.append("spelman.spec")

    # packaging/PKGBUILD
    pkgbuild = REPO_ROOT / "packaging" / "PKGBUILD"
    if pkgbuild.exists():
        s = pkgbuild.read_text()
        s = re.sub(r"^pkgver=.*$", f"pkgver={version}", s, count=1, flags=re.MULTILINE)
        pkgbuild.write_text(s)
        updated.append("PKGBUILD")

    # packaging/com.github.spelman.yml (Flatpak)
    flatpak = REPO_ROOT / "packaging" / "com.github.spelman.yml"
    if flatpak.exists():
        s = flatpak.read_text()
        s = re.sub(
            r"(archive/refs/tags/v)[0-9]+\.[0-9]+\.[0-9]+",
            rf"\g<1>{version}",
            s,
        )
        flatpak.write_text(s)
        updated.append("com.github.spelman.yml")

    print(f"Synced version to {version} in: {', '.join(updated) if updated else '(no files found)'}")


if __name__ == "__main__":
    main()
