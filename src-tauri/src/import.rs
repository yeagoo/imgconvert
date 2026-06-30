// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 ImgConvert contributors

//! 用户显式授权路径的导入扫描。
//!
//! 当前阶段只处理 Tauri 拖拽/文件选择返回的本机路径；后续 Flatpak portal
//! 与 macOS security-scoped bookmark 应接在这一层之下，而不是散落到前端。

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use imgconvert_core::{probe, Format, READABLE_FORMATS};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

const DEFAULT_MAX_FILES: usize = 20_000;
const DEFAULT_MAX_ENTRIES: usize = 100_000;
const DEFAULT_MAX_DEPTH: usize = 64;
const HARD_MAX_FILES: usize = 100_000;
const HARD_MAX_ENTRIES: usize = 500_000;
const HARD_MAX_DEPTH: usize = 256;
const PROBE_MAX_BYTES: u64 = 512 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanImportOptions {
    pub paths: Vec<String>,
    #[serde(default = "default_recursive")]
    pub recursive: bool,
    #[serde(default)]
    pub max_files: Option<usize>,
    #[serde(default)]
    pub max_entries: Option<usize>,
    #[serde(default)]
    pub max_depth: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportScanResult {
    pub files: Vec<ImportScanFile>,
    pub skipped: usize,
    pub errors: Vec<ImportScanError>,
    pub truncated: bool,
    pub cancelled: bool,
    pub limit_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportScanFile {
    pub path: String,
    pub key: String,
    pub relative_dir: Option<String>,
    pub metadata: Option<ImportImageMetadata>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportImageMetadata {
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub dpi_x: Option<f64>,
    pub dpi_y: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportScanError {
    pub path: String,
    pub message: String,
}

#[derive(Default)]
pub struct ImportScanState {
    current: Mutex<Option<ImportScanHandle>>,
    next_id: AtomicU64,
}

struct ImportScanHandle {
    id: u64,
    token: CancellationToken,
}

pub struct ImportScanRegistration {
    id: u64,
    token: CancellationToken,
}

#[derive(Debug, Clone, Copy)]
struct ScanLimits {
    max_files: usize,
    max_entries: usize,
    max_depth: usize,
}

struct PendingPath {
    path: PathBuf,
    depth: usize,
    relative_base: Option<PathBuf>,
}

struct ScannedFile {
    path: PathBuf,
    relative_dir: Option<PathBuf>,
    metadata: Option<ImportImageMetadata>,
}

struct Scanner {
    allowed_extensions: BTreeSet<String>,
    recursive: bool,
    limits: ScanLimits,
    cancel: CancellationToken,
    files: BTreeMap<PathBuf, ScannedFile>,
    entries_seen: usize,
    skipped: usize,
    errors: Vec<ImportScanError>,
    truncated: bool,
    cancelled: bool,
    limit_reason: Option<String>,
}

impl ImportScanState {
    pub fn begin(&self) -> Result<ImportScanRegistration, String> {
        let mut current = self
            .current
            .lock()
            .map_err(|_| "导入扫描状态锁已损坏".to_string())?;
        if current.is_some() {
            return Err("已有导入扫描正在运行".to_string());
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let token = CancellationToken::new();
        *current = Some(ImportScanHandle {
            id,
            token: token.clone(),
        });
        Ok(ImportScanRegistration { id, token })
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

impl ImportScanRegistration {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}

fn default_recursive() -> bool {
    true
}

pub fn scan_import_paths(
    options: ScanImportOptions,
    cancel: CancellationToken,
) -> ImportScanResult {
    let mut scanner = Scanner {
        allowed_extensions: readable_extensions(),
        recursive: options.recursive,
        limits: ScanLimits::from_options(&options),
        cancel,
        files: BTreeMap::new(),
        entries_seen: 0,
        skipped: 0,
        errors: Vec::new(),
        truncated: false,
        cancelled: false,
        limit_reason: None,
    };

    scanner.scan(options.paths);
    scanner.finish()
}

fn readable_extensions() -> BTreeSet<String> {
    let mut extensions = BTreeSet::new();
    for format in READABLE_FORMATS {
        match format {
            Format::Jpeg => {
                extensions.insert("jpg".to_string());
                extensions.insert("jpeg".to_string());
            }
            Format::Png => {
                extensions.insert("png".to_string());
            }
            Format::WebP => {
                extensions.insert("webp".to_string());
            }
            Format::Avif => {
                extensions.insert("avif".to_string());
            }
        }
    }
    extensions
}

impl ScanLimits {
    fn from_options(options: &ScanImportOptions) -> Self {
        Self {
            max_files: normalize_limit(options.max_files, DEFAULT_MAX_FILES, HARD_MAX_FILES),
            max_entries: normalize_limit(
                options.max_entries,
                DEFAULT_MAX_ENTRIES,
                HARD_MAX_ENTRIES,
            ),
            max_depth: normalize_limit(options.max_depth, DEFAULT_MAX_DEPTH, HARD_MAX_DEPTH),
        }
    }
}

fn normalize_limit(value: Option<usize>, default: usize, hard_max: usize) -> usize {
    value.unwrap_or(default).clamp(1, hard_max)
}

impl Scanner {
    fn scan(&mut self, paths: Vec<String>) {
        let mut stack = Vec::new();
        for raw_path in paths.into_iter().rev() {
            if !self.push_path(&mut stack, PathBuf::from(raw_path), 0, None) {
                break;
            }
        }

        while let Some(pending) = stack.pop() {
            if self.should_stop() {
                break;
            }
            self.scan_path(pending, &mut stack);
        }
    }

    fn push_path(
        &mut self,
        stack: &mut Vec<PendingPath>,
        path: PathBuf,
        depth: usize,
        relative_base: Option<PathBuf>,
    ) -> bool {
        if self.should_stop() {
            return false;
        }
        if depth > self.limits.max_depth {
            self.skipped += 1;
            self.truncate("目录深度超过上限");
            return false;
        }
        if self.entries_seen >= self.limits.max_entries {
            self.truncate("扫描条目达到上限");
            return false;
        }

        self.entries_seen += 1;
        stack.push(PendingPath {
            path,
            depth,
            relative_base,
        });
        true
    }

    fn scan_path(&mut self, pending: PendingPath, stack: &mut Vec<PendingPath>) {
        let metadata = match fs::symlink_metadata(&pending.path) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.error(&pending.path, format!("无法读取路径: {error}"));
                return;
            }
        };

        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            self.scan_symlink(pending.path, pending.relative_base.as_deref());
        } else if file_type.is_file() {
            self.consider_file(pending.path, pending.relative_base.as_deref());
        } else if file_type.is_dir() {
            self.scan_dir(pending.path, pending.depth, pending.relative_base, stack);
        } else {
            self.skipped += 1;
        }
    }

    fn scan_symlink(&mut self, path: PathBuf, relative_base: Option<&Path>) {
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.error(&path, format!("无法读取符号链接目标: {error}"));
                return;
            }
        };

        if metadata.is_file() {
            self.consider_file(path, relative_base);
        } else {
            // 不递归符号链接目录，避免循环和越过用户明确选择的目录边界。
            self.skipped += 1;
        }
    }

    fn scan_dir(
        &mut self,
        path: PathBuf,
        depth: usize,
        relative_base: Option<PathBuf>,
        stack: &mut Vec<PendingPath>,
    ) {
        if !self.recursive {
            self.skipped += 1;
            return;
        }

        let child_depth = depth.saturating_add(1);
        if child_depth > self.limits.max_depth {
            self.skipped += 1;
            self.truncate("目录深度超过上限");
            return;
        }

        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) => {
                self.error(&path, format!("无法读取目录: {error}"));
                return;
            }
        };

        let child_relative_base = relative_base.unwrap_or_else(|| path.clone());
        for entry in entries {
            if self.should_stop() {
                break;
            }
            match entry {
                Ok(entry) => {
                    if !self.push_path(
                        stack,
                        entry.path(),
                        child_depth,
                        Some(child_relative_base.clone()),
                    ) {
                        break;
                    }
                }
                Err(error) => self.error(&path, format!("无法读取目录项: {error}")),
            }
        }
    }

    fn consider_file(&mut self, path: PathBuf, relative_base: Option<&Path>) {
        if !self.has_allowed_extension(&path) {
            self.skipped += 1;
            return;
        }

        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if self.files.contains_key(&key) {
            return;
        }
        if self.files.len() >= self.limits.max_files {
            self.truncate("文件数量达到上限");
            return;
        }

        let metadata = probe_file_metadata(&path);
        let relative_dir = relative_dir_for(&path, relative_base);
        self.files.insert(
            key,
            ScannedFile {
                path,
                relative_dir,
                metadata,
            },
        );
        if self.files.len() >= self.limits.max_files {
            self.truncate("文件数量达到上限");
        }
    }

    fn has_allowed_extension(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| {
                self.allowed_extensions
                    .contains(&extension.to_ascii_lowercase())
            })
            .unwrap_or(false)
    }

    fn should_stop(&mut self) -> bool {
        if self.cancel.is_cancelled() {
            self.cancelled = true;
        }
        self.cancelled || self.truncated
    }

    fn truncate(&mut self, reason: impl Into<String>) {
        if !self.truncated {
            self.truncated = true;
            self.limit_reason = Some(reason.into());
        }
    }

    fn finish(mut self) -> ImportScanResult {
        let scanned_files = std::mem::take(&mut self.files);
        let mut files = Vec::with_capacity(scanned_files.len());
        for (key, file) in scanned_files {
            match file.path.to_str() {
                Some(path) => files.push(ImportScanFile {
                    path: path.to_string(),
                    key: key.to_string_lossy().to_string(),
                    relative_dir: file
                        .relative_dir
                        .as_ref()
                        .map(|relative_dir| relative_dir.to_string_lossy().to_string()),
                    metadata: file.metadata,
                }),
                None => {
                    self.skipped += 1;
                    self.error(&file.path, "路径不是有效 UTF-8");
                }
            }
        }

        ImportScanResult {
            files,
            skipped: self.skipped,
            errors: self.errors,
            truncated: self.truncated,
            cancelled: self.cancelled,
            limit_reason: self.limit_reason,
        }
    }

    fn error(&mut self, path: &Path, message: impl Into<String>) {
        self.errors.push(ImportScanError {
            path: path.to_string_lossy().to_string(),
            message: message.into(),
        });
    }
}

