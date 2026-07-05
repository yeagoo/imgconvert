<!-- SPDX-License-Identifier: Apache-2.0 -->

# Flatpak HEIC Extension

This directory contains the optional Flatpak extension packaging surface for
HEIC/HEIF decode-only support.

The main `com.ivmm.imgconvert` Flatpak still sets
`IMGCONVERT_DISABLE_EXTERNAL_CODECS=1`, so host PATH/XDG helpers are not
discovered from inside the sandbox. It also sets
`IMGCONVERT_ALLOW_FLATPAK_CODEC_EXTENSIONS=1`, which allows only the mounted
Flatpak extension point at `/app/extensions/codecs`.

The extension point is:

```yaml
com.ivmm.imgconvert.Codecs:
  version: "1"
  directory: extensions/codecs
  subdirectories: true
  no-autodownload: true
  autodelete: true
```

The checked-in build manifest is:

```bash
pnpm run release:flatpak:heic:verify
pnpm run release:flatpak:heic:download-check
pnpm run release:flatpak:heic:smoke
```

`release:flatpak:heic:download-check` uses `flatpak-builder --download-only`
to verify pinned upstream source URLs and sha256 values without requiring the
main runtime/app to be installed. It may still print a missing-runtime message;
the exit code is the pass/fail signal. `release:flatpak:heic:smoke` requires a
local Flatpak environment that can resolve the main `com.ivmm.imgconvert`
runtime/app.

The real extension:

- use id `com.ivmm.imgconvert.Codecs.Heic`;
- use branch `1`;
- builds `libde265` v1.1.1 and `libheif` v1.23.1 from pinned upstream source
  tarballs;
- installs a tiny `imgconvert-heic-helper` wrapper around `heif-convert`;
- installs `imgconvert-codec-heic.json` at the extension root;
- declares `writable: []` in the codec manifest;
- installs upstream LGPL `COPYING` files under the extension license directory;
- disables HEIC encoding, `x265`, and other GPL-only codec paths.

This extension does not make the main Flatpak package claim built-in HEIC
support. It is a separate LGPL addon/helper package.
