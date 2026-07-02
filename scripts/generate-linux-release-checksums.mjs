// SPDX-License-Identifier: Apache-2.0

import { createHash } from "node:crypto";
import {
  createReadStream,
  existsSync,
  mkdirSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const options = {
  profile: "release",
  target: "",
  bundles: ["deb", "rpm", "appimage"],
  output: "",
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg.startsWith("--profile=")) {
    options.profile = arg.slice("--profile=".length);
  } else if (arg.startsWith("--target=")) {
    options.target = arg.slice("--target=".length);
  } else if (arg.startsWith("--bundles=")) {
    options.bundles = arg
      .slice("--bundles=".length)
      .split(",")
      .map((bundle) => bundle.trim().toLowerCase())
      .filter(Boolean);
  } else if (arg.startsWith("--output=")) {
    options.output = arg.slice("--output=".length);
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

if (!["debug", "release"].includes(options.profile)) {
  fail(`unsupported profile: ${options.profile}`);
}

const expectedExtensions = {
  deb: ".deb",
  rpm: ".rpm",
  appimage: ".AppImage",
};

for (const bundle of options.bundles) {
  if (!expectedExtensions[bundle]) {
    fail(`unsupported Linux bundle: ${bundle}`);
  }
}

const bundleRootParts = ["src-tauri", "target"];
if (options.target) {
  bundleRootParts.push(options.target);
}
bundleRootParts.push(options.profile, "bundle");
const bundleRoot = path.join(repoRoot, ...bundleRootParts);
const output = path.resolve(repoRoot, options.output || path.join(bundleRoot, "SHA256SUMS"));

const artifacts = options.bundles.flatMap((bundle) => {
  const bundleDir = path.join(bundleRoot, bundle);
  return collectFiles(bundleDir).filter((file) => file.endsWith(expectedExtensions[bundle]));
});

if (artifacts.length === 0) {
  fail(`no Linux bundle artifacts found under ${path.relative(repoRoot, bundleRoot)}`);
}

const lines = [];
for (const artifact of artifacts.sort()) {
  const hash = await sha256File(artifact);
  lines.push(`${hash}  ${path.relative(path.dirname(output), artifact)}`);
}

mkdirSync(path.dirname(output), { recursive: true });
writeFileSync(output, `${lines.join("\n")}\n`, "utf8");

console.log(`wrote ${path.relative(repoRoot, output)} (${artifacts.length} artifact(s))`);

function collectFiles(dir) {
  if (!existsSync(dir)) {
    return [];
  }
  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const entryPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectFiles(entryPath));
    } else if (entry.isFile() && statSync(entryPath).size > 0) {
      files.push(entryPath);
    }
  }
  return files;
}

function sha256File(file) {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    const stream = createReadStream(file);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("error", reject);
    stream.on("end", () => resolve(hash.digest("hex")));
  });
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
