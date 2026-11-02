#!/bin/bash
# OpenPaper installer for macOS. Run it again to update.
#
#   curl -fsSL https://raw.githubusercontent.com/sionic-ai/openpaper/main/install.sh | bash
#
# Pass options after `bash -s --`. Run with --help for the list.
# Exit codes: 0 ready, 1 error, 2 app installed but Claude Code is not logged in.

set -euo pipefail

REPO="sionic-ai/openpaper"
APP="OpenPaper.app"
OLD_APP="Nanran.app"
ASSET="OpenPaper-macos-universal.zip"
SUPPORT="$HOME/Library/Application Support"

# NANRAN_* are read as a fallback so that the in-app updater of older Nanran builds,
# which still sets NANRAN_DIR and NANRAN_NO_CLAUDE, upgrades to OpenPaper in place.
NO_CLAUDE="${OPENPAPER_NO_CLAUDE:-${NANRAN_NO_CLAUDE:-0}}"
NO_LOGIN="${OPENPAPER_NO_LOGIN:-0}"
NO_OPEN="${OPENPAPER_NO_OPEN:-0}"
DIR="${OPENPAPER_DIR:-${NANRAN_DIR:-}}"
VERSION="${OPENPAPER_VERSION:-latest}"
ACTION=install
PURGE=0

usage() {
  cat <<'EOF'
Usage: curl -fsSL https://raw.githubusercontent.com/sionic-ai/openpaper/main/install.sh | bash -s -- [options]

  --no-claude      skip installing and logging in to Claude Code   (OPENPAPER_NO_CLAUDE=1)
  --no-login       never start the Claude Code login               (OPENPAPER_NO_LOGIN=1)
  --no-open        do not open the app at the end                  (OPENPAPER_NO_OPEN=1)
  --dir <path>     install into this folder                        (OPENPAPER_DIR)
                   default: /Applications, or ~/Applications if that is not writable
  --version <tag>  install this release tag instead of the latest  (OPENPAPER_VERSION)
  --check          print what is installed and change nothing
  --uninstall      remove the app and keep your papers
  --purge          with --uninstall, also delete your papers

Exit codes: 0 ready, 1 error, 2 app installed but Claude Code is not logged in.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --no-claude) NO_CLAUDE=1 ;;
    --no-login) NO_LOGIN=1 ;;
    --no-open) NO_OPEN=1 ;;
    --dir) DIR="${2:?--dir needs a path}"; shift ;;
    --version) VERSION="${2:?--version needs a tag}"; shift ;;
    --check) ACTION=check ;;
    --uninstall) ACTION=uninstall ;;
    --purge) PURGE=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1 (see --help)" >&2; exit 1 ;;
  esac
  shift
done

say() { printf 'OpenPaper: %s\n' "$*"; }
fail() { printf 'OpenPaper error: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || fail "OpenPaper runs on macOS only."
if [ "$(id -u)" -eq 0 ] && [ -n "${SUDO_USER:-}" ]; then
  fail "Run this without sudo. The app and Claude Code install into your user account."
fi
[ "$PURGE" = 0 ] || [ "$ACTION" = uninstall ] || fail "--purge only works together with --uninstall."

# Folders to look in: the --dir folder if given, otherwise the two default locations.
search_dirs() {
  if [ -n "$DIR" ]; then echo "$DIR"; else printf '%s\n' /Applications "$HOME/Applications"; fi
}

find_claude() {
  local c
  for c in "$HOME/.local/bin/claude" "$HOME/.claude/local/claude" "$HOME/.npm-global/bin/claude" \
           /opt/homebrew/bin/claude /usr/local/bin/claude; do
    [ -x "$c" ] && { echo "$c"; return 0; }
  done
  command -v claude 2>/dev/null
}

logged_in() { "$1" auth status 2>/dev/null | grep -q '"loggedIn": *true'; }

# Quit the copy of an app that lives at this exact path, not a dev build or another copy.
# The path is escaped and anchored, so ~/Applications/X.app never matches /Applications/X.app.
quit_app() {
  local pat
  # shellcheck disable=SC2016  # the $ in the sed bracket is a literal character
  pat="^$(printf '%s' "$1/Contents/MacOS/" | sed 's/[][\.*^$(){}+?|/]/\\&/g')"
  if pgrep -f "$pat" >/dev/null 2>&1; then
    say "Quitting $(basename "$1")"
    pkill -f "$pat" || true
    sleep 1
  fi
}

# --check ------------------------------------------------------------------------------
if [ "$ACTION" = check ]; then
  app=""
  while read -r d; do
    [ -d "$d/$APP" ] && { app="$d/$APP"; break; }
  done < <(search_dirs)
  claude="$(find_claude || true)"
  echo "app: ${app:-not installed}"
  if [ -n "$app" ]; then
    v="$(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$app/Contents/Info.plist" 2>/dev/null)" \
      || v=unknown
    echo "app_version: $v"
  fi
  echo "claude: ${claude:-not installed}"
  login=no
  if [ -n "$claude" ] && logged_in "$claude"; then login=yes; fi
  [ -z "$claude" ] || echo "claude_login: $login"
  [ -n "$app" ] || exit 1
  [ "$login" = yes ] || exit 2
  exit 0
