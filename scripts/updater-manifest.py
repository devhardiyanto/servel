#!/usr/bin/env python3
"""Susun latest.json (manifest updater Tauri) dari aset sebuah rilis GitHub.

Dipakai job `updater-manifest` di .github/workflows/release.yml. Bisa juga
dijalankan manual untuk memperbaiki manifest rilis yang sudah terbit:

  gh api repos/OWNER/REPO/releases/tags/v3.2.1 > release.json
  # unduh semua aset *.sig ke folder sigs/
  python3 scripts/updater-manifest.py --version 3.2.1 --repo OWNER/REPO \
      --release release.json --sigs sigs --out latest.json
"""
import argparse
import datetime
import json
import os
import sys

# (akhiran nama aset, kunci platform di manifest)
TARGETS = [
    ("_x64-setup.exe", ["windows-x86_64", "windows-x86_64-nsis"]),
    ("_amd64.AppImage", ["linux-x86_64", "linux-x86_64-appimage"]),
    ("_x64.app.tar.gz", ["darwin-x86_64", "darwin-x86_64-app"]),
    ("_aarch64.app.tar.gz", ["darwin-aarch64", "darwin-aarch64-app"]),
]


def build(version, repo, asset_names, sigs_dir):
    """Kembalikan (manifest, daftar masalah)."""
    platforms = {}
    problems = []
    for suffix, keys in TARGETS:
        matches = [n for n in asset_names if n.endswith(suffix)]
        if len(matches) != 1:
            problems.append(f"expected exactly one asset ending in {suffix}, found {matches}")
            continue
        name = matches[0]
        sig_path = os.path.join(sigs_dir, name + ".sig")
        if not os.path.isfile(sig_path):
            problems.append(f"signature {name}.sig is missing")
            continue
        with open(sig_path, encoding="utf-8") as f:
            signature = f.read().strip()
        if not signature:
            problems.append(f"signature {name}.sig is empty")
            continue
        # releases/latest/download: sama dengan manifest v3.x yang sudah
        # terbit. Updater hanya membaca manifest dari rilis terbaru, jadi
        # URL ini selalu menunjuk aset rilis yang sama.
        url = f"https://github.com/{repo}/releases/latest/download/{name}"
        for key in keys:
            platforms[key] = {"signature": signature, "url": url}
    now = datetime.datetime.now(datetime.timezone.utc)
    manifest = {
        "version": version,
        "notes": "",
        "pub_date": now.strftime("%Y-%m-%dT%H:%M:%S.") + f"{now.microsecond // 1000:03d}Z",
        "platforms": platforms,
    }
    return manifest, problems


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--version", required=True)
    ap.add_argument("--repo", required=True)
    ap.add_argument("--release", required=True, help="JSON rilis dari GitHub API")
    ap.add_argument("--sigs", required=True, help="folder berisi aset *.sig")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    with open(a.release, encoding="utf-8") as f:
        names = [x["name"] for x in json.load(f)["assets"]]
    manifest, problems = build(a.version, a.repo, names, a.sigs)
    if problems:
        for p in problems:
            print(f"::error::{p}", file=sys.stderr)
        return 1
    with open(a.out, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
