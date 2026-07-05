<!-- SPDX-License-Identifier: Apache-2.0 -->

# Tauri Updater Release

ImgConvert uses the Tauri v2 updater with a static `latest.json` hosted on
GitHub Releases:

```text
https://github.com/yeagoo/imgconvert/releases/latest/download/latest.json
```

The default `src-tauri/tauri.conf.json` intentionally does not contain updater
keys or endpoints. Release builds opt in by generating an extra Tauri config.

## One-time key setup

Generate the updater signing key pair on a trusted machine:

```bash
pnpm tauri signer generate --ci -w ~/.tauri/imgconvert-updater.key
```

Store these GitHub repository secrets:

- `TAURI_UPDATER_PUBKEY`: contents of `~/.tauri/imgconvert-updater.key.pub`
- `TAURI_SIGNING_PRIVATE_KEY`: contents of `~/.tauri/imgconvert-updater.key`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: optional, only if the key has a password

Do not commit either key.

## Local signed AppImage updater build

```bash
export TAURI_UPDATER_PUBKEY="$(cat ~/.tauri/imgconvert-updater.key.pub)"
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/imgconvert-updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
export TAURI_UPDATER_ENDPOINTS='["https://github.com/yeagoo/imgconvert/releases/latest/download/latest.json"]'

pnpm run release:linux:updater

export TAURI_UPDATER_ARTIFACT_BASE_URL="https://github.com/yeagoo/imgconvert/releases/download/v0.1.0"
pnpm run release:updater:manifest
pnpm run release:updater:verify
```

The Linux updater path scrubs the AppImage first, then re-signs the final
AppImage with `tauri signer sign`. This avoids publishing a signature for a
pre-scrub artifact. The verify step checks that `latest.json` points at the
final local artifact and that each manifest signature matches the adjacent
`.sig` file.

Upload these files to the GitHub Release:

- `target/updater/latest.json`
- `src-tauri/target/release/bundle/appimage/*.AppImage`
- `src-tauri/target/release/bundle/appimage/*.AppImage.sig`
- `src-tauri/target/release/bundle/SHA256SUMS`

After the release is published, verify the public updater surface:

```bash
pnpm run release:updater:smoke -- --repo=yeagoo/imgconvert --tag=v0.1.0 --platform=linux-x86_64
```

On a different CPU architecture, add `--no-run` to validate `latest.json`,
artifact download, and `.sig` consistency without executing the downloaded
AppImage.

## GitHub Actions

Run the manual workflow `Updater Release` with:

- `tag`: the release tag, for example `v0.1.0`
- `publish_release=false`: build and upload workflow artifacts only
- `publish_release=true`: upload `latest.json`, AppImage, signature, and
  checksums to the GitHub Release

The workflow uses the same static endpoint:

```text
https://github.com/<owner>/<repo>/releases/latest/download/latest.json
```

Before uploading assets, the workflow executes the signed AppImage with
`IMGCONVERT_PACKAGE_CONVERT_SMOKE=1`, so the published updater artifact has
already passed the same hidden conversion smoke used by Linux package tests.

Flatpak updates remain managed by Flathub. `.deb` and `.rpm` updates remain
distribution/package-channel work; the Tauri updater is for direct-distribution
artifacts such as AppImage, macOS `.app.tar.gz`, and Windows installers.
