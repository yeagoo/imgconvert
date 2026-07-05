// SPDX-License-Identifier: Apache-2.0

import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const appId = "io.github.yeagoo.imgconvert";
const extensionId = "io.github.yeagoo.imgconvert.Codecs.Heic";
const tempRoot = mkdtempSync(path.join(os.tmpdir(), "imgconvert-flatpak-heic-"));
const repoDir = path.join(tempRoot, "repo");
const smokeOutputDir = path.join(tempRoot, "output");
const smokeRemotePrefix = "imgconvert-heic-smoke-";
const remoteName = `${smokeRemotePrefix}${process.pid}`;

const options = {
  sample: process.env.IMGCONVERT_FLATPAK_HEIC_SMOKE_INPUT ?? "",
  keepRemote: false,
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg.startsWith("--sample=")) {
    options.sample = path.resolve(repoRoot, arg.slice("--sample=".length));
  } else if (arg === "--keep-remote") {
    options.keepRemote = true;
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

try {
  ensureCommand("flatpak");
  ensureCommand("flatpak-builder");
  cleanupSmokeInstalls();
  cleanupSmokeRemotes();
  run("node", ["scripts/check-flatpak-heic-extension.mjs"]);
  run("node", ["scripts/check-flathub-metadata.mjs", "--no-appstreamcli"]);

  mkdirSync(repoDir, { recursive: true });
  mkdirSync(smokeOutputDir, { recursive: true, mode: 0o700 });

  const sample = options.sample ? requireSample(options.sample) : generateHeicSample();
  buildMainPackage();
  updateLocalRepoSummary();
  addLocalRepoRemote();
  buildHeicExtension();
  updateLocalRepoSummary();
  installFromLocalRepo();
  runHeicPathSmoke(sample);
  console.log("Flatpak HEIC extension real sandbox smoke passed.");
} finally {
  if (!options.keepRemote) {
    cleanupSmokeInstalls(remoteName);
    spawnSync("flatpak", ["remote-delete", "--user", "--force", remoteName], {
      cwd: repoRoot,
      stdio: "ignore",
    });
  }
  rmSync(tempRoot, { force: true, recursive: true });
}

function buildMainPackage() {
  run("node", [
    "scripts/smoke-flatpak-runtime.mjs",
    "--prepare",
    "--no-install",
    `--repo=${repoDir}`,
  ]);
}

function buildHeicExtension() {
  run("node", [
    "scripts/smoke-flatpak-heic-extension.mjs",
    `--repo=${repoDir}`,
    `--install-deps-from=${remoteName}`,
  ]);
}

function addLocalRepoRemote() {
  const remoteUrl = pathToFileUrl(repoDir);
  run("flatpak", [
    "remote-add",
    "--user",
    "--if-not-exists",
    "--no-gpg-verify",
    remoteName,
    remoteUrl,
  ]);
}

function updateLocalRepoSummary() {
  run("flatpak", ["build-update-repo", repoDir]);
}

function installFromLocalRepo() {
  run("flatpak", [
    "install",
    "--user",
    "--assumeyes",
    "--noninteractive",
    "--reinstall",
    remoteName,
    `${appId}//stable`,
  ]);
  run("flatpak", [
    "install",
    "--user",
    "--assumeyes",
    "--noninteractive",
    "--reinstall",
    remoteName,
    `${extensionId}//1`,
  ]);
}

function runHeicPathSmoke(sample) {
  run("flatpak", [
    "run",
    "--user",
    "--command=imgconvert",
    `--filesystem=${path.dirname(sample)}:ro`,
    `--filesystem=${smokeOutputDir}:rw`,
    "--env=IMGCONVERT_PATH_CONVERT_SMOKE=1",
    `--env=IMGCONVERT_PATH_CONVERT_SMOKE_INPUT=${sample}`,
    "--env=IMGCONVERT_PATH_CONVERT_SMOKE_FORMAT=png",
    `--env=IMGCONVERT_PATH_CONVERT_SMOKE_DIR=${smokeOutputDir}`,
    "--env=IMGCONVERT_DISABLE_EXTERNAL_CODECS=1",
    "--env=IMGCONVERT_ALLOW_FLATPAK_CODEC_EXTENSIONS=1",
    appId,
  ]);
}

function generateHeicSample() {
  ensureCommand("heif-enc");
  const fixturePng = path.join(tempRoot, "fixture.png");
  const fixtureHeic = path.join(tempRoot, "fixture.heic");
  writeFileSync(fixturePng, Buffer.from(fixturePngBase64(), "base64"));
  run("heif-enc", ["-q", "80", fixturePng, "-o", fixtureHeic]);
  return requireSample(fixtureHeic);
}

function requireSample(sample) {
  if (!existsSync(sample)) {
    fail(`HEIC sample does not exist: ${sample}`);
  }
  const bytes = readFileSync(sample);
  if (!isHeicMagic(bytes)) {
    fail(`sample is not a HEIF/HEIC file: ${sample}`);
  }
  return sample;
}

function isHeicMagic(bytes) {
  if (bytes.length < 12 || bytes.subarray(4, 8).toString("ascii") !== "ftyp") {
    return false;
  }
  const brands = bytes.subarray(8, Math.min(bytes.length, 64)).toString("ascii");
  return ["heic", "heix", "hevc", "hevx", "heif", "heis", "mif1", "msf1"].some((brand) =>
    brands.includes(brand),
  );
}

function fixturePngBase64() {
  return [
    "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQEAIAAADAAbR1AAAAIGNIUk0AAHomAACAhAAA+gAAAIDo",
    "AAB1MAAA6mAAADqYAAAXcJy6UTwAAAAGYktHRP///////wlY99wAAAAHdElNRQfqBwUOKx8PE2Pw",
    "AAAASUlEQVQ4y2P8+bO4WEyMgWaA5aMbcySfPA0t+ODKEkFbC2jtg49uzJF8CrT0wZAPItrH",
    "gStLBP+Q9gHtLaB1KqJ9PhgeQURDCwA77h12znVq7gAAACV0RVh0ZGF0ZTpjcmVhdGUAMjAy",
    "Ni0wNy0wNVQxNDo0MzozMSswMDowMGCqJRkAAAAldEVYdGRhdGU6bW9kaWZ5ADIwMjYtMDct",
    "MDVUMTQ6NDM6MzErMDA6MDAR952lAAAAKHRFWHRkYXRlOnRpbWVzdGFtcAAyMDI2LTA3LTA1",
    "VDE0OjQzOjMxKzAwOjAwRuK8egAAAABJRU5ErkJggg==",
  ].join("");
}

function ensureCommand(command) {
  const result = spawnSync("sh", ["-c", `command -v "$1" >/dev/null 2>&1`, "sh", command], {
    stdio: "ignore",
  });
  if (result.status !== 0) {
    fail(`${command} is required for Flatpak HEIC real sandbox smoke`);
  }
}

function cleanupSmokeInstalls(expectedOrigin = null) {
  for (const ref of [`${extensionId}//1`, `${appId}//stable`]) {
    const origin = installedOrigin(ref);
    if (!origin) {
      continue;
    }
    const shouldRemove = expectedOrigin
      ? origin === expectedOrigin
      : origin.startsWith(smokeRemotePrefix);
    if (!shouldRemove) {
      continue;
    }
    spawnSync("flatpak", ["uninstall", "--user", "--assumeyes", "--noninteractive", ref], {
      cwd: repoRoot,
      stdio: "ignore",
    });
  }
}

function cleanupSmokeRemotes() {
  const result = spawnSync("flatpak", ["remotes", "--user", "--columns=name"], {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  if (result.status !== 0) {
    return;
  }
  for (const name of result.stdout.split(/\r?\n/).map((line) => line.trim())) {
    if (!name.startsWith(smokeRemotePrefix)) {
      continue;
    }
    spawnSync("flatpak", ["remote-delete", "--user", "--force", name], {
      cwd: repoRoot,
      stdio: "ignore",
    });
  }
}

function installedOrigin(ref) {
  const result = spawnSync("flatpak", ["info", "--user", "--show-origin", ref], {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  });
  if (result.status !== 0) {
    return "";
  }
  return result.stdout.trim();
}

function pathToFileUrl(value) {
  const resolved = path.resolve(value);
  return `file://${resolved.split(path.sep).map(encodeURIComponent).join("/")}`;
}

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: "inherit",
  });
  if (result.status !== 0) {
    fail(`${command} ${args.join(" ")} failed with exit code ${result.status}`);
  }
}

function fail(message) {
  console.error(`smoke-flatpak-heic-runtime: ${message}`);
  process.exit(1);
}
