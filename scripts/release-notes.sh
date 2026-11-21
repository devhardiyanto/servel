#!/usr/bin/env bash
# Susun isi draft rilis GitHub untuk satu tag.
#
#   scripts/release-notes.sh <tag> [tag-sebelumnya] > notes.md
#
# Isi (berurutan):
#   1. Catatan dari docs/releases/v*.md untuk SEMUA versi di atas tag
#      sebelumnya sampai tag ini, terbaru dulu. Rilis yang menggabungkan
#      beberapa versi (mis. v3.11.0–v3.14.0 dalam satu tag) mendapat semua
#      catatannya sekaligus.
#   2. Daftar commit sejak tag sebelumnya (dilipat).
#   3. Cara instal.
#
# Draft tetap untuk disunting manual sebelum dipublikasikan; skrip ini hanya
# menyiapkan bahan awalnya.
set -euo pipefail

TAG="${1:?usage: release-notes.sh <tag> [previous-tag]}"
PREV="${2:-}"
if [ -z "$PREV" ]; then
  PREV="$(git describe --tags --abbrev=0 "${TAG}^" 2>/dev/null || true)"
fi
REPO="${GITHUB_REPOSITORY:-devhardiyanto/servel}"

ver() { echo "${1#v}"; }
# true bila $1 > $2 (semver, tanpa prerelease).
gt() { [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" = "$1" ]; }
le() { [ "$1" = "$2" ] || gt "$2" "$1"; }

CUR="$(ver "$TAG")"
LOW="$(ver "${PREV:-v0.0.0}")"

# Versi yang punya catatan dan masuk rentang (LOW, CUR], terbaru dulu.
mapfile -t VERSIONS < <(
  for f in docs/releases/v*.md; do
    [ -e "$f" ] || continue
    v="$(basename "$f" .md)"; v="${v#v}"
    if gt "$v" "$LOW" && le "$v" "$CUR"; then echo "$v"; fi
  done | sort -V -r
)

if [ "${#VERSIONS[@]}" -eq 0 ]; then
  echo "> Belum ada catatan di docs/releases/ untuk ${TAG}. Tulis ringkasan perubahan di sini."
  echo
elif [ "${#VERSIONS[@]}" -gt 1 ]; then
  echo "Rilis ini menggabungkan ${#VERSIONS[@]} versi: $(printf 'v%s, ' "${VERSIONS[@]}" | sed 's/, $//')."
  echo
fi

for v in "${VERSIONS[@]}"; do
  f="docs/releases/v${v}.md"
  if [ "${#VERSIONS[@]}" -gt 1 ]; then
    echo "## v${v}"
    echo
    # Judul "# Kova vX" dibuang; heading di dalamnya turun satu tingkat.
    sed -e '1{/^# /d}' -e 's/^## /### /' "$f" | sed -e '1{/^$/d}'
  else
    sed -e '1{/^# /d}' "$f" | sed -e '1{/^$/d}'
  fi
  echo
done

if [ -n "$PREV" ]; then
  RANGE="${PREV}..${TAG}"
else
  RANGE="${TAG}"
fi
echo "<details><summary>Commits (${RANGE})</summary>"
echo
git log --pretty='* %s' "$RANGE"
echo
echo "</details>"
echo
cat <<EOF
## Install

- **Windows:** download \`.exe\` (NSIS installer), run, follow prompts.
- **macOS:** download \`.dmg\`, open, drag Kova.app to Applications.
- **Linux:** download \`.AppImage\`, \`chmod +x\`, run. Needs Docker Engine with the Compose v2 plugin; Docker Desktop is not required.

See [README](https://github.com/${REPO}#readme) for prerequisites (Windows: WSL 3.0.1+ for built-in WSL containers, or Docker Desktop; macOS: Docker Desktop; Linux: Docker Engine).
EOF
