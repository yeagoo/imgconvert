// SPDX-License-Identifier: Apache-2.0

import { mkdirSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = path.join(
  repoRoot,
  "packaging",
  "flatpak",
  "extensions",
  "heic",
  "com.ivmm.imgconvert.Codecs.Heic.yml",
);
const buildDir = path.join(repoRoot, "target", "flatpak", "heic-extension-build");
const downloadDir = path.join(repoRoot, "target", "flatpak", "heic-extension-download");

const options = {
  checkOnly: false,
  downloadOnly: false,
  install: false,
  repo: "",
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg === "--check-only") {
    options.checkOnly = true;
  } else if (arg === "--download-only") {
    options.downloadOnly = true;
  } else if (arg === "--install") {
    options.install = true;
  } else if (arg.startsWith("--repo=")) {
    options.repo = path.resolve(repoRoot, arg.slice("--repo=".length));
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

run("node", ["scripts/check-flatpak-heic-extension.mjs"]);

if (options.checkOnly) {
  console.log("Flatpak HEIC extension smoke skipped build (--check-only).");
  process.exit(0);
}

if (options.downloadOnly && options.install) {
  fail("--download-only cannot be combined with --install");
}

if (!commandExists("flatpak-builder")) {
  fail("flatpak-builder is required for HEIC extension build smoke");
}

const activeBuildDir = options.downloadOnly ? downloadDir : buildDir;
mkdirSync(activeBuildDir, { recursive: true });
const args = ["--force-clean"];
if (options.downloadOnly) {
  console.log(
    "Flatpak HEIC extension download-only smoke allows the main app runtime to be missing.",
  );
  args.push("--download-only", "--allow-missing-runtimes");
} else {
  args.push("--install-deps-from=flathub");
}
if (options.repo) {
  mkdirSync(options.repo, { recursive: true });
  args.push(`--repo=${options.repo}`);
}
if (options.install) {
  args.push("--user", "--install");
}
args.push(activeBuildDir, manifest);

run("flatpak-builder", args, {
  cwd: path.join(repoRoot, "packaging", "flatpak", "extensions", "heic"),
});

console.log(
  options.downloadOnly
    ? "Flatpak HEIC extension source download smoke passed."
    : "Flatpak HEIC extension build smoke passed.",
);

function commandExists(command) {
  const result = spawnSync("sh", ["-c", `command -v "$1" >/dev/null 2>&1`, "sh", command], {
    stdio: "ignore",
  });
  return result.status === 0;
}

function run(command, args, extra = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: "inherit",
    ...extra,
  });
  if (result.status !== 0) {
    fail(`${command} ${args.join(" ")} failed with exit code ${result.status}`);
  }
}

function fail(message) {
  console.error(`smoke-flatpak-heic-extension: ${message}`);
  process.exit(1);
}
