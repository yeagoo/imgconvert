// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const allTargets = readFuzzTargets();
const options = {
  artifactsRoot: path.join(repoRoot, "fuzz", "artifacts"),
  output: path.join(repoRoot, "target", "fuzz-corpus", "minimize-report.json"),
  targets: [],
  artifactPaths: [],
  run: false,
  replay: true,
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg === "--run") {
    options.run = true;
  } else if (arg === "--dry-run") {
    options.run = false;
  } else if (arg === "--replay") {
    options.replay = true;
  } else if (arg === "--skip-replay" || arg === "--no-replay") {
    options.replay = false;
  } else if (arg.startsWith("--target=")) {
    options.targets.push(...splitList(arg.slice("--target=".length)));
  } else if (arg.startsWith("--artifact=")) {
    options.artifactPaths.push(resolveFromRepo(arg.slice("--artifact=".length)));
  } else if (arg.startsWith("--artifacts-root=")) {
    options.artifactsRoot = resolveFromRepo(arg.slice("--artifacts-root=".length));
  } else if (arg.startsWith("--output=")) {
    options.output = resolveFromRepo(arg.slice("--output=".length));
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

const selectedTargets =
  options.targets.length === 0 ? allTargets : unique(options.targets).map(validateTarget);
const artifacts = collectArtifacts(selectedTargets);
const artifactReports = artifacts.map((artifact) => ({
  target: artifact.target,
  artifact: reportPathLabel(artifact.path),
  bytes: artifact.bytes,
  action: options.run ? "pending" : "dry-run",
}));

let failed = 0;
if (options.run && artifacts.length > 0) {
  ensureCargoFuzzAvailable();
  for (const [index, artifact] of artifacts.entries()) {
    const result = spawnSync("cargo", ["fuzz", "tmin", artifact.target, artifact.path], {
      cwd: repoRoot,
      env: process.env,
      stdio: "inherit",
    });
    if (result.error) {
      artifactReports[index].action = "failed";
      artifactReports[index].error = result.error.message;
      failed += 1;
      continue;
    }
    if ((result.status ?? 1) !== 0) {
      artifactReports[index].action = "failed";
      artifactReports[index].exitCode = result.status ?? 1;
      failed += 1;
      continue;
    }
    artifactReports[index].action = "minimized";
    artifactReports[index].bytesAfter = statFile(artifact.path).size;
  }
}

let replayReport = {
  requested: options.run && options.replay && artifacts.length > 0 && failed === 0,
  status: "skipped",
};
if (replayReport.requested) {
  const result = spawnSync(
    process.execPath,
    [
      "scripts/replay-fuzz-corpus.mjs",
      "--skip-prepare",
      "--include-artifacts",
      `--artifacts-root=${options.artifactsRoot}`,
    ],
    {
      cwd: repoRoot,
      env: process.env,
      stdio: "inherit",
    },
  );
  if (result.error) {
    replayReport = {
      ...replayReport,
      status: "failed",
      error: result.error.message,
    };
    failed += 1;
  } else if ((result.status ?? 1) !== 0) {
    replayReport = {
      ...replayReport,
      status: "failed",
      exitCode: result.status ?? 1,
    };
    failed += 1;
  } else {
    replayReport = {
      ...replayReport,
      status: "passed",
    };
  }
}

writeReport(artifactReports, replayReport);

console.log(
  `fuzz artifact minimize ${options.run ? "run" : "dry-run"}: targets=${selectedTargets.length}, artifacts=${artifacts.length}, failed=${failed}, report=${path.relative(repoRoot, options.output)}`,
);

if (failed > 0) {
  process.exit(1);
}

function collectArtifacts(selected) {
  const artifacts = [];
  if (options.artifactPaths.length > 0) {
    for (const artifactPath of options.artifactPaths) {
      const target = inferTargetForArtifact(artifactPath, selected);
      if (!selected.includes(target)) {
        fail(
          `artifact ${reportPathLabel(artifactPath)} belongs to ${target}, which is not in selected targets: ${selected.join(", ")}`,
        );
      }
      const info = statFile(artifactPath);
      artifacts.push({ target, path: artifactPath, bytes: info.size });
    }
    return sortArtifacts(artifacts);
  }

  for (const target of selected) {
    const targetDir = path.join(options.artifactsRoot, target);
    if (!existsSync(targetDir)) {
      continue;
    }
    for (const artifactPath of walkFiles(targetDir)) {
      const info = statFile(artifactPath);
      if (info.size === 0) {
        continue;
      }
      artifacts.push({ target, path: artifactPath, bytes: info.size });
    }
  }
  return sortArtifacts(artifacts);
}

function* walkFiles(root) {
  let entries;
  try {
    entries = readdirSync(root, { withFileTypes: true });
  } catch (error) {
    fail(`artifact directory is not readable: ${reportPathLabel(root)} (${error.message})`);
  }
  entries.sort((left, right) => left.name.localeCompare(right.name));
  for (const entry of entries) {
    const fullPath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      yield* walkFiles(fullPath);
    } else if (entry.isFile()) {
      yield fullPath;
    }
  }
}

