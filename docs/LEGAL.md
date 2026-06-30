# 许可证与合规

本项目以 **Apache-2.0** 发布(宽松许可)。本文件汇总许可证策略、上架合规、HEVC 专利规避,以及第三方组件处理。

> ⚠️ 以下为工程视角整理,**不替代法律意见**;正式发布/上架前建议对最终依赖树做一次法律审阅。

> **历史**:本项目曾计划采用 AGPL-3.0。2026-06-29 因转向**混合架构**并以 **Mac App Store / Microsoft Store / Flathub** 为目标,改为 **Apache-2.0**——商店分发要求宽松许可,且进程内方案需静态链接编解码库,copyleft 不可行。

## Apache-2.0

- **宽松许可**:允许商用、闭源衍生、上架各应用商店;保留版权与许可声明即可。
- **专利授权条款**:相比 MIT 多了显式专利授权(对图像编解码这类领域更稳),含专利报复终止条款。
- **NOTICE 要求**:分发时须保留本仓库 `NOTICE` 及所依赖 Apache-2.0 组件的 NOTICE 内容。
- **SPDX 标识统一为 `Apache-2.0`**(`Cargo.toml`/`package.json` license 字段与各源文件头一致)。
- **贡献**:inbound = outbound(同 Apache-2.0)+ DCO,不要求 CLA,见 [../CONTRIBUTING.md](../CONTRIBUTING.md)。Apache-2.0 §5 已含贡献条款。

## 上架可行性

- Apache-2.0 是宽松许可,**与 Mac App Store / Microsoft Store / Flathub 均兼容**。
- ⚠️ **「单一静态二进制」仅指 codec(Codex 修正)**:编解码器构建期静态链接进可执行文件;**但 Linux GUI/WebView 仍动态依赖系统 webkit2gtk/gtk**(.deb 会声明 `libwebkit2gtk-4.1-0` 等)。AppImage/Flatpak 若打包到任何 LGPL 库,需单独列 LGPL 的 notice/source/可 relink 合规策略。
- 真正的合规风险不在本项目许可证,而在**依赖树是否混入 copyleft**(见下)。

## ⚠️ 依赖许可红线(宽松 + 上架的硬约束)

转 Apache-2.0 + 静态链接 + 上架后,依赖策略与之前的 AGPL 时代**完全相反**:

| 许可证 | 处理 | 原因 |
|---|---|---|
| MIT / BSD / Apache-2.0 / ISC / Zlib / MPL-2.0 | ✅ 允许 | 宽松,可静态链接、可商用上架(MPL 文件级 copyleft,改动需公开但可链接) |
| **GPL-2.0 / GPL-3.0 / AGPL** | ⛔ **禁止** | 静态链接会传染整个二进制;与商店分发/闭源不兼容 |
| **LGPL(任意版本)** | ⛔ **禁止**(含 libheif) | LGPL 要求可重新链接,与静态链接 + 商店不兼容。⚠️ **评审 #1 指出的矛盾**:曾设想「Linux 用 libheif(LGPL)做 HEIC」与本规则冲突 → **裁决:Linux v1 不做 HEIC**(见下),规则不破例。例外:Tauri 自身依赖的 **webkit2gtk(LGPL)是系统动态库**,属平台运行时,不在我们分发的 codec 静态链接范围内。 |

**具体被排除的库**(参考项目用过、我们不能用):
- **`imagequant` / libimagequant(GPL-3.0)**——有损 PNG 量化 → 改用 `color_quant`(MIT,NeuQuant)或 `image` 内置量化,或只做 oxipng 无损。
- **`dssim` / dssim-core(AGPL)**——视觉差异 → 改用 `ssimulacra2`(宽松,发布前核对其 crate 许可)。
- **libvips(LGPL)/ libheif(LGPL)/ x265(GPL)**——旧引擎链 → 已整体放弃。

## 复用上游代码的规则(转宽松后收紧)

- ✅ **只能直接复用宽松许可(MIT/BSD/Apache-2.0/MPL-2.0)的源码**,并保留其许可证 + 版权声明,在 `THIRD_PARTY_LICENSES` / `NOTICE` 列出来源(仓库 + commit/tag),保持文件级 SPDX 头。
- ⛔ **不能再直接复制 GPL/AGPL 源码**(与 Apache-2.0 不兼容):
  - **vert(AGPL)**、**Hando(AGPL)**、**springbok 引擎(经 imageoptimize 含 GPL/AGPL)** → **只借鉴思路、自行重写**,不得拷贝代码。
- ✅ 可直接借鉴/移植代码的宽松参考:**slimg(MIT)**、**DropWebP(MIT)**、**tavif(MIT)**、**compressor_tauri(Apache-2.0)**——仍需保留各自版权与许可声明。

## HEVC / HEIC

