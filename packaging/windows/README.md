<!-- SPDX-License-Identifier: Apache-2.0 -->

# Windows Packaging

This directory documents the first Windows release surface.

- Direct distribution uses Tauri's automatic `src-tauri/tauri.windows.conf.json` merge on Windows.
- The direct installer guardrails are intentionally explicit: no downgrade installs, SHA-256 signing digest, a silent embedded WebView2 bootstrapper, a minimum WebView2 runtime version, a pinned WiX `upgradeCode`, and a current-user NSIS default install.
- The repository does not store signing certificates, certificate thumbprints, Partner Center credentials, or timestamping secrets. Configure those only in the Windows release runner or signing environment.

Local preflight from any host:

```bash
pnpm run release:windows:direct:check
pnpm run release:windows:check
pnpm run release:windows:smoke -- --allow-non-windows --skip-convert-smoke
```

Runtime smoke on Windows:

```powershell
pnpm run release:windows:smoke
```

Direct distribution build on Windows:

```powershell
pnpm run release:windows
```

GitHub Actions also exposes `Windows Smoke`. Pushes to `main` run Windows
guardrails, Rust backend checks, and a hidden real conversion smoke. Manual runs
can enable `build_direct` to build unsigned `.msi` and NSIS `.exe` artifacts.
Code signing is not configured in this repository; sign installers only in the
release runner or signing environment.

Microsoft Store is a separate candidate path. Tauri does not make MSIX a first-class bundle target for this repo, so the store route still needs a Windows runner, MSIX packaging, `runFullTrust`, Partner Center setup, and real install smoke testing.

Store candidate preflight must compile with external codec/helper discovery disabled:

```bash
IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 pnpm run release:windows:store:check
```

Windows HEIC remains decode-only by product policy. The future system route is WIC with runtime detection of the HEIF/HEVC extensions; the optional free HEIC helper is a separately distributed decode-only plugin and must not be bundled into the store main package unless the channel rules are revalidated.
