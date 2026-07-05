// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const allowNonMacos = args.includes("--allow-non-macos");

if (os.platform() !== "darwin" && !allowNonMacos) {
  console.error(
    "AVIF platform benchmark is intended for macOS/Apple Silicon. Pass --allow-non-macos only for smoke testing the benchmark harness.",
  );
  process.exit(1);
}

const env = {
  ...process.env,
  IMGCONVERT_PLATFORM_BENCHMARK_WIDTH: process.env.IMGCONVERT_AVIF_BENCHMARK_WIDTH ?? "1024",
  IMGCONVERT_PLATFORM_BENCHMARK_HEIGHT: process.env.IMGCONVERT_AVIF_BENCHMARK_HEIGHT ?? "768",
  IMGCONVERT_PLATFORM_BENCHMARK_ITERATIONS: process.env.IMGCONVERT_AVIF_BENCHMARK_ITERATIONS ?? "3",
  IMGCONVERT_PLATFORM_BENCHMARK_FORMATS: "avif",
  IMGCONVERT_PLATFORM_BENCHMARK_AVIF_SPEEDS: process.env.IMGCONVERT_AVIF_BENCHMARK_SPEEDS ?? "8,10",
};

const commandArgs = ["scripts/benchmark-platform.mjs"];

console.log(
  `running AVIF benchmark (${env.IMGCONVERT_PLATFORM_BENCHMARK_WIDTH}x${env.IMGCONVERT_PLATFORM_BENCHMARK_HEIGHT}, speeds ${env.IMGCONVERT_PLATFORM_BENCHMARK_AVIF_SPEEDS})`,
);

const result = spawnSync(process.execPath, commandArgs, {
  cwd: repoRoot,
  env,
  stdio: "inherit",
});

if (result.error) {
  console.error(`failed to start benchmark-platform: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
