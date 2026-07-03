// SPDX-License-Identifier: Apache-2.0

import { existsSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const options = {
  profile: "release",
  dmg: process.env.IMGCONVERT_MACOS_DMG ?? null,
  keychainProfile: process.env.IMGCONVERT_NOTARYTOOL_PROFILE ?? null,
  appleId: process.env.APPLE_ID ?? null,
  password: process.env.APPLE_PASSWORD ?? null,
  teamId: process.env.APPLE_TEAM_ID ?? null,
  apiKey: process.env.APPLE_API_KEY ?? null,
  apiIssuer: process.env.APPLE_API_ISSUER ?? null,
  apiKeyPath: process.env.APPLE_API_KEY_PATH ?? null,
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg.startsWith("--profile=")) {
    options.profile = arg.slice("--profile=".length);
  } else if (arg.startsWith("--dmg=")) {
    options.dmg = arg.slice("--dmg=".length);
  } else if (arg.startsWith("--keychain-profile=")) {
    options.keychainProfile = arg.slice("--keychain-profile=".length);
  } else if (arg.startsWith("--apple-id=")) {
    options.appleId = arg.slice("--apple-id=".length);
  } else if (arg.startsWith("--password=")) {
    options.password = arg.slice("--password=".length);
  } else if (arg.startsWith("--team-id=")) {
    options.teamId = arg.slice("--team-id=".length);
  } else if (arg.startsWith("--api-key=")) {
    options.apiKey = arg.slice("--api-key=".length);
  } else if (arg.startsWith("--api-issuer=")) {
    options.apiIssuer = arg.slice("--api-issuer=".length);
  } else if (arg.startsWith("--api-key-path=")) {
    options.apiKeyPath = arg.slice("--api-key-path=".length);
  } else if (arg === "--help" || arg === "-h") {
    printHelp();
    process.exit(0);
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

if (process.platform !== "darwin") {
  fail("macOS notarization requires macOS");
}
if (!["debug", "release"].includes(options.profile)) {
  fail(`unsupported profile: ${options.profile}`);
}

const dmg = options.dmg ? path.resolve(options.dmg) : findLatestDmg();
if (!existsSync(dmg)) {
  fail(`DMG does not exist: ${dmg}`);
}

run("xcrun", ["notarytool", "submit", dmg, ...notaryCredentials(), "--wait"], "notarytool submit");
run("xcrun", ["stapler", "staple", dmg], "stapler staple");
run(
  "spctl",
  ["--assess", "--type", "open", "--context", "context:primary-signature", "-v", dmg],
  "Gatekeeper assessment",
);
run(
  "node",
  [
    "scripts/check-macos-bundle-artifacts.mjs",
    `--profile=${options.profile}`,
    "--bundles=dmg",
    "--require-signed",
    "--require-notarized",
  ],
  "signed/notarized DMG artifact verification",
);

console.log(`ok ${path.relative(repoRoot, dmg)} notarized and stapled`);

function notaryCredentials() {
  if (options.keychainProfile) {
    return ["--keychain-profile", options.keychainProfile];
  }
  if (options.apiKey && options.apiIssuer && options.apiKeyPath) {
    if (!existsSync(options.apiKeyPath)) {
      fail(`APPLE_API_KEY_PATH does not exist: ${options.apiKeyPath}`);
    }
    return ["--key", options.apiKeyPath, "--key-id", options.apiKey, "--issuer", options.apiIssuer];
  }
  if (options.appleId && options.password && options.teamId) {
    return [
      "--apple-id",
      options.appleId,
      "--password",
      options.password,
      "--team-id",
      options.teamId,
    ];
  }
  fail(
    "notarization requires IMGCONVERT_NOTARYTOOL_PROFILE, or APPLE_API_KEY/APPLE_API_ISSUER/APPLE_API_KEY_PATH, or APPLE_ID/APPLE_PASSWORD/APPLE_TEAM_ID",
  );
}

function findLatestDmg() {
  const dir = path.join(cargoTargetRoot(), options.profile, "bundle", "dmg");
  const dmgs = collectFiles(dir).filter((file) => file.toLowerCase().endsWith(".dmg"));
  if (dmgs.length === 0) {
    fail(`missing DMG artifact under ${path.relative(repoRoot, dir)}`);
  }
  dmgs.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return dmgs[0];
}

function collectFiles(dir) {
  if (!existsSync(dir)) {
    return [];
  }
  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const entryPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectFiles(entryPath));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }
  return files;
}

function run(command, args, label) {
  console.log(`\n> ${label}`);
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: "inherit",
  });
  if (result.error) {
    fail(`${label} failed to start: ${result.error.message}`);
  }
  if (result.status !== 0) {
    fail(`${label} failed with exit code ${result.status ?? 1}`);
  }
}

function cargoTargetRoot() {
  return process.env.CARGO_TARGET_DIR
    ? path.resolve(process.env.CARGO_TARGET_DIR)
    : path.join(repoRoot, "src-tauri", "target");
}

function printHelp() {
  console.log(`Usage: node scripts/notarize-macos-dmg.mjs [options]

Options:
  --profile=<debug|release>       Cargo profile, defaults to release.
  --dmg=<path>                    DMG path. Defaults to the newest DMG artifact.
  --keychain-profile=<name>       notarytool keychain profile.
  --apple-id=<id>                 Apple ID for notarytool.
  --password=<password>           App-specific password for notarytool.
  --team-id=<TEAMID>              Apple team ID for notarytool.
  --api-key=<KEYID>               App Store Connect API key ID.
  --api-issuer=<ISSUERID>         App Store Connect issuer ID.
  --api-key-path=<path>           App Store Connect private key path.
`);
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
