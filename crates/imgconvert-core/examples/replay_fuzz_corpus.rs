// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::ffi::OsString;
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use imgconvert_core::{
    codec_for, convert_best_of_with_color_policy_timeout, detect_lossy_artifacts,
    inspect_metadata_semantics, normalize_metadata_semantics, probe, thumbnail, AvifSubsample,
    ColorManagementPolicy, EncodeOptions, Format, RawMetadata,
};

const MAX_REPLAY_INPUT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DECODE_PIXELS: u64 = 16_000_000;
const MAX_CONVERT_PIXELS: u64 = 1_000_000;
const MAX_METADATA_REPLAY_BYTES: u64 = 1024 * 1024;

fn main() -> ExitCode {
    match run() {
        Ok(report) => {
            println!("{}", report.to_json());
            if report.failed == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("replay fuzz corpus failed: {error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ReplayReport, String> {
    let config = Config::parse(env::args_os().skip(1))?;
    let targets = [
        Target {
            name: "decode_pipeline",
            kind: TargetKind::Decode,
        },
        Target {
            name: "convert_pipeline",
            kind: TargetKind::Convert,
        },
        Target {
            name: "metadata_semantics",
            kind: TargetKind::Metadata,
        },
    ];

    let mut reports = Vec::with_capacity(targets.len());
    for target in targets {
        let mut inputs = Vec::new();
        collect_files(&config.corpus_root.join(target.name), &mut inputs)?;
        if config.include_artifacts {
            collect_files(&config.artifacts_root.join(target.name), &mut inputs)?;
        }
        inputs.sort();
        inputs.dedup();
        reports.push(replay_target(target, &inputs));
    }

    Ok(ReplayReport::from_targets(reports))
}

struct Config {
    corpus_root: PathBuf,
    artifacts_root: PathBuf,
    include_artifacts: bool,
}

impl Config {
    fn parse(args: impl Iterator<Item = OsString>) -> Result<Self, String> {
        let mut config = Self {
            corpus_root: PathBuf::from("fuzz/corpus"),
            artifacts_root: PathBuf::from("fuzz/artifacts"),
            include_artifacts: true,
        };

        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            let arg = arg
                .into_string()
                .map_err(|_| "arguments must be valid UTF-8".to_string())?;
            match arg.as_str() {
                "--include-artifacts" => config.include_artifacts = true,
                "--no-artifacts" => config.include_artifacts = false,
                "--corpus-root" => {
                    config.corpus_root = next_path_arg(&mut args, "--corpus-root")?;
                }
                "--artifacts-root" => {
                    config.artifacts_root = next_path_arg(&mut args, "--artifacts-root")?;
                }
                "--help" | "-h" => return Err(usage()),
                _ if arg.starts_with("--corpus-root=") => {
                    config.corpus_root = PathBuf::from(&arg["--corpus-root=".len()..]);
                }
                _ if arg.starts_with("--artifacts-root=") => {
                    config.artifacts_root = PathBuf::from(&arg["--artifacts-root=".len()..]);
                }
                _ => return Err(format!("unknown argument: {arg}\n{}", usage())),
            }
        }

        Ok(config)
    }
}

fn next_path_arg(
    args: &mut std::iter::Peekable<impl Iterator<Item = OsString>>,
    label: &str,
) -> Result<PathBuf, String> {
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{label} requires a path"))
}

fn usage() -> String {
    "usage: replay_fuzz_corpus [--corpus-root <path>] [--artifacts-root <path>] [--include-artifacts|--no-artifacts]".to_string()
}

#[derive(Clone, Copy)]
struct Target {
    name: &'static str,
    kind: TargetKind,
}

#[derive(Clone, Copy)]
enum TargetKind {
    Decode,
    Convert,
    Metadata,
}

struct ReplayReport {
    targets: Vec<TargetReport>,
    total_files: u64,
    passed: u64,
    skipped: u64,
    failed: u64,
}

impl ReplayReport {
    fn from_targets(targets: Vec<TargetReport>) -> Self {
        Self {
            total_files: targets.iter().map(|target| target.total_files).sum(),
            passed: targets.iter().map(|target| target.passed).sum(),
            skipped: targets.iter().map(|target| target.skipped).sum(),
            failed: targets.iter().map(|target| target.failed).sum(),
            targets,
        }
    }

    fn to_json(&self) -> String {
        let targets = self
            .targets
            .iter()
            .map(TargetReport::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"schemaVersion\":1,\"totalFiles\":{},\"passed\":{},\"skipped\":{},\"failed\":{},\"targets\":[{}]}}",
            self.total_files, self.passed, self.skipped, self.failed, targets
        )
    }
}

struct TargetReport {
    name: &'static str,
    total_files: u64,
    passed: u64,
    skipped: u64,
    failed: u64,
    failures: Vec<Failure>,
}

impl TargetReport {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            total_files: 0,
            passed: 0,
            skipped: 0,
            failed: 0,
            failures: Vec::new(),
        }
    }

    fn to_json(&self) -> String {
        let failures = self
            .failures
            .iter()
            .map(Failure::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"name\":{},\"totalFiles\":{},\"passed\":{},\"skipped\":{},\"failed\":{},\"failures\":[{}]}}",
            json_string(self.name),
            self.total_files,
            self.passed,
            self.skipped,
            self.failed,
            failures
        )
    }
}

