// SPDX-License-Identifier: Apache-2.0

import { mkdtempSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const isWindows = os.platform() === "win32";
let temporaryCertificateDir = null;

const options = {
  profile: "release",
  bundles: ["msi", "nsis"],
  verifyOnly: false,
  allowNonWindows: false,
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg.startsWith("--profile=")) {
    options.profile = arg.slice("--profile=".length);
  } else if (arg.startsWith("--bundles=")) {
    options.bundles = parseBundles(arg.slice("--bundles=".length));
  } else if (arg === "--verify-only") {
    options.verifyOnly = true;
  } else if (arg === "--allow-non-windows") {
    options.allowNonWindows = true;
  } else if (arg === "--help" || arg === "-h") {
    printHelp();
    process.exit(0);
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

if (!isWindows && !options.allowNonWindows) {
  fail(
    "Windows installer signing must run on Windows. Pass --allow-non-windows only for preflight.",
  );
}
if (!["debug", "release"].includes(options.profile)) {
  fail(`unsupported profile: ${options.profile}`);
}
for (const bundle of options.bundles) {
  if (!["msi", "nsis"].includes(bundle)) {
    fail(`unsupported Windows bundle: ${bundle}`);
  }
}

const artifacts = collectWindowsArtifacts(options.profile, options.bundles);
if (artifacts.length === 0) {
  fail("no Windows installers found to sign");
}

if (!isWindows) {
  for (const artifact of artifacts) {
    console.log(
      `preflight artifact ${path.relative(repoRoot, artifact)} (${statSync(artifact).size} bytes)`,
    );
  }
  process.exit(0);
}

const certificatePath = materializeCertificate();
try {
  for (const artifact of artifacts) {
    if (!options.verifyOnly) {
      signArtifact(artifact, certificatePath);
    }
    verifyArtifact(artifact);
  }
} finally {
  if (temporaryCertificateDir) {
    rmSync(temporaryCertificateDir, { recursive: true, force: true });
  }
}

console.log(`Windows installer signing completed (${artifacts.length} artifact(s)).`);

function materializeCertificate() {
  if (options.verifyOnly) {
    return null;
  }
  if (process.env.WINDOWS_CERTIFICATE_BASE64) {
    const dir = mkdtempSync(path.join(os.tmpdir(), "imgconvert-windows-cert-"));
    temporaryCertificateDir = dir;
    const certificatePath = path.join(dir, "signing.pfx");
    writeFileSync(certificatePath, Buffer.from(process.env.WINDOWS_CERTIFICATE_BASE64, "base64"));
    return certificatePath;
  }
  if (process.env.WINDOWS_CERTIFICATE_PATH) {
    return path.resolve(process.env.WINDOWS_CERTIFICATE_PATH);
  }
  if (process.env.WINDOWS_CERTIFICATE_SHA1) {
    return null;
  }
  fail(
    "set WINDOWS_CERTIFICATE_BASE64, WINDOWS_CERTIFICATE_PATH, or WINDOWS_CERTIFICATE_SHA1 before signing",
  );
}

function signArtifact(artifact, certificatePath) {
  const args = ["sign", "/fd", "SHA256", "/td", "SHA256"];
  args.push("/tr", process.env.WINDOWS_TIMESTAMP_URL ?? "http://timestamp.digicert.com");
  if (certificatePath) {
    args.push("/f", certificatePath);
    if (process.env.WINDOWS_CERTIFICATE_PASSWORD) {
      args.push("/p", process.env.WINDOWS_CERTIFICATE_PASSWORD);
    }
  } else {
    args.push("/sha1", process.env.WINDOWS_CERTIFICATE_SHA1);
  }
  args.push(artifact);
  run("signtool.exe", args, `sign ${path.basename(artifact)}`);
}

function verifyArtifact(artifact) {
  run(
    "signtool.exe",
    ["verify", "/pa", "/all", artifact],
    `verify signature ${path.basename(artifact)}`,
  );
}

function collectWindowsArtifacts(profile, bundles) {
  const expectedExtensions = {
    msi: ".msi",
    nsis: ".exe",
  };
  const bundleRoot = path.join(cargoTargetRoot(), profile, "bundle");
  const artifacts = [];
  for (const bundle of bundles) {
    const bundleDir = path.join(bundleRoot, bundle);
    artifacts.push(
      ...collectFiles(bundleDir).filter((file) =>
        file.toLowerCase().endsWith(expectedExtensions[bundle]),
      ),
    );
  }
  return artifacts.sort();
}

function collectFiles(dir) {
  try {
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
  } catch {
    return [];
  }
}

function parseBundles(value) {
  return value
    .split(",")
    .map((bundle) => bundle.trim().toLowerCase())
    .filter(Boolean);
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
  console.log(`Usage: node scripts/sign-windows-installers.mjs [options]

Options:
  --profile=<profile>     release or debug, defaults to release.
  --bundles=<list>        Comma-separated bundles, defaults to msi,nsis.
  --verify-only           Verify existing Authenticode signatures without signing.
  --allow-non-windows     Allow non-Windows artifact preflight.

Environment:
  WINDOWS_CERTIFICATE_BASE64      Base64-encoded PFX certificate.
  WINDOWS_CERTIFICATE_PATH        Existing PFX certificate path.
  WINDOWS_CERTIFICATE_PASSWORD    PFX password when required.
  WINDOWS_CERTIFICATE_SHA1        Cert store thumbprint alternative.
  WINDOWS_TIMESTAMP_URL           RFC3161 timestamp URL, defaults to DigiCert.
`);
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
