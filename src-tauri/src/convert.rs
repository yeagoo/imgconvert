// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 ImgConvert contributors

//! Tauri 转换命令胶水。
//!
//! 图片编解码由 `imgconvert-core` 进程内完成；这里只负责路径解析、文件读写、
//! 覆盖策略和前端能力矩阵。

use std::collections::{HashSet, VecDeque};
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use imgconvert_core::{
    EncodeOptions, Format, LOSSLESS_FORMATS, READABLE_FORMATS, WRITABLE_FORMATS,
};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tokio_util::sync::CancellationToken;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const MAX_BATCH_CONCURRENCY: usize = 8;

enum WriteMode {
    Replace,
    CreateNew,
}

struct IndexedConvertOptions {
    index: usize,
    options: ConvertOptions,
}

/// 前端启动时读取的进程内 core 能力矩阵。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub readable: Vec<&'static str>,
    pub writable: Vec<&'static str>,
    pub lossless: Vec<&'static str>,
    /// Linux v1 不含 HEIC；macOS/Windows 后续接系统原生能力探测。
    pub heic: bool,
}

/// 单张图片的转换参数,由前端传入。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertOptions {
    /// 输入文件绝对路径。
    pub input: String,
    /// 输出目录(若为空则与输入同目录)。
    pub out_dir: Option<String>,
    /// 从导入目录根到输入文件父目录的相对目录,用于保留目录结构。
    pub relative_dir: Option<String>,
    /// 目标格式:avif | webp | jpeg | png。
    pub format: String,
    /// 质量 1-100(有损时生效)。
    pub quality: u8,
    /// 是否无损(对支持的格式生效:webp/png)。
    pub lossless: bool,
    /// 是否覆盖已存在的输出文件。
    pub overwrite: bool,
    /// 覆盖策略:ask | skip | overwrite。ask 由前端确认后以 overwrite 重试。
    pub overwrite_mode: Option<String>,
    /// 文件名模板,支持 %name% / %extension% / %date%。
    pub file_name_template: Option<String>,
    /// 元数据保留开关预留;core P2 才实现实际透传。
    pub preserve_metadata: Option<bool>,
}

/// 单张转换结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertResult {
    pub input: String,
    pub output: String,
    /// 输入文件大小(字节)。
    pub in_size: u64,
    /// 输出文件大小(字节)。
    pub out_size: u64,
}

/// 转换前输出路径规划,用于前端 ask 覆盖策略一次性收集决策。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionPlanEntry {
    pub index: usize,
    pub input: String,
    pub output: Option<String>,
    pub exists: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub total: usize,
    pub completed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub cancelled: bool,
}

/// 批量转换进度事件。`index` 是本次 batch options 数组中的下标。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum BatchProgressEvent {
    Started {
        total: usize,
    },
    FileStarted {
        index: usize,
        input: String,
    },
    FileProgress {
        index: usize,
        percent: f64,
        stage: &'static str,
    },
    FileFinished {
        index: usize,
        result: ConvertResult,
    },
    FileSkipped {
        index: usize,
        input: String,
        message: String,
    },
    FileError {
        index: usize,
        input: String,
        message: String,
    },
    Cancelled {
        completed: usize,
        total: usize,
    },
    Finished {
        summary: BatchSummary,
    },
}

#[derive(Default)]
pub struct BatchState {
    current: Mutex<Option<BatchHandle>>,
    next_id: AtomicU64,
}

struct BatchHandle {
    id: u64,
    token: CancellationToken,
}

pub struct BatchRegistration {
    id: u64,
    token: CancellationToken,
}

impl BatchState {
    pub fn begin(&self) -> Result<BatchRegistration, String> {
        let mut current = self
            .current
            .lock()
            .map_err(|_| "批量任务状态锁已损坏".to_string())?;
        if current.is_some() {
            return Err("已有批量任务正在运行".to_string());
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::new();
        *current = Some(BatchHandle {
            id,
            token: token.clone(),
        });
        Ok(BatchRegistration { id, token })
    }

    pub fn finish(&self, id: u64) {
        if let Ok(mut current) = self.current.lock() {
            if current.as_ref().is_some_and(|handle| handle.id == id) {
                *current = None;
            }
        }
    }

    pub fn cancel_current(&self) -> bool {
        if let Ok(current) = self.current.lock() {
            if let Some(handle) = current.as_ref() {
                handle.token.cancel();
                return true;
            }
        }
        false
    }
}

impl BatchRegistration {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}

/// 返回当前平台/core 支持的格式能力。
pub fn capabilities() -> Capabilities {
    Capabilities {
        readable: format_ids(READABLE_FORMATS),
        writable: format_ids(WRITABLE_FORMATS),
        lossless: format_ids(LOSSLESS_FORMATS),
        heic: false,
    }
}

fn format_ids(formats: &[Format]) -> Vec<&'static str> {
    formats.iter().map(|format| format.id()).collect()
}

