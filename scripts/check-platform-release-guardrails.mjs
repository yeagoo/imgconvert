// SPDX-License-Identifier: Apache-2.0

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const options = {
  platforms: ["macos", "windows"],
  channels: ["direct", "store"],
  requireStoreEnv: false,
};

for (const arg of process.argv.slice(2)) {
  if (arg === "--") {
    continue;
  } else if (arg.startsWith("--platform=")) {
    const value = arg.slice("--platform=".length).toLowerCase();
    options.platforms =
      value === "all" ? ["macos", "windows"] : splitOption(value, ["macos", "windows"]);
  } else if (arg.startsWith("--channel=")) {
    const value = arg.slice("--channel=".length).toLowerCase();
    options.channels =
      value === "all" ? ["direct", "store"] : splitOption(value, ["direct", "store"]);
  } else if (arg === "--require-store-env") {
    options.requireStoreEnv = true;
  } else {
    fail(`unknown argument: ${arg}`);
  }
}

const packageJson = readJson(path.join(repoRoot, "package.json"));
const tauriConfigPath = path.join(repoRoot, "src-tauri", "tauri.conf.json");
const tauriConfig = readJson(tauriConfigPath);
const srcTauriRoot = path.join(repoRoot, "src-tauri");
const failures = [];

checkCommonBundleMetadata();

if (options.platforms.includes("macos")) {
  checkMacos();
}
if (options.platforms.includes("windows")) {
  checkWindows();
}
if (options.channels.includes("store")) {
  checkStoreCodecGuardrail();
}

