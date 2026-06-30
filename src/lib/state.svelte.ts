// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 ImgConvert contributors

import {
  Channel,
  invoke,
  isTauri as tauriIsTauri,
} from "@tauri-apps/api/core";
import { confirm as confirmDialog } from "@tauri-apps/plugin-dialog";
import { load, type Store } from "@tauri-apps/plugin-store";

// ---- 类型 ----
export type ItemStatus = "pending" | "running" | "done" | "skipped" | "error";
export interface QueueItem {
  path: string;
  key: string;
  name: string;
  status: ItemStatus;
  detail: string;
  targetFormat: string | null;
  metadata: ImageMetadata | null;
  progress?: number;
  preview?: boolean;
}
export interface Capabilities {
  readable: string[];
  writable: string[];
  lossless: string[];
  heic: boolean;
}
export interface ConvertResult {
  input: string;
  output: string;
  inSize: number;
  outSize: number;
}
export interface ImportScanError {
  path: string;
  message: string;
}
export interface ImportScanFile {
  path: string;
  key: string;
  metadata: ImageMetadata | null;
}
export interface ImageMetadata {
  format: string;
  width: number;
  height: number;
  dpiX: number | null;
  dpiY: number | null;
}
export interface ImportScanResult {
  files: ImportScanFile[];
  skipped: number;
  errors: ImportScanError[];
  truncated: boolean;
  cancelled: boolean;
  limitReason: string | null;
}
export interface BatchSummary {
  total: number;
  completed: number;
  skipped: number;
  failed: number;
  cancelled: boolean;
}
type ConvertRequest = {
  input: string;
  format: string;
  quality: number;
  lossless: boolean;
  overwrite: boolean;
  overwriteMode: OverwriteMode;
  outDir: string | null;
  fileNameTemplate: string;
  preserveMetadata: boolean;
};
type BatchConvertRequest = {
  options: ConvertRequest[];
  concurrency: number | null;
};
type BatchProgressEvent =
  | { event: "started"; data: { total: number } }
  | { event: "fileStarted"; data: { index: number; input: string } }
  | {
      event: "fileProgress";
      data: { index: number; percent: number; stage: string };
    }
  | { event: "fileFinished"; data: { index: number; result: ConvertResult } }
  | {
      event: "fileSkipped";
      data: { index: number; input: string; message: string };
    }
  | {
      event: "fileError";
      data: { index: number; input: string; message: string };
    }
  | { event: "cancelled"; data: { completed: number; total: number } }
  | { event: "finished"; data: { summary: BatchSummary } };
type BatchJob = {
  item: QueueItem;
  options: ConvertRequest;
};
export type Theme = "light" | "dark" | "system";
export type OverwriteMode = "ask" | "skip" | "overwrite";

export interface Settings {
  format: string;
  quality: number;
  lossless: boolean;
  overwrite: OverwriteMode;
  outDir: string | null;
  fileNameTemplate: string;
  preserveMetadata: boolean;
  concurrency: number;
  theme: Theme;
  reduceMotion: boolean;
}

// ---- 常量 ----
const CORE_CAPABILITIES: Capabilities = {
  readable: ["jpeg", "png", "webp", "avif"],
  writable: ["jpeg", "png", "webp", "avif"],
  lossless: ["png", "webp"],
  heic: false,
};

const FORMAT_EXTENSIONS: Record<string, string[]> = {
  jpeg: ["jpg", "jpeg"],
  png: ["png"],
  webp: ["webp"],
  avif: ["avif"],
};

export const FORMAT_CATEGORIES = [
  { value: "modern", label: "现代格式" },
  { value: "standard", label: "通用格式" },
];

export const FORMATS: {
  value: string;
  label: string;
  category: string;
  description: string;
  note?: string;
}[] = [
  {
    value: "avif",
    label: "AVIF",
    category: "modern",
    description: "高压缩率,适合网页与归档",
  },
  {
    value: "webp",
    label: "WebP",
    category: "modern",
    description: "体积小,兼容现代浏览器",
  },
  {
    value: "jpeg",
    label: "JPEG",
    category: "standard",
    description: "照片通用,兼容性最好",
  },
  {
    value: "png",
    label: "PNG",
    category: "standard",
    description: "无损图形与透明背景",
  },
];

