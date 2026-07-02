// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 The ImgConvert Authors

//! ImgConvert 进程内编解码核心。
//!
//! 设计(参考 slimg,MIT):统一中间表示 `ImageData`(RGBA8)+ `Codec` trait +
//! `Format` 检测 + 顶层 `convert` 管线。P0.5 已跑通 **JPEG / PNG / WebP / AVIF**。
//! HEIC(系统原生)、更深层的色彩/元数据语义处理为后续尖刺/阶段。
//!
//! 色深:v1 仅 **8-bit SDR**(16-bit/HDR 推后,见 docs/ENGINE.md §1)。

use std::fmt;
use std::io::{Cursor, Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};

use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use image::ImageDecoder;
use ssimulacra2::{compute_frame_ssimulacra2, ColorPrimaries, Rgb, TransferCharacteristic, Xyb};

/// 像素数上限(防超大分配 / C 层 OOM;~100MP,可后续配置)。
pub const MAX_PIXELS: usize = 100_000_000;

/// JPEG 不支持 alpha:透明像素合成到此背景色(P0.5 固定白底,背景色 P2 再配置)。
const JPEG_FLATTEN_BG: [u8; 3] = [255, 255, 255];

/// AVIF 编码器内部线程上限。文件级并发由 Tauri 层控制,避免两层并发叠加。
pub const AVIF_ENCODER_MAX_THREADS: i32 = 1;

/// 当前 AVIF 后端是否已验证像素级真无损。rav1e 后端尚不能声明该能力。
pub const AVIF_LOSSLESS_SUPPORTED: bool = false;

/// 自动质量每次搜索的质量间隔。最终高位质量始终会被纳入候选。
pub const AUTO_QUALITY_STEP: u8 = 4;

/// 自动质量最坏情况下的 SSIMULACRA2 评分次数:JPEG 最多 6 次,WebP 额外比较一次 lossless。
pub const AUTO_QUALITY_MAX_SCORING_EVALUATIONS: usize = 7;

const JPEG_GRID_ARTIFACT_MIN_DIMENSION: u32 = 32;
const JPEG_GRID_ARTIFACT_MIN_BOUNDARY_DELTA: f64 = 2.0;
const JPEG_GRID_ARTIFACT_SCORE_THRESHOLD: f64 = 1.6;

/// 校验尺寸并返回期望的 RGBA8 字节数(checked,拒绝 0 / 溢出 / 超上限)。
fn rgba_byte_len(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 {
        return Err(Error::Invalid("尺寸不能为 0".into()));
    }
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| Error::Invalid("width*height 溢出".into()))?;
    if pixels > MAX_PIXELS {
        return Err(Error::Unsupported(format!(
            "像素数 {pixels} 超过上限 {MAX_PIXELS}"
        )));
    }
    pixels
        .checked_mul(4)
        .ok_or_else(|| Error::Invalid("RGBA 缓冲长度溢出".into()))
}

/// 统一中间像素表示:RGBA8,行优先,长度 = width*height*4。
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// ICC profile bytes, without container-specific wrappers.
    pub icc: Option<Vec<u8>>,
    /// EXIF TIFF payload, without JPEG `Exif\0\0` prefix.
    pub exif: Option<Vec<u8>>,
    /// Raw XMP packet bytes, without container-specific wrappers.
    pub xmp: Option<Vec<u8>>,
}

impl ImageData {
    /// 校验后构造(保证不变量:尺寸非 0、无溢出、长度匹配、不超上限)。
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self> {
        let expected = rgba_byte_len(width, height)?;
        if rgba.len() != expected {
            return Err(Error::Invalid(format!(
                "RGBA 长度 {} != 期望 {expected}（{width}x{height}）",
                rgba.len()
            )));
        }
        Ok(Self {
            width,
            height,
            rgba,
            icc: None,
            exif: None,
            xmp: None,
        })
    }

    /// 校验当前不变量(编码入口在跨 C 边界前调用,防 panic/越界)。
    pub fn validate(&self) -> Result<()> {
        let expected = rgba_byte_len(self.width, self.height)?;
        if self.rgba.len() != expected {
            return Err(Error::Invalid(format!(
                "ImageData 不变量破坏:RGBA 长度 {} != {expected}",
                self.rgba.len()
            )));
        }
        Ok(())
    }

    /// 把 RGBA 合成到不透明背景,产出 RGB(供 JPEG 等无 alpha 格式使用)。
    fn flatten_to_rgb(&self, bg: [u8; 3]) -> Vec<u8> {
        let mut rgb = Vec::with_capacity(self.width as usize * self.height as usize * 3);
        for px in self.rgba.chunks_exact(4) {
            let a = px[3] as u32;
            let inv = 255 - a;
            for c in 0..3 {
                // out = src*a/255 + bg*(255-a)/255,整数运算
                rgb.push(((px[c] as u32 * a + bg[c] as u32 * inv) / 255) as u8);
            }
        }
        rgb
    }
}

/// 不解码像素的图片头部探测结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageProbe {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub dpi: Option<Dpi>,
}

/// 图片声明的物理分辨率。并非所有容器都会提供该信息。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dpi {
    pub x: f64,
    pub y: f64,
}

/// 缩略图编码结果。当前统一输出 PNG,便于前端以 Blob URL 直接显示。
#[derive(Debug, Clone, PartialEq)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub png: Vec<u8>,
}

/// 支持的格式(P0.5 范围)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Jpeg,
    Png,
    WebP,
    Avif,
}

/// 进程内 core 当前可读格式。
pub const READABLE_FORMATS: &[Format] = &[Format::Jpeg, Format::Png, Format::WebP, Format::Avif];

/// 进程内 core 当前可写格式。
pub const WRITABLE_FORMATS: &[Format] = &[Format::Jpeg, Format::Png, Format::WebP, Format::Avif];

/// 支持真无损编码选项的格式(JPEG/AVIF 当前实现不声明无损)。
pub const LOSSLESS_FORMATS: &[Format] = &[Format::Png, Format::WebP];

impl Format {
    /// 稳定的前后端格式 id。
    pub const fn id(self) -> &'static str {
        match self {
            Format::Jpeg => "jpeg",
            Format::Png => "png",
            Format::WebP => "webp",
            Format::Avif => "avif",
        }
    }

    /// 默认输出文件扩展名。
    pub const fn default_extension(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Png => "png",
            Format::WebP => "webp",
            Format::Avif => "avif",
        }
    }

    /// 当前 encoder 是否支持无损选项。
    pub const fn supports_lossless(self) -> bool {
        matches!(self, Format::Png | Format::WebP)
    }

    /// 按 magic bytes 检测(优先于扩展名)。
    pub fn from_magic(bytes: &[u8]) -> Option<Format> {
        if bytes.len() >= 3 && bytes[0..3] == [0xFF, 0xD8, 0xFF] {
            return Some(Format::Jpeg);
        }
        if bytes.len() >= 8 && bytes[0..8] == [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'] {
            return Some(Format::Png);
        }
        // RIFF....WEBP + 首个 chunk FourCC(VP8 / VP8L / VP8X),避免误判普通 RIFF。
        if bytes.len() >= 16
            && &bytes[0..4] == b"RIFF"
            && &bytes[8..12] == b"WEBP"
            && matches!(&bytes[12..16], b"VP8 " | b"VP8L" | b"VP8X")
        {
            return Some(Format::WebP);
        }
        // AVIF:ISO-BMFF `ftyp` box,品牌列表含 avif/avis。
        if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
            let end = bytes.len().min(32);
            if bytes[8..end]
                .windows(4)
                .any(|w| w == b"avif" || w == b"avis")
            {
                return Some(Format::Avif);
            }
        }
        None
    }

    /// 按扩展名检测(magic 失败时的回退)。
    pub fn from_ext(ext: &str) -> Option<Format> {
        match ext.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Some(Format::Jpeg),
            "png" => Some(Format::Png),
            "webp" => Some(Format::WebP),
            "avif" => Some(Format::Avif),
            _ => None,
        }
    }
}

/// 编码参数。
#[derive(Debug, Clone, Copy)]
pub struct EncodeOptions {
    /// 质量 1..=100(有损格式有效)。
    pub quality: u8,
    /// 真无损(对 WebP 生效;PNG 恒无损;JPEG/AVIF 当前忽略)。
    pub lossless: bool,
    /// JPEG 是否使用 progressive scan。
    pub jpeg_progressive: bool,
    /// oxipng 优化级别 0..=6。
    pub png_oxipng_level: u8,
    /// 实验性 PNG 有损限色。默认关闭。
    pub png_lossy_quantize: bool,
    /// PNG 限色颜色数 64..=256。
    pub png_quant_colors: u16,
    /// WebP method 0..=6。
    pub webp_method: u8,
    /// AVIF speed 0..=10(越大越快)。
    pub avif_speed: u8,
    /// AVIF 色度采样。默认 4:4:4 保持当前保真语义。
    pub avif_subsample: AvifSubsample,
    /// WebP near-lossless 0..=100。100 表示关闭 near-lossless 预处理。
    pub webp_near_lossless: u8,
    /// WebP 使用更慢但更锐利的 RGB→YUV 转换。
    pub webp_sharp_yuv: bool,
    /// MozJPEG trellis 是否把 progressive scans 纳入考虑。
    pub jpeg_trellis: bool,
    /// 是否保留 ICC/EXIF 等容器元数据。
    pub preserve_metadata: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvifSubsample {
    Yuv444,
    Yuv420,
}

/// 自动质量搜索参数。
#[derive(Debug, Clone, Copy)]
pub struct AutoQualityOptions {
    /// 搜索最低质量。调用侧应传入全局/格式质量下限；内部会 clamp 到 1..=100。
    pub min_quality: u8,
    /// SSIMULACRA2 目标分。越高越接近源图。
    pub target_score: f64,
}

impl Default for AutoQualityOptions {
    fn default() -> Self {
        Self {
            min_quality: 30,
            target_score: 80.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutoQualityResult {
    pub bytes: Vec<u8>,
    pub quality: u8,
    pub score: Option<f64>,
    pub used_lossless: bool,
}

/// 对无损容器里疑似有损来源痕迹的轻量提示。当前仅检测 PNG 中的 JPEG 8x8 网格。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LossyArtifactHint {
    pub format: Format,
    pub jpeg_grid_score: f64,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            quality: 80,
            lossless: false,
            jpeg_progressive: true,
            png_oxipng_level: 4,
            png_lossy_quantize: false,
            png_quant_colors: 256,
            webp_method: 4,
            avif_speed: 8,
            avif_subsample: AvifSubsample::Yuv444,
            webp_near_lossless: 100,
            webp_sharp_yuv: false,
            jpeg_trellis: true,
            preserve_metadata: false,
        }
    }
}

impl EncodeOptions {
    fn quality_clamped(&self) -> f32 {
        self.quality.clamp(1, 100) as f32
    }

    fn png_oxipng_level_clamped(&self) -> u8 {
        self.png_oxipng_level.min(6)
    }

    fn png_quant_colors_clamped(&self) -> usize {
        usize::from(self.png_quant_colors.clamp(64, 256))
    }

    fn webp_method_clamped(&self) -> i32 {
        self.webp_method.min(6) as i32
    }

    fn avif_speed_clamped(&self) -> i32 {
        self.avif_speed.min(10) as i32
    }

    fn webp_near_lossless_clamped(&self) -> i32 {
        self.webp_near_lossless.min(100) as i32
    }

