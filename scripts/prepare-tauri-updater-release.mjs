// SPDX-License-Identifier: Apache-2.0

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const packageJson = JSON.parse(readFileSync(path.join(repoRoot, "package.json"), "utf8"));
const defaultOutput = path.join(
  repoRoot,
  "src-tauri",
  "target",
  "updater",
  "tauri.updater.generated.conf.json",
);

const options = {
  pubkey: process.env.TAURI_UPDATER_PUBKEY ?? "",
  endpoints: process.env.TAURI_UPDATER_ENDPOINTS ?? "",
  output: defaultOutput,
  windowsInstallMode: process.env.TAURI_UPDATER_WINDOWS_INSTALL_MODE ?? "passive",
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg.startsWith("--pubkey=")) {
    options.pubkey = arg.slice("--pubkey=".length);
  } else if (arg.startsWith("--endpoints=")) {
    options.endpoints = arg.slice("--endpoints=".length);
  } else if (arg.startsWith("--output=")) {
    options.output = path.resolve(repoRoot, arg.slice("--output=".length));
  } else if (arg.startsWith("--windows-install-mode=")) {
    options.windowsInstallMode = arg.slice("--windows-install-mode=".length);
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

const pubkey = validatePubkey(options.pubkey);
const endpoints = parseEndpoints(options.endpoints).map(validateEndpoint);
validateWindowsInstallMode(options.windowsInstallMode);

const config = {
  $schema: "https://schema.tauri.app/config/2",
  bundle: {
    createUpdaterArtifacts: true,
  },
  plugins: {
    updater: {
      pubkey,
      endpoints,
      windows: {
        installMode: options.windowsInstallMode,
      },
    },
  },
};

mkdirSync(path.dirname(options.output), { recursive: true });
writeFileSync(options.output, `${JSON.stringify(config, null, 2)}\n`);
console.log(`Tauri updater config prepared: ${path.relative(repoRoot, options.output)}`);

function validatePubkey(raw) {
  const value = raw.trim();
  if (!value) {
    fail("TAURI_UPDATER_PUBKEY is required");
  }
  if (existsSync(path.resolve(repoRoot, value)) || existsSync(value)) {
    fail("TAURI_UPDATER_PUBKEY must be the public key content, not a file path");
  }
  if (/PRIVATE KEY/i.test(value)) {
    fail("TAURI_UPDATER_PUBKEY must not contain a private key");
  }
  if (value.length < 40) {
    fail("TAURI_UPDATER_PUBKEY looks too short");
  }
  return value;
}

function parseEndpoints(raw) {
  const value = raw.trim();
  if (!value) {
    fail("TAURI_UPDATER_ENDPOINTS is required");
  }
  try {
    const parsed = JSON.parse(value);
    if (Array.isArray(parsed)) {
      return parsed.map((endpoint) => String(endpoint).trim()).filter(Boolean);
    }
  } catch {
    // Fall through to comma/newline parsing.
  }
  return value
    .split(/[\n,]+/)
    .map((endpoint) => endpoint.trim())
    .filter(Boolean);
}

function validateEndpoint(endpoint) {
  const normalized = endpoint
    .replaceAll("{{target}}", "linux")
    .replaceAll("{{arch}}", "x86_64")
    .replaceAll("{{current_version}}", packageJson.version);
  let url;
  try {
    url = new URL(normalized);
  } catch {
    fail(`invalid updater endpoint: ${endpoint}`);
  }
  if (url.protocol !== "https:") {
    fail(`updater endpoint must use HTTPS: ${endpoint}`);
  }
  return endpoint;
}

function validateWindowsInstallMode(mode) {
  if (!["passive", "basicUi", "quiet"].includes(mode)) {
    fail(`unsupported Windows updater install mode: ${mode}`);
  }
}

function fail(message) {
  console.error(`prepare-tauri-updater-release: ${message}`);
  process.exit(1);
}
