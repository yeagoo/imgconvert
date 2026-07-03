// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tmpRoot = mkdtempSync(path.join(os.tmpdir(), "imgconvert-macos-smoke-"));
const isMacos = os.platform() === "darwin";

const options = {
  allowNonMacos: false,
  buildDirect: false,
  buildMas: false,
  skipBenchmark: false,
  skipHeic: false,
  notarizeDmg: process.env.IMGCONVERT_MACOS_DMG ?? null,
  notaryProfile: process.env.IMGCONVERT_NOTARYTOOL_PROFILE ?? null,
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg === "--allow-non-macos") {
    options.allowNonMacos = true;
  } else if (arg === "--build-direct") {
    options.buildDirect = true;
  } else if (arg === "--build-mas") {
    options.buildMas = true;
  } else if (arg === "--skip-benchmark") {
    options.skipBenchmark = true;
  } else if (arg === "--skip-heic") {
    options.skipHeic = true;
  } else if (arg.startsWith("--notarize-dmg=")) {
    options.notarizeDmg = arg.slice("--notarize-dmg=".length);
  } else if (arg.startsWith("--notary-profile=")) {
    options.notaryProfile = arg.slice("--notary-profile=".length);
  } else if (arg === "--help" || arg === "-h") {
    printHelp();
    process.exit(0);
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

if (!isMacos && !options.allowNonMacos) {
  fail("macOS runtime smoke must run on macOS. Pass --allow-non-macos only for script preflight.");
}

run("pnpm", ["run", "release:macos:check"], "macOS release guardrails");
run("pnpm", ["run", "release:macos:store:check"], "macOS store guardrails", {
  IMGCONVERT_DISABLE_EXTERNAL_CODECS: "1",
});

if (options.buildDirect) {
  requireMacos("--build-direct");
  run("pnpm", ["run", "release:macos"], "macOS direct DMG build");
}

if (options.buildMas) {
  requireMacos("--build-mas");
  run("pnpm", ["run", "release:macos:mas"], "macOS MAS candidate build");
}

if (!options.skipBenchmark) {
  const benchmarkArgs = ["scripts/benchmark-macos-avif.mjs"];
  if (!isMacos && options.allowNonMacos) {
    benchmarkArgs.push("--allow-non-macos");
  }
  run("node", benchmarkArgs, "Apple Silicon AVIF benchmark");
}

if (!options.skipHeic) {
  runHeicSmokeIfConfigured();
}

if (options.notarizeDmg) {
  requireMacos("--notarize-dmg");
  runNotarization(options.notarizeDmg);
}

console.log("macOS runtime smoke completed.");

function runHeicSmokeIfConfigured() {
  const input = process.env.IMGCONVERT_MACOS_HEIC_SMOKE_INPUT;
  if (!input) {
    console.log("skipping HEIC ImageIO smoke: IMGCONVERT_MACOS_HEIC_SMOKE_INPUT is not set");
    return;
  }
  requireMacos("HEIC ImageIO smoke");

  run(
    "cargo",
    ["+1.96.0", "run", "--manifest-path", "src-tauri/Cargo.toml", "--bin", "imgconvert"],
    "macOS ImageIO HEIC path conversion smoke",
    {
      IMGCONVERT_DISABLE_EXTERNAL_CODECS: "1",
      IMGCONVERT_PATH_CONVERT_SMOKE: "1",
      IMGCONVERT_PATH_CONVERT_SMOKE_INPUT: input,
      IMGCONVERT_PATH_CONVERT_SMOKE_FORMAT: process.env.IMGCONVERT_MACOS_HEIC_SMOKE_FORMAT ?? "png",
      IMGCONVERT_PATH_CONVERT_SMOKE_DIR:
        process.env.IMGCONVERT_MACOS_HEIC_SMOKE_DIR ?? path.join(tmpRoot, "heic"),
    },
  );
}

function runNotarization(dmgPath) {
  if (!options.notaryProfile) {
    fail("notarization requires --notary-profile or IMGCONVERT_NOTARYTOOL_PROFILE");
  }
  if (!existsSync(dmgPath)) {
    fail(`DMG does not exist: ${dmgPath}`);
  }
  run(
    "xcrun",
    ["notarytool", "submit", dmgPath, "--keychain-profile", options.notaryProfile, "--wait"],
    "notarytool submit",
  );
  run("xcrun", ["stapler", "staple", dmgPath], "stapler staple");
  run(
    "spctl",
    ["--assess", "--type", "open", "--context", "context:primary-signature", "-v", dmgPath],
    "Gatekeeper assessment",
  );
  run(
    "node",
    [
      "scripts/check-macos-bundle-artifacts.mjs",
      "--profile=release",
      "--bundles=dmg",
      "--require-signed",
      "--require-notarized",
    ],
    "signed/notarized DMG artifact verification",
  );
}

function run(command, args, label, extraEnv = {}) {
  console.log(`\n> ${label}`);
  let result = spawnSync(command, args, {
    cwd: repoRoot,
    env: { ...process.env, ...extraEnv },
    stdio: "inherit",
  });
  if (result.error?.code === "ENOENT" && command === "pnpm") {
    result = spawnSync("corepack", ["pnpm", ...args], {
      cwd: repoRoot,
      env: { ...process.env, ...extraEnv },
      stdio: "inherit",
    });
  }
  if (result.error) {
    fail(`${label} failed to start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    fail(`${label} failed with exit code ${result.status ?? 1}`);
  }
}

function requireMacos(feature) {
  if (!isMacos) {
    fail(`${feature} requires macOS`);
  }
}

function printHelp() {
  console.log(`Usage: node scripts/smoke-macos-runtime.mjs [options]

Options:
  --allow-non-macos       Allow non-macOS script preflight.
  --build-direct          Build direct .dmg candidate with Tauri.
  --build-mas             Build MAS candidate with store codec guardrail.
  --skip-benchmark        Skip AVIF benchmark.
  --skip-heic             Skip HEIC path conversion smoke.
  --notarize-dmg=<path>   Submit, staple, and assess an existing DMG.
  --notary-profile=<name> Keychain profile for notarytool.

Environment:
  IMGCONVERT_MACOS_HEIC_SMOKE_INPUT   Optional .heic/.heif fixture for ImageIO smoke.
  IMGCONVERT_MACOS_HEIC_SMOKE_FORMAT  Output format for HEIC smoke, defaults to png.
  IMGCONVERT_MACOS_DMG                Existing DMG path for notarization.
  IMGCONVERT_NOTARYTOOL_PROFILE       notarytool keychain profile.
`);
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