fn relative_dir_for(path: &Path, relative_base: Option<&Path>) -> Option<PathBuf> {
    let base = relative_base?;
    let parent = path.parent()?;
    let relative = parent.strip_prefix(base).ok()?;
    if relative.as_os_str().is_empty() {
        None
    } else {
        Some(relative.to_path_buf())
    }
}

fn probe_file_metadata(path: &Path) -> Option<ImportImageMetadata> {
    let mut file = File::open(path).ok()?;
    let mut bytes = Vec::with_capacity(16 * 1024);
    file.by_ref()
        .take(PROBE_MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    let info = probe(&bytes).ok()?;
    Some(ImportImageMetadata {
        format: info.format.id().to_string(),
        width: info.width,
        height: info.height,
        dpi_x: info.dpi.map(|dpi| dpi.x),
        dpi_y: info.dpi.map(|dpi| dpi.y),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!(
            "imgconvert-import-{name}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn options(paths: Vec<PathBuf>) -> ScanImportOptions {
        ScanImportOptions {
            paths: paths
                .into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            recursive: true,
            max_files: None,
            max_entries: None,
            max_depth: None,
        }
    }

    fn scan(options: ScanImportOptions) -> ImportScanResult {
        scan_import_paths(options, CancellationToken::new())
    }

    fn result_paths(result: &ImportScanResult) -> Vec<String> {
        result.files.iter().map(|file| file.path.clone()).collect()
    }

    fn png_with_dimensions(width: u32, height: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&[8, 6, 0, 0, 0]);

        let mut png = Vec::new();
        png.extend_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']);
        append_png_chunk(&mut png, b"IHDR", &data);
        append_png_chunk(&mut png, b"IDAT", &[]);
        append_png_chunk(&mut png, b"IEND", &[]);
        png
    }

    fn append_png_chunk(png: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        png.extend_from_slice(name);
        png.extend_from_slice(data);
        let mut crc = 0xffff_ffffu32;
        for &byte in name.iter().chain(data.iter()) {
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 == 1 {
                    crc = (crc >> 1) ^ 0xedb8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        png.extend_from_slice(&(!crc).to_be_bytes());
    }

    #[test]
    fn scans_nested_directories_filters_extensions_and_reports_skips() {
        let dir = unique_test_dir("nested");
        let nested = dir.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let jpg = dir.join("a.jpg");
        let png = nested.join("b.PNG");
        let txt = nested.join("notes.txt");
        fs::write(&jpg, b"jpg").unwrap();
        fs::write(&png, b"png").unwrap();
        fs::write(&txt, b"text").unwrap();

        let result = scan(options(vec![dir.clone()]));
        let paths = result_paths(&result);

        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&jpg.to_string_lossy().to_string()));
        assert!(paths.contains(&png.to_string_lossy().to_string()));
        assert_eq!(result.skipped, 1);
        assert!(!result.truncated);
        assert!(!result.cancelled);
        assert!(result.errors.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn deduplicates_direct_files_also_found_through_directory_scan() {
        let dir = unique_test_dir("dedupe");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("same.webp");
        fs::write(&file, b"webp").unwrap();

        let result = scan(options(vec![file.clone(), dir.clone()]));

        assert_eq!(
            result_paths(&result),
            vec![file.to_string_lossy().to_string()]
        );
        assert_eq!(
            result.files[0].key,
            fs::canonicalize(&file).unwrap().to_string_lossy()
        );
        assert_eq!(result.skipped, 0);
        assert!(result.errors.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn directory_scan_reports_relative_output_directories() {
        let dir = unique_test_dir("relative-dir");
        let nested = dir.join("album").join("day-1");
        fs::create_dir_all(&nested).unwrap();
        let file = nested.join("image.png");
        fs::write(&file, png_with_dimensions(4, 3)).unwrap();

        let result = scan(options(vec![dir.clone()]));

        assert_eq!(result.files.len(), 1);
        assert_eq!(
            result.files[0].relative_dir.as_deref(),
            Some(Path::new("album").join("day-1").to_string_lossy().as_ref())
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scan_pings_image_dimensions_when_header_is_available() {
        let dir = unique_test_dir("metadata");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("sample.png");
        fs::write(&file, png_with_dimensions(64, 48)).unwrap();

        let result = scan(options(vec![file.clone()]));

        assert_eq!(result.files.len(), 1);
        let metadata = result.files[0].metadata.as_ref().unwrap();
        assert_eq!(metadata.format, "png");
        assert_eq!((metadata.width, metadata.height), (64, 48));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_paths_are_reported_without_failing_the_scan() {
        let missing = unique_test_dir("missing").join("gone.png");

        let result = scan(options(vec![missing.clone()]));

        assert!(result.files.is_empty());
        assert_eq!(result.skipped, 0);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].path, missing.to_string_lossy().to_string());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_file_uses_canonical_key_for_deduplication() {
        let dir = unique_test_dir("symlink-file");
        fs::create_dir_all(&dir).unwrap();
        let real = dir.join("real.png");
        let link = dir.join("link.png");
        fs::write(&real, b"png").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let result = scan(options(vec![link.clone(), real.clone()]));

        assert_eq!(result.files.len(), 1);
        assert_eq!(
            result.files[0].key,
            fs::canonicalize(&real).unwrap().to_string_lossy()
        );
        assert!(result.errors.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn does_not_recurse_into_symlink_directories() {
        let dir = unique_test_dir("symlink-dir");
        let real = dir.join("real");
        let link = dir.join("link");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("hidden.png"), b"png").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let result = scan(options(vec![link]));

        assert!(result.files.is_empty());
        assert_eq!(result.skipped, 1);
        assert!(result.errors.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn max_files_truncates_scan() {
        let dir = unique_test_dir("max-files");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.png"), b"png").unwrap();
        fs::write(dir.join("b.png"), b"png").unwrap();

        let mut options = options(vec![dir.clone()]);
        options.max_files = Some(1);
        let result = scan(options);

        assert_eq!(result.files.len(), 1);
        assert!(result.truncated);
        assert_eq!(result.limit_reason.as_deref(), Some("文件数量达到上限"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn max_entries_truncates_before_unbounded_stack_growth() {
        let dir = unique_test_dir("max-entries");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.png"), b"png").unwrap();
        fs::write(dir.join("b.png"), b"png").unwrap();

        let mut options = options(vec![dir.clone()]);
        options.max_entries = Some(1);
        let result = scan(options);

        assert!(result.files.is_empty());
        assert!(result.truncated);
        assert_eq!(result.limit_reason.as_deref(), Some("扫描条目达到上限"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cancellation_stops_scan_without_erroring() {
        let dir = unique_test_dir("cancel");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.png"), b"png").unwrap();
        let token = CancellationToken::new();
        token.cancel();

        let result = scan_import_paths(options(vec![dir.clone()]), token);

        assert!(result.files.is_empty());
        assert!(result.cancelled);
        assert!(!result.truncated);

        fs::remove_dir_all(dir).unwrap();
    }
}