    fn avif_pixel_format(&self) -> avif::avifPixelFormat {
        match self.avif_subsample {
            AvifSubsample::Yuv444 => avif::AVIF_PIXEL_FORMAT_YUV444,
            AvifSubsample::Yuv420 => avif::AVIF_PIXEL_FORMAT_YUV420,
        }
    }
}

/// 核心错误。
#[derive(Debug)]
pub enum Error {
    Decode(String),
    Encode(String),
    /// 输入不变量错误(尺寸/长度/溢出)。
    Invalid(String),
    Unsupported(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Decode(m) => write!(f, "解码失败: {m}"),
            Error::Encode(m) => write!(f, "编码失败: {m}"),
            Error::Invalid(m) => write!(f, "非法输入: {m}"),
            Error::Unsupported(m) => write!(f, "不支持: {m}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// 编解码器接口。
pub trait Codec {
    fn decode(&self, bytes: &[u8]) -> Result<ImageData>;
    fn encode(&self, img: &ImageData, opts: &EncodeOptions) -> Result<Vec<u8>>;
}

/// 取某格式的编解码器(ZST,Box 不实际堆分配)。
pub fn codec_for(format: Format) -> Box<dyn Codec> {
    match format {
        Format::Jpeg => Box::new(JpegCodec),
        Format::Png => Box::new(PngCodec),
        Format::WebP => Box::new(WebpCodec),
        Format::Avif => Box::new(AvifCodec),
    }
}

/// 顶层管线:输入字节 →(检测)→ 解码 → 编码到目标格式。
pub fn convert(input: &[u8], target: Format, opts: &EncodeOptions) -> Result<Vec<u8>> {
    convert_best_of(input, target, &[*opts])
}

/// 多候选管线:输入只解码一次,然后用多个编码参数竞争,返回体积最小的候选。
pub fn convert_best_of(input: &[u8], target: Format, options: &[EncodeOptions]) -> Result<Vec<u8>> {
    let src =
        Format::from_magic(input).ok_or_else(|| Error::Unsupported("无法识别输入格式".into()))?;
    let img = codec_for(src).decode(input)?;
    encode_best_of(&img, target, options)
}

/// 自动质量管线:仅 JPEG/WebP。输入只解码一次,按 SSIMULACRA2 找到达目标分的最低质量。
pub fn convert_auto_quality(
    input: &[u8],
    target: Format,
    options: &[EncodeOptions],
    auto: &AutoQualityOptions,
) -> Result<AutoQualityResult> {
    if !matches!(target, Format::Jpeg | Format::WebP) {
        return Err(Error::Unsupported("自动质量仅支持 JPEG/WebP".into()));
    }
    let src =
        Format::from_magic(input).ok_or_else(|| Error::Unsupported("无法识别输入格式".into()))?;
    let img = codec_for(src).decode(input)?;
    encode_auto_quality(&img, target, options, auto)
}

fn encode_best_of(img: &ImageData, target: Format, options: &[EncodeOptions]) -> Result<Vec<u8>> {
    if options.is_empty() {
        return Err(Error::Invalid("至少需要一个编码候选".into()));
    }

    let codec = codec_for(target);
    let mut best: Option<Vec<u8>> = None;
    for opts in options {
        let candidate = codec.encode(img, opts)?;
        if best
            .as_ref()
            .is_none_or(|current| candidate.len() < current.len())
        {
            best = Some(candidate);
        }
    }
    best.ok_or_else(|| Error::Invalid("没有可用编码候选".into()))
}

fn encode_auto_quality(
    img: &ImageData,
    target: Format,
    options: &[EncodeOptions],
    auto: &AutoQualityOptions,
) -> Result<AutoQualityResult> {
    if options.is_empty() {
        return Err(Error::Invalid("至少需要一个编码候选".into()));
    }

    if img.width < 8 || img.height < 8 {
        let bytes = encode_best_of(img, target, options)?;
        return Ok(AutoQualityResult {
            bytes,
            quality: highest_candidate_quality(options),
            score: None,
            used_lossless: options.iter().any(|opts| opts.lossless),
        });
    }

    let min_quality = auto.min_quality.clamp(1, 100);
    let max_quality = highest_candidate_quality(options).max(min_quality);
    let levels = auto_quality_levels(min_quality, max_quality);
    let mut low = 0usize;
    let mut high = levels.len();
    let mut best: Option<ScoredCandidate> = None;
    let mut max_candidate: Option<ScoredCandidate> = None;

    while low < high {
        let mid = low + (high - low) / 2;
        let quality = levels[mid];
        let candidate = encode_scored_quality_candidate(img, target, options, quality)?;
        if candidate.score >= auto.target_score {
            best = Some(candidate);
            high = mid;
        } else {
            if quality == max_quality {
                max_candidate = Some(candidate);
            }
            low = mid + 1;
        }
    }

    let mut selected = if let Some(best) = best {
        best
    } else if let Some(max_candidate) = max_candidate {
        max_candidate
    } else {
        encode_scored_quality_candidate(img, target, options, max_quality)?
    };

    if target == Format::WebP && !options.iter().any(|opts| opts.lossless) {
        let lossless = encode_lossless_webp_candidate(img, options)?;
        if lossless.bytes.len() < selected.bytes.len() {
            selected = lossless;
        }
    }

    Ok(AutoQualityResult {
        bytes: selected.bytes,
        quality: selected.quality,
        score: Some(selected.score),
        used_lossless: selected.used_lossless,
    })
}

#[derive(Debug)]
struct ScoredCandidate {
    bytes: Vec<u8>,
    quality: u8,
    score: f64,
    used_lossless: bool,
}

fn highest_candidate_quality(options: &[EncodeOptions]) -> u8 {
    options.iter().map(|opts| opts.quality).max().unwrap_or(80)
}

fn auto_quality_levels(min_quality: u8, max_quality: u8) -> Vec<u8> {
    let min_quality = min_quality.min(max_quality);
    let mut levels = Vec::new();
    let mut quality = min_quality;
    while quality < max_quality {
        levels.push(quality);
        quality = quality.saturating_add(AUTO_QUALITY_STEP).min(max_quality);
    }
    if levels.last().copied() != Some(max_quality) {
        levels.push(max_quality);
    }
    levels
}

fn auto_quality_lossy_scoring_evaluation_limit(min_quality: u8, max_quality: u8) -> usize {
    let level_count =
        auto_quality_levels(min_quality.clamp(1, 100), max_quality.clamp(1, 100)).len();
    if level_count <= 1 {
        return level_count;
    }

    let mut span = level_count;
    let mut evaluations = 0usize;
    while span > 0 {
        evaluations += 1;
        span /= 2;
    }
    evaluations.saturating_add(1).min(level_count)
}

pub fn auto_quality_scoring_evaluation_limit(
    target: Format,
    min_quality: u8,
    max_quality: u8,
) -> usize {
    let lossy = auto_quality_lossy_scoring_evaluation_limit(min_quality, max_quality);
    if target == Format::WebP {
        lossy.saturating_add(1)
    } else {
        lossy
    }
}

fn encode_scored_quality_candidate(
    img: &ImageData,
    target: Format,
    options: &[EncodeOptions],
    quality: u8,
) -> Result<ScoredCandidate> {
    let quality_options = options
        .iter()
        .map(|opts| {
            let mut candidate = *opts;
            candidate.quality = quality;
            candidate.lossless = false;
            candidate
        })
        .collect::<Vec<_>>();
    let bytes = encode_best_of(img, target, &quality_options)?;
    let decoded = codec_for(target).decode(&bytes)?;
    let score = ssimulacra2_score(img, &decoded)?;
    Ok(ScoredCandidate {
        bytes,
        quality,
        score,
        used_lossless: false,
    })
}

fn encode_lossless_webp_candidate(
    img: &ImageData,
    options: &[EncodeOptions],
) -> Result<ScoredCandidate> {
    let lossless_options = options
        .iter()
        .map(|opts| {
            let mut candidate = *opts;
            candidate.quality = 100;
            candidate.lossless = true;
            candidate.webp_near_lossless = 100;
            candidate
        })
        .collect::<Vec<_>>();
    let bytes = encode_best_of(img, Format::WebP, &lossless_options)?;
    let decoded = WebpCodec.decode(&bytes)?;
    let score = ssimulacra2_score(img, &decoded)?;
    Ok(ScoredCandidate {
        bytes,
        quality: 100,
        score,
        used_lossless: true,
    })
}

fn ssimulacra2_score(source: &ImageData, distorted: &ImageData) -> Result<f64> {
    if source.width != distorted.width || source.height != distorted.height {
        return Err(Error::Encode("自动质量评分要求候选尺寸与源图一致".into()));
    }
    let source_frame = ssimulacra2_frame(source)?;
    let distorted_frame = ssimulacra2_frame(distorted)?;
    compute_frame_ssimulacra2(source_frame, distorted_frame)
        .map_err(|e| Error::Encode(format!("SSIMULACRA2 评分失败: {e}")))
}

fn ssimulacra2_frame(img: &ImageData) -> Result<Xyb> {
    let data = composited_srgb_for_score(img);
    let rgb = Rgb::new(
        data,
        img.width as usize,
        img.height as usize,
        TransferCharacteristic::SRGB,
        ColorPrimaries::BT709,
    )
    .map_err(|e| Error::Encode(format!("构造 SSIMULACRA2 RGB 帧失败: {e:?}")))?;
    Xyb::try_from(rgb).map_err(|e| Error::Encode(format!("转换 SSIMULACRA2 XYB 帧失败: {e:?}")))
}

fn composited_srgb_for_score(img: &ImageData) -> Vec<[f32; 3]> {
    let mut rgb = Vec::with_capacity(img.rgba.len() / 4);
    for pixel in img.rgba.chunks_exact(4) {
        let alpha = f32::from(pixel[3]) / 255.0;
        let inv_alpha = 1.0 - alpha;
        rgb.push([
            (f32::from(pixel[0]) * alpha + 255.0 * inv_alpha) / 255.0,
            (f32::from(pixel[1]) * alpha + 255.0 * inv_alpha) / 255.0,
            (f32::from(pixel[2]) * alpha + 255.0 * inv_alpha) / 255.0,
        ]);
    }
    rgb
}

/// 探测无损容器中是否有明显 JPEG 8x8 网格痕迹。该结果只应作为保守策略 hint。
pub fn detect_lossy_artifacts(input: &[u8]) -> Result<Option<LossyArtifactHint>> {
    let format =
        Format::from_magic(input).ok_or_else(|| Error::Unsupported("无法识别输入格式".into()))?;
    if format != Format::Png {
        return Ok(None);
    }

    let img = codec_for(format).decode(input)?;
    Ok(jpeg_grid_artifact_score(&img).and_then(|score| {
        (score >= JPEG_GRID_ARTIFACT_SCORE_THRESHOLD).then_some(LossyArtifactHint {
            format,
            jpeg_grid_score: score,
        })
    }))
}

fn jpeg_grid_artifact_score(img: &ImageData) -> Option<f64> {
    if img.width < JPEG_GRID_ARTIFACT_MIN_DIMENSION || img.height < JPEG_GRID_ARTIFACT_MIN_DIMENSION
    {
        return None;
    }

    let width = img.width as usize;
    let height = img.height as usize;
    let mut boundary_sum = 0u64;
    let mut boundary_count = 0u64;
    let mut interior_sum = 0u64;
    let mut interior_count = 0u64;

    for y in 0..height {
        for x in 1..width {
            let delta = luma_delta(img, x, y, x - 1, y);
            if x % 8 == 0 {
                boundary_sum += delta;
                boundary_count += 1;
            } else {
                interior_sum += delta;
                interior_count += 1;
            }
        }
    }

    for y in 1..height {
        for x in 0..width {
            let delta = luma_delta(img, x, y, x, y - 1);
            if y % 8 == 0 {
                boundary_sum += delta;
                boundary_count += 1;
            } else {
                interior_sum += delta;
                interior_count += 1;
            }
        }
    }

    if boundary_count == 0 || interior_count == 0 {
        return None;
    }

    let boundary_avg = boundary_sum as f64 / boundary_count as f64;
    let interior_avg = interior_sum as f64 / interior_count as f64;
    if boundary_avg < JPEG_GRID_ARTIFACT_MIN_BOUNDARY_DELTA || interior_avg <= 0.0 {
        return None;
    }
    Some(boundary_avg / interior_avg.max(1.0))
}

fn luma_delta(img: &ImageData, ax: usize, ay: usize, bx: usize, by: usize) -> u64 {
    let a = composited_luma(pixel_at_unchecked(img, ax, ay));
    let b = composited_luma(pixel_at_unchecked(img, bx, by));
    u64::from(a.abs_diff(b))
}

fn pixel_at_unchecked(img: &ImageData, x: usize, y: usize) -> &[u8] {
    let offset = (y * img.width as usize + x) * 4;
    &img.rgba[offset..offset + 4]
}

fn composited_luma(pixel: &[u8]) -> u16 {
    let alpha = u32::from(pixel[3]);
    let inv_alpha = 255 - alpha;
    let r = (u32::from(pixel[0]) * alpha + 255 * inv_alpha) / 255;
    let g = (u32::from(pixel[1]) * alpha + 255 * inv_alpha) / 255;
    let b = (u32::from(pixel[2]) * alpha + 255 * inv_alpha) / 255;
    ((77 * r + 150 * g + 29 * b + 128) / 256) as u16
}

/// 读取图片头部元数据,用于导入阶段的尺寸/DPI ping。
pub fn probe(input: &[u8]) -> Result<ImageProbe> {
    let format =
        Format::from_magic(input).ok_or_else(|| Error::Unsupported("无法识别输入格式".into()))?;
    let (width, height, dpi) = match format {
        Format::Jpeg => probe_jpeg(input)?,
        Format::Png => probe_png(input)?,
        Format::WebP => probe_webp(input)?,
        Format::Avif => probe_avif(input)?,
    };
    rgba_byte_len(width, height)?;
    Ok(ImageProbe {
        format,
        width,
        height,
        dpi,
    })
}

/// 生成最长边不超过 `max_edge` 的 PNG 缩略图。全透明图片返回 `Ok(None)`。
pub fn thumbnail(input: &[u8], max_edge: u32) -> Result<Option<Thumbnail>> {
    let src =
        Format::from_magic(input).ok_or_else(|| Error::Unsupported("无法识别输入格式".into()))?;
    let img = codec_for(src).decode(input)?;
    if img.rgba.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        return Ok(None);
    }

    let max_edge = max_edge.clamp(32, 512);
    let (width, height) = thumbnail_dimensions(img.width, img.height, max_edge);
    let resized = if width == img.width && height == img.height {
        image::RgbaImage::from_raw(img.width, img.height, img.rgba)
            .ok_or_else(|| Error::Invalid("无法构造缩略图像素缓冲".into()))?
    } else {
        let source = image::RgbaImage::from_raw(img.width, img.height, img.rgba)
            .ok_or_else(|| Error::Invalid("无法构造缩略图像素缓冲".into()))?;
        image::imageops::resize(
            &source,
            width,
            height,
            image::imageops::FilterType::Triangle,
        )
    };
    let png = encode_png_rgba(resized.width(), resized.height(), resized.as_raw())?;
    Ok(Some(Thumbnail { width, height, png }))
}

fn thumbnail_dimensions(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    if width <= max_edge && height <= max_edge {
        return (width, height);
    }

    if width >= height {
        let scaled_height = ((height as u64 * max_edge as u64) + (width as u64 / 2)) / width as u64;
        (max_edge, scaled_height.max(1) as u32)
    } else {
        let scaled_width = ((width as u64 * max_edge as u64) + (height as u64 / 2)) / height as u64;
        (scaled_width.max(1) as u32, max_edge)
    }
}

fn encode_png_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>> {
    use image::ImageEncoder;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| Error::Encode(e.to_string()))?;
    Ok(png)
}

fn be_u16(bytes: &[u8]) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(0..2)?.try_into().ok()?))
}

