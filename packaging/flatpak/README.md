<!-- SPDX-License-Identifier: Apache-2.0 -->

# Flatpak Packaging

This directory contains the ImgConvert Flatpak packaging surface:

- `com.ivmm.imgconvert.yml` builds from a generated release archive instead of the repository root.
- `pnpm run release:flatpak:prepare` creates that release archive with vendored Corepack/pnpm (`.flatpak-vendor/corepack.tgz`) plus vendored Cargo/npm inputs under `target/flatpak/sources/`, then updates the manifest `sha256`.
- `finish-args` intentionally avoid broad host filesystem access so file access continues to go through user-selected portal paths.
- The manifest tracks the supported GNOME `50` runtime; do not leave Flathub packaging pinned to EOL GNOME branches.
- AppStream metadata uses `metadata_license=CC0-1.0`; the application/project license remains `Apache-2.0`.
- `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1` is set for the Flatpak main package. Optional HEIC helpers are outside the main package and need a separate extension/helper design before Flathub use.

Local build/smoke on a host with Flatpak tooling:

```bash
pnpm run release:flatpak:smoke
```

The smoke command prepares the generated source archive, verifies the manifest, builds with `flatpak-builder`, runs the hidden conversion smoke inside the build sandbox, installs the app into the user Flatpak installation, then runs the same conversion smoke through `flatpak run`. It adds the Flathub user remote when missing so the GNOME runtime and SDK extensions can be resolved.

If you need to spell the sequence out while debugging:

```bash
pnpm run release:flatpak:prepare
pnpm run release:flatpak:verify
flatpak-builder --user --assumeyes --install-deps-from=flathub --install --force-clean target/flatpak/build-dir packaging/flatpak/com.ivmm.imgconvert.yml
flatpak run --user --command=imgconvert --env=IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 --env=IMGCONVERT_PACKAGE_CONVERT_SMOKE=1 com.ivmm.imgconvert
```

For a Flathub PR, publish `target/flatpak/sources/imgconvert-<version>-source.tar.gz` to a release URL first, then rewrite the manifest source while keeping the generated `sha256`:

```bash
pnpm run release:flatpak:prepare -- --source-url=https://github.com/yeagoo/imgconvert/releases/download/v0.1.0/imgconvert-0.1.0-source.tar.gz
pnpm run release:flatpak:verify
```

Do not add LGPL/GPL codec helpers to the main Flatpak manifest. HEIC helper support needs a separate extension/helper design and channel review before Flathub use.
