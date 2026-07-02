// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 ImgConvert contributors

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if std::env::var_os("IMGCONVERT_PACKAGE_CONVERT_SMOKE").is_some() {
        if let Err(error) = run_package_conversion_smoke() {
            eprintln!("imgconvert package conversion smoke failed: {error}");
            std::process::exit(70);
        }
        return;
    }
    tauri_app_lib::run()
}

fn run_package_conversion_smoke() -> Result<(), String> {
    use imgconvert_core::{codec_for, convert, EncodeOptions, Format, ImageData};

    let source = ImageData::new(16, 16, smoke_rgba()).map_err(|error| error.to_string())?;
    let source_png = codec_for(Format::Png)
        .encode(&source, &EncodeOptions::default())
        .map_err(|error| error.to_string())?;
    let formats = smoke_formats()?;
    let out_dir = smoke_output_dir()?;
    let mut verified = Vec::new();

    for format in formats {
        let options = EncodeOptions {
            quality: 82,
            avif_speed: 10,
            ..Default::default()
        };
        let encoded = convert(&source_png, format, &options).map_err(|error| error.to_string())?;
        if Format::from_magic(&encoded) != Some(format) {
            return Err(format!("{} output magic mismatch", format.id()));
        }
        let probe = imgconvert_core::probe(&encoded).map_err(|error| error.to_string())?;
        if probe.width != 16 || probe.height != 16 {
            return Err(format!(
                "{} output dimensions mismatch: {}x{}",
                format.id(),
                probe.width,
                probe.height
            ));
        }
        let output = out_dir.join(format!("smoke.{}", format.default_extension()));
        std::fs::write(&output, encoded)
            .map_err(|error| format!("write {} failed: {error}", output.display()))?;
        verified.push(format.id());
    }

    println!(
        "imgconvert package conversion smoke passed: {}",
        verified.join(",")
    );
    Ok(())
}

fn smoke_rgba() -> Vec<u8> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16u8 {
        for x in 0..16u8 {
            rgba.extend_from_slice(&[
                x.saturating_mul(16),
                y.saturating_mul(16),
                x.wrapping_add(y).saturating_mul(8),
                255,
            ]);
        }
    }
    rgba
}

fn smoke_formats() -> Result<Vec<imgconvert_core::Format>, String> {
    let raw = std::env::var("IMGCONVERT_PACKAGE_CONVERT_SMOKE_FORMATS")
        .unwrap_or_else(|_| "jpeg,webp,png,avif".to_string());
    let mut formats = Vec::new();
    for item in raw
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let format = match item.to_ascii_lowercase().as_str() {
            "jpeg" | "jpg" => imgconvert_core::Format::Jpeg,
            "png" => imgconvert_core::Format::Png,
            "webp" => imgconvert_core::Format::WebP,
            "avif" => imgconvert_core::Format::Avif,
            other => return Err(format!("unsupported smoke format: {other}")),
        };
        if !formats.contains(&format) {
            formats.push(format);
        }
    }
    if formats.is_empty() {
        return Err("no smoke formats requested".to_string());
    }
    Ok(formats)
}

fn smoke_output_dir() -> Result<std::path::PathBuf, String> {
    let dir = std::env::var_os("IMGCONVERT_PACKAGE_CONVERT_SMOKE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("imgconvert-package-smoke-{}", std::process::id()))
        });
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("create smoke output dir {} failed: {error}", dir.display()))?;
    Ok(dir)
}