fn be_u32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(0..4)?.try_into().ok()?))
}

fn le_u32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?))
}

fn probe_png(bytes: &[u8]) -> Result<(u32, u32, Option<Dpi>)> {
    if bytes.len() < 33 || &bytes[12..16] != b"IHDR" {
        return Err(Error::Decode("PNG 头不完整".into()));
    }
    let width = be_u32(&bytes[16..20]).ok_or_else(|| Error::Decode("PNG 宽度缺失".into()))?;
    let height = be_u32(&bytes[20..24]).ok_or_else(|| Error::Decode("PNG 高度缺失".into()))?;

    let mut dpi = None;
    let mut offset = 8usize;
    while offset + 8 <= bytes.len() {
        let length = be_u32(&bytes[offset..offset + 4])
            .ok_or_else(|| Error::Decode("PNG chunk 长度缺失".into()))?
            as usize;
        let chunk = &bytes[offset + 4..offset + 8];
        let data_start = offset + 8;
        let Some(data_end) = data_start.checked_add(length) else {
            break;
        };
        let Some(next_offset) = data_end.checked_add(4) else {
            break;
        };
        if next_offset > bytes.len() {
            break;
        }

        if chunk == b"pHYs" && length == 9 {
            let data = &bytes[data_start..data_end];
            if data[8] == 1 {
                let pixels_per_meter_x = be_u32(&data[0..4]).unwrap_or(0);
                let pixels_per_meter_y = be_u32(&data[4..8]).unwrap_or(0);
                if pixels_per_meter_x > 0 && pixels_per_meter_y > 0 {
                    dpi = Some(Dpi {
                        x: pixels_per_meter_x as f64 * 0.0254,
                        y: pixels_per_meter_y as f64 * 0.0254,
                    });
                }
            }
        }
        if chunk == b"IDAT" || chunk == b"IEND" {
            break;
        }
        offset = next_offset;
    }

    Ok((width, height, dpi))
}

fn probe_jpeg(bytes: &[u8]) -> Result<(u32, u32, Option<Dpi>)> {
    if bytes.len() < 4 || bytes[0..2] != [0xff, 0xd8] {
        return Err(Error::Decode("JPEG 头不完整".into()));
    }

    let mut offset = 2usize;
    let mut dpi = None;
    let mut dimensions = None;
    let mut orientation = image::metadata::Orientation::NoTransforms;
    while offset < bytes.len() {
        while offset < bytes.len() && bytes[offset] != 0xff {
            offset += 1;
        }
        while offset < bytes.len() && bytes[offset] == 0xff {
            offset += 1;
        }
        if offset >= bytes.len() {
            break;
        }

        let marker = bytes[offset];
        offset += 1;
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if offset + 2 > bytes.len() {
            break;
        }

        let segment_length = be_u16(&bytes[offset..offset + 2])
            .ok_or_else(|| Error::Decode("JPEG segment 长度缺失".into()))?
            as usize;
        if segment_length < 2 {
            return Err(Error::Decode("JPEG segment 长度非法".into()));
        }
        let data_start = offset + 2;
        let data_length = segment_length - 2;
        let Some(data_end) = data_start.checked_add(data_length) else {
            break;
        };
        if data_end > bytes.len() {
            break;
        }
        let data = &bytes[data_start..data_end];

        if marker == 0xe0 && dpi.is_none() {
            dpi = parse_jfif_dpi(data);
        }
        if marker == 0xe1 && data.len() > 6 && &data[0..6] == b"Exif\0\0" {
            if let Some(value) = image::metadata::Orientation::from_exif_chunk(&data[6..]) {
                orientation = value;
            }
        }
        if is_jpeg_sof(marker) {
            if data.len() < 5 {
                return Err(Error::Decode("JPEG SOF 数据不完整".into()));
            }
            let height =
                be_u16(&data[1..3]).ok_or_else(|| Error::Decode("JPEG 高度缺失".into()))?;
            let width = be_u16(&data[3..5]).ok_or_else(|| Error::Decode("JPEG 宽度缺失".into()))?;
            dimensions = Some((width as u32, height as u32));
        }

        offset = data_end;
    }

    let Some((width, height)) = dimensions else {
        return Err(Error::Decode("无法读取 JPEG 尺寸".into()));
    };
    let (width, height) = oriented_dimensions(width, height, orientation);
    Ok((width, height, dpi))
}

fn parse_jfif_dpi(data: &[u8]) -> Option<Dpi> {
    if data.len() < 12 || &data[0..5] != b"JFIF\0" {
        return None;
    }
    let unit = data[7];
    let x_density = be_u16(&data[8..10])?;
    let y_density = be_u16(&data[10..12])?;
    if x_density == 0 || y_density == 0 {
        return None;
    }
    match unit {
        1 => Some(Dpi {
            x: x_density as f64,
            y: y_density as f64,
        }),
        2 => Some(Dpi {
            x: x_density as f64 * 2.54,
            y: y_density as f64 * 2.54,
        }),
        _ => None,
    }
}

fn is_jpeg_sof(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
}

fn probe_webp(bytes: &[u8]) -> Result<(u32, u32, Option<Dpi>)> {
    let feat =
        webp::BitstreamFeatures::new(bytes).ok_or_else(|| Error::Decode("非法 WebP 头".into()))?;
    if feat.has_animation() {
        return Err(Error::Unsupported("动画 WebP 暂不支持".into()));
    }
    Ok((feat.width(), feat.height(), None))
}

fn probe_avif(bytes: &[u8]) -> Result<(u32, u32, Option<Dpi>)> {
    unsafe {
        let decoder = DecoderGuard(avif::avifDecoderCreate());
        if decoder.0.is_null() {
            return Err(Error::Decode("avifDecoderCreate 返回 null".into()));
        }
        if avif::avifDecoderSetIOMemory(decoder.0, bytes.as_ptr(), bytes.len())
            != avif::AVIF_RESULT_OK
        {
            return Err(Error::Decode("avifDecoderSetIOMemory 失败".into()));
        }
        if avif::avifDecoderParse(decoder.0) != avif::AVIF_RESULT_OK {
            return Err(Error::Decode("avifDecoderParse 失败".into()));
        }
        let image = (*decoder.0).image;
        if image.is_null() {
            return Err(Error::Decode("decoder.image 为 null".into()));
        }
        Ok(((*image).width, (*image).height, None))
    }
}

// ---------- 通用解码(image crate)----------

fn decode_via_image(bytes: &[u8], format: image::ImageFormat) -> Result<ImageData> {
    let mut header_limits = image::Limits::default();
    header_limits.max_image_width = Some(MAX_PIXELS as u32);
    header_limits.max_image_height = Some(MAX_PIXELS as u32);
    let mut header_reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    header_reader.limits(header_limits);
    let (width, height) = header_reader
        .into_dimensions()
        .map_err(|e| Error::Decode(e.to_string()))?;
    let expected_rgba_len = rgba_byte_len(width, height)?;

    let mut decode_limits = image::Limits::default();
    decode_limits.max_image_width = Some(width);
    decode_limits.max_image_height = Some(height);
    decode_limits.max_alloc = Some(expected_rgba_len as u64);
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(decode_limits);
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| Error::Decode(e.to_string()))?;
    let orientation = decoder
        .orientation()
        .map_err(|e| Error::Decode(e.to_string()))?;
    let mut img =
        image::DynamicImage::from_decoder(decoder).map_err(|e| Error::Decode(e.to_string()))?;
    img.apply_orientation(orientation);
    let rgba = img.to_rgba8();
    ImageData::new(rgba.width(), rgba.height(), rgba.into_raw())
}

fn oriented_dimensions(
    width: u32,
    height: u32,
    orientation: image::metadata::Orientation,
) -> (u32, u32) {
    match orientation {
        image::metadata::Orientation::Rotate90
        | image::metadata::Orientation::Rotate270
        | image::metadata::Orientation::Rotate90FlipH
        | image::metadata::Orientation::Rotate270FlipH => (height, width),
        _ => (width, height),
    }
}

#[derive(Default)]
struct Metadata {
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
    xmp: Option<Vec<u8>>,
}

impl Metadata {
    fn is_empty(&self) -> bool {
        self.icc.as_ref().is_none_or(Vec::is_empty)
            && self.exif.as_ref().is_none_or(Vec::is_empty)
            && self.xmp.as_ref().is_none_or(Vec::is_empty)
    }
}

fn normalize_exif_orientation(mut exif: Vec<u8>) -> Vec<u8> {
    let _ = image::metadata::Orientation::remove_from_exif_chunk(&mut exif);
    exif
}

fn metadata_from_image_format(bytes: &[u8], format: Format) -> Metadata {
    match format {
        Format::Jpeg => extract_jpeg_metadata(bytes, true),
        Format::Png => extract_png_metadata(bytes, true),
        Format::WebP => extract_webp_metadata(bytes),
        Format::Avif => Metadata::default(),
    }
}

// ---------- JPEG metadata ----------

const JPEG_EXIF_PREFIX: &[u8; 6] = b"Exif\0\0";
const JPEG_ICC_PREFIX: &[u8; 12] = b"ICC_PROFILE\0";
const JPEG_XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const JPEG_APP_DATA_LIMIT: usize = 65_533;
const JPEG_ICC_CHUNK_HEADER: usize = JPEG_ICC_PREFIX.len() + 2;
const JPEG_ICC_CHUNK_PAYLOAD_LIMIT: usize = JPEG_APP_DATA_LIMIT - JPEG_ICC_CHUNK_HEADER;

fn extract_jpeg_metadata(bytes: &[u8], normalize_exif: bool) -> Metadata {
    let mut metadata = Metadata::default();
    if bytes.len() < 4 || bytes[0..2] != [0xff, 0xd8] {
        return metadata;
    }

    let mut icc_chunks: Vec<Option<Vec<u8>>> = Vec::new();
    let mut icc_count = 0usize;
    let mut offset = 2usize;
    while offset < bytes.len() {
        while offset < bytes.len() && bytes[offset] != 0xff {
            offset += 1;
        }
        while offset < bytes.len() && bytes[offset] == 0xff {
            offset += 1;
        }
        if offset >= bytes.len() {
            break;
        }

        let marker = bytes[offset];
        offset += 1;
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if offset + 2 > bytes.len() {
            break;
        }

        let Some(segment_length) = be_u16(&bytes[offset..offset + 2]).map(usize::from) else {
            break;
        };
        if segment_length < 2 {
            break;
        }
        let data_start = offset + 2;
        let data_length = segment_length - 2;
        let Some(data_end) = data_start.checked_add(data_length) else {
            break;
        };
        if data_end > bytes.len() {
            break;
        }
        let data = &bytes[data_start..data_end];

        if marker == 0xe1 && metadata.exif.is_none() && data.starts_with(JPEG_EXIF_PREFIX) {
            let exif = data[JPEG_EXIF_PREFIX.len()..].to_vec();
            metadata.exif = Some(if normalize_exif {
                normalize_exif_orientation(exif)
            } else {
                exif
            });
        } else if marker == 0xe1 && metadata.xmp.is_none() && data.starts_with(JPEG_XMP_PREFIX) {
            let xmp = data[JPEG_XMP_PREFIX.len()..].to_vec();
            if !xmp.is_empty() {
                metadata.xmp = Some(xmp);
            }
        } else if marker == 0xe2
            && data.len() >= JPEG_ICC_CHUNK_HEADER
            && &data[..JPEG_ICC_PREFIX.len()] == JPEG_ICC_PREFIX
        {
            let seq = data[JPEG_ICC_PREFIX.len()] as usize;
            let count = data[JPEG_ICC_PREFIX.len() + 1] as usize;
            if seq > 0 && count > 0 && seq <= count {
                if icc_count != count {
                    icc_count = count;
                    icc_chunks = vec![None; count];
                }
                if icc_chunks[seq - 1].is_none() {
                    icc_chunks[seq - 1] = Some(data[JPEG_ICC_CHUNK_HEADER..].to_vec());
                }
            }
        }

        offset = data_end;
    }

    if icc_count > 0 && icc_chunks.iter().all(Option::is_some) {
        let total = icc_chunks
            .iter()
            .filter_map(Option::as_ref)
            .map(Vec::len)
            .sum();
        let mut icc = Vec::with_capacity(total);
        for chunk in icc_chunks.into_iter().flatten() {
            icc.extend_from_slice(&chunk);
        }
        if !icc.is_empty() {
            metadata.icc = Some(icc);
        }
    }

    metadata
}