fn parse_format(format: &str) -> Result<Format, String> {
    match format.to_ascii_lowercase().as_str() {
        "jpeg" | "jpg" => Ok(Format::Jpeg),
        "png" => Ok(Format::Png),
        "webp" => Ok(Format::WebP),
        "avif" => Ok(Format::Avif),
        "heic" | "heif" => Err("HEIC 在 Linux v1 暂不支持".to_string()),
        "tiff" | "tif" => Err("TIFF 暂未纳入 v1 可写格式".to_string()),
        other => Err(format!("不支持的目标格式: {other}")),
    }
}

/// 计算目标格式对应的文件扩展名。
fn ext_for_format(format: &str) -> Result<&'static str, String> {
    parse_format(format).map(Format::default_extension)
}

fn date_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    // Howard Hinnant civil-from-days algorithm, public domain.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    format!("{year:04}{m:02}{d:02}")
}

fn sanitize_file_stem(stem: &str) -> String {
    let cleaned: String = stem
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    let trimmed = cleaned.trim_matches([' ', '.']);
    if trimmed.is_empty() {
        "converted".to_string()
    } else {
        trimmed.to_string()
    }
}

fn apply_file_name_template(template: Option<&str>, source_stem: &str, ext: &str) -> String {
    let template = template
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("%name%");
    let rendered = template
        .replace("%name%", source_stem)
        .replace("%extension%", ext)
        .replace("%date%", &date_stamp());
    sanitize_file_stem(&rendered)
}

/// 根据输入路径与目标格式计算输出路径。
fn output_path(opts: &ConvertOptions) -> Result<PathBuf, String> {
    let input = Path::new(&opts.input);
    let stem = input
        .file_stem()
        .ok_or_else(|| format!("无法解析文件名: {}", opts.input))?;
    let ext = ext_for_format(&opts.format)?;
    let stem = stem.to_string_lossy();
    let output_stem = apply_file_name_template(opts.file_name_template.as_deref(), &stem, ext);

    let dir: PathBuf = match opts.out_dir.as_deref() {
        Some(d) if !d.is_empty() => {
            let mut dir = PathBuf::from(d);
            if let Some(relative_dir) = safe_relative_dir(opts.relative_dir.as_deref())? {
                dir.push(relative_dir);
            }
            dir
        }
        _ => input
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    };

    let mut out = dir.join(output_stem);
    out.set_extension(ext);
    Ok(out)
}

fn safe_relative_dir(relative_dir: Option<&str>) -> Result<Option<PathBuf>, String> {
    let Some(relative_dir) = relative_dir
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let path = Path::new(relative_dir);
    if path.is_absolute() {
        return Err(format!("输出相对目录不能是绝对路径: {relative_dir}"));
    }

    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => {
                let part = sanitize_file_stem(&part.to_string_lossy());
                if !part.is_empty() {
                    safe.push(part);
                }
            }
            std::path::Component::CurDir => {}
            _ => return Err(format!("输出相对目录包含非法路径片段: {relative_dir}")),
        }
    }

    if safe.as_os_str().is_empty() {
        Ok(None)
    } else {
        Ok(Some(safe))
    }
}

pub fn conversion_plan(options: &[ConvertOptions]) -> Vec<ConversionPlanEntry> {
    options
        .iter()
        .enumerate()
        .map(|(index, options)| match output_path(options) {
            Ok(output) => ConversionPlanEntry {
                index,
                input: options.input.clone(),
                exists: output.exists(),
                output: Some(output.to_string_lossy().to_string()),
                error: None,
            },
            Err(error) => ConversionPlanEntry {
                index,
                input: options.input.clone(),
                output: None,
                exists: false,
                error: Some(error),
            },
        })
        .collect()
}

