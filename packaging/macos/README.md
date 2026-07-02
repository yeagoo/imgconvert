<!-- SPDX-License-Identifier: Apache-2.0 -->

# macOS Packaging

This directory documents the first macOS release surface.

- Direct distribution uses Tauri's automatic `src-tauri/tauri.macos.conf.json` merge and `entitlements.macos.direct.plist`.
- Mac App Store builds must merge `src-tauri/tauri.macos.mas.conf.json` and set `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1` before compiling, so optional external codec/helper discovery is compiled off.
- The MAS entitlement set is intentionally narrow: App Sandbox, user-selected read/write files, and app-scoped bookmarks. Do not add broad network or filesystem entitlements without a concrete feature need.
- HEIC import uses the macOS system ImageIO framework as a read-only `system-imageio` provider. It does not link libheif, does not bundle x265, and does not enable HEIC output until encoding, patents, and sandbox behavior are separately audited.
- Runtime file access is routed through the security-scoped resource shim in `src-tauri/src/macos_security.rs`. Persistent MAS access still needs real bookmark data from the dialog layer and must balance start/stop access for every scoped URL.

Local preflight from any host:

```bash
pnpm run release:macos:check
IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 pnpm run release:macos:store:check
pnpm run release:macos:smoke -- --allow-non-macos --skip-benchmark --skip-heic
```

Direct distribution build on macOS:

```bash
pnpm tauri build --ci --bundles dmg
```

Direct distribution signing/notarization checklist:

```bash
pnpm tauri build --ci --bundles dmg
xcrun notarytool submit <ImgConvert.dmg> --keychain-profile <profile> --wait
xcrun stapler staple <ImgConvert.dmg>
spctl --assess --type open --context context:primary-signature -v <ImgConvert.dmg>
```

MAS candidate build on macOS, after Apple signing/provisioning is configured:

```bash
IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 pnpm tauri build --ci --config src-tauri/tauri.macos.mas.conf.json
```

Apple Silicon AVIF benchmark, used to decide whether rav1e speed 8 remains a sane default on macOS:

```bash
pnpm run bench:avif:macos
IMGCONVERT_AVIF_BENCHMARK_SPEEDS=6,8,10 IMGCONVERT_AVIF_BENCHMARK_ITERATIONS=5 pnpm run bench:avif:macos
```

Runtime smoke on macOS:

```bash
pnpm run release:macos:smoke
IMGCONVERT_MACOS_HEIC_SMOKE_INPUT=/path/to/sample.heic pnpm run release:macos:smoke
pnpm run release:macos:smoke -- --build-direct
pnpm run release:macos:smoke -- --notarize-dmg=/path/to/ImgConvert.dmg --notary-profile=<profile>
```

macOS release acceptance still requires a real machine pass:

- HEIC `.heic/.heif` import through ImageIO in direct build.
- MAS sandbox file-open/output-directory flow using security-scoped bookmarks. The hidden path smoke covers the conversion backend; the interactive bookmark acquisition flow still needs a real GUI pass.
- AVIF benchmark on Apple Silicon before changing defaults.
- `.dmg` Developer ID signing, `notarytool` submission, stapling, and Gatekeeper assessment.
