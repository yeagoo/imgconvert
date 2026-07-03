// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 ImgConvert contributors

//! 用户显式授权路径边界。
//!
//! Tauri 直发包当前拿到的是本机路径；Flatpak portal 可能给 portal 映射路径，
//! macOS App Sandbox 还需要 security-scoped bookmark 生命周期。上层导入/转换
//! 只依赖这里产出的 grant，避免后续把平台授权逻辑散落到业务代码。

use std::path::{Path, PathBuf};

use crate::macos_security::ScopedResource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedPath {
    path: PathBuf,
}

impl AuthorizedPath {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.path
    }

    pub fn scoped_access(&self) -> ScopedPathAccess {
        ScopedPathAccess::start(&self.path)
    }
}

#[derive(Debug)]
pub struct ScopedPathAccess {
    #[allow(dead_code)]
    path: PathBuf,
    _resource: ScopedResource,
}

impl ScopedPathAccess {
    pub fn start(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            _resource: ScopedResource::start(path),
        }
    }

    #[cfg(all(test, not(target_os = "macos")))]
    pub fn started(&self) -> bool {
        self._resource.started()
    }
}

pub fn user_selected_paths(paths: Vec<String>) -> Vec<AuthorizedPath> {
    paths
        .into_iter()
        .filter_map(|path| {
            selected_path_to_path_buf(&path)
                .or_else(|_| selected_path_to_path_buf(path.trim()))
                .ok()
                .map(AuthorizedPath::new)
        })
        .collect()
}

pub fn output_directory(path: Option<&str>) -> Option<AuthorizedPath> {
    path.and_then(|path| selected_path_to_path_buf(path).ok())
        .map(AuthorizedPath::new)
}

pub fn clipboard_temp_path(path: impl Into<PathBuf>) -> AuthorizedPath {
    AuthorizedPath::new(path)
}

pub fn scoped_path_access(path: &Path) -> ScopedPathAccess {
    ScopedPathAccess::start(path)
}

fn selected_path_to_path_buf(path: &str) -> Result<PathBuf, ()> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(());
    }
    if let Ok(url) = tauri::Url::parse(trimmed) {
        if url.scheme() == "file" {
            return url.to_file_path().map_err(|_| ());
        }
        if url.scheme().len() != 1 {
            return Err(());
        }
    }
    Ok(PathBuf::from(trimmed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_selected_paths_preserve_noncanonical_portal_paths() {
        let paths = user_selected_paths(vec![
            "  ".to_string(),
            "/run/user/1000/doc/by-app/imgconvert/photo.png".to_string(),
        ]);

        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0].path(),
            Path::new("/run/user/1000/doc/by-app/imgconvert/photo.png")
        );
    }

    #[test]
    fn output_directory_treats_empty_values_as_same_directory() {
        assert!(output_directory(None).is_none());
        assert!(output_directory(Some(" ")).is_none());
        assert_eq!(
            output_directory(Some("/tmp/out")).unwrap().path(),
            Path::new("/tmp/out")
        );
    }

    #[test]
    fn selected_paths_accept_file_urls_for_macos_scoped_dialogs() {
        let paths = user_selected_paths(vec!["file:///tmp/photo.png".to_string()]);

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].path(), Path::new("/tmp/photo.png"));
    }

    #[test]
    fn selected_paths_reject_non_file_urls() {
        assert!(user_selected_paths(vec!["https://example.com/photo.png".to_string()]).is_empty());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn scoped_access_is_noop_off_macos() {
        assert!(!scoped_path_access(Path::new("/tmp")).started());
    }
}
