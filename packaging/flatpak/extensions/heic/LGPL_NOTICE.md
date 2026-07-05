<!-- SPDX-License-Identifier: Apache-2.0 -->

# ImgConvert HEIC Flatpak Extension Notice

This optional Flatpak extension is separate from the Apache-2.0 ImgConvert main
application package.

The extension builds and distributes LGPL codec components:

- `libheif` from `strukturag/libheif`
- `libde265` from `strukturag/libde265`

The Flatpak manifest installs each upstream `COPYING` file into the extension
under `share/licenses/io.github.yeagoo.imgconvert.Codecs.Heic/`.

The extension is decode-only. It intentionally disables HEIC encoding, `x265`,
and GPL-only codec paths.