fn temp_file(out: &Path) -> Result<(PathBuf, File), String> {
    let parent = out.parent().unwrap_or_else(|| Path::new("."));
    let pid = std::process::id();

    for _ in 0..64 {
        let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let tmp = parent.join(format!(".imgconvert-{pid}-{nanos}-{counter}.tmp"));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("无法创建临时文件: {e}")),
        }
    }

    Err("无法创建唯一临时文件".to_string())
}

fn write_output(out: &Path, bytes: &[u8], mode: WriteMode) -> Result<(), String> {
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("无法创建输出目录: {e}"))?;
    }

    match mode {
        WriteMode::Replace => replace_output(out, bytes),
        WriteMode::CreateNew => write_new_output(out, bytes),
    }
}

fn write_temp_output(out: &Path, bytes: &[u8]) -> Result<PathBuf, String> {
    let (tmp, mut tmp_file) = temp_file(out)?;
    if let Err(e) = tmp_file.write_all(bytes).and_then(|_| tmp_file.flush()) {
        drop(tmp_file);
        let _ = fs::remove_file(&tmp);
        return Err(format!("无法写入临时文件: {e}"));
    }
    drop(tmp_file);
    Ok(tmp)
}

fn replace_output(out: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = write_temp_output(out, bytes)?;

    match fs::rename(&tmp, out) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && out.exists() => {
            if let Err(remove_error) = fs::remove_file(out) {
                let _ = fs::remove_file(&tmp);
                return Err(format!("无法替换已存在的输出文件: {remove_error}"));
            }
            if let Err(rename_error) = fs::rename(&tmp, out) {
                let _ = fs::remove_file(&tmp);
                return Err(format!("无法写入输出文件: {rename_error}"));
            }
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(format!("无法写入输出文件: {e}"))
        }
    }
}

fn write_new_output(out: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                format!("输出已存在(未开启覆盖): {}", out.display())
            } else {
                format!("无法创建输出文件: {e}")
            }
        })?;

    if let Err(e) = file.write_all(bytes).and_then(|_| file.flush()) {
        drop(file);
        let _ = fs::remove_file(out);
        return Err(format!("无法写入输出文件: {e}"));
    }

    Ok(())
}

/// 执行一次转换。
pub fn convert(opts: &ConvertOptions) -> Result<ConvertResult, String> {
    let input = Path::new(&opts.input);
    if !input.exists() {
        return Err(format!("输入文件不存在: {}", opts.input));
    }

    let out = output_path(opts)?;
    let source_modified = fs::metadata(input)
        .and_then(|metadata| metadata.modified())
        .ok();
    let overwrite_mode = match opts.overwrite_mode.as_deref() {
        Some(mode @ ("ask" | "skip" | "overwrite")) => mode,
        Some(other) => return Err(format!("不支持的覆盖策略: {other}")),
        None if opts.overwrite => "overwrite",
        None => "skip",
    };
    if out.exists() && overwrite_mode != "overwrite" {
        if overwrite_mode == "ask" {
            return Err(format!("输出已存在(需要确认覆盖): {}", out.display()));
        }
        return Err(format!("输出已存在(未开启覆盖): {}", out.display()));
    }

    let target = parse_format(&opts.format)?;
    if opts.preserve_metadata.unwrap_or(false) {
        return Err("保留元数据尚未实现".to_string());
    }
    let source = fs::read(input).map_err(|e| format!("无法读取输入文件: {e}"))?;
    let encoded = imgconvert_core::convert(
        &source,
        target,
        &EncodeOptions {
            quality: opts.quality.clamp(1, 100),
            lossless: opts.lossless && target.supports_lossless(),
        },
    )
    .map_err(|e| e.to_string())?;

    let write_mode = if overwrite_mode == "overwrite" {
        WriteMode::Replace
    } else {
        WriteMode::CreateNew
    };
    write_output(&out, &encoded, write_mode)?;
    if let Some(modified) = source_modified {
        let _ = set_modified_time(&out, modified);
    }

    let in_size = fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let out_size = fs::metadata(&out).map(|m| m.len()).unwrap_or(0);

    Ok(ConvertResult {
        input: opts.input.clone(),
        output: out.to_string_lossy().to_string(),
        in_size,
        out_size,
    })
}

