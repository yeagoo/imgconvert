// SPDX-License-Identifier: Apache-2.0

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const workflowsDir = path.join(repoRoot, ".github", "workflows");
const failures = [];

const packageJson = readJson(path.join(repoRoot, "package.json"));
const ci = readWorkflow("ci.yml");
const linuxRelease = readWorkflow("release-linux.yml");
const updaterRelease = readWorkflow("release-updater.yml");
const macosSmoke = readWorkflow("macos-smoke.yml");
const windowsSmoke = readWorkflow("windows-smoke.yml");

checkManualOnly("ci.yml", ci);
checkManualOnly("release-linux.yml", linuxRelease);
checkManualOnly("release-updater.yml", updaterRelease);
checkManualOnly("macos-smoke.yml", macosSmoke);
checkManualOnly("windows-smoke.yml", windowsSmoke);
checkCiWorkflow();
checkLinuxReleaseWorkflow();
checkUpdaterReleaseWorkflow();
checkPaidPlatformWorkflow("macos-smoke.yml", macosSmoke, "macOS", "macos-15");
checkPaidPlatformWorkflow("windows-smoke.yml", windowsSmoke, "Windows", "windows-latest");

if (!packageJson.scripts?.["ci:cost:check"]?.includes("check-ci-cost-guardrails.mjs")) {
  failures.push("package.json must expose ci:cost:check");
}
if (!packageJson.scripts?.["quality:security"]?.includes("ci:cost:check")) {
  failures.push("quality:security must include ci:cost:check");
}

if (failures.length > 0) {
  console.error("CI cost guardrail check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log("CI cost guardrail check passed.");

function checkManualOnly(name, text) {
  for (const trigger of ["push", "pull_request", "pull_request_target", "schedule"]) {
    if (hasYamlKeyLine(text, trigger)) {
      failures.push(`${name} must not use automatic trigger ${trigger}`);
    }
  }
  if (!text.includes("workflow_dispatch:")) {
    failures.push(`${name} must be manually dispatchable`);
  }
}

function checkCiWorkflow() {
  requireBooleanInputDefault(ci, "platform_checks", false, "ci.yml");
  requireBooleanInputDefault(ci, "fuzz_corpus", false, "ci.yml");
  requireBooleanInputDefault(ci, "package_smoke", false, "ci.yml");
  requireBooleanInputDefault(ci, "package_smoke_arm64", false, "ci.yml");

  if (!ci.includes("name: Fuzz Corpus Replay")) {
    failures.push("ci.yml must expose an optional fuzz corpus replay job");
  }
  if (!ci.includes("inputs.fuzz_corpus")) {
    failures.push("ci.yml fuzz corpus job must be gated by inputs.fuzz_corpus");
  }
  if (!ci.includes("pnpm run fuzz:ci")) {
    failures.push("ci.yml fuzz corpus job must run pnpm run fuzz:ci");
  }
  if (!ci.includes("inputs.package_smoke_arm64")) {
    failures.push("ci.yml package smoke matrix must require package_smoke_arm64 for arm64");
  }
  if (!ci.includes("ubuntu-24.04-arm")) {
    failures.push("ci.yml must keep the arm64 runner explicit when enabled");
  }
}

function checkLinuxReleaseWorkflow() {
  requireBooleanInputDefault(linuxRelease, "docker_smoke", false, "release-linux.yml");
  requireBooleanInputDefault(linuxRelease, "build_arm64", false, "release-linux.yml");

  if (!linuxRelease.includes("inputs.build_arm64")) {
    failures.push("release-linux.yml must require build_arm64 before adding arm64 to matrix");
  }
  if (!linuxRelease.includes("ubuntu-24.04-arm")) {
    failures.push("release-linux.yml must keep the arm64 runner explicit when enabled");
  }
  if (!linuxRelease.includes("inputs.docker_smoke")) {
    failures.push("release-linux.yml Docker smoke must be gated by docker_smoke input");
  }
}

function checkUpdaterReleaseWorkflow() {
  requireBooleanInputDefault(updaterRelease, "publish_release", false, "release-updater.yml");
  requireBooleanInputDefault(updaterRelease, "draft_release", true, "release-updater.yml");
  requireBooleanInputDefault(updaterRelease, "prerelease", false, "release-updater.yml");

  if (!updaterRelease.includes("pnpm run release:linux:updater")) {
    failures.push("release-updater.yml must build signed Linux updater artifacts");
  }
  if (!updaterRelease.includes("pnpm run release:updater:manifest")) {
    failures.push("release-updater.yml must generate latest.json");
  }
  if (!updaterRelease.includes("pnpm run release:updater:verify")) {
    failures.push("release-updater.yml must verify latest.json before upload");
  }
  if (!updaterRelease.includes("inputs.publish_release")) {
    failures.push("release-updater.yml publishing must be gated by publish_release");
  }
}

function checkPaidPlatformWorkflow(name, text, label, runner) {
  requireBooleanInputDefault(text, "confirm_paid_runner", false, name);
  if (!text.includes(`runs-on: ${runner}`)) {
    failures.push(`${name} must keep ${label} runner explicit`);
  }
  if (!text.includes("inputs.confirm_paid_runner")) {
    failures.push(`${name} must gate ${label} runner job by confirm_paid_runner`);
  }
}

function requireBooleanInputDefault(text, inputName, expected, workflowName) {
  const inputBlock = text.match(
    new RegExp(`\\n\\s{6}${escapeRegExp(inputName)}:\\n(?<body>(?:\\s{8}.+\\n)+)`),
  );
  if (!inputBlock?.groups?.body) {
    failures.push(`${workflowName} must define workflow_dispatch input ${inputName}`);
    return;
  }
  if (!new RegExp(`\\n\\s{8}type:\\s*boolean\\b`).test(inputBlock.groups.body)) {
    failures.push(`${workflowName} input ${inputName} must be boolean`);
  }
  if (!new RegExp(`\\n\\s{8}default:\\s*${expected}\\b`).test(inputBlock.groups.body)) {
    failures.push(`${workflowName} input ${inputName} must default to ${expected}`);
  }
}

function hasYamlKeyLine(text, key) {
  const pattern = new RegExp(`^\\s*${escapeRegExp(key)}\\s*:`);
  return text.split(/\r?\n/).some((line) => pattern.test(line));
}

function readWorkflow(name) {
  return readFileSync(path.join(workflowsDir, name), "utf8");
}

function readJson(file) {
  return JSON.parse(readFileSync(file, "utf8"));
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