export const IMAGE_EXTS = Object.values(FORMAT_EXTENSIONS).flat();

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && tauriIsTauri();
}

// ---- 状态(runes)----
export const settings = $state<Settings>({
  format: "avif",
  quality: 80,
  lossless: false,
  overwrite: "skip",
  outDir: null,
  fileNameTemplate: "%name%",
  preserveMetadata: false,
  concurrency: 0,
  theme: "system",
  reduceMotion: false,
});

export const queue = $state<QueueItem[]>([]);

interface UiState {
  converting: boolean;
  cancelRequested: boolean;
  dragActive: boolean;
  importing: boolean;
  importCancelRequested: boolean;
  importMessage: string;
  importErrors: ImportScanError[];
}

export const ui = $state<UiState>({
  converting: false,
  cancelRequested: false,
  dragActive: false,
  importing: false,
  importCancelRequested: false,
  importMessage: "",
  importErrors: [],
});

export const engine = $state<{ text: string; ok: boolean }>({
  text: "检测 core 能力中…",
  ok: false,
});

export const capabilities = $state<Capabilities>({
  readable: [...CORE_CAPABILITIES.readable],
  writable: [...CORE_CAPABILITIES.writable],
  lossless: [...CORE_CAPABILITIES.lossless],
  heic: CORE_CAPABILITIES.heic,
});

// ---- 派生 ----
export function supportsLossless(format: string): boolean {
  return capabilities.lossless.includes(format.toLowerCase());
}

export function formatLabel(format: string): string {
  return FORMATS.find((f) => f.value === format)?.label ?? format.toUpperCase();
}

export function writableFormats() {
  const writable = new Set(capabilities.writable);
  return FORMATS.filter((format) => writable.has(format.value));
}

export function readableExtensions(): string[] {
  const exts = capabilities.readable.flatMap((format) => FORMAT_EXTENSIONS[format] ?? []);
  return Array.from(new Set(exts));
}

export function formatFromExt(ext: string): string | null {
  const normalized = ext.toLowerCase();
  for (const [format, exts] of Object.entries(FORMAT_EXTENSIONS)) {
    if (exts.includes(normalized)) return format;
  }
  return null;
}

export function itemTargetFormat(item: QueueItem): string {
  return item.targetFormat ?? settings.format;
}

export function itemProgress(item: QueueItem): number {
  if (item.status === "done") return 100;
  if (item.status === "skipped") return 100;
  if (item.status === "running") return item.progress ?? 55;
  if (item.status === "error") return 100;
  return item.progress ?? 0;
}

export function formatAccent(format: string | null): {
  text: string;
  border: string;
  background: string;
} {
  switch (format) {
    case "avif":
      return {
        text: "text-sky-700 dark:text-sky-300",
        border: "border-sky-500/35",
        background: "bg-sky-500/10",
      };
    case "webp":
      return {
        text: "text-emerald-700 dark:text-emerald-300",
        border: "border-emerald-500/35",
        background: "bg-emerald-500/10",
      };
    case "jpeg":
      return {
        text: "text-amber-700 dark:text-amber-300",
        border: "border-amber-500/35",
        background: "bg-amber-500/10",
      };
    case "png":
      return {
        text: "text-indigo-700 dark:text-indigo-300",
        border: "border-indigo-500/35",
        background: "bg-indigo-500/10",
      };
    default:
      return {
        text: "text-primary",
        border: "border-primary/25",
        background: "bg-primary/10",
      };
  }
}

// ---- 工具 ----
export const extOf = (p: string) =>
  p.toLowerCase().match(/\.([a-z0-9]+)$/)?.[1] ?? "";
export const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;
export function fmtSize(b: number): string {
  if (b <= 0) return "0 B";
  const u = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(b) / Math.log(1024));
  return `${(b / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${u[i]}`;
}

export function formatImageMetadata(metadata: ImageMetadata | null): string {
  if (!metadata) return "";
  const parts = [`${metadata.width}×${metadata.height}`];
  const dpi = formatDpi(metadata);
  if (dpi) parts.push(dpi);
  return parts.join(" · ");
}

