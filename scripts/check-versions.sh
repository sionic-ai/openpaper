#!/bin/bash
# Checks that every file carrying the app version agrees, so a release bump that misses one fails in CI
# instead of after the macOS release build. With a tag argument (v1.2.3), also checks the tag matches.
#
#   scripts/check-versions.sh [tag]

set -euo pipefail
cd "$(dirname "$0")/.."

pkg="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' package.json)"
conf="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' src-tauri/tauri.conf.json)"
# The first version line in Cargo.toml is the [package] one.
cargo="$(sed -n 's/^version = "\(.*\)"$/\1/p' src-tauri/Cargo.toml | head -1)"
lock="$(sed -n '/^name = "openpaper"$/{n;s/^version = "\(.*\)"$/\1/p;}' src-tauri/Cargo.lock)"

printf '%-26s %s\n' package.json "${pkg:-missing}" src-tauri/tauri.conf.json "${conf:-missing}" \
  src-tauri/Cargo.toml "${cargo:-missing}" src-tauri/Cargo.lock "${lock:-missing}"

fail=0
for v in "$conf" "$cargo" "$lock"; do
  [ -n "$pkg" ] && [ "$v" = "$pkg" ] || fail=1
done
if [ "$fail" = 1 ]; then
  echo "The app version differs between these files. Set the same version in all of them." >&2
  exit 1
fi
if ! grep -qx "## $pkg" CHANGELOG.md; then
  echo "CHANGELOG.md has no \"## $pkg\" section. Add what changed; the app shows it after an update." >&2
  exit 1
fi
if [ $# -gt 0 ] && [ "$1" != "v$pkg" ]; then
  echo "Tag $1 does not match the app version $pkg (expected v$pkg)." >&2
  exit 1
fi
echo "All versions are $pkg."
