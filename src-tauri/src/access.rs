// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 ImgConvert contributors

//! 用户显式授权路径边界。
//!
//! Tauri 直发包当前拿到的是本机路径；Flatpak portal 可能给 portal 映射路径，
//! macOS App Sandbox 还需要 security-scoped bookmark 生命周期。上层导入/转换
//! 只依赖这里产出的 grant，避免后续把平台授权逻辑散落到业务代码。

use std::path::{Path, PathBuf};

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
}

pub fn user_selected_paths(paths: Vec<String>) -> Vec<AuthorizedPath> {
    paths
        .into_iter()
        .filter_map(|path| {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(AuthorizedPath::new(trimmed))
            }
        })
        .collect()
}

pub fn output_directory(path: Option<&str>) -> Option<AuthorizedPath> {
    path.map(str::trim)
        .filter(|path| !path.is_empty())
        .map(AuthorizedPath::new)
}

pub fn clipboard_temp_path(path: impl Into<PathBuf>) -> AuthorizedPath {
    AuthorizedPath::new(path)
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
}
