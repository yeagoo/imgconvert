// SPDX-License-Identifier: Apache-2.0

import { existsSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const options = {
  profile: "release",
  target: "",
  bundle: "deb",
  image: "",
  timeoutSeconds: 15,
  convertSmoke: false,
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg.startsWith("--profile=")) {
    options.profile = arg.slice("--profile=".length);
  } else if (arg.startsWith("--target=")) {
    options.target = arg.slice("--target=".length);
  } else if (arg.startsWith("--bundle=")) {
    options.bundle = arg.slice("--bundle=".length).toLowerCase();
  } else if (arg.startsWith("--image=")) {
    options.image = arg.slice("--image=".length);
  } else if (arg.startsWith("--timeout=")) {
    options.timeoutSeconds = Number.parseInt(arg.slice("--timeout=".length), 10);
  } else if (arg === "--convert-smoke") {
    options.convertSmoke = true;
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

if (!["debug", "release"].includes(options.profile)) {
  fail(`unsupported profile: ${options.profile}`);
}
if (!["deb", "rpm", "appimage"].includes(options.bundle)) {
  fail(`unsupported Linux bundle: ${options.bundle}`);
}
if (!Number.isFinite(options.timeoutSeconds) || options.timeoutSeconds < 3) {
  fail(`unsupported timeout: ${options.timeoutSeconds}`);
}

const artifact = findArtifact();
if (!artifact) {
  fail(`no ${options.bundle} artifact found`);
}

if (options.image) {
  smokeInDocker(artifact);
} else {
  smokeOnHost(artifact);
}

function findArtifact() {
  const extension = {
    deb: ".deb",
    rpm: ".rpm",
    appimage: ".AppImage",
  }[options.bundle];
  const parts = ["src-tauri", "target"];
  if (options.target) {
    parts.push(options.target);
  }
  parts.push(options.profile, "bundle", options.bundle);
  const dir = path.join(repoRoot, ...parts);
  return collectFiles(dir)
    .filter((file) => file.endsWith(extension))
    .sort((left, right) => statSync(right).mtimeMs - statSync(left).mtimeMs)[0];
}

function smokeOnHost(artifact) {
  const absolute = path.resolve(artifact);
  if (options.bundle === "deb") {
    run("sudo", ["apt-get", "install", "-y", absolute]);
  } else if (options.bundle === "rpm") {
    run("sudo", ["dnf", "install", "-y", absolute]);
  } else {
    run("chmod", ["+x", absolute]);
  }
  const command = options.bundle === "appimage" ? absolute : "imgconvert";
  runLaunchSmoke(command);
  if (options.convertSmoke) {
    runConversionSmoke(command);
  }
}

function smokeInDocker(artifact) {
  const docker = resolveDockerCommand();
  const absolute = path.resolve(artifact);
  const mounted = `/tmp/imgconvert-package${path.extname(absolute)}`;
  const script = dockerSmokeScript(mounted);
  run(
    docker.command,
    [
      ...docker.prefixArgs,
      "run",
      "--rm",
      "-v",
      `${absolute}:${mounted}:ro`,
      options.image,
      "sh",
      "-lc",
      script,
    ],
    `docker package smoke in ${options.image}`,
  );
}

function dockerSmokeScript(artifact) {
  const appimageRuntimeArtifact = "/tmp/imgconvert-package-run.AppImage";
  const aptGet =
    "apt-get -o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30";
  const launch = launchCommand(
    options.bundle === "appimage" ? appimageRuntimeArtifact : "imgconvert",
  );
  const convertSmoke = options.convertSmoke
    ? conversionSmokeCommand(options.bundle === "appimage" ? appimageRuntimeArtifact : "imgconvert")
    : ":";
  const aptMirror = dockerAptMirrorCommand();
  if (options.bundle === "deb") {
    return [
      "set -e",
      aptMirror,
      `${aptGet} update`,
      `DEBIAN_FRONTEND=noninteractive ${aptGet} install -y --no-install-recommends xvfb`,
      `DEBIAN_FRONTEND=noninteractive ${aptGet} install -y --no-install-recommends ${quoteShell(artifact)}`,
      launch,
      convertSmoke,
    ].join("; ");
  }
  if (options.bundle === "rpm") {
    return [
      "set -e",
      "dnf -y --setopt=install_weak_deps=False install xorg-x11-server-Xvfb",
      `dnf -y --setopt=install_weak_deps=False install ${quoteShell(artifact)}`,
      launch,
      convertSmoke,
    ].join("; ");
  }
  return [
    "set -e",
    aptMirror,
    `${aptGet} update`,
    `DEBIAN_FRONTEND=noninteractive ${aptGet} install -y --no-install-recommends xvfb libgtk-3-0 libwebkit2gtk-4.1-0 libayatana-appindicator3-1`,
    `DEBIAN_FRONTEND=noninteractive ${aptGet} install -y --no-install-recommends libasound2t64 || DEBIAN_FRONTEND=noninteractive ${aptGet} install -y --no-install-recommends libasound2`,
    `cp ${quoteShell(artifact)} ${quoteShell(appimageRuntimeArtifact)}`,
    `chmod +x ${quoteShell(appimageRuntimeArtifact)}`,
    launch,
    convertSmoke,
  ].join("; ");
}

function dockerAptMirrorCommand() {
  const mirror = process.env.IMGCONVERT_DOCKER_APT_MIRROR?.trim();
  if (!mirror) {
    return ":";
  }
  validateAptMirror(mirror);
  const mirrorReplacement = mirror
    .replaceAll("\\", "\\\\")
    .replaceAll("&", "\\&")
    .replaceAll("|", "\\|")
    .replaceAll('"', '\\"')
    .replaceAll("$", "\\$")
    .replaceAll("`", "\\`");
  return [
    "for source_file in /etc/apt/sources.list /etc/apt/sources.list.d/*.sources; do",
    '  [ -e "$source_file" ] || continue;',
    "  sed -i",
    `    -e "s|http://ports.ubuntu.com/ubuntu-ports|${mirrorReplacement}|g"`,
    `    -e "s|http://archive.ubuntu.com/ubuntu|${mirrorReplacement}|g"`,
    `    -e "s|http://security.ubuntu.com/ubuntu|${mirrorReplacement}|g"`,
    '    "$source_file";',
    "done",
  ].join(" ");
}

function validateAptMirror(mirror) {
  let url;
  try {
    url = new URL(mirror);
  } catch {
    fail(`invalid IMGCONVERT_DOCKER_APT_MIRROR: ${mirror}`);
  }
  if (!["http:", "https:"].includes(url.protocol)) {
    fail("IMGCONVERT_DOCKER_APT_MIRROR must use http or https");
  }
  if (/[\s"'`$\\|&;]/.test(mirror)) {
    fail("IMGCONVERT_DOCKER_APT_MIRROR must not contain shell metacharacters or whitespace");
  }
}

function runLaunchSmoke(command) {
  const result = spawnSync("sh", ["-lc", launchCommand(command)], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    process.stdout.write(result.stdout);
    process.stderr.write(result.stderr);
    fail(`launch smoke failed with exit code ${result.status}`);
  }
}

function runConversionSmoke(command) {
  const result = spawnSync("sh", ["-lc", conversionSmokeCommand(command)], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    process.stdout.write(result.stdout);
    process.stderr.write(result.stderr);
    fail(`conversion smoke failed with exit code ${result.status}`);
  }
}

function conversionSmokeCommand(command) {
  const quoted = command.includes("/") ? quoteShell(command) : command;
  const env = [
    "IMGCONVERT_DISABLE_EXTERNAL_CODECS=1",
    "IMGCONVERT_PACKAGE_CONVERT_SMOKE=1",
    "IMGCONVERT_PACKAGE_CONVERT_SMOKE_FORMATS=jpeg,webp,png,avif",
    ...(options.bundle === "appimage" ? ["APPIMAGE_EXTRACT_AND_RUN=1"] : []),
  ].join(" ");
  return `env ${env} ${quoted}`;
}

function launchCommand(command) {
  const quoted = command.includes("/") ? quoteShell(command) : command;
  const env = [
    "IMGCONVERT_DISABLE_EXTERNAL_CODECS=1",
    "WEBKIT_DISABLE_COMPOSITING_MODE=1",
    ...(options.bundle === "appimage" ? ["APPIMAGE_EXTRACT_AND_RUN=1"] : []),
  ].join(" ");
  return [
    "if command -v xvfb-run >/dev/null 2>&1 && command -v xauth >/dev/null 2>&1; then",
    "  set +e",
    `  timeout ${options.timeoutSeconds}s xvfb-run -a env ${env} ${quoted}`,
    "  code=$?",
    "  set -e",
    "else",
    '  if ! command -v Xvfb >/dev/null 2>&1; then echo "xvfb-run or Xvfb is required for package launch smoke" >&2; exit 127; fi',
    '  display=":$((90 + ($$ % 100)))"',
    '  xvfb_log="/tmp/imgconvert-xvfb-$$.log"',
    '  Xvfb "$display" -screen 0 1280x720x24 -nolisten tcp >"$xvfb_log" 2>&1 &',
    "  xvfb_pid=$!",
    '  trap \'kill "$xvfb_pid" >/dev/null 2>&1 || true; rm -f "$xvfb_log"\' EXIT INT TERM',
    "  sleep 1",
    '  if ! kill -0 "$xvfb_pid" >/dev/null 2>&1; then cat "$xvfb_log" >&2; exit 127; fi',
    "  set +e",
    `  timeout ${options.timeoutSeconds}s env DISPLAY="$display" ${env} ${quoted}`,
    "  code=$?",
    "  set -e",
    '  kill "$xvfb_pid" >/dev/null 2>&1 || true',
    "  trap - EXIT INT TERM",
    '  rm -f "$xvfb_log"',
    "fi",
    'if [ "$code" = "0" ] || [ "$code" = "124" ]; then exit 0; fi',
    'echo "launch exited with $code" >&2',
    "exit $code",
  ].join("\n");
}

function collectFiles(dir) {
  if (!existsSync(dir)) {
    return [];
  }
  const files = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const entryPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectFiles(entryPath));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }
  return files;
}

function commandExists(command) {
  const result = spawnSync("sh", ["-c", `command -v ${quoteShell(command)} >/dev/null 2>&1`], {
    stdio: "ignore",
  });
  return result.status === 0;
}

function resolveDockerCommand() {
  if (!commandExists("docker")) {
    fail("docker is required for --image smoke mode");
  }
  if (commandSucceeds("docker", ["info"])) {
    return { command: "docker", prefixArgs: [] };
  }
  if (commandExists("sudo") && commandSucceeds("sudo", ["-n", "docker", "info"])) {
    return { command: "sudo", prefixArgs: ["-n", "docker"] };
  }
  fail("docker daemon is not accessible; start Docker or run this smoke with Docker permissions");
}

function commandSucceeds(command, args) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    stdio: "ignore",
  });
  return result.status === 0;
}

function run(command, args, label = `${command} ${args.join(" ")}`) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    stdio: "inherit",
  });
  if (result.status !== 0) {
    fail(`${label} failed with exit code ${result.status}`);
  }
}

function quoteShell(value) {
  return `'${value.replaceAll("'", "'\\''")}'`;
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