function inferTargetForArtifact(artifactPath, selected) {
  const relative = path.relative(options.artifactsRoot, artifactPath);
  const firstSegment = relative.split(path.sep)[0];
  if (allTargets.includes(firstSegment)) {
    return firstSegment;
  }
  if (selected.length === 1) {
    return selected[0];
  }
  fail(
    `cannot infer fuzz target for ${reportPathLabel(artifactPath)}; pass --target=<target> with explicit artifacts`,
  );
}

function statFile(file) {
  let info;
  try {
    info = statSync(file);
  } catch (error) {
    fail(`artifact is not readable: ${reportPathLabel(file)} (${error.message})`);
  }
  if (!info.isFile()) {
    fail(`artifact is not a regular file: ${reportPathLabel(file)}`);
  }
  return info;
}

function ensureCargoFuzzAvailable() {
  const result = spawnSync("cargo", ["fuzz", "--help"], {
    cwd: repoRoot,
    env: process.env,
    encoding: "utf8",
    maxBuffer: 4 * 1024 * 1024,
  });
  if (result.error || (result.status ?? 1) !== 0) {
    fail("cargo-fuzz is required for --run; install it with `cargo install cargo-fuzz`");
  }
}

function writeReport(artifacts, replay) {
  mkdirSync(path.dirname(options.output), { recursive: true });
  writeFileSync(
    options.output,
    `${JSON.stringify(
      {
        schemaVersion: 1,
        generatedAt: new Date().toISOString(),
        host: {
          platform: os.platform(),
          arch: os.arch(),
        },
        mode: options.run ? "run" : "dry-run",
        targets: selectedTargets,
        artifactsRoot: reportPathLabel(options.artifactsRoot),
        artifactCount: artifacts.length,
        artifacts,
        replay,
      },
      null,
      2,
    )}\n`,
  );
}

function readFuzzTargets() {
  const cargoToml = readFileSync(path.join(repoRoot, "fuzz", "Cargo.toml"), "utf8");
  const targets = [...cargoToml.matchAll(/^\[\[bin\]\][\s\S]*?^name\s*=\s*"([^"]+)"/gm)].map(
    (match) => match[1],
  );
  if (targets.length === 0) {
    fail("no fuzz targets found in fuzz/Cargo.toml");
  }
  return targets;
}

function validateTarget(target) {
  if (!allTargets.includes(target)) {
    fail(`unknown fuzz target: ${target}; expected one of ${allTargets.join(", ")}`);
  }
  return target;
}

function splitList(raw) {
  return raw
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
}

function sortArtifacts(artifacts) {
  return artifacts.sort((left, right) => {
    const target = left.target.localeCompare(right.target);
    return target === 0 ? left.path.localeCompare(right.path) : target;
  });
}

function unique(items) {
  return [...new Set(items)];
}

function resolveFromRepo(value) {
  return path.resolve(repoRoot, value);
}

function reportPathLabel(value) {
  const relative = path.relative(repoRoot, value);
  if (relative && !relative.startsWith("..") && !path.isAbsolute(relative)) {
    return relative;
  }
  return path.basename(value);
}

function fail(message) {
  console.error(`minimize fuzz artifacts failed: ${message}`);
  process.exit(1);
}