if (failures.length > 0) {
  console.error("platform release guardrail check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `platform release guardrail check passed (${options.platforms.join(", ")}; ${options.channels.join(", ")}).`,
);

function checkCommonBundleMetadata() {
  if (tauriConfig.version !== packageJson.version) {
    failures.push(
      `tauri.conf.json version ${tauriConfig.version ?? "<missing>"} does not match package.json ${packageJson.version}`,
    );
  }
  if (!tauriConfig.productName?.trim()) {
    failures.push("tauri.conf.json productName is required");
  }
  if (!/^([a-zA-Z0-9-]+\.)+[a-zA-Z0-9-]+$/.test(tauriConfig.identifier ?? "")) {
    failures.push("tauri.conf.json identifier must be a reverse-DNS id");
  }
  if (tauriConfig.bundle?.active !== true) {
    failures.push("tauri.conf.json bundle.active must be true for platform releases");
  }
  if (tauriConfig.bundle?.license !== "Apache-2.0") {
    failures.push("tauri.conf.json bundle.license must stay Apache-2.0");
  }

  const licenseFile = tauriConfig.bundle?.licenseFile;
  if (!licenseFile) {
    failures.push("tauri.conf.json bundle.licenseFile is required");
  } else if (!existsSync(path.resolve(repoRoot, "src-tauri", licenseFile))) {
    failures.push(`bundle.licenseFile does not exist: ${licenseFile}`);
  }

  if (!Array.isArray(tauriConfig.bundle?.icon) || tauriConfig.bundle.icon.length === 0) {
    failures.push("tauri.conf.json bundle.icon must include platform icons");
  }
}

function checkMacos() {
  requireBundleIcon(".icns", "macOS");
  checkMacosRuntimeGuardrails();
  const directSelected = options.channels.includes("direct");
  const storeSelected = options.channels.includes("store");
  if (directSelected && tauriConfig.bundle?.targets === undefined) {
    failures.push("macOS direct distribution needs bundle targets configured");
  }
  if (directSelected) {
    checkMacosDirectConfig();
  }
  if (storeSelected && !storeCodecGuardrailStaticFilesPresent()) {
    failures.push("macOS store channel requires build-time external codec disable guardrails");
  }
  if (storeSelected) {
    checkMacosStoreConfig();
  }
}

function checkMacosRuntimeGuardrails() {
  const packageScripts = packageJson.scripts ?? {};
  if (!packageScripts["bench:avif:macos"]?.includes("benchmark-macos-avif.mjs")) {
    failures.push("package.json must expose bench:avif:macos for Apple Silicon AVIF timing");
  }
  if (!packageScripts["release:macos:smoke"]?.includes("smoke-macos-runtime.mjs")) {
    failures.push(
      "package.json must expose release:macos:smoke for real-machine runtime acceptance",
    );
  }

  const systemCodecs = readText(path.join(repoRoot, "src-tauri", "src", "macos_system_codecs.rs"));
  if (!systemCodecs.includes("ImageIO.framework")) {
    failures.push("macOS system codec bridge must use ImageIO.framework");
  }
  if (!systemCodecs.includes("system-imageio")) {
    failures.push("macOS ImageIO HEIC provider kind must stay system-imageio");
  }
  if (!systemCodecs.includes("writable: Vec::new()")) {
    failures.push(
      "macOS ImageIO HEIC provider must remain read-only until HEIC encoding is audited",
    );
  }
  if (!systemCodecs.includes("CGImageSourceCreateWithURL")) {
    failures.push(
      "macOS ImageIO HEIC bridge must use file URL decode instead of buffering full input",
    );
  }

  const security = readText(path.join(repoRoot, "src-tauri", "src", "macos_security.rs"));
  for (const expected of [
    "CFURLStartAccessingSecurityScopedResource",
    "CFURLStopAccessingSecurityScopedResource",
  ]) {
    if (!security.includes(expected)) {
      failures.push(`macOS security scope shim must call ${expected}`);
    }
  }

  const readme = readText(path.join(repoRoot, "packaging", "macos", "README.md"));
  for (const expected of [
    "ImageIO",
    "security-scoped",
    "notarytool",
    "bench:avif:macos",
    "release:macos:smoke",
  ]) {
    if (!readme.includes(expected)) {
      failures.push(`packaging/macos/README.md must document ${expected}`);
    }
  }
}

function checkWindows() {
  requireBundleIcon(".ico", "Windows");
  checkWindowsRuntimeGuardrails();
  const directSelected = options.channels.includes("direct");
  const storeSelected = options.channels.includes("store");
  if (directSelected) {
    checkWindowsDirectConfig();
  }
  if (storeSelected && !storeCodecGuardrailStaticFilesPresent()) {
    failures.push("Windows store channel requires build-time external codec disable guardrails");
  }
  if (storeSelected) {
    checkWindowsStoreDocs();
  }
}

function checkWindowsRuntimeGuardrails() {
  const packageScripts = packageJson.scripts ?? {};
  if (!packageScripts["release:windows:smoke"]?.includes("smoke-windows-runtime.mjs")) {
    failures.push("package.json must expose release:windows:smoke for Windows runtime acceptance");
  }
  if (!packageScripts["release:windows"]?.includes("check-windows-bundle-artifacts.mjs")) {
    failures.push("package.json must expose release:windows with installer artifact verification");
  }
  for (const script of [
    "scripts/smoke-windows-runtime.mjs",
    "scripts/clean-windows-bundles.mjs",
    "scripts/check-windows-bundle-artifacts.mjs",
  ]) {
    if (!existsSync(path.join(repoRoot, script))) {
      failures.push(`${script} is required for Windows direct runtime/build smoke`);
    }
  }
}

function checkStoreCodecGuardrail() {
  const buildRs = readText(path.join(repoRoot, "src-tauri", "build.rs"));
  const externalCodecs = readText(path.join(repoRoot, "src-tauri", "src", "external_codecs.rs"));

  if (!buildRs.includes("rerun-if-env-changed=IMGCONVERT_DISABLE_EXTERNAL_CODECS")) {
    failures.push("src-tauri/build.rs must rerun when IMGCONVERT_DISABLE_EXTERNAL_CODECS changes");
  }
  if (!externalCodecs.includes('option_env!("IMGCONVERT_DISABLE_EXTERNAL_CODECS")')) {
    failures.push(
      "external_codecs.rs must read IMGCONVERT_DISABLE_EXTERNAL_CODECS at compile time",
    );
  }
  if (options.requireStoreEnv && !truthy(process.env.IMGCONVERT_DISABLE_EXTERNAL_CODECS)) {
    failures.push(
      "store release builds must set IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 so external codec/helper discovery is compiled off",
    );
  }
}

function checkMacosDirectConfig() {
  const configPath = path.join(srcTauriRoot, "tauri.macos.conf.json");
  const config = readJson(configPath);
  const macos = config.bundle?.macOS;
  if (!macos) {
    failures.push("src-tauri/tauri.macos.conf.json must define bundle.macOS");
    return;
  }
  if (macos.hardenedRuntime !== true) {
    failures.push("macOS direct config must enable hardenedRuntime");
  }
  if (macos.entitlements !== "entitlements.macos.direct.plist") {
    failures.push("macOS direct config must use entitlements.macos.direct.plist");
  }
  const entitlements = readEntitlements(macos.entitlements);
  if (!entitlements) {
    return;
  }
  if (entitlements.get("com.apple.security.app-sandbox") === true) {
    failures.push("macOS direct entitlements must not enable App Sandbox");
  }
  checkForbiddenMacosEntitlements(entitlements, "macOS direct entitlements");
}

function checkMacosStoreConfig() {
  const configPath = path.join(srcTauriRoot, "tauri.macos.mas.conf.json");
  const config = readJson(configPath);
  const macos = config.bundle?.macOS;
  if (!macos) {
    failures.push("src-tauri/tauri.macos.mas.conf.json must define bundle.macOS");
    return;
  }
  if (macos.hardenedRuntime !== true) {
    failures.push("macOS MAS config must enable hardenedRuntime");
  }
  if (macos.entitlements !== "entitlements.macos.mas.plist") {
    failures.push("macOS MAS config must use entitlements.macos.mas.plist");
  }
  const entitlements = readEntitlements(macos.entitlements);
  if (!entitlements) {
    return;
  }
  requireEntitlement(entitlements, "com.apple.security.app-sandbox", true, "macOS MAS");
  requireEntitlement(
    entitlements,
    "com.apple.security.files.user-selected.read-write",
    true,
    "macOS MAS",
  );
  requireEntitlement(
    entitlements,
    "com.apple.security.files.bookmarks.app-scope",
    true,
    "macOS MAS",
  );
  checkForbiddenMacosEntitlements(entitlements, "macOS MAS entitlements");
}

function checkWindowsDirectConfig() {
  const configPath = path.join(srcTauriRoot, "tauri.windows.conf.json");
  const config = readJson(configPath);
  const windows = config.bundle?.windows;
  if (!windows) {
    failures.push("src-tauri/tauri.windows.conf.json must define bundle.windows");
    return;
  }
  if (windows.allowDowngrades !== false) {
    failures.push("Windows direct config must set allowDowngrades=false");
  }
  if ((windows.digestAlgorithm ?? "").toLowerCase() !== "sha256") {
    failures.push("Windows direct config must set digestAlgorithm=sha256");
  }

  const webviewMode = windows.webviewInstallMode;
  if (!webviewMode || typeof webviewMode !== "object") {
    failures.push("Windows direct config must set an explicit webviewInstallMode object");
  } else {
    if (webviewMode.type !== "embedBootstrapper") {
      failures.push("Windows direct config must use WebView2 embedBootstrapper");
    }
    if (webviewMode.silent !== true) {
      failures.push("Windows direct WebView2 bootstrapper must run silent");
    }
  }

  if (!/^\d+\.\d+\.\d+\.\d+$/.test(windows.minimumWebview2Version ?? "")) {
    failures.push("Windows direct config must set a four-part minimumWebview2Version");
  }
  if (!isUuid(windows.wix?.upgradeCode)) {
    failures.push("Windows direct WiX config must pin a stable upgradeCode UUID");
  }
  if (windows.nsis?.installMode !== "currentUser") {
    failures.push("Windows direct NSIS config must default to currentUser installMode");
  }
}

function checkWindowsStoreDocs() {
  const readme = readText(path.join(repoRoot, "packaging", "windows", "README.md"));
  if (!readme) {
    failures.push("Windows store channel requires packaging/windows/README.md");
    return;
  }
  for (const expected of [
    "MSIX",
    "runFullTrust",
    "IMGCONVERT_DISABLE_EXTERNAL_CODECS=1",
    "release:windows:store:check",
  ]) {
    if (!readme.includes(expected)) {
      failures.push(`packaging/windows/README.md must document ${expected}`);
    }
  }
}

function storeCodecGuardrailStaticFilesPresent() {
  const buildRs = readText(path.join(repoRoot, "src-tauri", "build.rs"));
  const externalCodecs = readText(path.join(repoRoot, "src-tauri", "src", "external_codecs.rs"));
  return (
    buildRs.includes("rerun-if-env-changed=IMGCONVERT_DISABLE_EXTERNAL_CODECS") &&
    externalCodecs.includes('option_env!("IMGCONVERT_DISABLE_EXTERNAL_CODECS")')
  );
}

function requireBundleIcon(extension, platform) {
  const icons = tauriConfig.bundle?.icon ?? [];
  const icon = icons.find((item) => item.endsWith(extension));
  if (!icon) {
    failures.push(`tauri.conf.json bundle.icon must include a ${platform} ${extension} icon`);
    return;
  }
  if (!existsSync(path.resolve(repoRoot, "src-tauri", icon))) {
    failures.push(`${platform} icon does not exist: ${icon}`);
  }
}

function readEntitlements(fileName) {
  if (!fileName) {
    failures.push("macOS entitlements path is required");
    return null;
  }
  if (path.isAbsolute(fileName) || fileName.includes("..")) {
    failures.push(`macOS entitlements path must be relative to src-tauri: ${fileName}`);
    return null;
  }
  const file = path.join(srcTauriRoot, fileName);
  const text = readText(file);
  if (!text) {
    failures.push(`macOS entitlements file does not exist or is empty: ${fileName}`);
    return null;
  }
  if (!text.includes("<plist") || !text.includes("<dict>")) {
    failures.push(`macOS entitlements file must be a plist dict: ${fileName}`);
    return null;
  }
  return parseBooleanPlist(text);
}

function parseBooleanPlist(text) {
  const values = new Map();
  const pattern = /<key>([^<]+)<\/key>\s*<(true|false)\/>/g;
  for (const match of text.matchAll(pattern)) {
    values.set(match[1], match[2] === "true");
  }
  return values;
}

function requireEntitlement(values, key, expected, label) {
  if (values.get(key) !== expected) {
    failures.push(`${label} entitlements must set ${key}=${expected}`);
  }
}

function checkForbiddenMacosEntitlements(values, label) {
  for (const key of [
    "com.apple.security.network.server",
    "com.apple.security.files.all",
    "com.apple.security.temporary-exception.files.absolute-path.read-only",
    "com.apple.security.temporary-exception.files.absolute-path.read-write",
    "com.apple.security.temporary-exception.mach-lookup.global-name",
  ]) {
    if (values.has(key)) {
      failures.push(`${label} must not include broad temporary entitlement ${key}`);
    }
  }
}

function splitOption(value, allowed) {
  const items = value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
  for (const item of items) {
    if (!allowed.includes(item)) {
      fail(`unsupported option value: ${item}`);
    }
  }
  if (items.length === 0) {
    fail("option value must not be empty");
  }
  return [...new Set(items)];
}

function readJson(file) {
  try {
    return JSON.parse(readFileSync(file, "utf8"));
  } catch (error) {
    fail(`failed to read ${path.relative(repoRoot, file)}: ${error.message}`);
  }
}

function readText(file) {
  try {
    return readFileSync(file, "utf8");
  } catch {
    return "";
  }
}

function truthy(value) {
  return /^(1|true|yes|on)$/i.test(value ?? "");
}

function isUuid(value) {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value ?? "");
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
