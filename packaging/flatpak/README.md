<!-- SPDX-License-Identifier: Apache-2.0 -->

# Flatpak Packaging

This directory contains the ImgConvert Flatpak packaging surface:

- `com.ivmm.imgconvert.yml` builds from a generated release archive instead of the repository root.
- `pnpm run release:flatpak:prepare` creates that release archive with vendored Corepack/pnpm (`.flatpak-vendor/corepack.tgz`) plus vendored Cargo/npm inputs under `target/flatpak/sources/`, then updates the manifest `sha256`.
- `finish-args` intentionally avoid broad host filesystem access so file access continues to go through user-selected portal paths.
- The manifest tracks the supported GNOME `50` runtime; do not leave Flathub packaging pinned to EOL GNOME branches.
- AppStream metadata uses `metadata_license=CC0-1.0`; the application/project license remains `Apache-2.0`.
- `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1` is set for the Flatpak main package so host PATH/XDG helpers stay disabled inside the sandbox.
- `IMGCONVERT_ALLOW_FLATPAK_CODEC_EXTENSIONS=1` allows only the application-owned extension point `com.ivmm.imgconvert.Codecs` mounted at `/app/extensions/codecs`.
- Optional HEIC support uses the addon under `extensions/heic/` (`com.ivmm.imgconvert.Codecs.Heic`) and must be built as a separate decode-only LGPL extension, not bundled into the Apache-2.0 main package.

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

Do not add LGPL/GPL codec helpers to the main Flatpak manifest. HEIC helper support stays in a separate extension/helper package and still needs Flathub addon review before public use.

## Optional HEIC Extension

The main manifest defines this extension point:

```yaml
com.ivmm.imgconvert.Codecs:
  version: "1"
  directory: extensions/codecs
  subdirectories: true
```

A HEIC extension installs `imgconvert-codec-heic.json` plus a decode-only helper
under its extension root. The real manifest
`extensions/heic/com.ivmm.imgconvert.Codecs.Heic.yml` builds `libde265` and
`libheif` from pinned upstream source tarballs, disables HEIC encoding and x265,
then wraps `heif-convert` as `bin/imgconvert-heic-helper`.

Static verification:

```bash
pnpm run release:flatpak:heic:verify
```

Source URL and checksum smoke, without requiring the main app runtime to be
installed:

```bash
pnpm run release:flatpak:heic:download-check
```

`flatpak-builder` may still print that `com.ivmm.imgconvert` is not installed
in this mode; that is expected for `--allow-missing-runtimes`. Treat the command
exit code as the pass/fail signal.

Build smoke on a host with Flatpak tooling and the main app runtime available:

```bash
pnpm run release:flatpak:heic:smoke
```

The expected codec manifest keeps HEIC read-only:

```json
{
  "readable": ["heic", "heif", "hif"],
  "writable": [],
  "decode": {
    "kind": "heic-to-png-file",
    "args": ["{input}", "{output}", "{metadata}"]
  }
}
```

The extension installs upstream LGPL license texts under its own extension
license directory. Patent/channel review is still separate from repository
packaging. Do not claim built-in HEIC support from the main Flatpak package.