- HEIC = **HEIF 容器 + 常用 HEVC 编码**,受**多个专利池**(MPEG LA / Access Advance / Via LA 等)覆盖,**无 AV1 那种干净 RF 授权**。**不随包分发任何 HEVC 编码器(如 x265)**。
- **按平台处理**(评审 #1/#5/#9):
  - **Linux v1:不做 HEIC**(避免 libheif/LGPL + x265/GPL + 专利)。
  - **macOS(后续):ImageIO**(经 `objc2`/`core-graphics` 进程内,不 shell `sips`);⚠️ 沙盒内 HEVC 编码能否用须实测。
  - **Windows(后续):WIC** —— 查看 HEIC 常需 **HEIF Image Extensions + HEVC Video Extensions**(后者部分地区付费);运行时探测 WIC 是否注册 HEIF/HEVC 编解码,缺失则引导安装。**v1 产品策略仅承诺解码**(不代表 WIC 技术上绝对不能编码),**不承诺开箱即用**。
- ⚠️ **「调用系统编解码器 = 专利免责」不成立**:平台(Apple/微软)为其系统 API 已向池方付费,这是**事实上的安全垫**,**非法律免责**。实务上针对「仅调系统 API、不捆绑编码器」的小应用追诉概率极低(Squoosh/ImageOptim 同此),但:
  - 营销/文案**勿平铺「支持 HEIC」**,按平台能力如实写。
  - **商用/收费版上线前找 IP 律师**就 HEVC 专利出书面意见。

## AV1 / AVIF 专利

- AVIF(经 libavif + rav1e)基于 AV1,**AOMedia 提供免版税专利授权(AOM Patent License 1.0)**,含**防御性终止**条款(你用专利告人则授权失效)。业界普遍视为商用安全。
- 但**不为零**:可能存在非 AOMedia 成员的必要专利;Sisvel 曾运营 AV1 池(争议)。**需核实** 2025–2026 诉讼状态。
- 文案表述:「AVIF 基于 AV1,采用开放生态编码器;不捆绑 HEVC 编码器」,**避免**写「无专利风险 / patent-free」。

## 应用内归属(NOTICE)义务

- **评审一致要求**:Apache-2.0 §4(d) 要保留 NOTICE;BSD 类要求二进制分发保留版权与免责声明;**`mozjpeg` 的 IJG 许可含命名条款**(不得以 IJG 名义背书)。
- 商店/Flatpak 二进制里用户拿不到仓库文件 → **必须在 App 内做「开源许可」页**,内嵌 `THIRD_PARTY_LICENSES` 全文(IJG/BSD/Apache NOTICE 完整文本,非链接)。
- `cargo-about` 生成后**人工复核**:确认 C 库(libwebp/libavif/rav1e/mozjpeg/dav1d,以及 **libavif 常拉的 libyuv(BSD-3)**)的 NOTICE/版权段被完整包含,而非只有 Rust crate 的 SPDX 摘要。`image`/`fast_image_resize` 的精确 SPDX(可能 MIT-only)同样以 cargo-about 输出为准。

## 第三方组件(均宽松,可静态链接上架)

> 这些是混合架构计划接入的核心编解码 crate;进入分发物前以 `cargo-about` 实际生成的清单为准。

- `image`(MIT/Apache)—— 解码/容器/TIFF
- `mozjpeg`(IJG/BSD,封装 MIT/Apache)—— JPEG
- `oxipng`(MIT)—— PNG 无损
- `webp`(MIT/Apache;libwebp BSD-3)—— WebP
- `libavif-sys`(**BSD-2-Clause,待 cargo-about 实测确认**)+ libavif(BSD-2)+ rav1e(BSD-2)+ dav1d(BSD-2,解码)—— AVIF(采 DropWebP 路线,**取代裸 ravif**)
- `fast_image_resize`(MIT/Apache,**待核精确 SPDX**)—— 缩放
- `ssimulacra2`(**BSD-2-Clause**,crates.io 实查确认)—— 质量判定
- `color_quant`(MIT)—— 有损 PNG 量化(替代 imagequant)
- Tauri / Svelte / shadcn-svelte / phosphor-svelte / Tailwind —— 均宽松(MIT 等)

## 合规自动化与审计范围

- `src-tauri/deny.toml`(cargo-deny)**禁止 GPL/AGPL/LGPL**,只放行宽松许可;CI 自动运行 `cargo deny check licenses`。
- `cargo-about`(`about.toml` 白名单 + 模板)**自动生成 `THIRD_PARTY_LICENSES.md`**,在 CI 跑(不手动)。
- 完整许可清单需综合:`cargo deny`(Cargo)+ `license-checker`(npm)+ `cargo-about`(NOTICE/THIRD_PARTY)。
- ⚠️ cargo-deny **不覆盖** npm 包与系统框架调用(ImageIO/WIC 是系统能力,不分发,无许可负担)。
