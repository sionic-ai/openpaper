#!/bin/bash
# Build a universal macOS app and publish it as a GitHub release asset.
#
#   scripts/release.sh v0.2.0 [--skip-build]
#
# The version comes from src-tauri/tauri.conf.json; this script bumps nothing.
# install.sh downloads releases/latest/download/OpenPaper-macos-universal.zip,
# so keep the asset name unchanged.
set -euo pipefail

TAG="${1:?usage: scripts/release.sh <tag> [--skip-build]}"
REPO="${OPENPAPER_REPO:-sionic-ai/openpaper}"
ASSET="OpenPaper-macos-universal.zip"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/src-tauri/target/universal-apple-darwin/release/bundle/macos/OpenPaper.app"
OUT="$ROOT/src-tauri/target/$ASSET"

cd "$ROOT"
if [ "${2:-}" != "--skip-build" ]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  pnpm install --frozen-lockfile
  pnpm tauri build --target universal-apple-darwin
fi
[ -d "$APP" ] || { echo "Missing $APP" >&2; exit 1; }

PLIST="$APP/Contents/Info.plist"
EXE="$(/usr/libexec/PlistBuddy -c 'Print CFBundleExecutable' "$PLIST")"
VERSION="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$PLIST")"
lipo -archs "$APP/Contents/MacOS/$EXE"
codesign --verify --deep "$APP"
[ "$TAG" = "v$VERSION" ] || { echo "Tag $TAG does not match the app version $VERSION (expected v$VERSION)" >&2; exit 1; }
rm -f "$OUT" "$OUT.sha256"
ditto -c -k --sequesterRsrc --keepParent "$APP" "$OUT"
# install.sh refuses a download without a matching checksum file next to it.
(cd "$(dirname "$OUT")" && shasum -a 256 "$ASSET" > "$ASSET.sha256")
cat "$OUT.sha256"

NOTES="OpenPaper $VERSION (macOS universal, ad-hoc signed).

Install or update: \`curl -fsSL https://raw.githubusercontent.com/$REPO/main/install.sh | bash\`"

if gh release view "$TAG" -R "$REPO" >/dev/null 2>&1; then
  # Replacing assets of a published release needs an explicit opt-in.
  [ "${OPENPAPER_CLOBBER:-0}" = 1 ] || { echo "Release $TAG exists. Set OPENPAPER_CLOBBER=1 to replace its assets." >&2; exit 1; }
  gh release upload "$TAG" "$OUT" "$OUT.sha256" -R "$REPO" --clobber
else
  gh release create "$TAG" "$OUT" "$OUT.sha256" -R "$REPO" --title "OpenPaper $VERSION" --notes "$NOTES" --target main
fi
echo "Released $TAG: https://github.com/$REPO/releases/tag/$TAG"
