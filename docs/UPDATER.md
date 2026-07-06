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

If the key pair is stored at the default path, the local release command loads
the private key only into child process environment variables and does not write
it into the repository:

```bash
pnpm run release:updater:local
```

That command builds the updater-enabled AppImage, scrubs it, re-signs the final
artifact, writes `target/updater/latest.json`, and verifies that the manifest
matches the local artifact and `.sig`.

The default updater endpoint is:

```text
https://github.com/yeagoo/imgconvert/releases/latest/download/latest.json
```

The default artifact base URL is:

```text
https://github.com/yeagoo/imgconvert/releases/download/v0.1.1
```

Override these when preparing a non-default repository or tag:

```bash
export TAURI_UPDATER_PUBKEY="$(cat ~/.tauri/imgconvert-updater.key.pub)"
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/imgconvert-updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
export TAURI_UPDATER_ENDPOINTS='["https://github.com/yeagoo/imgconvert/releases/latest/download/latest.json"]'

pnpm run release:linux:updater

export TAURI_UPDATER_ARTIFACT_BASE_URL="https://github.com/yeagoo/imgconvert/releases/download/v0.1.1"
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
pnpm run release:updater:smoke -- --repo=yeagoo/imgconvert --tag=v0.1.1 --platform=linux-x86_64
```

On a different CPU architecture, add `--no-run` to validate `latest.json`,
artifact download, and `.sig` consistency without executing the downloaded
AppImage.

## In-app upgrade smoke

The cheap preflight validates that an old updater-enabled release can see a
newer public `latest.json` and that both old/new AppImage signatures match the
release manifests:

```bash
pnpm run release:updater:upgrade-smoke:eligibility
```

The real GUI smoke requires a Linux x86_64 desktop/X11 environment because it
launches the old AppImage, clicks the application's update dialog, waits for the
AppImage to be replaced by the latest release artifact, then runs the hidden
package conversion smoke on the updated file:

```bash
pnpm run release:updater:upgrade-smoke -- --from-tag=v0.1.0 --to-tag=v0.1.1
```

On GitHub Actions, run the manual workflow `Updater Upgrade Smoke` with
`confirm_runner=true`. It installs `Xvfb` and `xdotool`, then executes the same
script on an Ubuntu x86_64 runner.

Because already-published old releases cannot receive new test hooks, the
`v0.1.0 -> v0.1.1` smoke uses UI clicks. Future old versions that already
contain this script and workflow can reuse the same upgrade smoke without
changing the old binary.

## GitHub Actions

Run the manual workflow `Updater Release` with:

- `tag`: the release tag, for example `v0.1.1`
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
