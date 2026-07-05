// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const options = {
  prepare: true,
  requireReal: false,
  includeArtifacts: true,
  corpusRoot: path.join(repoRoot, "fuzz", "corpus"),
  artifactsRoot: path.join(repoRoot, "fuzz", "artifacts"),
  output: path.join(repoRoot, "target", "fuzz-corpus", "replay-report.json"),
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg === "--prepare") {
    options.prepare = true;
  } else if (arg === "--skip-prepare") {
    options.prepare = false;
  } else if (arg === "--require-real") {
    options.requireReal = true;
  } else if (arg === "--include-artifacts") {
    options.includeArtifacts = true;
  } else if (arg === "--no-artifacts") {
    options.includeArtifacts = false;
  } else if (arg.startsWith("--corpus-root=")) {
    options.corpusRoot = resolveFromRepo(arg.slice("--corpus-root=".length));
  } else if (arg.startsWith("--artifacts-root=")) {
    options.artifactsRoot = resolveFromRepo(arg.slice("--artifacts-root=".length));
  } else if (arg.startsWith("--output=")) {
    options.output = resolveFromRepo(arg.slice("--output=".length));
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

if (options.prepare) {
  const prepareArgs = ["scripts/prepare-fuzz-corpus.mjs"];
  if (options.requireReal) {
    prepareArgs.push("--require-real");
  }
  run(process.execPath, prepareArgs, "prepare fuzz corpus");
}

const replayArgs = [
  "+1.96.0",
  "run",
  "--quiet",
  "-p",
  "imgconvert-core",
  "--example",
  "replay_fuzz_corpus",
  "--",
  "--corpus-root",
  options.corpusRoot,
  "--artifacts-root",
  options.artifactsRoot,
  options.includeArtifacts ? "--include-artifacts" : "--no-artifacts",
];

const replay = spawnSync("cargo", replayArgs, {
  cwd: repoRoot,
  env: process.env,
  encoding: "utf8",
  maxBuffer: 32 * 1024 * 1024,
});

if (replay.stderr) {
  process.stderr.write(replay.stderr);
}
if (replay.error) {
  fail(`failed to start fuzz corpus replay: ${replay.error.message}`);
}

let report;
try {
  report = JSON.parse(replay.stdout);
} catch (error) {
  if (replay.stdout) {
    process.stdout.write(replay.stdout);
  }
  fail(`fuzz corpus replay did not emit valid JSON: ${error.message}`);
}

const enrichedReport = {
  ...report,
  generatedAt: new Date().toISOString(),
  host: {
    platform: os.platform(),
    arch: os.arch(),
  },
  corpusRoot: reportPathLabel(options.corpusRoot),
  artifactsRoot: reportPathLabel(options.artifactsRoot),
  includedArtifacts: options.includeArtifacts,
};

mkdirSync(path.dirname(options.output), { recursive: true });
writeFileSync(options.output, `${JSON.stringify(enrichedReport, null, 2)}\n`);

const outputPath = path.relative(repoRoot, options.output);
console.log(
  `fuzz corpus replay: files=${report.totalFiles}, passed=${report.passed}, skipped=${report.skipped}, failed=${report.failed}, report=${outputPath}`,
);

if ((replay.status ?? 1) !== 0 || report.failed > 0) {
  process.exit(replay.status ?? 1);
}

function run(command, args, label) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    env: process.env,
    stdio: "inherit",
  });
  if (result.error) {
    fail(`failed to start ${label}: ${result.error.message}`);
  }
  if ((result.status ?? 1) !== 0) {
    process.exit(result.status ?? 1);
  }
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
  console.error(`replay fuzz corpus failed: ${message}`);
  process.exit(1);
}
