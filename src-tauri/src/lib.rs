// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 ImgConvert contributors

mod convert;
mod import;

use convert::{
    BatchProgressEvent, BatchState, BatchSummary, Capabilities, ConvertOptions, ConvertResult,
};
use import::{ImportScanResult, ImportScanState, ScanImportOptions};
use tauri::ipc::Channel;
use tauri::State;

/// 返回进程内 core 的格式能力矩阵。
#[tauri::command]
fn capabilities() -> Capabilities {
    convert::capabilities()
}

/// 转换单张图片。前端按文件循环调用以便逐项汇报进度。
#[tauri::command]
async fn convert_image(options: ConvertOptions) -> Result<ConvertResult, String> {
    // 图像编解码和文件 IO 都是阻塞工作,放到阻塞线程池避免卡住异步运行时。
    tauri::async_runtime::spawn_blocking(move || convert::convert(&options))
        .await
        .map_err(|e| format!("任务调度失败: {e}"))?
}

/// 批量转换图片。进度通过 Tauri Channel 返回,取消在文件边界生效。
#[tauri::command]
async fn convert_batch(
    options: Vec<ConvertOptions>,
    progress: Channel<BatchProgressEvent>,
    state: State<'_, BatchState>,
) -> Result<BatchSummary, String> {
    let batch = state.begin()?;
    let batch_id = batch.id();
    let cancel = batch.token();

    let result = tauri::async_runtime::spawn_blocking(move || {
        convert::convert_batch(options, progress, cancel)
    })
    .await
    .map_err(|e| format!("批量任务调度失败: {e}"))
    .and_then(|inner| inner);

    state.finish(batch_id);
    result
}

/// 请求取消当前批量任务。返回值表示是否找到正在运行的任务并发出取消信号。
#[tauri::command]
fn cancel_batch(state: State<'_, BatchState>) -> bool {
    state.cancel_current()
}

/// 扫描用户显式授权的文件/目录路径,递归过滤出可读图片文件。
#[tauri::command]
async fn scan_import_paths(
    options: ScanImportOptions,
    state: State<'_, ImportScanState>,
) -> Result<ImportScanResult, String> {
    let scan = state.begin()?;
    let scan_id = scan.id();
    let cancel = scan.token();

    let result =
        tauri::async_runtime::spawn_blocking(move || import::scan_import_paths(options, cancel))
            .await
            .map_err(|e| format!("导入扫描任务调度失败: {e}"));

    state.finish(scan_id);
    result
}

/// 请求取消当前导入扫描。返回值表示是否找到正在运行的扫描并发出取消信号。
#[tauri::command]
fn cancel_import_scan(state: State<'_, ImportScanState>) -> bool {
    state.cancel_current()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(BatchState::default())
        .manage(ImportScanState::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            capabilities,
            convert_image,
            convert_batch,
            cancel_batch,
            scan_import_paths,
            cancel_import_scan
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