fn insert_jpeg_metadata(mut jpeg: Vec<u8>, metadata: &Metadata) -> Result<Vec<u8>> {
    if metadata.is_empty() {
        return Ok(jpeg);
    }
    if jpeg.len() < 2 || jpeg[0..2] != [0xff, 0xd8] {
        return Err(Error::Encode("JPEG SOI 缺失,无法写入元数据".into()));
    }

    let mut app_segments = Vec::new();
    if let Some(exif) = metadata.exif.as_deref().filter(|exif| !exif.is_empty()) {
        let data_len = JPEG_EXIF_PREFIX.len() + exif.len();
        if data_len > JPEG_APP_DATA_LIMIT {
            return Err(Error::Encode("EXIF 超过 JPEG APP1 上限".into()));
        }
        app_segments.extend_from_slice(&[0xff, 0xe1]);
        app_segments.extend_from_slice(&((data_len + 2) as u16).to_be_bytes());
        app_segments.extend_from_slice(JPEG_EXIF_PREFIX);
        app_segments.extend_from_slice(exif);
    }
    if let Some(xmp) = metadata.xmp.as_deref().filter(|xmp| !xmp.is_empty()) {
        let data_len = JPEG_XMP_PREFIX.len() + xmp.len();
        if data_len > JPEG_APP_DATA_LIMIT {
            return Err(Error::Encode("XMP 超过 JPEG APP1 上限".into()));
        }
        app_segments.extend_from_slice(&[0xff, 0xe1]);
        app_segments.extend_from_slice(&((data_len + 2) as u16).to_be_bytes());
        app_segments.extend_from_slice(JPEG_XMP_PREFIX);
        app_segments.extend_from_slice(xmp);
    }
    if let Some(icc) = metadata.icc.as_deref().filter(|icc| !icc.is_empty()) {
        let chunk_count = icc.len().div_ceil(JPEG_ICC_CHUNK_PAYLOAD_LIMIT);
        if chunk_count == 0 || chunk_count > 255 {
            return Err(Error::Encode("ICC 超过 JPEG APP2 分块上限".into()));
        }
        for (index, chunk) in icc.chunks(JPEG_ICC_CHUNK_PAYLOAD_LIMIT).enumerate() {
            let data_len = JPEG_ICC_CHUNK_HEADER + chunk.len();
            app_segments.extend_from_slice(&[0xff, 0xe2]);
            app_segments.extend_from_slice(&((data_len + 2) as u16).to_be_bytes());
            app_segments.extend_from_slice(JPEG_ICC_PREFIX);
            app_segments.push((index + 1) as u8);
            app_segments.push(chunk_count as u8);
            app_segments.extend_from_slice(chunk);
        }
    }

    let mut out = Vec::with_capacity(jpeg.len() + app_segments.len());
    out.extend_from_slice(&jpeg[..2]);
    out.extend_from_slice(&app_segments);
    out.append(&mut jpeg.split_off(2));
    Ok(out)
}

// ---------- PNG metadata ----------

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const PNG_XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";

fn extract_png_metadata(bytes: &[u8], normalize_exif: bool) -> Metadata {
    let mut metadata = Metadata::default();
    if bytes.len() < PNG_SIGNATURE.len() || &bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return metadata;
    }

    let mut offset = PNG_SIGNATURE.len();
    while offset + 8 <= bytes.len() {
        let Some(length) = be_u32(&bytes[offset..offset + 4]).map(|value| value as usize) else {
            break;
        };
        let chunk = &bytes[offset + 4..offset + 8];
        let data_start = offset + 8;
        let Some(data_end) = data_start.checked_add(length) else {
            break;
        };
        let Some(next_offset) = data_end.checked_add(4) else {
            break;
        };
        if next_offset > bytes.len() {
            break;
        }
        let data = &bytes[data_start..data_end];

        if chunk == b"iCCP" && metadata.icc.is_none() {
            metadata.icc = extract_png_iccp(data);
        } else if chunk == b"eXIf" && metadata.exif.is_none() && !data.is_empty() {
            let exif = data.to_vec();
            metadata.exif = Some(if normalize_exif {
                normalize_exif_orientation(exif)
            } else {
                exif
            });
        } else if chunk == b"iTXt" && metadata.xmp.is_none() {
            metadata.xmp = extract_png_xmp_itxt(data);
        }
        if chunk == b"IEND" {
            break;
        }

        offset = next_offset;
    }

    metadata
}

fn extract_png_iccp(data: &[u8]) -> Option<Vec<u8>> {
    let name_end = data.iter().position(|byte| *byte == 0)?;
    let compression_method = *data.get(name_end + 1)?;
    if compression_method != 0 {
        return None;
    }
    let compressed = data.get(name_end + 2..)?;
    let mut decoder = ZlibDecoder::new(compressed);
    let mut icc = Vec::new();
    decoder.read_to_end(&mut icc).ok()?;
    (!icc.is_empty()).then_some(icc)
}

fn extract_png_xmp_itxt(data: &[u8]) -> Option<Vec<u8>> {
    let keyword_end = data.iter().position(|byte| *byte == 0)?;
    if &data[..keyword_end] != PNG_XMP_KEYWORD {
        return None;
    }

    let rest = data.get(keyword_end + 1..)?;
    let compression_flag = *rest.first()?;
    let compression_method = *rest.get(1)?;
    if compression_flag != 0 || compression_method != 0 {
        return None;
    }

    let language_start = 2usize;
    let language_len = rest
        .get(language_start..)?
        .iter()
        .position(|byte| *byte == 0)?;
    let translated_start = language_start + language_len + 1;
    let translated_len = rest
        .get(translated_start..)?
        .iter()
        .position(|byte| *byte == 0)?;
    let xmp_start = translated_start + translated_len + 1;
    let xmp = rest.get(xmp_start..)?.to_vec();
    (!xmp.is_empty()).then_some(xmp)
}

fn insert_png_metadata(png: Vec<u8>, metadata: &Metadata) -> Result<Vec<u8>> {
    if metadata.is_empty() {
        return Ok(png);
    }
    if png.len() < PNG_SIGNATURE.len() || &png[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return Err(Error::Encode("PNG signature 缺失,无法写入元数据".into()));
    }

    let mut out = Vec::with_capacity(
        png.len()
            + metadata.icc.as_ref().map_or(0, Vec::len)
            + metadata.xmp.as_ref().map_or(0, Vec::len),
    );
    out.extend_from_slice(PNG_SIGNATURE);

    let mut inserted = false;
    let mut offset = PNG_SIGNATURE.len();
    while offset + 8 <= png.len() {
        let length = be_u32(&png[offset..offset + 4])
            .ok_or_else(|| Error::Encode("PNG chunk 长度缺失".into()))?
            as usize;
        let chunk = &png[offset + 4..offset + 8];
        let data_start = offset + 8;
        let data_end = data_start
            .checked_add(length)
            .ok_or_else(|| Error::Encode("PNG chunk 长度溢出".into()))?;
        let next_offset = data_end
            .checked_add(4)
            .ok_or_else(|| Error::Encode("PNG chunk 偏移溢出".into()))?;
        if next_offset > png.len() {
            return Err(Error::Encode("PNG chunk 截断,无法写入元数据".into()));
        }

        if !is_replaced_png_metadata_chunk(chunk, &png[data_start..data_end]) {
            out.extend_from_slice(&png[offset..next_offset]);
        }
        if chunk == b"IHDR" && !inserted {
            write_png_metadata_chunks(&mut out, metadata)?;
            inserted = true;
        }
        if chunk == b"IEND" {
            break;
        }

        offset = next_offset;
    }

    if !inserted {
        return Err(Error::Encode("PNG IHDR 缺失,无法写入元数据".into()));
    }
    Ok(out)
}

fn is_replaced_png_metadata_chunk(chunk: &[u8], data: &[u8]) -> bool {
    chunk == b"iCCP" || chunk == b"eXIf" || (chunk == b"iTXt" && is_png_xmp_itxt(data))
}

fn is_png_xmp_itxt(data: &[u8]) -> bool {
    data.get(..PNG_XMP_KEYWORD.len()) == Some(PNG_XMP_KEYWORD)
        && data.get(PNG_XMP_KEYWORD.len()) == Some(&0)
}

fn write_png_metadata_chunks(out: &mut Vec<u8>, metadata: &Metadata) -> Result<()> {
    if let Some(icc) = metadata.icc.as_deref().filter(|icc| !icc.is_empty()) {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(icc)
            .map_err(|e| Error::Encode(format!("压缩 PNG iCCP: {e}")))?;
        let compressed = encoder
            .finish()
            .map_err(|e| Error::Encode(format!("压缩 PNG iCCP: {e}")))?;
        let mut data = Vec::with_capacity("ICC Profile".len() + 2 + compressed.len());
        data.extend_from_slice(b"ICC Profile");
        data.push(0);
        data.push(0);
        data.extend_from_slice(&compressed);
        write_png_chunk(out, b"iCCP", &data)?;
    }
    if let Some(exif) = metadata.exif.as_deref().filter(|exif| !exif.is_empty()) {
        write_png_chunk(out, b"eXIf", exif)?;
    }
    if let Some(xmp) = metadata.xmp.as_deref().filter(|xmp| !xmp.is_empty()) {
        let mut data = Vec::with_capacity(PNG_XMP_KEYWORD.len() + 5 + xmp.len());
        data.extend_from_slice(PNG_XMP_KEYWORD);
        data.push(0);
        data.push(0); // uncompressed
        data.push(0); // zlib compression method field, ignored when uncompressed
        data.push(0); // empty language tag
        data.push(0); // empty translated keyword
        data.extend_from_slice(xmp);
        write_png_chunk(out, b"iTXt", &data)?;
    }
    Ok(())
}

fn write_png_chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) -> Result<()> {
    let length = u32::try_from(data.len())
        .map_err(|_| Error::Encode("PNG metadata chunk 超过 u32 上限".into()))?;
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(name.len() + data.len());
    crc_input.extend_from_slice(name);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32fast::hash(&crc_input).to_be_bytes());
    Ok(())
}

// ---------- WebP metadata ----------

const WEBP_HEADER_LEN: usize = 12;
const WEBP_FLAG_ICC: u8 = 0x20;
const WEBP_FLAG_ALPHA: u8 = 0x10;
const WEBP_FLAG_EXIF: u8 = 0x08;
const WEBP_FLAG_XMP: u8 = 0x04;

fn extract_webp_metadata(bytes: &[u8]) -> Metadata {
    let mut metadata = Metadata::default();
    let Ok(chunks) = parse_webp_chunks(bytes) else {
        return metadata;
    };
    for chunk in chunks {
        if chunk.fourcc == *b"ICCP" && metadata.icc.is_none() && !chunk.data.is_empty() {
            metadata.icc = Some(chunk.data.to_vec());
        } else if chunk.fourcc == *b"EXIF" && metadata.exif.is_none() && !chunk.data.is_empty() {
            metadata.exif = Some(chunk.data.to_vec());
        } else if chunk.fourcc == *b"XMP " && metadata.xmp.is_none() && !chunk.data.is_empty() {
            metadata.xmp = Some(chunk.data.to_vec());
        }
    }
    metadata
}

#[derive(Clone, Copy)]
struct WebpChunk<'a> {
    fourcc: [u8; 4],
    data: &'a [u8],
}