fn set_modified_time(path: &Path, modified: SystemTime) -> Result<(), String> {
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| format!("无法打开输出文件以保留时间戳: {e}"))?;
    file.set_times(FileTimes::new().set_modified(modified))
        .map_err(|e| format!("无法保留源文件修改时间: {e}"))
}

pub fn convert_batch(
    options: Vec<ConvertOptions>,
    progress: Channel<BatchProgressEvent>,
    cancel: CancellationToken,
    concurrency: Option<usize>,
) -> Result<BatchSummary, String> {
    let total = options.len();
    let mut summary = BatchSummary {
        total,
        completed: 0,
        skipped: 0,
        failed: 0,
        cancelled: false,
    };

    send_progress(&progress, BatchProgressEvent::Started { total })?;
    if total == 0 {
        send_progress(
            &progress,
            BatchProgressEvent::Finished {
                summary: summary.clone(),
            },
        )?;
        return Ok(summary);
    }
    if cancel.is_cancelled() {
        summary.cancelled = true;
        send_cancelled_progress(&progress, &summary)?;
        send_progress(
            &progress,
            BatchProgressEvent::Finished {
                summary: summary.clone(),
            },
        )?;
        return Ok(summary);
    }

    let worker_count = resolve_batch_concurrency(concurrency, total);
    let (jobs, preflight_events) = prepare_batch_work(options);
    for event in preflight_events {
        apply_batch_event(&progress, &mut summary, event)?;
    }
    if jobs.is_empty() {
        send_progress(
            &progress,
            BatchProgressEvent::Finished {
                summary: summary.clone(),
            },
        )?;
        return Ok(summary);
    }

    let queue = Arc::new(Mutex::new(jobs));
    let (tx, rx) = mpsc::channel::<BatchProgressEvent>();
    let coordinator_cancel = cancel.clone();

    let send_error = thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let tx = tx.clone();
            let worker_cancel = cancel.clone();
            scope.spawn(move || {
                batch_worker_loop(queue, tx, worker_cancel);
            });
        }
        drop(tx);

        let mut send_error = None;
        for event in rx {
            if send_error.is_some() {
                continue;
            }
            if let Err(error) = apply_batch_event(&progress, &mut summary, event) {
                coordinator_cancel.cancel();
                send_error = Some(error);
            }
        }
        send_error
    });

    if let Some(error) = send_error {
        return Err(error);
    }

    if cancel.is_cancelled() {
        summary.cancelled = true;
        send_cancelled_progress(&progress, &summary)?;
    }

    send_progress(
        &progress,
        BatchProgressEvent::Finished {
            summary: summary.clone(),
        },
    )?;
    Ok(summary)
}

fn prepare_batch_work(
    options: Vec<ConvertOptions>,
) -> (VecDeque<IndexedConvertOptions>, Vec<BatchProgressEvent>) {
    let mut jobs = VecDeque::new();
    let mut preflight_events = Vec::new();
    let mut output_paths = HashSet::new();

    for (index, options) in options.into_iter().enumerate() {
        match output_path(&options) {
            Ok(output) => {
                let key = output_conflict_key(&output);
                if !output_paths.insert(key) {
                    preflight_events.push(BatchProgressEvent::FileError {
                        index,
                        input: options.input,
                        message: format!("输出路径在本批次内重复: {}", output.display()),
                    });
                    continue;
                }
            }
            Err(_) => {
                // Let convert() report the exact validation error through the normal file path.
            }
        }

        jobs.push_back(IndexedConvertOptions { index, options });
    }

    (jobs, preflight_events)
}

fn output_conflict_key(output: &Path) -> PathBuf {
    let Some(parent) = output.parent() else {
        return output.to_path_buf();
    };
    let Some(file_name) = output.file_name() else {
        return output.to_path_buf();
    };
    match fs::canonicalize(parent) {
        Ok(parent) => parent.join(file_name),
        Err(_) => output.to_path_buf(),
    }
}