fi

# --uninstall --------------------------------------------------------------------------
if [ "$ACTION" = uninstall ]; then
  removed=0
  while read -r d; do
    for a in "$APP" "$OLD_APP"; do
      [ -d "$d/$a" ] || continue
      quit_app "$d/$a"
      rm -rf "${d:?}/$a" || fail "Couldn't remove $d/$a."
      say "Removed $d/$a"
      removed=1
    done
  done < <(search_dirs)
  [ "$removed" = 1 ] || say "No app found to remove."
  if [ "$PURGE" = 1 ]; then
    for data in "$SUPPORT/dev.rlaope.openpaper" "$SUPPORT/dev.rlaope.nanran"; do
      [ -d "$data" ] || continue
      rm -rf "$data" || fail "Couldn't remove $data."
      say "Removed $data"
    done
  elif [ -d "$SUPPORT/dev.rlaope.openpaper" ]; then
    say "Kept your papers in $SUPPORT/dev.rlaope.openpaper (add --purge to delete them)."
  fi
  exit 0
fi

# Install: 1. app ----------------------------------------------------------------------
if [ "$VERSION" = latest ]; then
  URL="https://github.com/$REPO/releases/latest/download/$ASSET"
else
  [[ "$VERSION" =~ ^v?[0-9]+(\.[0-9]+){1,2}([-+][0-9A-Za-z.]+)?$ ]] || fail "Not a release tag: $VERSION"
  URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET"
fi
if [ -z "$DIR" ]; then
  if [ -w /Applications ]; then DIR=/Applications; else DIR="$HOME/Applications"; fi
fi
mkdir -p "$DIR" || fail "Couldn't create $DIR."
DIR="$(cd "$DIR" && pwd -P)"   # resolve symlinks so the running-app match uses the real path

TMP="$(mktemp -d "${TMPDIR:-/tmp}/openpaper-install.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

say "Downloading $VERSION"
curl -fL --proto '=https' --progress-bar --retry 3 -o "$TMP/$ASSET" "$URL" || fail "Couldn't download $URL"
curl -fsSL --proto '=https' --retry 3 -o "$TMP/$ASSET.sha256" "$URL.sha256" || fail "Couldn't download $URL.sha256"
( cd "$TMP" && shasum -a 256 -c "$ASSET.sha256" >/dev/null ) || fail "The download does not match its checksum. Nothing was installed."
ditto -x -k "$TMP/$ASSET" "$TMP/unpacked" || fail "Couldn't unpack the download."
[ -d "$TMP/unpacked/$APP" ] || fail "The download has no $APP in it."

quit_app "$DIR/$APP"
# Any running Nanran must stop: OpenPaper moves its library over on first launch.
for d in "$DIR" /Applications "$HOME/Applications"; do quit_app "$d/$OLD_APP"; done
rm -rf "${DIR:?}/$APP"
ditto "$TMP/unpacked/$APP" "$DIR/$APP" || fail "Couldn't copy the app into $DIR."
# The build is ad-hoc signed, not notarized. Clear the download quarantine so it opens.
xattr -dr com.apple.quarantine "$DIR/$APP" 2>/dev/null || true
say "Installed $DIR/$APP"
if [ -d "$DIR/$OLD_APP" ]; then
  rm -rf "${DIR:?}/$OLD_APP" && say "Removed the old $DIR/$OLD_APP (your papers move over on first launch)"
fi

# 2. Claude Code -----------------------------------------------------------------------
STATUS=0
if [ "$NO_CLAUDE" != 1 ]; then
  CLAUDE="$(find_claude || true)"
  if [ -z "$CLAUDE" ]; then
    say "Installing Claude Code with the official installer"
    curl -fsSL https://claude.ai/install.sh | bash || fail "Couldn't install Claude Code."
    CLAUDE="$(find_claude || true)"
    [ -n "$CLAUDE" ] || fail "Claude Code was installed, but the claude command wasn't found."
  fi
  say "Claude Code: $CLAUDE"

  if logged_in "$CLAUDE"; then
    say "Claude Code is logged in."
  elif [ "$NO_LOGIN" != 1 ] && ( : < /dev/tty ) 2>/dev/null; then
    say "Log in to Claude Code in the browser window that opens."
    "$CLAUDE" auth login < /dev/tty > /dev/tty 2>&1 || true
    logged_in "$CLAUDE" || STATUS=2
  else
    STATUS=2
  fi
  [ "$STATUS" = 0 ] || say "Claude Code is not logged in. Run 'claude auth login', then open OpenPaper."
fi

# 3. Open ------------------------------------------------------------------------------
if [ "$NO_OPEN" != 1 ]; then
  open "$DIR/$APP" || say "Couldn't open the app. Open $DIR/$APP from Finder."
fi
[ "$STATUS" = 0 ] && say "Ready."
exit "$STATUS"