function formatDpi(metadata: ImageMetadata): string {
  const x = metadata.dpiX;
  const y = metadata.dpiY;
  if (!x || !y) return "";
  const roundedX = Math.round(x);
  const roundedY = Math.round(y);
  return roundedX === roundedY ? `${roundedX} DPI` : `${roundedX}×${roundedY} DPI`;
}

export interface AddPathsResult {
  added: number;
  duplicates: number;
  skipped: number;
}

type AddPathInput = string | ImportScanFile;

// ---- 队列操作(就地变更,保持响应式)----
export function addPaths(paths: AddPathInput[]): AddPathsResult {
  const result: AddPathsResult = { added: 0, duplicates: 0, skipped: 0 };
  if (ui.converting) return result;

  const readable = readableExtensions();
  const existingKeys = new Set(queue.map((item) => item.key));
  const existingPaths = new Set(queue.map((item) => item.path));

  for (const input of paths) {
    const candidate = normalizeAddPathInput(input);
    if (!readable.includes(extOf(candidate.path))) {
      result.skipped += 1;
      continue;
    }
    if (existingKeys.has(candidate.key) || existingPaths.has(candidate.path)) {
      result.duplicates += 1;
      continue;
    }
    queue.push({
      path: candidate.path,
      key: candidate.key,
      name: baseName(candidate.path),
      status: "pending",
      detail: "",
      targetFormat: null,
      metadata: candidate.metadata ?? null,
    });
    existingKeys.add(candidate.key);
    existingPaths.add(candidate.path);
    result.added += 1;
  }
  return result;
}

function normalizeAddPathInput(input: AddPathInput): ImportScanFile {
  if (typeof input === "string") return { path: input, key: input, metadata: null };
  return {
    path: input.path,
    key: input.key || input.path,
    metadata: input.metadata ?? null,
  };
}

export async function importPaths(paths: string[]) {
  if (ui.converting || ui.importing || paths.length === 0) return;

  ui.importing = true;
  ui.importCancelRequested = false;
  ui.importMessage = "正在扫描导入…";
  ui.importErrors = [];
  try {
    if (!isTauriRuntime()) {
      const added = addPaths(paths);
      ui.importMessage = formatImportSummary(added, null);
      return;
    }

    const scan = await invoke<ImportScanResult>("scan_import_paths", {
      options: {
        paths,
        recursive: true,
      },
    });
    ui.importErrors = scan.errors;
    if (scan.cancelled) {
      ui.importMessage = "已取消导入扫描";
      return;
    }

    const added = addPaths(scan.files);
    ui.importMessage = formatImportSummary(added, scan);
  } catch (e) {
    ui.importMessage = `导入失败:${e}`;
    ui.importErrors = [];
  } finally {
    ui.importing = false;
    ui.importCancelRequested = false;
  }
}

export async function cancelImportScan() {
  if (!ui.importing || ui.importCancelRequested) return;
  ui.importCancelRequested = true;

  if (!isTauriRuntime()) {
    ui.importing = false;
    ui.importMessage = "已取消导入扫描";
    return;
  }

  try {
    await invoke<boolean>("cancel_import_scan");
  } catch (e) {
    ui.importMessage = `取消导入扫描失败:${e}`;
  }
}

function formatImportSummary(added: AddPathsResult, scan: ImportScanResult | null): string {
  const skipped = (scan?.skipped ?? 0) + added.skipped;
  const scanErrors = scan?.errors.length ?? 0;
  const parts: string[] = [];
  if (added.added > 0) parts.push(`已添加 ${added.added} 个文件`);
  if (added.duplicates > 0) parts.push(`${added.duplicates} 个重复`);
  if (skipped > 0) parts.push(`跳过 ${skipped} 个`);
  if (scanErrors > 0) parts.push(`${scanErrors} 个错误`);
  if (scan?.truncated) parts.push(scan.limitReason ?? "扫描已截断");
  return parts.join(" · ") || "未找到支持的图片";
}