fn parse_webp_chunks(bytes: &[u8]) -> Result<Vec<WebpChunk<'_>>> {
    if bytes.len() < WEBP_HEADER_LEN || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return Err(Error::Decode("WebP RIFF 头不完整".into()));
    }

    let mut chunks = Vec::new();
    let mut offset = WEBP_HEADER_LEN;
    while offset + 8 <= bytes.len() {
        let fourcc: [u8; 4] = bytes[offset..offset + 4]
            .try_into()
            .map_err(|_| Error::Decode("WebP chunk fourcc 缺失".into()))?;
        let length = le_u32(&bytes[offset + 4..offset + 8])
            .ok_or_else(|| Error::Decode("WebP chunk 长度缺失".into()))?
            as usize;
        let data_start = offset + 8;
        let data_end = data_start
            .checked_add(length)
            .ok_or_else(|| Error::Decode("WebP chunk 长度溢出".into()))?;
        let next_offset = data_end
            .checked_add(length % 2)
            .ok_or_else(|| Error::Decode("WebP chunk padding 溢出".into()))?;
        if next_offset > bytes.len() {
            return Err(Error::Decode("WebP chunk 截断".into()));
        }
        chunks.push(WebpChunk {
            fourcc,
            data: &bytes[data_start..data_end],
        });
        offset = next_offset;
    }
    Ok(chunks)
}

fn insert_webp_metadata(
    webp: Vec<u8>,
    width: u32,
    height: u32,
    has_alpha: bool,
    metadata: &Metadata,
) -> Result<Vec<u8>> {
    if metadata.is_empty() {
        return Ok(webp);
    }
    let chunks = parse_webp_chunks(&webp).map_err(|e| Error::Encode(e.to_string()))?;
    let existing_vp8x = chunks
        .iter()
        .find(|chunk| chunk.fourcc == *b"VP8X")
        .map(|chunk| chunk.data);

    let mut vp8x = if let Some(data) = existing_vp8x {
        if data.len() != 10 {
            return Err(Error::Encode("WebP VP8X chunk 长度非法".into()));
        }
        data.to_vec()
    } else {
        make_vp8x_data(width, height, has_alpha)?
    };
    vp8x[0] &= !(WEBP_FLAG_ICC | WEBP_FLAG_ALPHA | WEBP_FLAG_EXIF | WEBP_FLAG_XMP);
    if has_alpha {
        vp8x[0] |= WEBP_FLAG_ALPHA;
    }
    if metadata.icc.as_ref().is_some_and(|icc| !icc.is_empty()) {
        vp8x[0] |= WEBP_FLAG_ICC;
    }
    if metadata.exif.as_ref().is_some_and(|exif| !exif.is_empty()) {
        vp8x[0] |= WEBP_FLAG_EXIF;
    }
    if metadata.xmp.as_ref().is_some_and(|xmp| !xmp.is_empty()) {
        vp8x[0] |= WEBP_FLAG_XMP;
    }

    let mut out = Vec::with_capacity(
        webp.len()
            + metadata.icc.as_ref().map_or(0, Vec::len)
            + metadata.xmp.as_ref().map_or(0, Vec::len),
    );
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(b"WEBP");
    write_webp_chunk(&mut out, b"VP8X", &vp8x)?;
    if let Some(icc) = metadata.icc.as_deref().filter(|icc| !icc.is_empty()) {
        write_webp_chunk(&mut out, b"ICCP", icc)?;
    }
    for chunk in chunks {
        if matches!(&chunk.fourcc, b"VP8X" | b"ICCP" | b"EXIF" | b"XMP ") {
            continue;
        }
        write_webp_chunk(&mut out, &chunk.fourcc, chunk.data)?;
    }
    if let Some(exif) = metadata.exif.as_deref().filter(|exif| !exif.is_empty()) {
        write_webp_chunk(&mut out, b"EXIF", exif)?;
    }
    if let Some(xmp) = metadata.xmp.as_deref().filter(|xmp| !xmp.is_empty()) {
        write_webp_chunk(&mut out, b"XMP ", xmp)?;
    }

    let riff_size = u32::try_from(out.len() - 8)
        .map_err(|_| Error::Encode("WebP RIFF 超过 u32 上限".into()))?;
    out[4..8].copy_from_slice(&riff_size.to_le_bytes());
    Ok(out)
}

fn make_vp8x_data(width: u32, height: u32, has_alpha: bool) -> Result<Vec<u8>> {
    if width == 0 || height == 0 || width > 16_777_216 || height > 16_777_216 {
        return Err(Error::Encode("WebP VP8X canvas 尺寸非法".into()));
    }
    let mut data = vec![0u8; 10];
    if has_alpha {
        data[0] |= WEBP_FLAG_ALPHA;
    }
    write_u24_le(&mut data[4..7], width - 1);
    write_u24_le(&mut data[7..10], height - 1);
    Ok(data)
}

fn write_u24_le(dst: &mut [u8], value: u32) {
    dst[0] = (value & 0xff) as u8;
    dst[1] = ((value >> 8) & 0xff) as u8;
    dst[2] = ((value >> 16) & 0xff) as u8;
}

fn write_webp_chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], data: &[u8]) -> Result<()> {
    let length = u32::try_from(data.len())
        .map_err(|_| Error::Encode("WebP metadata chunk 超过 u32 上限".into()))?;
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    Ok(())
}

// ---------- JPEG ----------

struct JpegCodec;

impl Codec for JpegCodec {
    fn decode(&self, bytes: &[u8]) -> Result<ImageData> {
        let mut img = decode_via_image(bytes, image::ImageFormat::Jpeg)?;
        let metadata = metadata_from_image_format(bytes, Format::Jpeg);
        img.icc = metadata.icc;
        img.exif = metadata.exif;
        img.xmp = metadata.xmp;
        Ok(img)
    }

    fn encode(&self, img: &ImageData, opts: &EncodeOptions) -> Result<Vec<u8>> {
        img.validate()?;
        let quality = opts.quality_clamped();
        let width = img.width as usize;
        let height = img.height as usize;
        // JPEG 无 alpha:先把透明像素合成到背景,产出 RGB(避免透明区写入隐藏 RGB)。
        let rgb = img.flatten_to_rgb(JPEG_FLATTEN_BG);

        // catch_unwind 仅截 Rust panic;C 侧 segfault/abort 不在此列(靠 MAX_PIXELS 等约束)。
        let result = catch_unwind(AssertUnwindSafe(|| -> std::io::Result<Vec<u8>> {
            let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_EXT_RGB);
            if !opts.jpeg_progressive {
                comp.set_fastest_defaults();
            }
            comp.set_size(width, height);
            comp.set_quality(quality);
            if opts.jpeg_progressive {
                comp.set_progressive_mode();
            }
            comp.set_use_scans_in_trellis(opts.jpeg_trellis);
            let mut started = comp.start_compress(Vec::new())?;
            started.write_scanlines(&rgb)?;
            started.finish()
        }));

        match result {
            Ok(Ok(data)) => {
                if opts.preserve_metadata {
                    insert_jpeg_metadata(
                        data,
                        &Metadata {
                            icc: img.icc.clone(),
                            exif: img.exif.clone(),
                            xmp: img.xmp.clone(),
                        },
                    )
                } else {
                    Ok(data)
                }
            }
            Ok(Err(e)) => Err(Error::Encode(format!("mozjpeg: {e}"))),
            Err(payload) => {
                let msg = payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "未知 panic".into());
                Err(Error::Encode(format!("mozjpeg panic: {msg}")))
            }
        }
    }
}

// ---------- PNG ----------

struct PngCodec;

impl Codec for PngCodec {
    fn decode(&self, bytes: &[u8]) -> Result<ImageData> {
        let mut img = decode_via_image(bytes, image::ImageFormat::Png)?;
        let metadata = metadata_from_image_format(bytes, Format::Png);
        img.icc = metadata.icc;
        img.exif = metadata.exif;
        img.xmp = metadata.xmp;
        Ok(img)
    }

    fn encode(&self, img: &ImageData, opts: &EncodeOptions) -> Result<Vec<u8>> {
        img.validate()?;
        let rgba = if opts.png_lossy_quantize {
            quantize_rgba_for_png(&img.rgba, opts.png_quant_colors_clamped())
        } else {
            img.rgba.clone()
        };
        // 先用 image 编出基础 PNG,再用 oxipng 无损优化。
        let raw = encode_png_rgba(img.width, img.height, &rgba)?;
        let optimized = oxipng::optimize_from_memory(
            &raw,
            &oxipng::Options::from_preset(opts.png_oxipng_level_clamped()),
        )
        .map_err(|e| Error::Encode(format!("oxipng: {e}")))?;
        if opts.preserve_metadata {
            insert_png_metadata(
                optimized,
                &Metadata {
                    icc: img.icc.clone(),
                    exif: img.exif.clone(),
                    xmp: img.xmp.clone(),
                },
            )
        } else {
            Ok(optimized)
        }
    }
}

fn quantize_rgba_for_png(rgba: &[u8], colors: usize) -> Vec<u8> {
    if rgba.len() < 4 {
        return rgba.to_vec();
    }
    let quantizer = color_quant::NeuQuant::new(10, colors.clamp(64, 256), rgba);
    let mut out = rgba.to_vec();
    for pixel in out.chunks_exact_mut(4) {
        quantizer.map_pixel(pixel);
    }
    out
}

// ---------- WebP ----------

struct WebpCodec;

impl Codec for WebpCodec {
    fn decode(&self, bytes: &[u8]) -> Result<ImageData> {
        // 预读头部:校验尺寸/上限、拒绝动画,避免直接把超大流喂给 libwebp。
        let feat = webp::BitstreamFeatures::new(bytes)
            .ok_or_else(|| Error::Decode("非法 WebP 头".into()))?;
        if feat.has_animation() {
            return Err(Error::Unsupported("动画 WebP 暂不支持".into()));
        }
        rgba_byte_len(feat.width(), feat.height())?; // 尺寸/上限守卫

        let decoded = webp::Decoder::new(bytes)
            .decode()
            .ok_or_else(|| Error::Decode("libwebp 解码失败".into()))?;
        let rgba = decoded.to_image().to_rgba8();
        let mut img = ImageData::new(rgba.width(), rgba.height(), rgba.into_raw())?;
        let metadata = metadata_from_image_format(bytes, Format::WebP);
        img.icc = metadata.icc;
        img.exif = metadata.exif;
        img.xmp = metadata.xmp;
        Ok(img)
    }

    fn encode(&self, img: &ImageData, opts: &EncodeOptions) -> Result<Vec<u8>> {
        img.validate()?;
        let encoder = webp::Encoder::from_rgba(&img.rgba, img.width, img.height);
        let mut config =
            webp::WebPConfig::new().map_err(|_| Error::Encode("WebPConfig 初始化失败".into()))?;
        config.lossless = if opts.lossless { 1 } else { 0 };
        config.alpha_compression = if opts.lossless { 0 } else { 1 };
        config.quality = opts.quality_clamped();
        config.method = opts.webp_method_clamped();
        config.near_lossless = opts.webp_near_lossless_clamped();
        config.use_sharp_yuv = i32::from(opts.webp_sharp_yuv);
        // encode_advanced 返回 Result(避免 encode()/encode_lossless() 内部 unwrap 直接 panic)。
        let mem = encoder
            .encode_advanced(&config)
            .map_err(|e| Error::Encode(format!("libwebp: {e:?}")))?;
        let encoded = mem.to_vec();
        if opts.preserve_metadata {
            let has_alpha = img.rgba.chunks_exact(4).any(|pixel| pixel[3] < 255);
            insert_webp_metadata(
                encoded,
                img.width,
                img.height,
                has_alpha,
                &Metadata {
                    icc: img.icc.clone(),
                    exif: img.exif.clone(),
                    xmp: img.xmp.clone(),
                },
            )
        } else {
            Ok(encoded)
        }
    }
}

// ---------- AVIF(libavif-sys:rav1e 编码 + dav1d 解码)----------

use libavif_sys as avif;

struct AvifCodec;

/// RAII:保证 C 资源在任何返回路径都被释放(替代 IIFE 清理,clippy 友好)。
struct ImageGuard(*mut avif::avifImage);
impl Drop for ImageGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { avif::avifImageDestroy(self.0) }
        }
    }
}
struct EncoderGuard(*mut avif::avifEncoder);
impl Drop for EncoderGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { avif::avifEncoderDestroy(self.0) }
        }
    }
}
struct DecoderGuard(*mut avif::avifDecoder);
impl Drop for DecoderGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { avif::avifDecoderDestroy(self.0) }
        }
    }
}
struct RwDataGuard(avif::avifRWData);
impl Drop for RwDataGuard {
    fn drop(&mut self) {
        unsafe { avif::avifRWDataFree(&mut self.0) }
    }
}

