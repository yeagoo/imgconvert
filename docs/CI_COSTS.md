<!-- SPDX-License-Identifier: Apache-2.0 -->
# CI Cost Guardrails

ImgConvert workflows are intentionally manual-only. They must not use automatic
`push`, `pull_request`, or `schedule` triggers until the billing policy changes.

Default behavior:

- `CI`: Ubuntu-only quality checks. Optional Windows checks, fuzz corpus replay,
  and package smoke are off by default.
- `Linux Release`: builds amd64 by default. Linux arm64 and Docker runtime smoke
  are off by default.
- `macOS Smoke`: requires `confirm_paid_runner=true` before the `macos-15`
  runner is allocated.
- `Windows Smoke`: requires `confirm_paid_runner=true` before the
  `windows-latest` runner is allocated.

Run the static guardrail after changing workflows:

```bash
pnpm run ci:cost:check
```

Before deciding whether to spend hosted runner minutes, run the read-only
release readiness report:

```bash
pnpm run release:readiness
```

It does not build artifacts or trigger GitHub Actions. It only reports which
local checks/artifacts are present and which remaining items require external
credentials, paid runners, or store review.

Use the explicit expensive toggles only when you need the artifact or platform
signal from that run:

- `package_smoke_arm64` or `build_arm64`: Linux arm64 runner.
- `docker_smoke`: Docker install/runtime matrix after Linux release build.
- `confirm_paid_runner`: macOS/Windows hosted runner allocation.
- `build_direct`, `notarize_direct`, `build_mas_candidate`, `sign_direct`,
  `install_smoke`: packaging/signing work after the platform smoke has started.
