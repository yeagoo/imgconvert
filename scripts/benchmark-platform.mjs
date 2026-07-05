// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const options = {
  output: undefined,
  profile: "release",
  writeOutput: true,
};

for (const arg of args) {
  if (arg === "--") {
    continue;
  } else if (arg === "--no-output") {
    options.writeOutput = false;
  } else if (arg === "--debug") {
    options.profile = "debug";
  } else if (arg.startsWith("--profile=")) {
    const profile = arg.slice("--profile=".length);
    if (!["debug", "release"].includes(profile)) {
      console.error(`unsupported benchmark profile: ${profile}`);
      process.exit(2);
    }
    options.profile = profile;
  } else if (arg.startsWith("--output=")) {
    options.output = arg.slice("--output=".length);
  } else {
    console.error(`unknown argument: ${arg}`);
    process.exit(2);
  }
}

const env = {
  ...process.env,
  IMGCONVERT_PLATFORM_BENCHMARK: "1",
  IMGCONVERT_PLATFORM_BENCHMARK_WIDTH:
    process.env.IMGCONVERT_PLATFORM_BENCHMARK_WIDTH ??
    process.env.IMGCONVERT_AVIF_BENCHMARK_WIDTH ??
    "1024",
  IMGCONVERT_PLATFORM_BENCHMARK_HEIGHT:
    process.env.IMGCONVERT_PLATFORM_BENCHMARK_HEIGHT ??
    process.env.IMGCONVERT_AVIF_BENCHMARK_HEIGHT ??
    "768",
  IMGCONVERT_PLATFORM_BENCHMARK_ITERATIONS:
    process.env.IMGCONVERT_PLATFORM_BENCHMARK_ITERATIONS ??
    process.env.IMGCONVERT_AVIF_BENCHMARK_ITERATIONS ??
    "3",
  IMGCONVERT_PLATFORM_BENCHMARK_FORMATS:
    process.env.IMGCONVERT_PLATFORM_BENCHMARK_FORMATS ?? "avif,webp",
  IMGCONVERT_PLATFORM_BENCHMARK_AVIF_SPEEDS:
    process.env.IMGCONVERT_PLATFORM_BENCHMARK_AVIF_SPEEDS ??
    process.env.IMGCONVERT_AVIF_BENCHMARK_SPEEDS ??
    "8,10",
  IMGCONVERT_PLATFORM_BENCHMARK_WEBP_METHODS:
    process.env.IMGCONVERT_PLATFORM_BENCHMARK_WEBP_METHODS ?? "4,6",
  IMGCONVERT_PLATFORM_BENCHMARK_QUALITY: process.env.IMGCONVERT_PLATFORM_BENCHMARK_QUALITY ?? "82",
};

const commandArgs = [
  "+1.96.0",
  "run",
  "--manifest-path",
  "src-tauri/Cargo.toml",
  "--bin",
  "imgconvert",
];
if (options.profile === "release") {
  commandArgs.splice(2, 0, "--release");
}

console.error(
  [
    `running platform benchmark on ${os.platform()}/${os.arch()}`,
    `profile ${options.profile}`,
    `${env.IMGCONVERT_PLATFORM_BENCHMARK_WIDTH}x${env.IMGCONVERT_PLATFORM_BENCHMARK_HEIGHT}`,
    `formats ${env.IMGCONVERT_PLATFORM_BENCHMARK_FORMATS}`,
    `quality ${env.IMGCONVERT_PLATFORM_BENCHMARK_QUALITY}`,
  ].join(", "),
);

const result = spawnSync("cargo", commandArgs, {
  cwd: repoRoot,
  env,
  encoding: "utf8",
  maxBuffer: 32 * 1024 * 1024,
  stdio: ["ignore", "pipe", "inherit"],
});

if (result.error) {
  console.error(`failed to start cargo: ${result.error.message}`);
  process.exit(1);
}

if (result.stdout) {
  process.stdout.write(result.stdout);
}

if ((result.status ?? 1) !== 0) {
  process.exit(result.status ?? 1);
}

const events = parseJsonLines(result.stdout ?? "");
const report = buildReport(events, env, options.profile);
if (report.samples.length === 0) {
  console.error("platform benchmark produced no sample events");
  process.exit(1);
}

if (options.writeOutput) {
  const output = path.resolve(repoRoot, options.output ?? defaultOutputPath(report));
  mkdirSync(path.dirname(output), { recursive: true });
  writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
  console.error(`platform benchmark report: ${path.relative(repoRoot, output)}`);
}

printSummary(report);

function parseJsonLines(stdout) {
  const events = [];
  for (const line of stdout.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("{")) {
      continue;
    }
    try {
      events.push(JSON.parse(trimmed));
    } catch {
      // Keep the benchmark tolerant of cargo/plugin noise on stdout.
    }
  }
  return events;
}

function buildReport(events, benchEnv, profile) {
  const start = events.find((event) => event.event === "start") ?? {};
  const samples = events.filter((event) => event.event === "sample");
  const summaries = summarizeSamples(samples);
  return {
    schemaVersion: 1,
    generatedAt: new Date().toISOString(),
    host: {
      platform: os.platform(),
      arch: os.arch(),
      release: os.release(),
      cpus: os.cpus().length,
      totalMemoryBytes: os.totalmem(),
    },
    command: {
      profile,
      width: Number.parseInt(benchEnv.IMGCONVERT_PLATFORM_BENCHMARK_WIDTH, 10),
      height: Number.parseInt(benchEnv.IMGCONVERT_PLATFORM_BENCHMARK_HEIGHT, 10),
      iterations: Number.parseInt(benchEnv.IMGCONVERT_PLATFORM_BENCHMARK_ITERATIONS, 10),
      quality: Number.parseInt(benchEnv.IMGCONVERT_PLATFORM_BENCHMARK_QUALITY, 10),
      formats: benchEnv.IMGCONVERT_PLATFORM_BENCHMARK_FORMATS,
      avifSpeeds: benchEnv.IMGCONVERT_PLATFORM_BENCHMARK_AVIF_SPEEDS,
      webpMethods: benchEnv.IMGCONVERT_PLATFORM_BENCHMARK_WEBP_METHODS,
    },
    start,
    samples,
    summaries,
    recommendations: recommendDefaults(summaries),
  };
}