export function addDemoItems() {
  if (queue.length > 0) return;
  const demo = [
    "/preview/landing-hero.png",
    "/preview/product-photo.jpg",
    "/preview/icon-set.webp",
    "/preview/archive-sample.avif",
  ];
  for (const path of demo) {
    queue.push({
      path,
      key: path,
      name: baseName(path),
      status: "pending",
      detail: "网页预览示例",
      targetFormat: null,
      metadata: null,
      preview: true,
    });
  }
}

export function setItemTargetFormat(path: string, format: string | null) {
  if (ui.converting || ui.importing) return;

  const item = queue.find((it) => it.path === path);
  if (!item) return;
  const normalized = format?.toLowerCase() ?? null;
  if (normalized && !capabilities.writable.includes(normalized)) return;
  item.targetFormat = normalized;
  if (item.status !== "running") {
    item.status = "pending";
    item.detail = item.preview ? "网页预览示例" : "";
    item.progress = 0;
  }
}

export function resetItemFormats() {
  if (ui.converting || ui.importing) return;

  for (const item of queue) {
    item.targetFormat = null;
    if (item.status !== "running") {
      item.status = "pending";
      item.detail = item.preview ? "网页预览示例" : "";
      item.progress = 0;
    }
  }
}
export function removeItem(path: string) {
  if (ui.converting || ui.importing) return;
  const i = queue.findIndex((it) => it.path === path);
  if (i >= 0) queue.splice(i, 1);
}
export function clearQueue() {
  if (ui.converting || ui.importing) return;
  queue.splice(0, queue.length);
}

// ---- 转换 ----
export async function convertAll() {
  if (ui.converting || ui.importing || queue.length === 0) {
    return;
  }

  if (!isTauriRuntime()) {
    for (const item of queue) {
      if (item.status !== "done") {
        item.status = "error";
        item.detail = "网页预览不执行本地文件转换,请在 Tauri 桌面端运行";
        item.progress = 100;
      }
    }
    return;
  }

  ui.converting = true;
  ui.cancelRequested = false;
  ui.dragActive = false;
  try {
    if (settings.overwrite === "ask") {
      await convertAllWithAskPolicy();
    } else {
      await convertAllWithBatch();
    }
  } finally {
    ui.converting = false;
    ui.cancelRequested = false;
  }
}

export async function cancelConversion() {
  if (!ui.converting || ui.cancelRequested) return;
  ui.cancelRequested = true;
  if (isTauriRuntime()) {
    try {
      await invoke<boolean>("cancel_batch");
    } catch (e) {
      console.warn("取消批量任务失败:", e);
    }
  }
}

async function convertAllWithBatch() {
  const jobs = prepareBatchJobs();
  if (jobs.length === 0) return;

  for (const job of jobs) {
    job.item.status = "pending";
    job.item.detail = "排队中";
    job.item.progress = 0;
  }

  const progress = new Channel<BatchProgressEvent>((event) => {
    handleBatchProgress(event, jobs);
  });

  try {
    const request: BatchConvertRequest = {
      options: jobs.map((job) => job.options),
      concurrency: batchConcurrency(),
    };
    const summary = await invoke<BatchSummary>("convert_batch", {
      ...request,
      progress,
    });
    if (summary.cancelled) {
      finalizeCancelledJobs(jobs);
    }
  } catch (e) {
    const msg = String(e);
    for (const job of jobs) {
      if (job.item.status === "pending" || job.item.status === "running") {
        job.item.status = "error";
        job.item.detail = `批量任务失败:${msg}`;
        job.item.progress = 100;
      }
    }
  }
}

function batchConcurrency(): number | null {
  const concurrency = Math.round(settings.concurrency);
  return concurrency > 0 ? Math.min(8, concurrency) : null;
}

async function convertAllWithAskPolicy() {
  for (const item of queue) {
    if (ui.cancelRequested) break;
    if (item.status === "done") continue;

    const options = prepareSingleJob(item);
    if (!options) continue;

    item.status = "running";
    item.detail = "";
    item.progress = 5;
    try {
      const res = await convertOneWithPolicy(options);
      if (!res) {
        item.status = "skipped";
        item.detail = "已跳过(用户取消覆盖)";
        item.progress = 100;
        continue;
      }
      item.status = "done";
      item.detail = formatResultDetail(res);
      item.progress = 100;
    } catch (e) {
      const msg = String(e);
      if (settings.overwrite === "skip" && msg.includes("已存在")) {
        item.status = "skipped";
        item.detail = "已跳过(输出已存在)";
      } else {
        item.status = "error";
        item.detail = msg;
      }
      item.progress = 100;
    }
  }

  if (ui.cancelRequested) {
    for (const item of queue) {
      if (item.status === "running") {
        item.status = "pending";
        item.detail = "已取消";
        item.progress = 0;
      }
    }
  }
}

