// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 ImgConvert contributors

//! 用户显式授权路径的导入扫描。
//!
//! 当前阶段只处理 Tauri 拖拽/文件选择返回的本机路径；后续 Flatpak portal
//! 与 macOS security-scoped bookmark 应接在这一层之下，而不是散落到前端。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanImportOptions {
    pub paths: Vec<String>,
    pub extensions: Vec<String>,
    #[serde(default = "default_recursive")]
    pub recursive: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportScanResult {
    pub files: Vec<String>,
    pub skipped: usize,
    pub errors: Vec<ImportScanError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportScanError {
    pub path: String,
    pub message: String,
}

struct Scanner {
    allowed_extensions: BTreeSet<String>,
    recursive: bool,
    files: BTreeMap<PathBuf, PathBuf>,
    skipped: usize,
    errors: Vec<ImportScanError>,
}

fn default_recursive() -> bool {
    true
}

pub fn scan_import_paths(options: ScanImportOptions) -> ImportScanResult {
    let mut scanner = Scanner {
        allowed_extensions: normalize_extensions(&options.extensions),
        recursive: options.recursive,
        files: BTreeMap::new(),
        skipped: 0,
        errors: Vec::new(),
    };

    for raw_path in options.paths {
        scanner.scan_path(PathBuf::from(raw_path));
    }

    scanner.finish()
}

fn normalize_extensions(extensions: &[String]) -> BTreeSet<String> {
    extensions
        .iter()
        .filter_map(|extension| {
            let normalized = extension
                .trim()
                .trim_start_matches('.')
                .to_ascii_lowercase();
            (!normalized.is_empty()).then_some(normalized)
        })
        .collect()
}

impl Scanner {
    fn scan_path(&mut self, path: PathBuf) {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.error(&path, format!("无法读取路径: {error}"));
                return;
            }
        };

        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            self.scan_symlink(path);
        } else if file_type.is_file() {
            self.consider_file(path);
        } else if file_type.is_dir() {
            self.scan_dir(path);
        } else {
            self.skipped += 1;
        }
    }

    fn scan_symlink(&mut self, path: PathBuf) {
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                self.error(&path, format!("无法读取符号链接目标: {error}"));
                return;
            }
        };

        if metadata.is_file() {
            self.consider_file(path);
        } else {
            // 不递归符号链接目录，避免循环和越过用户明确选择的目录边界。
            self.skipped += 1;
        }
    }

    fn scan_dir(&mut self, path: PathBuf) {
        if !self.recursive {
            self.skipped += 1;
            return;
        }

        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) => {
                self.error(&path, format!("无法读取目录: {error}"));
                return;
            }
        };

        for entry in entries {
            match entry {
                Ok(entry) => self.scan_path(entry.path()),
                Err(error) => self.error(&path, format!("无法读取目录项: {error}")),
            }
        }
    }

    fn consider_file(&mut self, path: PathBuf) {
        if !self.has_allowed_extension(&path) {
            self.skipped += 1;
            return;
        }

        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        self.files.entry(key).or_insert(path);
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

    fn finish(mut self) -> ImportScanResult {
        let scanned_files = std::mem::take(&mut self.files);
        let mut files = Vec::with_capacity(scanned_files.len());
        for path in scanned_files.into_values() {
            match path.to_str() {
                Some(path) => files.push(path.to_string()),
                None => {
                    self.skipped += 1;
                    self.error(&path, "路径不是有效 UTF-8");
                }
            }
        }

        ImportScanResult {
            files,
            skipped: self.skipped,
            errors: self.errors,
        }
    }

    fn error(&mut self, path: &Path, message: impl Into<String>) {
        self.errors.push(ImportScanError {
            path: path.to_string_lossy().to_string(),
            message: message.into(),
        });
    }
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

    fn options(paths: Vec<PathBuf>, extensions: &[&str]) -> ScanImportOptions {
        ScanImportOptions {
            paths: paths
                .into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect(),
            extensions: extensions
                .iter()
                .map(|extension| extension.to_string())
                .collect(),
            recursive: true,
        }
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

        let result = scan_import_paths(options(vec![dir.clone()], &["jpg", "png"]));

        assert_eq!(result.files.len(), 2);
        assert!(result.files.contains(&jpg.to_string_lossy().to_string()));
        assert!(result.files.contains(&png.to_string_lossy().to_string()));
        assert_eq!(result.skipped, 1);
        assert!(result.errors.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn deduplicates_direct_files_also_found_through_directory_scan() {
        let dir = unique_test_dir("dedupe");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("same.webp");
        fs::write(&file, b"webp").unwrap();

        let result = scan_import_paths(options(vec![file.clone(), dir.clone()], &["webp"]));

        assert_eq!(result.files, vec![file.to_string_lossy().to_string()]);
        assert_eq!(result.skipped, 0);
        assert!(result.errors.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_paths_are_reported_without_failing_the_scan() {
        let missing = unique_test_dir("missing").join("gone.png");

        let result = scan_import_paths(options(vec![missing.clone()], &["png"]));

        assert!(result.files.is_empty());
        assert_eq!(result.skipped, 0);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].path, missing.to_string_lossy().to_string());
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

        let result = scan_import_paths(options(vec![link], &["png"]));

        assert!(result.files.is_empty());
        assert_eq!(result.skipped, 1);
        assert!(result.errors.is_empty());

        fs::remove_dir_all(dir).unwrap();
    }
}
