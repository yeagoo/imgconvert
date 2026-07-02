// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tmpRoot = mkdtempSync(path.join(os.tmpdir(), "imgconvert-windows-smoke-"));
const isWindows = os.platform() === "win32";

const options = {
  allowNonWindows: false,
  buildDirect: false,
  profile: "release",
  bundles: "msi,nsis",
  skipConvertSmoke: false,
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg === "--allow-non-windows") {
    options.allowNonWindows = true;
  } else if (arg === "--build-direct") {
    options.buildDirect = true;
  } else if (arg.startsWith("--profile=")) {
    options.profile = arg.slice("--profile=".length);
  } else if (arg.startsWith("--bundles=")) {
    options.bundles = arg.slice("--bundles=".length);
  } else if (arg === "--skip-convert-smoke") {
    options.skipConvertSmoke = true;
  } else if (arg === "--help" || arg === "-h") {
    printHelp();
    process.exit(0);
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

if (!isWindows && !options.allowNonWindows) {
  fail(
    "Windows runtime smoke must run on Windows. Pass --allow-non-windows only for script preflight.",
  );
}
if (!["debug", "release"].includes(options.profile)) {
  fail(`unsupported profile: ${options.profile}`);
}

run("pnpm", ["run", "release:windows:direct:check"], "Windows direct guardrails");
run("pnpm", ["run", "release:windows:store:check"], "Windows store guardrails", {
  IMGCONVERT_DISABLE_EXTERNAL_CODECS: "1",
});

if (!options.skipConvertSmoke) {
  runPackageConversionSmoke();
}

if (options.buildDirect) {
  requireWindows("--build-direct");
  run(
    "node",
    [
      "scripts/clean-windows-bundles.mjs",
      `--profile=${options.profile}`,
      `--bundles=${options.bundles}`,
    ],
    "clean stale Windows bundles",
  );
  const buildArgs = ["tauri", "build", "--ci", "--bundles", options.bundles];
  if (options.profile === "debug") {
    buildArgs.splice(2, 0, "--debug");
  }
  run("pnpm", buildArgs, "Windows direct installer build");
  run(
    "node",
    [
      "scripts/check-windows-bundle-artifacts.mjs",
      `--profile=${options.profile}`,
      `--bundles=${options.bundles}`,
    ],
    "Windows installer artifact check",
  );
}

console.log("Windows runtime smoke completed.");

function runPackageConversionSmoke() {
  requireWindows("Windows package conversion smoke");
  const executable = path.join(
    repoRoot,
    "src-tauri",
    "target",
    "debug",
    process.platform === "win32" ? "imgconvert.exe" : "imgconvert",
  );
  if (!existsSync(executable)) {
    run(
      "cargo",
      ["+1.96.0", "build", "--manifest-path", "src-tauri/Cargo.toml", "--bin", "imgconvert"],
      "build Windows smoke binary",
    );
  }
  run(executable, [], "Windows package conversion smoke", {
    IMGCONVERT_PACKAGE_CONVERT_SMOKE: "1",
    IMGCONVERT_PACKAGE_CONVERT_SMOKE_FORMATS: "jpeg,webp,png,avif",
    IMGCONVERT_PACKAGE_CONVERT_SMOKE_DIR: path.join(tmpRoot, "convert"),
  });
}

function run(command, args, label, extraEnv = {}) {
  console.log(`\n> ${label}`);
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    env: { ...process.env, ...extraEnv },
    shell: isWindows && ["pnpm", "node", "cargo"].includes(command),
    stdio: "inherit",
  });
  if (result.error) {
    fail(`${label} failed to start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    fail(`${label} failed with exit code ${result.status ?? 1}`);
  }
}

function requireWindows(feature) {
  if (!isWindows) {
    fail(`${feature} requires Windows`);
  }
}

function printHelp() {
  console.log(`Usage: node scripts/smoke-windows-runtime.mjs [options]

Options:
  --allow-non-windows     Allow non-Windows script preflight.
  --build-direct          Build direct .msi/.exe installers with Tauri.
  --profile=<profile>     release or debug, defaults to release.
  --bundles=<list>        Comma-separated Tauri bundles, defaults to msi,nsis.
  --skip-convert-smoke    Skip hidden package conversion smoke.
`);
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
