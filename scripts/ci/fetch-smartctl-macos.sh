#!/usr/bin/env bash
# Fetch the official smartmontools macOS build and extract smartctl.
# Usage: fetch-smartctl-macos.sh <output-file> [smartmontools-version]
# Output: a (universal) smartctl binary at <output-file>.
set -euo pipefail

OUT="${1:?usage: $0 <output-file> [version]}"
VERSION="${2:-${SMARTMONTOOLS_VERSION:-7.5}}"
TAG="RELEASE_${VERSION//./_}"
DMG_NAME="smartmontools-${VERSION}.dmg"
# 7.4 and older ship as "<ver>-1.dmg"
if [[ "$VERSION" != "7.5" && "${VERSION%%.*}" == "7" && "${VERSION#7.}" -lt 5 ]] 2>/dev/null; then
  DMG_NAME="smartmontools-${VERSION}-1.dmg"
fi
URL="https://github.com/smartmontools/smartmontools/releases/download/${TAG}/${DMG_NAME}"

WORK="$(mktemp -d)"
MNT="$WORK/mnt"
mkdir -p "$MNT"
cleanup() {
  hdiutil detach "$MNT" -quiet -force 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

echo "Downloading $URL"
curl -fsSL --retry 3 -o "$WORK/sm.dmg" "$URL"
hdiutil attach "$WORK/sm.dmg" -nobrowse -readonly -quiet -mountpoint "$MNT"

PKG="$(find "$MNT" -maxdepth 2 -name '*.pkg' | head -n1)"
[[ -n "$PKG" ]] || { echo "no .pkg found in dmg" >&2; exit 1; }
pkgutil --expand-full "$PKG" "$WORK/expanded"

SMARTCTL="$(find "$WORK/expanded" -type f -name smartctl -path '*sbin*' | head -n1)"
[[ -n "$SMARTCTL" ]] || { echo "smartctl not found in pkg" >&2; exit 1; }

mkdir -p "$(dirname "$OUT")"
cp "$SMARTCTL" "$OUT"
chmod 755 "$OUT"
xattr -cr "$OUT" 2>/dev/null || true

echo "--- $OUT"
file "$OUT"
lipo -info "$OUT" || true