struct Failure {
    path: String,
    kind: &'static str,
    message: String,
}

impl Failure {
    fn to_json(&self) -> String {
        format!(
            "{{\"path\":{},\"kind\":{},\"message\":{}}}",
            json_string(&self.path),
            json_string(self.kind),
            json_string(&self.message)
        )
    }
}

fn replay_target(target: Target, inputs: &[PathBuf]) -> TargetReport {
    let mut report = TargetReport::new(target.name);
    for path in inputs {
        report.total_files += 1;
        match replay_file(target.kind, path) {
            ReplayOutcome::Passed => report.passed += 1,
            ReplayOutcome::Skipped => report.skipped += 1,
            ReplayOutcome::Failed { kind, message } => {
                report.failed += 1;
                report.failures.push(Failure {
                    path: display_path(path),
                    kind,
                    message,
                });
            }
        }
    }
    report
}

enum ReplayOutcome {
    Passed,
    Skipped,
    Failed { kind: &'static str, message: String },
}

fn replay_file(target: TargetKind, path: &Path) -> ReplayOutcome {
    let Ok(metadata) = fs::metadata(path) else {
        return ReplayOutcome::Failed {
            kind: "read",
            message: "could not read metadata".to_string(),
        };
    };
    let max_bytes = match target {
        TargetKind::Decode | TargetKind::Convert => MAX_REPLAY_INPUT_BYTES,
        TargetKind::Metadata => MAX_METADATA_REPLAY_BYTES,
    };
    if metadata.len() > max_bytes {
        return ReplayOutcome::Skipped;
    }

    let data = match fs::read(path) {
        Ok(data) => data,
        Err(error) => {
            return ReplayOutcome::Failed {
                kind: "read",
                message: error.to_string(),
            }
        }
    };

    let result = catch_unwind(AssertUnwindSafe(|| match target {
        TargetKind::Decode => replay_decode_pipeline(&data),
        TargetKind::Convert => replay_convert_pipeline(&data),
        TargetKind::Metadata => replay_metadata_semantics(&data),
    }));

    match result {
        Ok(Ok(())) => ReplayOutcome::Passed,
        Ok(Err(message)) => ReplayOutcome::Failed {
            kind: "invariant",
            message,
        },
        Err(_) => ReplayOutcome::Failed {
            kind: "panic",
            message: "target panicked".to_string(),
        },
    }
}

fn replay_decode_pipeline(data: &[u8]) -> Result<(), String> {
    let Some(format) = Format::from_magic(data) else {
        return Ok(());
    };
    let Ok(info) = probe(data) else {
        return Ok(());
    };
    if u64::from(info.width) * u64::from(info.height) > MAX_DECODE_PIXELS {
        return Ok(());
    }

    let _ = thumbnail(data, 256);
    if format == Format::Png {
        let _ = detect_lossy_artifacts(data);
    }
    let _ = codec_for(format).decode(data);
    Ok(())
}

fn replay_convert_pipeline(data: &[u8]) -> Result<(), String> {
    let Ok(info) = probe(data) else {
        return Ok(());
    };
    if u64::from(info.width) * u64::from(info.height) > MAX_CONVERT_PIXELS {
        return Ok(());
    }

    let target = match data[0] & 0b11 {
        0 => Format::Png,
        1 => Format::Jpeg,
        2 => Format::WebP,
        _ => Format::Avif,
    };
    let options = EncodeOptions {
        quality: 82,
        lossless: matches!(target, Format::Png | Format::WebP) && (data[1] & 1 == 1),
        png_oxipng_level: 0,
        webp_method: 0,
        avif_speed: 10,
        avif_subsample: AvifSubsample::Yuv444,
        preserve_metadata: data[2] & 1 == 1,
        ..EncodeOptions::default()
    };

    if let Ok(encoded) = convert_best_of_with_color_policy_timeout(
        data,
        target,
        &[options],
        None,
        ColorManagementPolicy::PreserveEmbeddedProfile,
        Duration::from_secs(2),
    ) {
        let actual = Format::from_magic(&encoded);
        if actual != Some(target) {
            return Err(format!(
                "encoded output magic mismatch: expected {}, got {:?}",
                target.id(),
                actual.map(Format::id)
            ));
        }
        let _ = probe(&encoded);
    }
    Ok(())
}

fn replay_metadata_semantics(data: &[u8]) -> Result<(), String> {
    let first = data.len() / 3;
    let second = first * 2;
    let metadata = RawMetadata {
        icc: None,
        exif: Some(data[..first].to_vec()),
        xmp: Some(data[first..second].to_vec()),
        iptc: Some(data[second..].to_vec()),
    };

    let _ = inspect_metadata_semantics(&metadata);
    let normalized = normalize_metadata_semantics(metadata);
    let _ = inspect_metadata_semantics(&normalized);
    Ok(())
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }

    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries =
            fs::read_dir(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("{}: {error}", path.display()))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file() {
                files.push(path);
            }
        }
    }
    Ok(())
}

fn display_path(path: &Path) -> String {
    if let Ok(cwd) = env::current_dir() {
        if let Ok(relative) = path.strip_prefix(cwd) {
            return relative.display().to_string();
        }
    }
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<unknown>")
        .to_string()
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}