function prepareBatchJobs(): BatchJob[] {
  const jobs: BatchJob[] = [];
  for (const item of queue) {
    if (item.status === "done") continue;
    const options = prepareSingleJob(item);
    if (options) jobs.push({ item, options });
  }
  return jobs;
}

function prepareSingleJob(item: QueueItem): ConvertRequest | null {
  const format = itemTargetFormat(item);
  if (!capabilities.writable.includes(format)) {
    item.status = "error";
    item.detail = `不支持的目标格式:${formatLabel(format)}`;
    item.progress = 100;
    return null;
  }
  return buildConvertRequest(item, format);
}

function buildConvertRequest(item: QueueItem, format: string): ConvertRequest {
  return {
    input: item.path,
    format,
    quality: settings.quality,
    lossless: settings.lossless && supportsLossless(format),
    overwrite: settings.overwrite === "overwrite",
    overwriteMode: settings.overwrite,
    outDir: settings.outDir,
    fileNameTemplate: settings.fileNameTemplate,
    preserveMetadata: false,
  };
}

function handleBatchProgress(event: BatchProgressEvent, jobs: BatchJob[]) {
  switch (event.event) {
    case "started":
      return;
    case "fileStarted": {
      const job = jobs[event.data.index];
      if (!job) return;
      job.item.status = "running";
      job.item.detail = "读取并转换";
      job.item.progress = 5;
      return;
    }
    case "fileProgress": {
      const job = jobs[event.data.index];
      if (!job) return;
      job.item.status = "running";
      job.item.detail = event.data.stage;
      job.item.progress = Math.min(100, Math.max(0, Math.round(event.data.percent)));
      return;
    }
    case "fileFinished": {
      const job = jobs[event.data.index];
      if (!job) return;
      job.item.status = "done";
      job.item.detail = formatResultDetail(event.data.result);
      job.item.progress = 100;
      return;
    }
    case "fileSkipped": {
      const job = jobs[event.data.index];
      if (!job) return;
      job.item.status = "skipped";
      job.item.detail = formatSkipDetail(event.data.message);
      job.item.progress = 100;
      return;
    }
    case "fileError": {
      const job = jobs[event.data.index];
      if (!job) return;
      job.item.status = "error";
      job.item.detail = event.data.message;
      job.item.progress = 100;
      return;
    }
    case "cancelled":
      ui.cancelRequested = true;
      finalizeCancelledJobs(jobs);
      return;
    case "finished":
      if (event.data.summary.cancelled) {
        finalizeCancelledJobs(jobs);
      }
      return;
  }
}

function finalizeCancelledJobs(jobs: BatchJob[]) {
  for (const job of jobs) {
    if (job.item.status === "pending" || job.item.status === "running") {
      job.item.status = "pending";
      job.item.detail = "已取消";
      job.item.progress = 0;
    }
  }
}

function formatResultDetail(res: ConvertResult): string {
  const ratio = res.inSize > 0 ? Math.round((1 - res.outSize / res.inSize) * 100) : 0;
  return `${fmtSize(res.inSize)} → ${fmtSize(res.outSize)} (${ratio >= 0 ? "-" : "+"}${Math.abs(ratio)}%)`;
}

function formatSkipDetail(message: string): string {
  return message.includes("已存在") ? "已跳过(输出已存在)" : `已跳过:${message}`;
}

async function convertOneWithPolicy(options: ConvertRequest): Promise<ConvertResult | null> {
  try {
    return await invoke<ConvertResult>("convert_image", { options });
  } catch (e) {
    const msg = String(e);
    if (options.overwriteMode !== "ask" || !isOutputExistsError(msg)) {
      throw e;
    }

    const confirmed = await confirmDialog(formatOutputExistsMessage(msg), {
      title: "确认覆盖",
      kind: "warning",
      okLabel: "覆盖",
      cancelLabel: "跳过",
    });
    if (!confirmed) return null;

    return invoke<ConvertResult>("convert_image", {
      options: {
        ...options,
        overwrite: true,
        overwriteMode: "overwrite",
      },
    });
  }
}

