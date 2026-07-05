// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const passthrough = process.argv.slice(2).filter((arg) => arg !== "--");
const args = ["+1.96.0", "test", "-p", "imgconvert-core", "--test", "image_quality"];

if (passthrough.length > 0) {
  args.push("--", ...passthrough);
}

console.log("running image quality test suite: golden, corrupted input, determinism, artifacts");
const result = spawnSync("cargo", args, {
  cwd: repoRoot,
  stdio: "inherit",
});

if (result.error) {
  console.error(`failed to start cargo: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
