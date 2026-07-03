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

GitHub Actions exposes the manual-only `Windows Smoke` workflow. This keeps
runner cost predictable: the default run performs Windows guardrails, Rust
backend checks, and a hidden real conversion smoke. Manual runs can enable
`build_direct` to build `.msi` and NSIS `.exe` artifacts. Add `install_smoke`
to install each generated installer and run the hidden package smoke from the
installed `ImgConvert.exe`.

Signed direct distribution:

```powershell
$env:WINDOWS_CERTIFICATE_BASE64 = "<base64 pfx>"
$env:WINDOWS_CERTIFICATE_PASSWORD = "<pfx password>"
$env:WINDOWS_TIMESTAMP_URL = "http://timestamp.digicert.com"
pnpm run release:windows
pnpm run release:windows:sign
pnpm run release:windows:install-smoke
```

`release:windows:sign` also accepts `WINDOWS_CERTIFICATE_PATH` or
`WINDOWS_CERTIFICATE_SHA1`. The script signs with SHA-256 and RFC3161 timestamp
metadata, then verifies Authenticode with `signtool verify /pa /all`.

Microsoft Store is a separate candidate path. Tauri does not make MSIX a
first-class bundle target for this repo, so the store route uses an explicit
MSIX manifest template with `runFullTrust`. Prepare the manifest with Store
identity values from Partner Center:

Store candidate preflight must compile with external codec/helper discovery disabled:

```powershell
$env:IMGCONVERT_DISABLE_EXTERNAL_CODECS = "1"
$env:WINDOWS_STORE_IDENTITY_NAME = "<Partner Center package identity name>"
$env:WINDOWS_STORE_PUBLISHER = "CN=<Partner Center publisher id>"
$env:WINDOWS_STORE_PUBLISHER_DISPLAY_NAME = "<publisher display name>"
$env:WINDOWS_STORE_VERSION = "0.1.0.0"
pnpm run release:windows:store:check
pnpm run release:windows:msix:prepare
```

Actual Store submission still requires a Windows SDK packaging step
(`makeappx.exe`/`signtool.exe` or Partner Center tooling), Store assets, age
rating/privacy metadata, and a real install smoke on the submitted package. The
main package must keep `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1`; optional HEIC
helpers must not be bundled into the Store package unless channel rules are
revalidated.

Windows HEIC remains decode-only by product policy. The system route uses WIC
runtime detection of the Microsoft HEIF Image Extensions and HEVC Video
Extensions. When those are present, capabilities expose a read-only
`system-wic` provider for `heic`/`heif`/`hif`. When missing, plugin diagnostics
show a clear install hint. The optional free HEIC helper remains a separately
distributed decode-only plugin and is not bundled into the main app.
