<p align="center"><img src="assets/icon/icon-1024.png" width="128" alt="Patina icon"></p>

# Patina

A small, native Spotify player for macOS, written in Rust. Built for one listener, not published anywhere.

- **Playback** streams straight from Spotify through [librespot](https://github.com/librespot-org/librespot). You need a Premium account.
- **Library** (liked songs, playlists) comes from the Spotify Web API, using tokens from the same session.
- **UI** is [iced](https://iced.rs), drawn on the GPU. Colours follow the [design canvas](https://claude.ai/artifact/HXPVor3ZP1D8TJmzfexMvi): warm graphite with a Willys olive accent.
- **Updates** install themselves through [Sparkle 2](https://sparkle-project.org), fed from this repo's GitHub Releases.

## Run it

```sh
cargo run
```

Click **Sign in with Spotify**. Your browser opens Spotify's login page and redirects back to `http://127.0.0.1:8898/login`. After that, librespot keeps reusable credentials in `~/Library/Caches/dev.diegodscamara.Patina`, so later launches connect straight away.

Sign-in uses librespot's own client ID rather than your developer app, because Spotify only grants the `streaming` scope to its own clients. Your developer app is still useful later if we want Web API calls on a separate rate limit.

## Install

Grab `Patina-<version>.dmg` from [Releases](https://github.com/diegodscamara/patina/releases) and drag Patina into Applications.

Until releases are signed with a Developer ID, macOS blocks the first launch. Clear the quarantine flag once:

```sh
xattr -dr com.apple.quarantine /Applications/Patina.app
```

Updates installed by Sparkle after that don't need this.

## Release

1. Bump `version` in `Cargo.toml` and merge.
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
3. The **Release** workflow builds a universal app, signs it, makes the DMG, writes `appcast.xml`, and publishes both to a GitHub release.

The installed app checks `releases/latest/download/appcast.xml` once a day and installs new versions in the background. **Settings → Check for updates** checks right away.

### One-time setup for auto-update

Sparkle only installs updates signed with your EdDSA key.

1. Download [Sparkle 2.7.1](https://github.com/sparkle-project/Sparkle/releases/tag/2.7.1) and run `./bin/generate_keys` on your Mac. It stores the private key in your Keychain and prints the public key.
2. Export the private key: `./bin/generate_keys -x sparkle_private_key.txt`.
3. In this repo's **Settings → Secrets and variables → Actions**:
   - add a **variable** `SPARKLE_PUBLIC_KEY` with the public key,
   - add a **secret** `SPARKLE_PRIVATE_KEY` with the contents of `sparkle_private_key.txt`, then delete the file.

### Optional: Developer ID signing and notarization

Without these, releases are ad-hoc signed, which is fine for personal use. With a paid Apple Developer account, add these secrets and the workflow signs and notarizes automatically:

| Secret | Value |
| --- | --- |
| `DEVELOPER_ID` | `Developer ID Application: Your Name (TEAMID)` |
| `DEVELOPER_ID_P12` | the certificate exported as .p12, base64 encoded |
| `DEVELOPER_ID_P12_PASSWORD` | the .p12 password |
| `APPLE_ID`, `APPLE_TEAM_ID`, `APPLE_APP_PASSWORD` | for `notarytool` (use an app-specific password) |

## Package locally

```sh
PATINA_ARCHS=native scripts/package.sh   # dist/Patina-<version>.dmg
```

## Layout

| Path | What it is |
| --- | --- |
| `src/main.rs` | App state, update loop and views |
| `src/spotify.rs` | Sign-in, librespot player, Web API calls |
| `src/updater.rs` | Starts Sparkle when running from the installed app |
| `src/theme.rs` | Colours |
| `assets/icon/` | Worn groove icon (SVG source, 1024 px PNG, `.icns`, menu bar glyph) |
| `packaging/` | `Info.plist` template and entitlements |
| `scripts/package.sh` | Build, bundle, sign, DMG, appcast |

Not affiliated with Spotify.