fn batch_worker_loop(
    queue: Arc<Mutex<VecDeque<IndexedConvertOptions>>>,
    tx: mpsc::Sender<BatchProgressEvent>,
    cancel: CancellationToken,
) {
    loop {
        if cancel.is_cancelled() {
            break;
        }

        let Some(job) = next_batch_job(&queue) else {
            break;
        };
        if cancel.is_cancelled() {
            break;
        }

        let index = job.index;
        let opts = job.options;
        let input = opts.input.clone();

        if send_worker_event(
            &tx,
            BatchProgressEvent::FileStarted {
                index,
                input: input.clone(),
            },
            &cancel,
        )
        .is_err()
        {
            break;
        }
        if send_worker_event(
            &tx,
            BatchProgressEvent::FileProgress {
                index,
                percent: 5.0,
                stage: "读取并转换",
            },
            &cancel,
        )
        .is_err()
        {
            break;
        }

        let event = match convert(&opts) {
            Ok(result) => BatchProgressEvent::FileFinished { index, result },
            Err(message) if should_count_as_skipped(&opts, &message) => {
                BatchProgressEvent::FileSkipped {
                    index,
                    input,
                    message,
                }
            }
            Err(message) => BatchProgressEvent::FileError {
                index,
                input,
                message,
            },
        };

        if send_worker_event(&tx, event, &cancel).is_err() {
            break;
        }
    }
}

fn next_batch_job(
    queue: &Arc<Mutex<VecDeque<IndexedConvertOptions>>>,
) -> Option<IndexedConvertOptions> {
    queue.lock().ok()?.pop_front()
}

fn send_worker_event(
    tx: &mpsc::Sender<BatchProgressEvent>,
    event: BatchProgressEvent,
    cancel: &CancellationToken,
) -> Result<(), ()> {
    tx.send(event).map_err(|_| {
        cancel.cancel();
    })
}

fn apply_batch_event(
    progress: &Channel<BatchProgressEvent>,
    summary: &mut BatchSummary,
    event: BatchProgressEvent,
) -> Result<(), String> {
    match &event {
        BatchProgressEvent::FileFinished { .. } => summary.completed += 1,
        BatchProgressEvent::FileSkipped { .. } => summary.skipped += 1,
        BatchProgressEvent::FileError { .. } => summary.failed += 1,
        _ => {}
    }
    if matches!(event, BatchProgressEvent::FileFinished { .. }) {
        let index = match &event {
            BatchProgressEvent::FileFinished { index, .. } => *index,
            _ => unreachable!(),
        };
        send_progress(
            progress,
            BatchProgressEvent::FileProgress {
                index,
                percent: 100.0,
                stage: "完成",
            },
        )?;
    }
    send_progress(progress, event)
}

fn resolve_batch_concurrency(requested: Option<usize>, total: usize) -> usize {
    if total == 0 {
        return 0;
    }

    requested
        .filter(|value| *value > 0)
        .unwrap_or_else(default_batch_concurrency)
        .clamp(1, MAX_BATCH_CONCURRENCY)
        .min(total)
}

fn default_batch_concurrency() -> usize {
    thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(2)
        .saturating_sub(1)
        .clamp(1, MAX_BATCH_CONCURRENCY)
}

fn send_progress(
    progress: &Channel<BatchProgressEvent>,
    event: BatchProgressEvent,
) -> Result<(), String> {
    progress
        .send(event)
        .map_err(|e| format!("无法发送批量进度事件: {e}"))
}

fn send_cancelled_progress(
    progress: &Channel<BatchProgressEvent>,
    summary: &BatchSummary,
) -> Result<(), String> {
    send_progress(
        progress,
        BatchProgressEvent::Cancelled {
            completed: summary.completed,
            total: summary.total,
        },
    )
}

