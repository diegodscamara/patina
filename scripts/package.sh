#!/usr/bin/env bash
# Builds Patina.app, embeds Sparkle, signs it, and wraps it in a DMG.
# With a Sparkle private key it also writes the appcast for auto-updates.
#
# Runs on macOS. Used by .github/workflows/release.yml, and works locally too:
#
#   scripts/package.sh            # universal build, ad-hoc signed DMG in dist/
#
# Optional environment:
#   PATINA_VERSION         version string (default: from Cargo.toml)
#   PATINA_FEED_URL        Sparkle appcast URL (default: this repo's latest release)
#   SPARKLE_PUBLIC_KEY     EdDSA public key written to Info.plist
#   SPARKLE_PRIVATE_KEY    EdDSA private key; when set, dist/appcast.xml is written
#   DEVELOPER_ID           "Developer ID Application: Name (TEAMID)"; ad-hoc signing when unset
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD   notarize when all three are set
#   PATINA_ARCHS           "universal" (default) or "native"
set -euo pipefail

REPO="diegodscamara/patina"
SPARKLE_VERSION="2.7.1"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/Patina.app"

VERSION="${PATINA_VERSION:-$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)}"
FEED_URL="${PATINA_FEED_URL:-https://github.com/$REPO/releases/latest/download/appcast.xml}"
DMG="$DIST/Patina-$VERSION.dmg"

log() { printf '\n==> %s\n' "$*"; }

rm -rf "$DIST"
mkdir -p "$DIST"

# --- Build ---------------------------------------------------------------
if [[ "${PATINA_ARCHS:-universal}" == "universal" ]]; then
  log "Building universal binary"
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  cargo build --release --locked --target aarch64-apple-darwin
  cargo build --release --locked --target x86_64-apple-darwin
  lipo -create -output "$DIST/patina" \
    "$ROOT/target/aarch64-apple-darwin/release/patina" \
    "$ROOT/target/x86_64-apple-darwin/release/patina"
else
  log "Building for this Mac only"
  cargo build --release --locked
  cp "$ROOT/target/release/patina" "$DIST/patina"
fi

# --- Bundle --------------------------------------------------------------
log "Assembling Patina.app ($VERSION)"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$APP/Contents/Frameworks"
mv "$DIST/patina" "$APP/Contents/MacOS/patina"
cp "$ROOT/assets/icon/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
sed -e "s|@VERSION@|$VERSION|g" \
    -e "s|@FEED_URL@|$FEED_URL|g" \
    -e "s|@ED_PUBLIC_KEY@|${SPARKLE_PUBLIC_KEY:-}|g" \
    "$ROOT/packaging/Info.plist" > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"

# --- Sparkle -------------------------------------------------------------
# Kept outside target/: CI's Rust cache saves target/ and restores the framework
# without its symlinks, which codesign then rejects.
SPARKLE_DIR="$ROOT/.build/sparkle-$SPARKLE_VERSION"
if [[ ! -L "$SPARKLE_DIR/Sparkle.framework/Versions/Current" ]]; then
  rm -rf "$SPARKLE_DIR"
  log "Downloading Sparkle $SPARKLE_VERSION"
  mkdir -p "$SPARKLE_DIR"
  curl -fsSL "https://github.com/sparkle-project/Sparkle/releases/download/$SPARKLE_VERSION/Sparkle-$SPARKLE_VERSION.tar.xz" \
    | tar -xJ -C "$SPARKLE_DIR"
fi
ditto "$SPARKLE_DIR/Sparkle.framework" "$APP/Contents/Frameworks/Sparkle.framework"
# Patina loads the framework at runtime from Contents/Frameworks (src/updater.rs),
# so the binary doesn't link against it.

# --- Sign ----------------------------------------------------------------
if [[ -n "${DEVELOPER_ID:-}" ]]; then
  log "Signing with $DEVELOPER_ID"
  SIGN=(codesign --force --timestamp --options runtime --sign "$DEVELOPER_ID")
  FW="$APP/Contents/Frameworks/Sparkle.framework/Versions/B"
  "${SIGN[@]}" "$FW/XPCServices/Installer.xpc"
  "${SIGN[@]}" --preserve-metadata=entitlements "$FW/XPCServices/Downloader.xpc"
  "${SIGN[@]}" "$FW/Autoupdate"
  "${SIGN[@]}" "$FW/Updater.app"
  "${SIGN[@]}" "$APP/Contents/Frameworks/Sparkle.framework"
  "${SIGN[@]}" --entitlements "$ROOT/packaging/Patina.entitlements" "$APP"
else
  log "Signing ad-hoc (no DEVELOPER_ID)"
  codesign --force --deep --sign - "$APP"
fi
codesign --verify --deep --strict "$APP"

# --- DMG -----------------------------------------------------------------
log "Creating $(basename "$DMG")"
STAGE="$DIST/dmg"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/Patina.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "Patina" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null
rm -rf "$STAGE"
[[ -n "${DEVELOPER_ID:-}" ]] && codesign --force --timestamp --sign "$DEVELOPER_ID" "$DMG"

if [[ -n "${DEVELOPER_ID:-}" && -n "${APPLE_ID:-}" && -n "${APPLE_TEAM_ID:-}" && -n "${APPLE_APP_PASSWORD:-}" ]]; then
  log "Notarizing"
  xcrun notarytool submit "$DMG" --wait \
    --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD"
  xcrun stapler staple "$DMG"
fi

# --- Appcast -------------------------------------------------------------
if [[ -n "${SPARKLE_PRIVATE_KEY:-}" ]]; then
  log "Writing appcast.xml"
  KEY_FILE="$(mktemp)"
  trap 'rm -f "$KEY_FILE"' EXIT
  printf '%s' "$SPARKLE_PRIVATE_KEY" > "$KEY_FILE"
  SIGNATURE="$("$SPARKLE_DIR/bin/sign_update" --ed-key-file "$KEY_FILE" "$DMG")"
  PUB_DATE="$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')"
  cat > "$DIST/appcast.xml" <<XML
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Patina</title>
    <item>
      <title>Version $VERSION</title>
      <pubDate>$PUB_DATE</pubDate>
      <link>https://github.com/$REPO/releases/tag/v$VERSION</link>
      <sparkle:version>$VERSION</sparkle:version>
      <sparkle:shortVersionString>$VERSION</sparkle:shortVersionString>
      <sparkle:minimumSystemVersion>13.0</sparkle:minimumSystemVersion>
      <enclosure url="https://github.com/$REPO/releases/download/v$VERSION/$(basename "$DMG")" type="application/octet-stream" $SIGNATURE />
    </item>
  </channel>
</rss>
XML
fi

log "Done: $DMG"
