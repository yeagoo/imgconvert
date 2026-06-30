// SPDX-License-Identifier: Apache-2.0
// Copyright (C) 2026 The ImgConvert Authors

//! ImgConvert 进程内编解码核心。
//!
//! 设计(参考 slimg,MIT):统一中间表示 `ImageData`(RGBA8)+ `Codec` trait +
//! `Format` 检测 + 顶层 `convert` 管线。P0.5 已跑通 **JPEG / PNG / WebP / AVIF**。
//! HEIC(系统原生)、ICC/EXIF 透传为后续尖刺/阶段。
//!
//! 色深:v1 仅 **8-bit SDR**(16-bit/HDR 推后,见 docs/ENGINE.md §1)。

use std::fmt;
use std::io::Cursor;
use std::panic::{catch_unwind, AssertUnwindSafe};

use image::ImageDecoder;

/// 像素数上限(防超大分配 / C 层 OOM;~100MP,可后续配置)。
pub const MAX_PIXELS: usize = 100_000_000;

/// JPEG 不支持 alpha:透明像素合成到此背景色(P0.5 固定白底,背景色 P2 再配置)。
const JPEG_FLATTEN_BG: [u8; 3] = [255, 255, 255];

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
    /// ICC 配置文件预留(P2 透传;当前不填充)。
    pub icc: Option<Vec<u8>>,
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
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            quality: 80,
            lossless: false,
        }
    }
}

impl EncodeOptions {
    fn quality_clamped(&self) -> f32 {
        self.quality.clamp(1, 100) as f32
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
    let src =
        Format::from_magic(input).ok_or_else(|| Error::Unsupported("无法识别输入格式".into()))?;
    let img = codec_for(src).decode(input)?;
    codec_for(target).encode(&img, opts)
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

// ---------- JPEG ----------

struct JpegCodec;

impl Codec for JpegCodec {
    fn decode(&self, bytes: &[u8]) -> Result<ImageData> {
        // P0.5:解码走 image crate;mozjpeg 解码 + APP1/APP2 marker 保留留待 P2。
        decode_via_image(bytes, image::ImageFormat::Jpeg)
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
            comp.set_size(width, height);
            comp.set_quality(quality);
            comp.set_progressive_mode();
            let mut started = comp.start_compress(Vec::new())?;
            started.write_scanlines(&rgb)?;
            started.finish()
        }));

        match result {
            Ok(Ok(data)) => Ok(data),
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
        decode_via_image(bytes, image::ImageFormat::Png)
    }

    fn encode(&self, img: &ImageData, _opts: &EncodeOptions) -> Result<Vec<u8>> {
        img.validate()?;
        // 先用 image 编出基础 PNG,再用 oxipng 无损优化。
        let raw = encode_png_rgba(img.width, img.height, &img.rgba)?;
        let optimized = oxipng::optimize_from_memory(&raw, &oxipng::Options::from_preset(2))
            .map_err(|e| Error::Encode(format!("oxipng: {e}")))?;
        Ok(optimized)
    }
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
        ImageData::new(rgba.width(), rgba.height(), rgba.into_raw())
    }

    fn encode(&self, img: &ImageData, opts: &EncodeOptions) -> Result<Vec<u8>> {
        img.validate()?;
        let encoder = webp::Encoder::from_rgba(&img.rgba, img.width, img.height);
        // encode_simple 返回 Result(避免 encode()/encode_lossless() 内部 unwrap 直接 panic)。
        let mem = encoder
            .encode_simple(opts.lossless, opts.quality_clamped())
            .map_err(|e| Error::Encode(format!("libwebp: {e:?}")))?;
        Ok(mem.to_vec())
    }
}

// ---------- AVIF(libavif-sys:rav1e 编码 + dav1d 解码)----------

use libavif_sys as avif;

/// AVIF 编码速度(0=最慢最好 .. 10=最快);默认 8 待 arm64 实测(docs/ENGINE.md §2)。
const AVIF_ENC_SPEED: i32 = 8;

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

            // ICC 提取(验证 libavif 保留 ICC)。
            let icc_ref = &(*image).icc;
            let icc = if !icc_ref.data.is_null() && icc_ref.size > 0 {
                Some(std::slice::from_raw_parts(icc_ref.data, icc_ref.size).to_vec())
            } else {
                None
            };
            let mut id = ImageData::new(width, height, rgba)?;
            id.icc = icc;
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
                avif::AVIF_PIXEL_FORMAT_YUV444,
            ));
            if image.0.is_null() {
                return Err(Error::Encode("avifImageCreate 返回 null".into()));
            }
            if let Some(icc) = img.icc.as_ref() {
                if !icc.is_empty()
                    && avif::avifImageSetProfileICC(image.0, icc.as_ptr(), icc.len())
                        != avif::AVIF_RESULT_OK
                {
                    return Err(Error::Encode("avifImageSetProfileICC 失败".into()));
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
            (*encoder.0).maxThreads = 1; // 防 oversubscribe(评审 #4 / Claude N3)
            (*encoder.0).speed = AVIF_ENC_SPEED;
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

    fn jpeg_with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        assert!(jpeg.len() >= 2 && jpeg[0..2] == [0xff, 0xd8]);
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
                },
            )
            .unwrap();
        let oriented = jpeg_with_exif_orientation(&jpg, 6);

        let info = probe(&oriented).unwrap();

        assert_eq!(info.format, Format::Jpeg);
        assert_eq!((info.width, info.height), (24, 16));
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
    fn webp_lossless_roundtrip() {
        let img = synth(64, 48);
        let wp = WebpCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 80,
                    lossless: true,
                },
            )
            .unwrap();
        let back = WebpCodec.decode(&wp).unwrap();
        assert_eq!((back.width, back.height), (64, 48));
        assert_eq!(back.rgba, img.rgba); // libwebp 无损
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
    fn cross_convert_png_webp_jpeg() {
        let img = synth(100, 80);
        let png = PngCodec.encode(&img, &EncodeOptions::default()).unwrap();

        let webp = convert(
            &png,
            Format::WebP,
            &EncodeOptions {
                quality: 80,
                lossless: false,
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
            },
        )
        .unwrap();
        assert_eq!(Format::from_magic(&jpg), Some(Format::Jpeg));

        let back = convert(&jpg, Format::Png, &EncodeOptions::default()).unwrap();
        let final_img = PngCodec.decode(&back).unwrap();
        assert_eq!((final_img.width, final_img.height), (100, 80));
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
        img.icc = Some(fake_icc.clone());
        let av = AvifCodec
            .encode(
                &img,
                &EncodeOptions {
                    quality: 85,
                    lossless: false,
                },
            )
            .unwrap();
        let back = AvifCodec.decode(&av).unwrap();
        assert_eq!(back.icc, Some(fake_icc));
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
            },
        )
        .unwrap();
        assert_eq!(Format::from_magic(&av), Some(Format::Avif));
        let back = convert(&av, Format::Png, &EncodeOptions::default()).unwrap();
        assert_eq!(PngCodec.decode(&back).unwrap().width, 64);
    }
}