function isOutputExistsError(message: string): boolean {
  return message.includes("输出已存在");
}

function formatOutputExistsMessage(message: string): string {
  return message.replace(/^输出已存在(?:\(需要确认覆盖\)|\(未开启覆盖\))?:?\s*/, "输出文件已存在:\n");
}

// ---- 引擎检测 ----
export async function checkEngine() {
  if (!isTauriRuntime()) {
    capabilities.readable = [...CORE_CAPABILITIES.readable];
    capabilities.writable = [...CORE_CAPABILITIES.writable];
    capabilities.lossless = [...CORE_CAPABILITIES.lossless];
    capabilities.heic = CORE_CAPABILITIES.heic;
    engine.text = `网页预览 · ${capabilities.writable.map(formatLabel).join(" / ")}`;
    engine.ok = true;
    return;
  }

  try {
    const info = await invoke<Capabilities>("capabilities");
    capabilities.readable = [...info.readable];
    capabilities.writable = [...info.writable];
    capabilities.lossless = [...info.lossless];
    capabilities.heic = info.heic;

    if (!capabilities.writable.includes(settings.format)) {
      settings.format = capabilities.writable[0] ?? CORE_CAPABILITIES.writable[0];
      await persistSettings();
    }

    engine.text = `Core 就绪 · ${capabilities.writable.map(formatLabel).join(" / ")}`;
    engine.ok = capabilities.writable.length > 0;
  } catch (e) {
    engine.text = `Core 能力检测失败:${e}`;
    engine.ok = false;
  }
}

// ---- 主题 ----
export function applyTheme() {
  const dark =
    settings.theme === "dark" ||
    (settings.theme === "system" &&
      window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
  document.documentElement.classList.toggle("reduce-motion", settings.reduceMotion);
}

// ---- 持久化(Tauri Store)----
let store: Store | null = null;
export async function initPersistence() {
  if (!isTauriRuntime()) {
    applyTheme();
    return;
  }

  try {
    store = await load("settings.json", { defaults: {}, autoSave: 300 });
    const saved = await store.get<Partial<Settings>>("settings");
    if (saved) {
      Object.assign(settings, saved);
      normalizeSettings();
    }
  } catch (e) {
    console.warn("加载设置失败,用默认值:", e);
  }
  applyTheme();
}
export async function persistSettings() {
  if (!store) return;
  await store.set("settings", { ...settings });
}

function normalizeSettings() {
  const overwrite = settings.overwrite as unknown;
  if (overwrite !== "ask" && overwrite !== "skip" && overwrite !== "overwrite") {
    settings.overwrite = overwrite === true ? "overwrite" : "skip";
  }

  if (!["light", "dark", "system"].includes(settings.theme)) {
    settings.theme = "system";
  }
  if (typeof settings.format !== "string" || !settings.format.trim()) {
    settings.format = CORE_CAPABILITIES.writable[0];
  }
  if (settings.outDir !== null && typeof settings.outDir !== "string") {
    settings.outDir = null;
  }
  if (typeof settings.fileNameTemplate !== "string" || !settings.fileNameTemplate.trim()) {
    settings.fileNameTemplate = "%name%";
  }
  settings.preserveMetadata = false;
  if (typeof settings.concurrency !== "number" || !Number.isFinite(settings.concurrency)) {
    settings.concurrency = 0;
  } else {
    settings.concurrency = Math.min(8, Math.max(0, Math.round(settings.concurrency)));
  }
  if (typeof settings.lossless !== "boolean") {
    settings.lossless = false;
  }
  if (typeof settings.reduceMotion !== "boolean") {
    settings.reduceMotion = false;
  }
  if (typeof settings.quality !== "number" || !Number.isFinite(settings.quality)) {
    settings.quality = 80;
  } else {
    settings.quality = Math.min(100, Math.max(1, Math.round(settings.quality)));
  }
}