impl Codec for AvifCodec {
    fn decode(&self, bytes: &[u8]) -> Result<ImageData> {
        unsafe {
            let decoder = DecoderGuard(avif::avifDecoderCreate());
            if decoder.0.is_null() {
                return Err(Error::Decode("avifDecoderCreate 返回 null".into()));
            }
            if avif::avifDecoderSetIOMemory(decoder.0, bytes.as_ptr(), bytes.len())
                != avif::AVIF_RESULT_OK
            {
                return Err(Error::Decode("avifDecoderSetIOMemory 失败".into()));
            }
            if avif::avifDecoderParse(decoder.0) != avif::AVIF_RESULT_OK {
                return Err(Error::Decode("avifDecoderParse 失败".into()));
            }
            if avif::avifDecoderNextImage(decoder.0) != avif::AVIF_RESULT_OK {
                return Err(Error::Decode("avifDecoderNextImage 失败".into()));
            }
            let image = (*decoder.0).image; // 归 decoder 所有,随其一起释放
            if image.is_null() {
                return Err(Error::Decode("decoder.image 为 null".into()));
            }
            let (width, height) = ((*image).width, (*image).height);
            rgba_byte_len(width, height)?; // 尺寸/上限守卫,防超大分配

            let mut rgb: avif::avifRGBImage = std::mem::zeroed();
            avif::avifRGBImageSetDefaults(&mut rgb, image);
            rgb.format = avif::AVIF_RGB_FORMAT_RGBA;
            rgb.depth = 8;
            if avif::avifRGBImageAllocatePixels(&mut rgb) != avif::AVIF_RESULT_OK {
                return Err(Error::Decode("avifRGBImageAllocatePixels 失败".into()));
            }
            // Allocate 与 Free 之间不早返回:先无条件转换+拷贝,再 free,最后判错。
            let rgba = {
                let ok = avif::avifImageYUVToRGB(image, &mut rgb) == avif::AVIF_RESULT_OK;
                let buf = if ok {
                    // 按行拷成紧凑 RGBA(rowBytes 可能含填充)。
                    let row = width as usize * 4;
                    let mut buf = Vec::with_capacity(row * height as usize);
                    for y in 0..height as usize {
                        let src = rgb.pixels.add(y * rgb.rowBytes as usize);
                        buf.extend_from_slice(std::slice::from_raw_parts(src, row));
                    }
                    Some(buf)
                } else {
                    None
                };
                avif::avifRGBImageFreePixels(&mut rgb);
                buf.ok_or_else(|| Error::Decode("avifImageYUVToRGB 失败".into()))?
            };

            // ICC/EXIF 提取。EXIF 存为 TIFF payload,与 JPEG/PNG/WebP 路径保持一致。
            let icc_ref = &(*image).icc;
            let icc = if !icc_ref.data.is_null() && icc_ref.size > 0 {
                Some(std::slice::from_raw_parts(icc_ref.data, icc_ref.size).to_vec())
            } else {
                None
            };
            let exif_ref = &(*image).exif;
            let exif = if !exif_ref.data.is_null() && exif_ref.size > 0 {
                Some(std::slice::from_raw_parts(exif_ref.data, exif_ref.size).to_vec())
            } else {
                None
            };
            let mut id = ImageData::new(width, height, rgba)?;
            id.icc = icc;
            id.exif = exif;
            Ok(id)
        }
    }

    fn encode(&self, img: &ImageData, opts: &EncodeOptions) -> Result<Vec<u8>> {
        img.validate()?;
        // 注:真·无损还需 identity matrix;当前不把 AVIF 暴露为 lossless,只使用显式质量值。
        let quality: i32 = opts.quality.clamp(1, 100) as i32;
        unsafe {
            let image = ImageGuard(avif::avifImageCreate(
                img.width,
                img.height,
                8,
                opts.avif_pixel_format(),
            ));
            if image.0.is_null() {
                return Err(Error::Encode("avifImageCreate 返回 null".into()));
            }
            if opts.preserve_metadata {
                if let Some(icc) = img.icc.as_ref() {
                    if !icc.is_empty()
                        && avif::avifImageSetProfileICC(image.0, icc.as_ptr(), icc.len())
                            != avif::AVIF_RESULT_OK
                    {
                        return Err(Error::Encode("avifImageSetProfileICC 失败".into()));
                    }
                }
                if let Some(exif) = img.exif.as_ref() {
                    if !exif.is_empty()
                        && avif::avifImageSetMetadataExif(image.0, exif.as_ptr(), exif.len())
                            != avif::AVIF_RESULT_OK
                    {
                        return Err(Error::Encode("avifImageSetMetadataExif 失败".into()));
                    }
                }
            }
            let mut rgb: avif::avifRGBImage = std::mem::zeroed();
            avif::avifRGBImageSetDefaults(&mut rgb, image.0);
            rgb.format = avif::AVIF_RGB_FORMAT_RGBA;
            rgb.depth = 8;
            rgb.pixels = img.rgba.as_ptr() as *mut u8; // RGBToYUV 只读此缓冲
            rgb.rowBytes = img.width * 4;
            if avif::avifImageRGBToYUV(image.0, &rgb) != avif::AVIF_RESULT_OK {
                return Err(Error::Encode("avifImageRGBToYUV 失败".into()));
            }
            let encoder = EncoderGuard(avif::avifEncoderCreate());
            if encoder.0.is_null() {
                return Err(Error::Encode("avifEncoderCreate 返回 null".into()));
            }
            (*encoder.0).codecChoice = avif::AVIF_CODEC_CHOICE_RAV1E;
            (*encoder.0).maxThreads = AVIF_ENCODER_MAX_THREADS; // 防 oversubscribe(评审 #4 / Claude N3)
            (*encoder.0).speed = opts.avif_speed_clamped();
            (*encoder.0).quality = quality;
            (*encoder.0).qualityAlpha = quality;
            let mut output = RwDataGuard(avif::avifRWData::default());
            let r = avif::avifEncoderWrite(encoder.0, image.0, &mut output.0);
            if r == avif::AVIF_RESULT_OK && !output.0.data.is_null() {
                Ok(std::slice::from_raw_parts(output.0.data, output.0.size).to_vec())
            } else {
                Err(Error::Encode(format!("avifEncoderWrite (code {r})")))
            }
        }
    }
}

// ---------- 测试 ----------

#[cfg(test)]
mod tests {
    use super::*;

