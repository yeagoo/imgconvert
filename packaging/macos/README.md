<!-- SPDX-License-Identifier: Apache-2.0 -->

# macOS Packaging

This directory documents the first macOS release surface.

- Direct distribution uses Tauri's automatic `src-tauri/tauri.macos.conf.json` merge and `entitlements.macos.direct.plist`.
- Mac App Store builds must merge `src-tauri/tauri.macos.mas.conf.json` and set `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1` before compiling, so optional external codec/helper discovery is compiled off.
- The MAS entitlement set is intentionally narrow: App Sandbox, user-selected read/write files, and app-scoped bookmarks. Do not add broad network or filesystem entitlements without a concrete feature need.

Local preflight from any host:

```bash
pnpm run release:macos:check
IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 pnpm run release:macos:store:check
```

Direct distribution build on macOS:

```bash
pnpm tauri build --ci --bundles dmg
```

MAS candidate build on macOS, after Apple signing/provisioning is configured:

```bash
IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 pnpm tauri build --ci --config src-tauri/tauri.macos.mas.conf.json
```

The security-scoped bookmark runtime shim is still a separate implementation item. These files only lock the packaging and entitlement boundary so later MAS work does not accidentally ship with external helpers enabled.
