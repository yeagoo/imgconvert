// SPDX-License-Identifier: Apache-2.0

import { existsSync, mkdirSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = path.join(repoRoot, "packaging", "flatpak", "io.github.yeagoo.imgconvert.yml");
const buildRoot = path.join(repoRoot, "target", "flatpak");
const buildDir = path.join(buildRoot, "build-dir");
const stateDir = path.join(buildRoot, "builder-state");
const smokeDir = path.join(os.tmpdir(), `imgconvert-flatpak-smoke-${process.pid}`);
const appId = "io.github.yeagoo.imgconvert";
const flathubUrl = "https://flathub.org/repo/flathub.flatpakrepo";

const options = {
  prepare: false,
  skipFetch: false,
  noInstall: false,
  repo: "",
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--prepare") {
    options.prepare = true;
  } else if (arg === "--skip-fetch") {
    options.skipFetch = true;
  } else if (arg === "--no-install") {
    options.noInstall = true;
  } else if (arg.startsWith("--repo=")) {
    options.repo = path.resolve(repoRoot, arg.slice("--repo=".length));
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

if (!existsSync(manifestPath)) {
  fail(`missing ${path.relative(repoRoot, manifestPath)}`);
}

ensureCommand("flatpak");
ensureCommand("flatpak-builder");

if (options.prepare) {
  prepareReleaseArchive();
}
run("node", ["scripts/check-flatpak-manifest.mjs"]);

mkdirSync(buildRoot, { recursive: true });
rmSync(smokeDir, { force: true, recursive: true });
mkdirSync(smokeDir, { recursive: true, mode: 0o700 });

ensureFlathubRemote();
buildFlatpak();
runBuildSandboxSmoke();
if (!options.noInstall) {
  runInstalledSandboxSmoke();
}

console.log("Flatpak runtime smoke passed.");

function prepareReleaseArchive() {
  const prepareArgs = ["scripts/prepare-flatpak-release.mjs"];
  if (options.skipFetch) {
    prepareArgs.push("--skip-fetch");
  }
  const result = run("node", prepareArgs, { allowFailure: options.skipFetch });
  if (result.status === 0) {
    return;
  }
  console.warn("Flatpak prepare --skip-fetch failed; retrying with pnpm fetch.");
  run("node", ["scripts/prepare-flatpak-release.mjs"]);
}

function ensureFlathubRemote() {
  const remotes = run("flatpak", ["remotes", "--user", "--columns=name"], {
    capture: true,
    allowFailure: true,
  });
  if (remotes.status === 0 && remotes.stdout.split(/\r?\n/).includes("flathub")) {
    return;
  }
  run("flatpak", ["remote-add", "--user", "--if-not-exists", "flathub", flathubUrl]);
}

function buildFlatpak() {
  const args = [
    "--user",
    "--assumeyes",
    "--install-deps-from=flathub",
    "--state-dir",
    stateDir,
    "--force-clean",
    "--default-branch=stable",
  ];
  if (!options.noInstall) {
    args.push("--install");
  }
  if (options.repo) {
    mkdirSync(options.repo, { recursive: true });
    args.push(`--repo=${options.repo}`);
  }
  args.push(buildDir, manifestPath);
  run("flatpak-builder", args);
}

function runBuildSandboxSmoke() {
  run("flatpak-builder", [
    "--run",
    buildDir,
    manifestPath,
    "env",
    "IMGCONVERT_DISABLE_EXTERNAL_CODECS=1",
    "IMGCONVERT_PACKAGE_CONVERT_SMOKE=1",
    `IMGCONVERT_PACKAGE_CONVERT_SMOKE_DIR=${path.join(smokeDir, "build-sandbox")}`,
    "imgconvert",
  ]);
}

function runInstalledSandboxSmoke() {
  run("flatpak", [
    "run",
    "--user",
    "--command=imgconvert",
    "--env=IMGCONVERT_DISABLE_EXTERNAL_CODECS=1",
    "--env=IMGCONVERT_PACKAGE_CONVERT_SMOKE=1",
    `--env=IMGCONVERT_PACKAGE_CONVERT_SMOKE_DIR=${path.join(smokeDir, "installed")}`,
    appId,
  ]);
}

function ensureCommand(command) {
  const result = spawnSync(command, ["--version"], {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    fail(`${command} is required for Flatpak runtime smoke`);
  }
}

function run(command, args, opts = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: opts.capture ? ["ignore", "pipe", "pipe"] : "inherit",
  });
  if (result.status !== 0 && !opts.allowFailure) {
    if (opts.capture) {
      process.stdout.write(result.stdout);
      process.stderr.write(result.stderr);
    }
    fail(`${command} ${args.join(" ")} failed with exit code ${result.status}`);
  }
  return result;
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
