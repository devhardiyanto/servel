#!/usr/bin/env bash
# Pastikan AppImage tidak membutuhkan glibc lebih baru dari batas yang
# didukung. AppImage membawa library dari mesin build, jadi runner yang lebih
# baru diam-diam menaikkan syarat glibc bagi pengguna.
#
#   scripts/check-glibc.sh <file.AppImage> <versi-maks, mis. 2.35>
set -euo pipefail
APPIMAGE="${1:?usage: check-glibc.sh <file.AppImage> <max-glibc>}"
MAX="${2:?usage: check-glibc.sh <file.AppImage> <max-glibc>}"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
APPIMAGE="$(realpath "$APPIMAGE")"
(cd "$WORK" && "$APPIMAGE" --appimage-extract >/dev/null)

NEED="$(find "$WORK/squashfs-root" -type f \( -name '*.so*' -o -perm -u+x \) -exec sh -c 'file -b "$1" | grep -q ELF && objdump -T "$1" 2>/dev/null' _ {} \; \
  | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -n1)"
echo "Highest glibc required: ${NEED:-none} (limit ${MAX})"
if [ -n "$NEED" ] && [ "$(printf '%s\n%s\n' "$NEED" "$MAX" | sort -V | tail -n1)" != "$MAX" ]; then
  echo "::error::AppImage needs glibc $NEED, above the supported $MAX. Build Linux on an older runner."
  exit 1
fi