function summarizeSamples(samples) {
  const groups = new Map();
  for (const sample of samples) {
    const parameterName = sample.format === "avif" ? "speed" : "method";
    const parameter = sample[parameterName];
    const key = `${sample.format}:${parameterName}:${parameter}`;
    const group = groups.get(key) ?? {
      format: sample.format,
      parameterName,
      parameter,
      quality: sample.quality,
      samples: [],
    };
    group.samples.push(sample);
    groups.set(key, group);
  }

  return [...groups.values()]
    .map((group) => ({
      format: group.format,
      parameterName: group.parameterName,
      parameter: group.parameter,
      quality: group.quality,
      iterations: group.samples.length,
      medianMilliseconds: round(median(group.samples.map((sample) => sample.milliseconds)), 3),
      minMilliseconds: round(Math.min(...group.samples.map((sample) => sample.milliseconds)), 3),
      maxMilliseconds: round(Math.max(...group.samples.map((sample) => sample.milliseconds)), 3),
      medianMegapixelsPerSecond: round(
        median(group.samples.map((sample) => sample.megapixelsPerSecond)),
        3,
      ),
      medianBytes: Math.round(median(group.samples.map((sample) => sample.bytes))),
    }))
    .sort((a, b) =>
      a.format === b.format ? a.parameter - b.parameter : a.format.localeCompare(b.format),
    );
}

function recommendDefaults(summaries) {
  const avif8 = findSummary(summaries, "avif", "speed", 8);
  const avif10 = findSummary(summaries, "avif", "speed", 10);
  const webp4 = findSummary(summaries, "webp", "method", 4);
  const webp6 = findSummary(summaries, "webp", "method", 6);
  const recommendations = {};

  if (avif8 && avif10) {
    const sizeSaving = relativeSaving(avif10.medianBytes, avif8.medianBytes);
    const timeRatio = avif8.medianMilliseconds / avif10.medianMilliseconds;
    recommendations.avifSpeed = {
      currentDefault: 8,
      recommended: sizeSaving < 0.02 && timeRatio > 1.4 ? 10 : 8,
      speed8Vs10SizeSaving: round(sizeSaving, 4),
      speed8Vs10TimeRatio: round(timeRatio, 3),
      rationale:
        sizeSaving < 0.02 && timeRatio > 1.4
          ? "speed 8 saves less than 2% over speed 10 while taking more than 1.4x time"
          : "speed 8 still has enough size benefit for the default lossy AVIF profile",
    };
  }

  if (webp4 && webp6) {
    const sizeSaving = relativeSaving(webp4.medianBytes, webp6.medianBytes);
    const timeRatio = webp6.medianMilliseconds / webp4.medianMilliseconds;
    recommendations.webpMethod = {
      currentDefault: 4,
      recommended: sizeSaving >= 0.02 && timeRatio <= 1.35 ? 6 : 4,
      method6Vs4SizeSaving: round(sizeSaving, 4),
      method6Vs4TimeRatio: round(timeRatio, 3),
      rationale:
        sizeSaving >= 0.02 && timeRatio <= 1.35
          ? "method 6 saves at least 2% without exceeding 1.35x method 4 time"
          : "method 6 does not buy enough size reduction for the default WebP profile",
    };
  }

  return recommendations;
}

function findSummary(summaries, format, parameterName, parameter) {
  return summaries.find(
    (summary) =>
      summary.format === format &&
      summary.parameterName === parameterName &&
      summary.parameter === parameter,
  );
}

function relativeSaving(baselineBytes, candidateBytes) {
  if (!Number.isFinite(baselineBytes) || baselineBytes <= 0) {
    return 0;
  }
  return (baselineBytes - candidateBytes) / baselineBytes;
}

function median(values) {
  const sorted = values.filter(Number.isFinite).sort((a, b) => a - b);
  if (sorted.length === 0) {
    return 0;
  }
  const mid = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) {
    return sorted[mid];
  }
  return (sorted[mid - 1] + sorted[mid]) / 2;
}

function round(value, digits) {
  const factor = 10 ** digits;
  return Math.round(value * factor) / factor;
}

function defaultOutputPath(report) {
  const stamp = report.generatedAt
    .replaceAll(":", "")
    .replaceAll("-", "")
    .replace(/\.\d+Z$/, "Z");
  return path.join("target", "benchmarks", `platform-${stamp}-${os.platform()}-${os.arch()}.json`);
}

function printSummary(report) {
  console.error("platform benchmark summary:");
  for (const summary of report.summaries) {
    console.error(
      `  ${summary.format} ${summary.parameterName}=${summary.parameter}: median ${summary.medianMilliseconds} ms, ${summary.medianMegapixelsPerSecond} MP/s, ${summary.medianBytes} bytes`,
    );
  }
  for (const [key, recommendation] of Object.entries(report.recommendations)) {
    console.error(`  ${key}: keep/use ${recommendation.recommended} (${recommendation.rationale})`);
  }
}