    fn synth(width: u32, height: u32) -> ImageData {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&[
                    (x.wrapping_mul(4)) as u8,
                    (y.wrapping_mul(5)) as u8,
                    128,
                    255,
                ]);
            }
        }
        ImageData::new(width, height, rgba).unwrap()
    }

    fn quadrant_image(width: u32, height: u32) -> ImageData {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let rgb = match (x < width / 2, y < height / 2) {
                    (true, true) => [255, 0, 0],
                    (false, true) => [0, 255, 0],
                    (true, false) => [0, 0, 255],
                    (false, false) => [255, 255, 0],
                };
                rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        ImageData::new(width, height, rgba).unwrap()
    }

    fn jpeg_grid_artifact_image(width: u32, height: u32) -> ImageData {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let base = ((x + y) * 2).min(220) as u8;
                let block_bias = if ((x / 8) + (y / 8)) % 2 == 0 { 0 } else { 18 };
                let value = base.saturating_add(block_bias);
                rgba.extend_from_slice(&[value, value.saturating_add(2), value, 255]);
            }
        }
        ImageData::new(width, height, rgba).unwrap()
    }

    fn exif_with_orientation(orientation: u16) -> Vec<u8> {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"MM");
        tiff.extend_from_slice(&42u16.to_be_bytes());
        tiff.extend_from_slice(&8u32.to_be_bytes());
        tiff.extend_from_slice(&1u16.to_be_bytes());
        tiff.extend_from_slice(&0x0112u16.to_be_bytes()); // Orientation
        tiff.extend_from_slice(&3u16.to_be_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_be_bytes());
        tiff.extend_from_slice(&orientation.to_be_bytes());
        tiff.extend_from_slice(&0u16.to_be_bytes());
        tiff.extend_from_slice(&0u32.to_be_bytes());
        tiff
    }

    fn xmp_packet(label: &str) -> Vec<u8> {
        format!(
            r#"<?xpacket begin=""?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:imgconvert="https://ivmm.dev/imgconvert/" imgconvert:label="{label}"/></rdf:RDF></x:xmpmeta><?xpacket end="w"?>"#
        )
        .into_bytes()
    }

    fn jpeg_with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        assert!(jpeg.len() >= 2 && jpeg[0..2] == [0xff, 0xd8]);
        let tiff = exif_with_orientation(orientation);

        let mut app1 = Vec::new();
        app1.extend_from_slice(b"Exif\0\0");
        app1.extend_from_slice(&tiff);
        let segment_len = (app1.len() + 2) as u16;

        let mut out = Vec::with_capacity(jpeg.len() + app1.len() + 4);
        out.extend_from_slice(&jpeg[0..2]);
        out.extend_from_slice(&[0xff, 0xe1]);
        out.extend_from_slice(&segment_len.to_be_bytes());
        out.extend_from_slice(&app1);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    fn preserve_metadata_options() -> EncodeOptions {
        EncodeOptions {
            preserve_metadata: true,
            ..EncodeOptions::default()
        }
    }

    fn exif_orientation(exif: &[u8]) -> Option<image::metadata::Orientation> {
        image::metadata::Orientation::from_exif_chunk(exif)
    }

    fn pixel_at(img: &ImageData, x: u32, y: u32) -> &[u8] {
        let offset = ((y * img.width + x) * 4) as usize;
        &img.rgba[offset..offset + 4]
    }

    fn dominant_rgb_channel(pixel: &[u8]) -> usize {
        pixel[0..3]
            .iter()
            .enumerate()
            .max_by_key(|(_, value)| *value)
            .map(|(index, _)| index)
            .unwrap()
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

    fn png_with_dimensions(width: u32, height: u32) -> Vec<u8> {
        png_with_optional_phys(width, height, None)
    }

    fn png_with_optional_phys(width: u32, height: u32, phys: Option<(u32, u32)>) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&[8, 6, 0, 0, 0]); // RGBA8, deflate, standard filter/interlace

        let mut png = Vec::new();
        png.extend_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']);
        append_png_chunk(&mut png, b"IHDR", &data);
        if let Some((x, y)) = phys {
            let mut phys_data = Vec::new();
            phys_data.extend_from_slice(&x.to_be_bytes());
            phys_data.extend_from_slice(&y.to_be_bytes());
            phys_data.push(1);
            append_png_chunk(&mut png, b"pHYs", &phys_data);
        }
        append_png_chunk(&mut png, b"IDAT", &[]);
        append_png_chunk(&mut png, b"IEND", &[]);
        png
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

    fn jpeg_has_marker(bytes: &[u8], marker: u8) -> bool {
        bytes.windows(2).any(|window| window == [0xff, marker])
    }

    const DISPLAY_P3_ICC_DESC: &str = "ImgConvert Display P3 ICC fixture";

    fn display_p3_icc_profile() -> Vec<u8> {
        let tags: Vec<([u8; 4], Vec<u8>)> = vec![
            (*b"desc", icc_desc_tag(DISPLAY_P3_ICC_DESC)),
            (*b"wtpt", icc_xyz_tag(0.9642, 1.0, 0.8249)),
            (*b"rXYZ", icc_xyz_tag(0.515102, 0.241182, -0.001049)),
            (*b"gXYZ", icc_xyz_tag(0.291965, 0.692236, 0.041882)),
            (*b"bXYZ", icc_xyz_tag(0.157153, 0.066582, 0.784378)),
            (*b"rTRC", icc_curve_tag(2.2)),
            (*b"gTRC", icc_curve_tag(2.2)),
            (*b"bTRC", icc_curve_tag(2.2)),
        ];
        let tag_table_len = 4 + tags.len() * 12;
        let mut profile = vec![0u8; 128 + tag_table_len];
        let mut records = Vec::with_capacity(tags.len());

        for (signature, data) in tags {
            pad_to_4(&mut profile);
            let offset = u32::try_from(profile.len()).unwrap();
            let size = u32::try_from(data.len()).unwrap();
            profile.extend_from_slice(&data);
            pad_to_4(&mut profile);
            records.push((signature, offset, size));
        }

        let profile_size = u32::try_from(profile.len()).unwrap();
        profile[0..4].copy_from_slice(&profile_size.to_be_bytes());
        profile[8..12].copy_from_slice(&0x0430_0000u32.to_be_bytes());
        profile[12..16].copy_from_slice(b"mntr");
        profile[16..20].copy_from_slice(b"RGB ");
        profile[20..24].copy_from_slice(b"XYZ ");
        write_icc_datetime(&mut profile[24..36]);
        profile[36..40].copy_from_slice(b"acsp");
        profile[68..80].copy_from_slice(&icc_xyz_number_bytes(0.9642, 1.0, 0.8249));
        profile[80..84].copy_from_slice(b"ImgC");
        profile[128..132].copy_from_slice(&(records.len() as u32).to_be_bytes());
        for (index, (signature, offset, size)) in records.iter().enumerate() {
            let base = 132 + index * 12;
            profile[base..base + 4].copy_from_slice(signature);
            profile[base + 4..base + 8].copy_from_slice(&offset.to_be_bytes());
            profile[base + 8..base + 12].copy_from_slice(&size.to_be_bytes());
        }

        profile
    }

    fn write_icc_datetime(dst: &mut [u8]) {
        for (index, value) in [2026u16, 7, 2, 0, 0, 0].into_iter().enumerate() {
            dst[index * 2..index * 2 + 2].copy_from_slice(&value.to_be_bytes());
        }
    }

    fn icc_desc_tag(text: &str) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"desc");
        data.extend_from_slice(&[0; 4]);
        let mut ascii = text.as_bytes().to_vec();
        ascii.push(0);
        data.extend_from_slice(&(ascii.len() as u32).to_be_bytes());
        data.extend_from_slice(&ascii);
        pad_to_4(&mut data);
        data
    }

    fn icc_xyz_tag(x: f64, y: f64, z: f64) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"XYZ ");
        data.extend_from_slice(&[0; 4]);
        data.extend_from_slice(&icc_xyz_number_bytes(x, y, z));
        data
    }

    fn icc_curve_tag(gamma: f64) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(b"curv");
        data.extend_from_slice(&[0; 4]);
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&((gamma * 256.0).round() as u16).to_be_bytes());
        pad_to_4(&mut data);
        data
    }

    fn icc_xyz_number_bytes(x: f64, y: f64, z: f64) -> [u8; 12] {
        let mut bytes = [0u8; 12];
        bytes[0..4].copy_from_slice(&s15_fixed16(x).to_be_bytes());
        bytes[4..8].copy_from_slice(&s15_fixed16(y).to_be_bytes());
        bytes[8..12].copy_from_slice(&s15_fixed16(z).to_be_bytes());
        bytes
    }

    fn s15_fixed16(value: f64) -> i32 {
        (value * 65_536.0).round() as i32
    }

    fn pad_to_4(bytes: &mut Vec<u8>) {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(0);
        }
    }

    #[test]
    fn imagedata_validation() {
        assert!(ImageData::new(2, 2, vec![0u8; 16]).is_ok());
        assert!(ImageData::new(2, 2, vec![0u8; 10]).is_err()); // 长度不符
        assert!(ImageData::new(0, 2, vec![]).is_err()); // 0 尺寸
        assert!(ImageData::new(u32::MAX, u32::MAX, vec![]).is_err()); // 溢出/超上限
    }

    #[test]
    fn lossless_capability_is_true_lossless_only() {
        assert_eq!(LOSSLESS_FORMATS, &[Format::Png, Format::WebP]);
        assert!(Format::Png.supports_lossless());
        assert!(Format::WebP.supports_lossless());
        assert!(!Format::Jpeg.supports_lossless());
        assert!(!Format::Avif.supports_lossless());
    }

    #[test]
    fn avif_lossless_capability_remains_disabled_for_rav1e_backend() {
        assert!(!LOSSLESS_FORMATS.contains(&Format::Avif));
        assert!(!Format::Avif.supports_lossless());
        assert_eq!(AVIF_ENCODER_MAX_THREADS, 1);
        assert_eq!(
            AVIF_LOSSLESS_SUPPORTED,
            LOSSLESS_FORMATS.contains(&Format::Avif)
        );
    }

    #[test]
    fn encode_options_defaults_match_p2_baseline() {
        let opts = EncodeOptions::default();

        assert_eq!(opts.quality, 80);
        assert!(opts.jpeg_progressive);
        assert_eq!(opts.png_oxipng_level, 4);
        assert!(!opts.png_lossy_quantize);
        assert_eq!(opts.png_quant_colors, 256);
        assert_eq!(opts.webp_method, 4);
        assert_eq!(opts.avif_speed, 8);
        assert_eq!(opts.avif_subsample, AvifSubsample::Yuv444);
        assert_eq!(opts.webp_near_lossless, 100);
        assert!(!opts.webp_sharp_yuv);
        assert!(opts.jpeg_trellis);
        assert!(!opts.preserve_metadata);
        assert_eq!(AVIF_ENCODER_MAX_THREADS, 1);
    }

    #[test]
    fn auto_quality_levels_include_requested_ceiling() {
        assert_eq!(auto_quality_levels(30, 38), vec![30, 34, 38]);
        assert_eq!(auto_quality_levels(82, 82), vec![82]);
    }

    #[test]
    fn auto_quality_scoring_evaluation_limit_is_bounded() {
        assert_eq!(AUTO_QUALITY_MAX_SCORING_EVALUATIONS, 7);
        assert_eq!(
            auto_quality_scoring_evaluation_limit(Format::Jpeg, 1, 100),
            6
        );
        assert_eq!(
            auto_quality_scoring_evaluation_limit(Format::WebP, 1, 100),
            AUTO_QUALITY_MAX_SCORING_EVALUATIONS
        );
        assert!(auto_quality_scoring_evaluation_limit(Format::Jpeg, 30, 80) <= 6);
    }

    #[test]
    fn lossy_artifact_hint_detects_png_jpeg_grid_without_flagging_smooth_png() {
        let smooth = PngCodec
            .encode(&synth(64, 64), &EncodeOptions::default())
            .unwrap();
        assert_eq!(detect_lossy_artifacts(&smooth).unwrap(), None);

        let blocky = PngCodec
            .encode(&jpeg_grid_artifact_image(64, 64), &EncodeOptions::default())
            .unwrap();
        let hint = detect_lossy_artifacts(&blocky).unwrap().unwrap();
        assert_eq!(hint.format, Format::Png);
        assert!(hint.jpeg_grid_score >= JPEG_GRID_ARTIFACT_SCORE_THRESHOLD);
    }

    #[test]
    fn image_decode_rejects_huge_dimensions_before_pixel_decode() {
        let png = png_with_dimensions((MAX_PIXELS as u32 / 2) + 1, 2);
        let err = match decode_via_image(&png, image::ImageFormat::Png) {
            Ok(_) => panic!("huge PNG dimensions should be rejected"),
            Err(err) => err,
        };
        assert!(
            matches!(err, Error::Unsupported(ref message) if message.contains("超过上限")),
            "{err:?}"
        );
    }

    #[test]
    fn probe_png_dimensions_and_dpi_without_pixel_decode() {
        let png = png_with_optional_phys(320, 240, Some((11_811, 11_811))); // ~300 DPI
        let info = probe(&png).unwrap();

        assert_eq!(info.format, Format::Png);
        assert_eq!((info.width, info.height), (320, 240));
        let dpi = info.dpi.unwrap();
        assert!((dpi.x - 300.0).abs() < 0.1);
        assert!((dpi.y - 300.0).abs() < 0.1);
    }

    #[test]
    fn probe_encoded_formats_dimensions() {
        let img = synth(32, 24);
        let jpg = JpegCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let webp = WebpCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 80,
                    lossless: true,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();

        let jpg_probe = probe(&jpg).unwrap();
        let webp_probe = probe(&webp).unwrap();

        assert_eq!(jpg_probe.format, Format::Jpeg);
        assert_eq!((jpg_probe.width, jpg_probe.height), (32, 24));
        assert_eq!(webp_probe.format, Format::WebP);
        assert_eq!((webp_probe.width, webp_probe.height), (32, 24));
    }

    #[test]
    fn jpeg_decode_applies_exif_orientation() {
        let img = quadrant_image(16, 24);
        let jpg = JpegCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 100,
                    lossless: false,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        let oriented = jpeg_with_exif_orientation(&jpg, 6);

        let decoded = JpegCodec.decode(&oriented).unwrap();

        assert_eq!((decoded.width, decoded.height), (24, 16));
        assert_eq!(dominant_rgb_channel(pixel_at(&decoded, 2, 2)), 2);
        assert_eq!(dominant_rgb_channel(pixel_at(&decoded, 21, 2)), 0);
    }

    #[test]
    fn probe_jpeg_reports_exif_oriented_dimensions() {
        let img = quadrant_image(16, 24);
        let jpg = JpegCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 90,
                    lossless: false,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        let oriented = jpeg_with_exif_orientation(&jpg, 6);

        let info = probe(&oriented).unwrap();

        assert_eq!(info.format, Format::Jpeg);
        assert_eq!((info.width, info.height), (24, 16));
    }

    #[test]
    fn jpeg_metadata_is_stripped_by_default_and_preserved_when_requested() {
        let mut img = synth(32, 24);
        let icc = (0..70_000)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        let exif = exif_with_orientation(1);
        let xmp = xmp_packet("jpeg");
        img.icc = Some(icc.clone());
        img.exif = Some(exif.clone());
        img.xmp = Some(xmp.clone());

        let stripped = JpegCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let stripped_metadata = extract_jpeg_metadata(&stripped, false);
        assert!(stripped_metadata.icc.is_none());
        assert!(stripped_metadata.exif.is_none());
        assert!(stripped_metadata.xmp.is_none());

        let preserved = JpegCodec
            .encode(&img, &preserve_metadata_options())
            .unwrap();
        let metadata = extract_jpeg_metadata(&preserved, false);

        assert_eq!(metadata.icc, Some(icc.clone()));
        assert_eq!(metadata.exif, Some(exif.clone()));
        assert_eq!(metadata.xmp, Some(xmp.clone()));
        let decoded = JpegCodec.decode(&preserved).unwrap();
        assert_eq!(decoded.icc, Some(icc));
        assert_eq!(decoded.exif, Some(exif));
        assert_eq!(decoded.xmp, Some(xmp));
    }

    #[test]
    fn jpeg_preserved_exif_orientation_is_normalized_after_pixel_rotation() {
        let img = quadrant_image(16, 24);
        let jpg = JpegCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 100,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        let oriented = jpeg_with_exif_orientation(&jpg, 6);

        let decoded = JpegCodec.decode(&oriented).unwrap();
        let decoded_exif = decoded.exif.as_deref().unwrap();
        assert_eq!(
            exif_orientation(decoded_exif),
            Some(image::metadata::Orientation::NoTransforms)
        );

        let reencoded = JpegCodec
            .encode(&decoded, &preserve_metadata_options())
            .unwrap();
        let metadata = extract_jpeg_metadata(&reencoded, false);
        assert_eq!(
            metadata.exif.as_deref().and_then(exif_orientation),
            Some(image::metadata::Orientation::NoTransforms)
        );
    }

    #[test]
    fn thumbnail_downscales_to_max_edge_png() {
        let img = synth(320, 160);
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let thumb = thumbnail(&png, 96).unwrap().unwrap();

        assert_eq!((thumb.width, thumb.height), (96, 48));
        assert_eq!(Format::from_magic(&thumb.png), Some(Format::Png));
        let decoded = PngCodec.decode(&thumb.png).unwrap();
        assert_eq!((decoded.width, decoded.height), (96, 48));
    }

    #[test]
    fn thumbnail_skips_fully_transparent_images() {
        let mut img = synth(32, 32);
        for pixel in img.rgba.chunks_exact_mut(4) {
            pixel[3] = 0;
        }
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();

        assert!(thumbnail(&png, 96).unwrap().is_none());
    }

    #[test]
    fn magic_detection() {
        let img = synth(8, 8);
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let jpg = JpegCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let webp = WebpCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 80,
                    lossless: true,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        assert_eq!(Format::from_magic(&png), Some(Format::Png));
        assert_eq!(Format::from_magic(&jpg), Some(Format::Jpeg));
        assert_eq!(Format::from_magic(&webp), Some(Format::WebP));
        assert_eq!(Format::from_magic(b"not an image"), None);
        assert_eq!(Format::from_magic(b"RIFFxxxxWEBPxxxx"), None); // 错误 FourCC 不误判
    }

    #[test]
    fn bad_invariant_errors_not_panics() {
        // 绕过 new() 直接构造破坏不变量的 ImageData,编码入口应返回 Err 而非 panic。
        let bad = ImageData {
            width: 4,
            height: 4,
            rgba: vec![0u8; 10],
            icc: None,
            exif: None,
            xmp: None,
        };
        assert!(PngCodec.encode(&bad, &EncodeOptions::default()).is_err());
        assert!(WebpCodec.encode(&bad, &EncodeOptions::default()).is_err());
        assert!(JpegCodec.encode(&bad, &EncodeOptions::default()).is_err());
    }

    #[test]
    fn png_is_lossless() {
        let img = synth(64, 48);
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let back = PngCodec.decode(&png).unwrap();
        assert_eq!((back.width, back.height), (64, 48));
        assert_eq!(back.rgba, img.rgba); // PNG 无损:逐字节一致
    }

    #[test]
    fn png_lossy_quantization_limits_colors_when_enabled() {
        let img = synth(64, 48);
        let png = PngCodec
            .encode(
                &img,
                &EncodeOptions {
                    png_lossy_quantize: true,
                    png_quant_colors: 64,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        let back = PngCodec.decode(&png).unwrap();
        let unique = back
            .rgba
            .chunks_exact(4)
            .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
            .collect::<std::collections::HashSet<_>>();

        assert!(unique.len() <= 64);
    }

    #[test]
    fn png_metadata_is_stripped_by_default_and_preserved_when_requested() {
        let mut img = synth(32, 24);
        let icc = b"PNG-ICC-PROFILE".to_vec();
        let exif = exif_with_orientation(1);
        let xmp = xmp_packet("png");
        img.icc = Some(icc.clone());
        img.exif = Some(exif.clone());
        img.xmp = Some(xmp.clone());

        let stripped = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let stripped_metadata = extract_png_metadata(&stripped, false);
        assert!(stripped_metadata.icc.is_none());
        assert!(stripped_metadata.exif.is_none());
        assert!(stripped_metadata.xmp.is_none());

        let preserved = PngCodec.encode(&img, &preserve_metadata_options()).unwrap();
        let metadata = extract_png_metadata(&preserved, false);
        assert_eq!(metadata.icc, Some(icc.clone()));
        assert_eq!(metadata.exif, Some(exif.clone()));
        assert_eq!(metadata.xmp, Some(xmp.clone()));

        let decoded = PngCodec.decode(&preserved).unwrap();
        assert_eq!(decoded.icc, Some(icc));
        assert_eq!(decoded.exif, Some(exif));
        assert_eq!(decoded.xmp, Some(xmp));
        assert_eq!(decoded.rgba, img.rgba);
    }

    #[test]
    fn webp_lossless_roundtrip() {
        let img = synth(64, 48);
        let wp = WebpCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 80,
                    lossless: true,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        let back = WebpCodec.decode(&wp).unwrap();
        assert_eq!((back.width, back.height), (64, 48));
        assert_eq!(back.rgba, img.rgba); // libwebp 无损
    }

    #[test]
    fn auto_quality_returns_jpeg_within_quality_bounds() {
        let img = synth(32, 32);
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let result = convert_auto_quality(
            &png,
            Format::Jpeg,
            &[EncodeOptions {
                quality: 72,
                ..EncodeOptions::default()
            }],
            &AutoQualityOptions {
                min_quality: 40,
                target_score: 50.0,
            },
        )
        .unwrap();

        assert_eq!(Format::from_magic(&result.bytes), Some(Format::Jpeg));
        assert!((40..=72).contains(&result.quality));
        assert!(result.score.is_some());
        assert!(!result.used_lossless);
    }

    #[test]
    fn auto_quality_rejects_unsupported_targets() {
        let img = synth(16, 16);
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let err = convert_auto_quality(
            &png,
            Format::Png,
            &[EncodeOptions::default()],
            &AutoQualityOptions::default(),
        )
        .unwrap_err();

        assert!(matches!(err, Error::Unsupported(_)));
    }

    #[test]
    fn webp_metadata_is_stripped_by_default_and_preserved_when_requested() {
        let mut img = synth(32, 24);
        let icc = b"WEBP-ICC-PROFILE".to_vec();
        let exif = exif_with_orientation(1);
        let xmp = xmp_packet("webp");
        img.icc = Some(icc.clone());
        img.exif = Some(exif.clone());
        img.xmp = Some(xmp.clone());

        let opts = EncodeOptions {
            lossless: true,
            ..EncodeOptions::default()
        };
        let stripped = WebpCodec.encode(&img, &opts).unwrap();
        let stripped_metadata = extract_webp_metadata(&stripped);
        assert!(stripped_metadata.icc.is_none());
        assert!(stripped_metadata.exif.is_none());
        assert!(stripped_metadata.xmp.is_none());

        let preserved = WebpCodec
            .encode(
                &img,
                &EncodeOptions {
                    lossless: true,
                    preserve_metadata: true,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        let metadata = extract_webp_metadata(&preserved);
        assert_eq!(metadata.icc, Some(icc.clone()));
        assert_eq!(metadata.exif, Some(exif.clone()));
        assert_eq!(metadata.xmp, Some(xmp.clone()));

        let decoded = WebpCodec.decode(&preserved).unwrap();
        assert_eq!(decoded.icc, Some(icc));
        assert_eq!(decoded.exif, Some(exif));
        assert_eq!(decoded.xmp, Some(xmp));
        assert_eq!(decoded.rgba, img.rgba);
    }

    #[test]
    fn jpeg_alpha_flatten() {
        // 全透明图转 JPEG:应合成到白底、不 panic、尺寸正确。
        let mut img = synth(32, 32);
        for px in img.rgba.chunks_exact_mut(4) {
            px[3] = 0;
        }
        let jpg = JpegCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 90,
                    lossless: false,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        let back = JpegCodec.decode(&jpg).unwrap();
        assert_eq!((back.width, back.height), (32, 32));
        // 透明区合成白底 → 解出应接近白色(取中心像素,留有损余量)。
        let center = ((16 * 32 + 16) * 4) as usize;
        assert!(back.rgba[center] > 230 && back.rgba[center + 1] > 230);
    }

    #[test]
    fn jpeg_progressive_option_controls_sof_marker() {
        let img = synth(32, 32);
        let progressive = JpegCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let baseline = JpegCodec
            .encode(
                &img,
                &EncodeOptions {
                    jpeg_progressive: false,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();

        assert!(jpeg_has_marker(&progressive, 0xc2));
        assert_eq!(Format::from_magic(&baseline), Some(Format::Jpeg));
        assert!(!jpeg_has_marker(&baseline, 0xc2));
    }

    #[test]
    fn format_specific_encoder_options_are_clamped_and_valid() {
        let img = synth(32, 32);
        let png = PngCodec
            .encode(
                &img,
                &EncodeOptions {
                    png_oxipng_level: 99,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        let webp = WebpCodec
            .encode(
                &img,
                &EncodeOptions {
                    webp_method: 99,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();

        assert_eq!(Format::from_magic(&png), Some(Format::Png));
        assert_eq!(Format::from_magic(&webp), Some(Format::WebP));
    }

    #[test]
    fn convert_best_of_returns_smallest_candidate() {
        let img = synth(48, 32);
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let progressive_opts = EncodeOptions::default();
        let baseline_opts = EncodeOptions {
            jpeg_progressive: false,
            ..EncodeOptions::default()
        };
        let progressive = convert(&png, Format::Jpeg, &progressive_opts).unwrap();
        let baseline = convert(&png, Format::Jpeg, &baseline_opts).unwrap();

        let best = convert_best_of(&png, Format::Jpeg, &[progressive_opts, baseline_opts]).unwrap();

        assert_eq!(Format::from_magic(&best), Some(Format::Jpeg));
        assert_eq!(best.len(), progressive.len().min(baseline.len()));
    }

    #[test]
    fn convert_best_of_rejects_empty_candidates() {
        let img = synth(8, 8);
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();

        let err = convert_best_of(&png, Format::Jpeg, &[]).unwrap_err();

        assert!(matches!(err, Error::Invalid(message) if message.contains("编码候选")));
    }

    #[test]
    fn cross_convert_png_webp_jpeg() {
        let img = synth(100, 80);
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();

        let webp = convert(
            &png,
            Format::WebP,
            &EncodeOptions {
                quality: 80,
                lossless: false,
                ..EncodeOptions::default()
            },
        )
        .unwrap();
        assert_eq!(Format::from_magic(&webp), Some(Format::WebP));

        let jpg = convert(
            &webp,
            Format::Jpeg,
            &EncodeOptions {
                quality: 85,
                lossless: false,
                ..EncodeOptions::default()
            },
        )
        .unwrap();
        assert_eq!(Format::from_magic(&jpg), Some(Format::Jpeg));

        let back = convert(&jpg, Format::Png, &EncodeOptions::default()).unwrap();
        let final_img = PngCodec.decode(&back).unwrap();
        assert_eq!((final_img.width, final_img.height), (100, 80));
    }

    #[test]
    fn convert_preserves_metadata_only_when_requested() {
        let mut img = synth(32, 24);
        let icc = b"PIPELINE-ICC".to_vec();
        let exif = exif_with_orientation(1);
        let xmp = xmp_packet("pipeline");
        img.icc = Some(icc.clone());
        img.exif = Some(exif.clone());
        img.xmp = Some(xmp.clone());
        let png = PngCodec.encode(&img, &preserve_metadata_options()).unwrap();

        let stripped = convert(&png, Format::Jpeg, &EncodeOptions::default()).unwrap();
        let stripped_metadata = extract_jpeg_metadata(&stripped, false);
        assert!(stripped_metadata.icc.is_none());
        assert!(stripped_metadata.exif.is_none());
        assert!(stripped_metadata.xmp.is_none());

        let preserved = convert(&png, Format::Jpeg, &preserve_metadata_options()).unwrap();
        let metadata = extract_jpeg_metadata(&preserved, false);
        assert_eq!(metadata.icc, Some(icc));
        assert_eq!(metadata.exif, Some(exif));
        assert_eq!(metadata.xmp, Some(xmp));
    }

    #[test]
    fn convert_preserves_xmp_between_supported_containers_when_requested() {
        let mut img = synth(24, 18);
        let xmp = xmp_packet("cross-format");
        img.xmp = Some(xmp.clone());
        let source = PngCodec.encode(&img, &preserve_metadata_options()).unwrap();

        for target in [Format::Jpeg, Format::Png, Format::WebP] {
            let mut options = preserve_metadata_options();
            options.quality = 90;
            if target == Format::WebP {
                options.lossless = true;
            }

            let encoded = convert(&source, target, &options).unwrap();
            let decoded = codec_for(target).decode(&encoded).unwrap();
            assert_eq!(decoded.xmp, Some(xmp.clone()), "{target:?} lost XMP");
        }
    }

    #[test]
    fn display_p3_icc_is_preserved_from_png_to_all_writable_formats() {
        let mut img = synth(17, 13);
        let icc = display_p3_icc_profile();
        assert_eq!(&icc[36..40], b"acsp");
        assert!(icc
            .windows(DISPLAY_P3_ICC_DESC.len())
            .any(|window| window == DISPLAY_P3_ICC_DESC.as_bytes()));
        img.icc = Some(icc.clone());

        let source = PngCodec.encode(&img, &preserve_metadata_options()).unwrap();
        let source_back = PngCodec.decode(&source).unwrap();
        assert_eq!(source_back.icc, Some(icc.clone()));
        assert_eq!(source_back.rgba, img.rgba);

        for target in WRITABLE_FORMATS {
            let mut options = preserve_metadata_options();
            options.quality = 92;
            options.avif_speed = 10;
            if *target == Format::WebP {
                options.lossless = true;
                options.webp_method = 0;
            }

            let encoded = convert(&source, *target, &options).unwrap();
            assert_eq!(Format::from_magic(&encoded), Some(*target));
            let decoded = codec_for(*target).decode(&encoded).unwrap();
            assert_eq!((decoded.width, decoded.height), (img.width, img.height));
            assert_eq!(decoded.icc, Some(icc.clone()), "{target:?} lost ICC");
            if matches!(*target, Format::Png | Format::WebP) {
                assert_eq!(decoded.rgba, img.rgba, "{target:?} changed lossless pixels");
            }
        }
    }

    #[test]
    fn avif_roundtrip_dims() {
        let img = synth(48, 40);
        let av = AvifCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 80,
                    lossless: false,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        assert_eq!(Format::from_magic(&av), Some(Format::Avif));
        let back = AvifCodec.decode(&av).unwrap();
        assert_eq!((back.width, back.height), (48, 40));
    }

    #[test]
    fn avif_preserves_icc() {
        // 尖刺核心验证:libavif 容器逐字节保留 ICC(这正是弃用裸 ravif 的主因)。
        let mut img = synth(32, 32);
        let fake_icc = b"FAKE-ICC-PROFILE-BLOB-0123456789".to_vec();
        let exif = exif_with_orientation(1);
        img.icc = Some(fake_icc.clone());
        img.exif = Some(exif.clone());
        let stripped = AvifCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 85,
                    ..EncodeOptions::default()
                },
            )
            .unwrap();
        let stripped_back = AvifCodec.decode(&stripped).unwrap();
        assert_eq!(stripped_back.icc, None);
        assert_eq!(stripped_back.exif, None);

        let av = AvifCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 85,
                    ..preserve_metadata_options()
                },
            )
            .unwrap();
        let back = AvifCodec.decode(&av).unwrap();
        assert_eq!(back.icc, Some(fake_icc));
        assert_eq!(back.exif, Some(exif));
    }

    #[test]
    fn convert_png_to_avif_and_back() {
        let img = synth(64, 64);
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();
        let av = convert(
            &png,
            Format::Avif,
            &EncodeOptions {
                quality: 80,
                lossless: false,
                ..EncodeOptions::default()
            },
        )
        .unwrap();
        assert_eq!(Format::from_magic(&av), Some(Format::Avif));
        let back = convert(&av, Format::Png, &EncodeOptions::default()).unwrap();
        assert_eq!(PngCodec.decode(&back).unwrap().width, 64);
    }
}