fn should_count_as_skipped(opts: &ConvertOptions, message: &str) -> bool {
    let skip_mode = match opts.overwrite_mode.as_deref() {
        Some("skip") => true,
        None => !opts.overwrite,
        _ => false,
    };
    skip_mode && message.contains("已存在")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_dir(name: &str) -> PathBuf {
        let counter = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "imgconvert-{name}-{}-{counter}",
            std::process::id()
        ))
    }

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &byte in bytes {
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 == 1 {
                    crc = (crc >> 1) ^ 0xedb8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    fn adler32(bytes: &[u8]) -> u32 {
        const MOD: u32 = 65_521;
        let mut a = 1u32;
        let mut b = 0u32;
        for &byte in bytes {
            a = (a + byte as u32) % MOD;
            b = (b + a) % MOD;
        }
        (b << 16) | a
    }

    fn append_png_chunk(png: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        png.extend_from_slice(name);
        png.extend_from_slice(data);
        let mut crc_input = Vec::with_capacity(4 + data.len());
        crc_input.extend_from_slice(name);
        crc_input.extend_from_slice(data);
        png.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    }

    fn one_by_one_png() -> Vec<u8> {
        let scanline = [0, 255, 255, 255, 255]; // filter byte + opaque white RGBA pixel
        let mut zlib = vec![0x78, 0x01, 0x01, 5, 0, 0xfa, 0xff];
        zlib.extend_from_slice(&scanline);
        zlib.extend_from_slice(&adler32(&scanline).to_be_bytes());

        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);

        let mut png = Vec::new();
        png.extend_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']);
        append_png_chunk(&mut png, b"IHDR", &ihdr);
        append_png_chunk(&mut png, b"IDAT", &zlib);
        append_png_chunk(&mut png, b"IEND", &[]);
        png
    }

    #[test]
    fn write_output_create_new_does_not_clobber_existing_file() {
        let dir = unique_test_dir("create-new");
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.bin");
        fs::write(&out, b"old").unwrap();

        let err = write_output(&out, b"new", WriteMode::CreateNew).unwrap_err();
        assert!(err.contains("输出已存在"));
        assert_eq!(fs::read(&out).unwrap(), b"old");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn write_output_replace_replaces_and_cleans_temp_file() {
        let dir = unique_test_dir("replace");
        fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.bin");
        fs::write(&out, b"old").unwrap();

        write_output(&out, b"new", WriteMode::Replace).unwrap();

        assert_eq!(fs::read(&out).unwrap(), b"new");
        let leftovers = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".imgconvert-")
            })
            .count();
        assert_eq!(leftovers, 0);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn batch_state_allows_one_active_task_and_cancels_it() {
        let state = BatchState::default();
        let batch = state.begin().unwrap();

        assert!(state.begin().is_err());
        assert!(state.cancel_current());
        assert!(batch.token.is_cancelled());

        state.finish(batch.id());
        assert!(state.begin().is_ok());
    }

    #[test]
    fn batch_empty_options_finishes_cleanly() {
        let progress = Channel::<BatchProgressEvent>::new(|_| Ok(()));
        let summary = convert_batch(Vec::new(), progress, CancellationToken::new(), None).unwrap();

        assert_eq!(summary.total, 0);
        assert_eq!(summary.completed, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.failed, 0);
        assert!(!summary.cancelled);
    }

    #[test]
    fn batch_converts_one_file_and_writes_summary() {
        let dir = unique_test_dir("batch-convert");
        let out_dir = dir.join("out");
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("sample.png");
        fs::write(&input, one_by_one_png()).unwrap();

        let options = ConvertOptions {
            input: input.to_string_lossy().to_string(),
            out_dir: Some(out_dir.to_string_lossy().to_string()),
            relative_dir: None,
            format: "jpeg".to_string(),
            quality: 80,
            lossless: false,
            overwrite: false,
            overwrite_mode: Some("skip".to_string()),
            file_name_template: Some("%name%".to_string()),
            preserve_metadata: Some(false),
        };
        let progress = Channel::<BatchProgressEvent>::new(|_| Ok(()));
        let summary =
            convert_batch(vec![options], progress, CancellationToken::new(), Some(2)).unwrap();

        assert_eq!(summary.total, 1);
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.failed, 0);
        assert!(out_dir.join("sample.jpg").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn conversion_plan_reports_existing_outputs_per_entry() {
        let dir = unique_test_dir("plan-existing");
        let out_dir = dir.join("out");
        fs::create_dir_all(&out_dir).unwrap();
        let input = dir.join("sample.png");
        let output = out_dir.join("sample.jpg");
        fs::write(&input, one_by_one_png()).unwrap();
        fs::write(&output, b"old").unwrap();

        let options = ConvertOptions {
            input: input.to_string_lossy().to_string(),
            out_dir: Some(out_dir.to_string_lossy().to_string()),
            relative_dir: None,
            format: "jpeg".to_string(),
            quality: 80,
            lossless: false,
            overwrite: false,
            overwrite_mode: Some("ask".to_string()),
            file_name_template: Some("%name%".to_string()),
            preserve_metadata: Some(false),
        };

        let plan = conversion_plan(&[options]);

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].index, 0);
        assert!(plan[0].exists);
        assert_eq!(
            plan[0].output.as_deref(),
            Some(output.to_string_lossy().as_ref())
        );
        assert!(plan[0].error.is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn output_path_preserves_safe_relative_directory_under_out_dir() {
        let dir = unique_test_dir("relative-output");
        let input = dir.join("source").join("photo.png");
        let out_dir = dir.join("out");
        fs::create_dir_all(input.parent().unwrap()).unwrap();
        fs::write(&input, one_by_one_png()).unwrap();

        let options = ConvertOptions {
            input: input.to_string_lossy().to_string(),
            out_dir: Some(out_dir.to_string_lossy().to_string()),
            relative_dir: Some("album/day-1".to_string()),
            format: "jpeg".to_string(),
            quality: 80,
            lossless: false,
            overwrite: false,
            overwrite_mode: Some("skip".to_string()),
            file_name_template: Some("%name%".to_string()),
            preserve_metadata: Some(false),
        };

        let output = output_path(&options).unwrap();

        assert_eq!(
            output,
            out_dir.join("album").join("day-1").join("photo.jpg")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn output_path_rejects_unsafe_relative_directory() {
        let options = ConvertOptions {
            input: "photo.png".to_string(),
            out_dir: Some("out".to_string()),
            relative_dir: Some("../escape".to_string()),
            format: "jpeg".to_string(),
            quality: 80,
            lossless: false,
            overwrite: false,
            overwrite_mode: Some("skip".to_string()),
            file_name_template: Some("%name%".to_string()),
            preserve_metadata: Some(false),
        };

        let err = output_path(&options).unwrap_err();

        assert!(err.contains("非法路径片段"));
    }

    #[test]
    fn batch_converts_multiple_files_with_requested_concurrency() {
        let dir = unique_test_dir("batch-convert-concurrent");
        let out_dir = dir.join("out");
        fs::create_dir_all(&dir).unwrap();
        let input_a = dir.join("sample-a.png");
        let input_b = dir.join("sample-b.png");
        fs::write(&input_a, one_by_one_png()).unwrap();
        fs::write(&input_b, one_by_one_png()).unwrap();

        let options = [input_a.clone(), input_b.clone()]
            .into_iter()
            .map(|input| ConvertOptions {
                input: input.to_string_lossy().to_string(),
                out_dir: Some(out_dir.to_string_lossy().to_string()),
                relative_dir: None,
                format: "jpeg".to_string(),
                quality: 80,
                lossless: false,
                overwrite: false,
                overwrite_mode: Some("skip".to_string()),
                file_name_template: Some("%name%".to_string()),
                preserve_metadata: Some(false),
            })
            .collect();
        let progress = Channel::<BatchProgressEvent>::new(|_| Ok(()));
        let summary = convert_batch(options, progress, CancellationToken::new(), Some(2)).unwrap();

        assert_eq!(summary.total, 2);
        assert_eq!(summary.completed, 2);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.failed, 0);
        assert!(out_dir.join("sample-a.jpg").exists());
        assert!(out_dir.join("sample-b.jpg").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn batch_rejects_duplicate_output_paths_before_parallel_work() {
        let dir = unique_test_dir("batch-duplicate-output");
        let source_a = dir.join("a");
        let source_b = dir.join("b");
        let out_dir = dir.join("out");
        fs::create_dir_all(&source_a).unwrap();
        fs::create_dir_all(&source_b).unwrap();
        let input_a = source_a.join("sample.png");
        let input_b = source_b.join("sample.png");
        fs::write(&input_a, one_by_one_png()).unwrap();
        fs::write(&input_b, one_by_one_png()).unwrap();

        let options = [input_a.clone(), input_b.clone()]
            .into_iter()
            .map(|input| ConvertOptions {
                input: input.to_string_lossy().to_string(),
                out_dir: Some(out_dir.to_string_lossy().to_string()),
                relative_dir: None,
                format: "jpeg".to_string(),
                quality: 80,
                lossless: false,
                overwrite: true,
                overwrite_mode: Some("overwrite".to_string()),
                file_name_template: Some("%name%".to_string()),
                preserve_metadata: Some(false),
            })
            .collect();
        let progress = Channel::<BatchProgressEvent>::new(|_| Ok(()));
        let summary = convert_batch(options, progress, CancellationToken::new(), Some(2)).unwrap();

        assert_eq!(summary.total, 2);
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.failed, 1);
        assert!(!summary.cancelled);
        assert!(out_dir.join("sample.jpg").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn convert_preserves_source_modified_time_on_output() {
        let dir = unique_test_dir("preserve-mtime");
        let out_dir = dir.join("out");
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("sample.png");
        fs::write(&input, one_by_one_png()).unwrap();
        let modified = UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        set_modified_time(&input, modified).unwrap();

        let options = ConvertOptions {
            input: input.to_string_lossy().to_string(),
            out_dir: Some(out_dir.to_string_lossy().to_string()),
            relative_dir: Some("nested".to_string()),
            format: "jpeg".to_string(),
            quality: 80,
            lossless: false,
            overwrite: false,
            overwrite_mode: Some("skip".to_string()),
            file_name_template: Some("%name%".to_string()),
            preserve_metadata: Some(false),
        };

        let result = convert(&options).unwrap();
        let output_modified = fs::metadata(result.output).unwrap().modified().unwrap();
        let delta = output_modified
            .duration_since(modified)
            .or_else(|_| modified.duration_since(output_modified))
            .unwrap();

        assert!(delta < std::time::Duration::from_secs(2));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn batch_concurrency_respects_default_bounds_request_and_total() {
        assert_eq!(resolve_batch_concurrency(Some(0), 0), 0);
        assert_eq!(resolve_batch_concurrency(Some(1), 10), 1);
        assert_eq!(
            resolve_batch_concurrency(Some(999), 10),
            MAX_BATCH_CONCURRENCY
        );
        assert_eq!(resolve_batch_concurrency(Some(999), 3), 3);
        assert!((1..=MAX_BATCH_CONCURRENCY).contains(&resolve_batch_concurrency(None, 999)));
    }

    #[test]
    fn batch_cancelled_before_first_file_reports_cancelled() {
        let dir = unique_test_dir("batch-cancel");
        let out_dir = dir.join("out");
        fs::create_dir_all(&dir).unwrap();
        let input = dir.join("sample.png");
        fs::write(&input, one_by_one_png()).unwrap();

        let options = ConvertOptions {
            input: input.to_string_lossy().to_string(),
            out_dir: Some(out_dir.to_string_lossy().to_string()),
            relative_dir: None,
            format: "jpeg".to_string(),
            quality: 80,
            lossless: false,
            overwrite: false,
            overwrite_mode: Some("skip".to_string()),
            file_name_template: Some("%name%".to_string()),
            preserve_metadata: Some(false),
        };
        let token = CancellationToken::new();
        token.cancel();
        let progress = Channel::<BatchProgressEvent>::new(|_| Ok(()));
        let summary = convert_batch(vec![options], progress, token, Some(2)).unwrap();

        assert_eq!(summary.total, 1);
        assert_eq!(summary.completed, 0);
        assert_eq!(summary.skipped, 0);
        assert_eq!(summary.failed, 0);
        assert!(summary.cancelled);
        assert!(!out_dir.join("sample.jpg").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn batch_skip_mode_counts_existing_output_as_skipped() {
        let dir = unique_test_dir("batch-skip-existing");
        let out_dir = dir.join("out");
        fs::create_dir_all(&out_dir).unwrap();
        let input = dir.join("sample.png");
        let output = out_dir.join("sample.jpg");
        fs::write(&input, one_by_one_png()).unwrap();
        fs::write(&output, b"old").unwrap();

        let options = ConvertOptions {
            input: input.to_string_lossy().to_string(),
            out_dir: Some(out_dir.to_string_lossy().to_string()),
            relative_dir: None,
            format: "jpeg".to_string(),
            quality: 80,
            lossless: false,
            overwrite: false,
            overwrite_mode: Some("skip".to_string()),
            file_name_template: Some("%name%".to_string()),
            preserve_metadata: Some(false),
        };
        let progress = Channel::<BatchProgressEvent>::new(|_| Ok(()));
        let summary =
            convert_batch(vec![options], progress, CancellationToken::new(), Some(2)).unwrap();

        assert_eq!(summary.total, 1);
        assert_eq!(summary.completed, 0);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.failed, 0);
        assert!(!summary.cancelled);
        assert_eq!(fs::read(output).unwrap(), b"old");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn batch_progress_send_failure_aborts_batch() {
        let progress = Channel::<BatchProgressEvent>::new(|_| {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "closed").into())
        });
        let err = convert_batch(Vec::new(), progress, CancellationToken::new(), None).unwrap_err();

        assert!(err.contains("无法发送批量进度事件"));
    }
}
