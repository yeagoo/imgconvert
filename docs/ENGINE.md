# 引擎与打包技术参考(混合架构)

> **架构决策(2026-06-29 转向)**:从「libvips CLI 子进程」改为**混合架构**——
> **进程内宽松许可 Rust 编解码 crate + HEIC 走系统原生**。
> 目的:满足 **Mac App Store / Microsoft Store / Flathub** 上架(沙盒 + 宽松许可)。
> ⚠️ 范围澄清:6 个参考项目验证的是**进程内 crate 引擎选型**;**上架/沙盒路径无先例,需自建**(见 [REFERENCES.md](REFERENCES.md))。

## 0. 为什么放弃 libvips

- libvips 是 **LGPL**,其「可替换/relink」要求与 App Store DRM 冲突 → 上 MAS 有风险。
- libvips 走 **CLI 子进程**,与 App Sandbox(限制 fork/exec)冲突。
- 捆绑 libvips 需重定位/重签一长串 dylib(`dylibbundler` + `install_name_tool`),复杂且易出错;且 brew libheif 会牵入 **x265(GPL/专利)**。
- **混合架构反而更简单**:C 编解码器由 crate 在**构建期静态链接**进单一 Rust 二进制 → **无 dylib 捆绑、无 rpath 手术、签名只签一个二进制**;系统框架(ImageIO/WIC)始终存在,无需分发。

---

## 1. 进程内 core 设计(参考 slimg,MIT 可直接借鉴)

统一架构:**`Codec` trait + `ImageData`(RGBA8 统一中间表示)+ `get_codec(Format)` 分发 + `pipeline`(decode → 变换 → encode)**。core 为纯库,零 UI/IO 框架依赖,供 Tauri GUI / CLI / 测试共用。

```
crates/
  imgconvert-core/   # 纯库:Codec trait、ImageData、各 codec、pipeline、format 检测
  imgconvert-cli/    # (可选)命令行前端
src-tauri/           # Tauri 后端,仅做胶水:invoke 命令 + Channel 进度 + 系统 HEIC
```

要点:
- 统一中间表示 **`ImageData`**;`decode → (resize/crop) → encode` 串成 pipeline。⚠️ **色深(Codex 指出)**:v1 用 **RGBA8 + 仅 8-bit SDR**(明确不保 16-bit/HDR);若要色彩保真做到位,中期把像素表示升级为 `8/16/float` 枚举。`fast_image_resize` **不会自动在线性空间 resize**,ICC/线性化要自己在 resize 前后处理(P0.5 增色彩尖刺)。
- **崩溃防护(Codex 修正)**:`std::panic::catch_unwind` **只能截 Rust panic,挡不住 C 侧 segfault/abort/UB**。真正的健壮性靠:libjpeg error handler + **输入尺寸/像素上限** + 对可疑文件走隔离 worker + fuzz/corpus 测试。不要承诺「防住 C 崩溃」。
- **格式检测**:magic bytes + 扩展名。
- 解码**保留 ICC/EXIF**(见 §5),不要直接 `to_rgb8` 丢元数据。

---

## 2. 编解码 crate 选型与参数(全宽松,已被多项目验证)

| 格式 | 解码 | 编码 | crate(许可证) | 关键参数 |
|---|---|---|---|---|
| **JPEG** | `mozjpeg::Decompress`(保留 APP1/APP2) | `mozjpeg`(trellis 默认开,progressive) | `mozjpeg`(IJG/BSD) | quality 0–100;progressive(默认开,通常略小、更慢)。⚠️ **「JPEG→JPEG 无损系数转码(jpegtran 式)」走 DCT 系数域、绕过 RGBA8 管线**——`mozjpeg` crate 是否暴露该 transform API **需核实**;v1 暂不承诺,作单独设计点 P0.5 验证 |
| **PNG** | `image` | `image` 编码 → **`oxipng`** 优化 | `oxipng`(MIT)、`image`(MIT/Apache) | 无损 level 0–6(**甜点 4**);有损量化见下「⚠️ imagequant」 |
| **WebP** | `image` | **`webp`**(libwebp) | `webp`(Apache/MIT;底层 libwebp BSD) | quality(有损);**无损=独立 lossless 模式**(`encode_lossless`/`WebPConfig.lossless`,**不是 q100**——q100 仍是有损最高质量);`exact` 只保留透明区不可见 RGB,非无损开关;method 0–6(**甜点 4**);alpha |
| **AVIF** | `libavif-sys`(dav1d 默认) | **`libavif-sys`(codec-rav1e 默认)** | `libavif-sys`(**BSD-2,以 cargo-about 实测为准**)、libavif(BSD-2)、rav1e(BSD-2) | quality、speed 1–10(甜点 8);采 DropWebP 路线。⚠️ **弃用裸 `ravif`/`image::AvifEncoder` 的真因(Codex 修正)不是「丢 alpha」**(它们都能处理 RGBA8 alpha),而是 **ICC/EXIF/nclx 等容器元数据控制弱 + 后端不可插拔 + 解码一致性**;libavif 容器层能正确写 ICC/nclx/alpha/EXIF。后端可插拔:rav1e(默认,构建稳)/ aom / svt |
| **TIFF**(推后) | `image` | `image`(tiff feature) | `image` | v1 不做;无损 deflate/lzw |
| **GIF/BMP**(读) | `image` | — | `image` | 解码用 |
| ~~**JPEG XL**~~ | — | — | — | **删除**(评审一致:libjxl 重型 C++,过早) |
| 缩放 | — | — | `fast_image_resize`(MIT/Apache) | — |
| 质量判定 | — | — | **`ssimulacra2`**(宽松,**勿用 dssim/AGPL**) | 感知打分,用于「自动质量」(§6) |

### bench 默认值(Hando Apple Silicon 实测,直接采用)
- **oxipng level 4**(比 2 省 8–28%,时间 2×;6 只再省 ~1% 不值)
- **AVIF speed 8**(⚠️ 此值来自 x86 实测;**rav1e 无 arm64 汇编 → Apple Silicon 上明显更慢**,macOS 阶段须重测后再锁默认,评审 #2)
- **WebP method 4**(method 6 几乎不省;2 大 ~5%)
- **JPEG progressive 默认开**(通常比 baseline **略小**、但更慢——⚠️ Codex 指出「~3×」量级不实,以项目实测百分比为准;UI 提供 baseline「最大兼容」选项)

### ⚠️ 两个 copyleft 雷(宽松/上架必须避开)
- **`imagequant`(GPL-3.0/商业)**——有损 PNG 调色板量化。**替换**为:`image` 内置量化 / `color_quant`(NeuQuant,MIT)/ `quantette`(核实许可),或**只做 oxipng 无损**。
- **`dssim`(AGPL/GPL)**——视觉差异。**改用 `ssimulacra2`**(宽松)。

### 有损 vs 无损
- AVIF:libavif/rav1e 有损为主(quality/speed);真无损需单独补 identity matrix,当前 UI/能力矩阵不声明 lossless。
- WebP:`encode_lossless`(完全可逆,适合图形/截图)vs `encode(quality)`(有损)。
- PNG:默认无损(oxipng);有损=量化(避开 imagequant)。
- JPEG:永远有损;无损只能转码(DCT 系数重排,不改像素)。
- TIFF:deflate/lzw 无损;jpeg 有损。

---

## 3. HEIC + 系统原生(按平台能力;**Linux v1 不做**)

HEIC = HEVC,有专利;**不捆绑 x265**。⚠️ 关键决策(评审 #1):**「禁 LGPL」与「Linux 用 libheif」自相矛盾 → Linux v1 直接不含 HEIC**(避免 LGPL + x265/GPL + 专利)。HEIC 随 macOS/Windows 阶段再来,且**按平台能力运行时探测**,不硬编码、不在营销里平铺「支持 HEIC」。

| 平台 | HEIC 解码 | HEIC 编码 | 实现 |
|---|---|---|---|
| **Linux(v1)** | ❌ 不做 | ❌ 不做 | 避免 LGPL/x265/专利 |
| **macOS(后续)** | ImageIO | ImageIO | 进程内调 `CGImageSource`/`CGImageDestination`,经 `objc2`/`core-graphics`(**不 shell `sips`**);⚠️ **沙盒内 HEVC 编码能否用须实测**(评审) |
| **Windows(后续)** | WIC + 用户装 HEVC 扩展 | ❌ 基本不可行 | `windows-rs` 调 WIC;**仅解码**,运行时探测扩展是否注册,缺失则引导安装,**不承诺开箱即用**(评审 #5) |

- macOS 上 **ImageIO 也能写 AVIF**(macOS 13+),但 AVIF 我们已用 `libavif-sys` 跨平台统一,系统 ImageIO 仅作 HEIC 专用。
- 参考:DropWebP `backend/src/command.rs:55-85`(magic-byte 检测 `ftyp/heic` → 调系统解码)。

---

## 4. 跨平台 C 工具链(我们最担心的点,已有现成配方)

C 编解码器(rav1e/mozjpeg/libaom/dav1d via libavif)在**构建期**需要原生工具链:

- **NASM**(rav1e、mozjpeg 的 **x86/x86_64** 汇编)——⚠️ **仅 x86 需要,不是「所有平台」(Codex 修正)**:`mozjpeg-sys` 在 Intel 用 nasm、**ARM 用 gas**;libavif/rav1e 同理按 target arch 分。CI 工具链按「crate × feature × target arch」列矩阵,别一刀切。
  - ⚠️ **macOS x86 把 NASM 钉到 2.15.05**(NASM 2.16+ 会让 `libaom` cmake 探测失败;**需核实当前上游是否已修**,以摆脱钉版本)。
  - ⚠️ **装好后在 build 脚本里做版本检测,失败给明确错误**——别让 cmake 抛玄学报错(评审 #11)。这是已知脆弱点,长期关注上游修复以摆脱钉版本。
  - Windows:`ilammy/setup-nasm` 或 `choco install nasm` + MSVC(`ilammy/msvc-dev-cmd`)。
- **cmake**(libavif/libaom)、**meson + ninja**(dav1d)。
- **声明式装依赖**:用 **cargo-dist** 的 `[dist.dependencies.{apt,homebrew,chocolatey}]` 统一声明 nasm/cmake/meson/ninja(slimg 范式)。Linux v1 重点验证 Debian/Ubuntu(apt)+ Fedora(dnf)。
- AVIF 走 **`libavif-sys`(codec-rav1e)**:rav1e 纯 Rust 编码器,**构建可靠、规避 libaom 的 NASM 多趟优化坑**(DropWebP 实证);libavif 容器层经 cmake 构建。
- **静态链接**:codec 全静态进二进制。⚠️ 注意 **Linux 上 webkit2gtk 仍是动态系统库**(Tauri 依赖,各发行版版本不同),所以「单一静态二进制」仅指 codec 部分,不含系统 WebView。

---

## 5. ICC / EXIF 保真(脏活,Hando 有带测试的实现可参考重写)

- **JPEG**:ICC 要按 1-based 分块写多个 **APP2** marker;EXIF 写 APP1。解码保留 APP1/APP2。
- **WebP**:手动做 RIFF 容器手术插 `VP8X` + `ICCP`。
- **PNG**:`eXIf` 块要在 **oxipng 优化之后**再 splice(否则被剥)。
- **AVIF**:✅ **改用 `libavif-sys` 后可正确写 ICC/nclx/EXIF**(裸 `ravif`/`image` 能处理 alpha 但**容器元数据/ICC 控制弱**,这才是改用 libavif 的主因)。P0.5 须验证「带 ICC 的 sRGB→DisplayP3 往返色彩正确」。
- **EXIF orientation**:解码时把像素**真旋正**,再把 orientation tag 改写为 1(或丢 blob)。
- crate:`kamadak-exif`(MIT/BSD)、`img-parts`(容器操作)。

---

## 6. 自动质量(可选高级特性,Hando 思路,重写)

- ⚠️ **仅对 JPEG/WebP 开放**(Claude 自审 N2):`ssimulacra2` 二分搜索每轮一次完整编码,且需要可调 quality 旋钮。**PNG 默认无损(无 quality 可搜)、有损量化又是实验性默认关**,故 PNG 不进自动质量;**AVIF 编码慢,二分 = 不可用**(评审 #3),只给「固定 quality + 视觉无损」两档。
- 用 **`ssimulacra2`** 感知打分 + **二分搜索**:找「达到目标分 S 的最小质量」(质量阶梯 step≈4,~5 轮 encode+judge)。
- **无损候选 vs 有损候选同时竞争,取小者**。
- **代际损失防护**:对已是有损的源(JPEG/AVIF),按 **bits-per-pixel** 分级收紧门槛;**永不变差**(输出 > 源 × 0.98,即省不到 2% 视为无收益,跳过)。
- 「假无损」侦测:PNG 里藏 JPEG 8×8 网格指纹的降级处理。

---

## 7. 进度 / 取消(全员缺失,我们自建,直接领先)

- **进度**:用 **Tauri Channel**(有序、低延迟、按调用作用域),非 event。批量逐文件 + 文件内进度。
  ```rust
  use tauri::ipc::Channel;
  #[derive(Clone, serde::Serialize)]
  #[serde(rename_all = "camelCase", tag = "event", content = "data")]
  enum ProgressEvent { Started { total: usize }, FileProgress { index: usize, percent: f64 }, Finished }
  ```
  可参考 Hando 的 **EventSink trait**(生产/测试两实现)做可测试抽象,但用 Channel 落地。
- **取消**:`tokio_util::sync::CancellationToken` 存入 `.manage()`,长任务 `select!` 监听;参考项目**无一实现**,这是我们的增量。
- **并发**:全局信号量限并发(默认 `(num_cpus-1).clamp(1,8)`)。⚠️ **关键(评审 #4 + Codex)**:AVIF 编码器内部多线程;文件级并发 × 内部多线程 = **oversubscribe + OOM**。走 libavif 路线时应设 **libavif encoder `maxThreads=1`**(sys 层对应 C 字段;高层 `libavif` wrapper 是 `set_max_threads`),靠文件级并行,不要两层都开。
- **传参传文件路径**,⚠️ 别学 tavif/compressor 把整图 base64 穿 IPC。

---

## 8. 许可证合规与上架

### 许可证(本项目为上架采用 Apache-2.0)
- ⚠️ 已采用 **Apache-2.0**(宽松 + 专利授权)。这意味着:**Hando(AGPL)、springbok 引擎(GPL/AGPL)代码不可抄**,只能同款宽松 crate 重写;**imagequant(GPL)/dssim(AGPL)必须替换**;`deny.toml` 禁止 GPL/AGPL/LGPL。详见 [LEGAL.md](LEGAL.md)。
- 用 **`cargo-about`** 生成 `THIRD_PARTY_LICENSES`,**CI 自动跑**。
- ⚠️ **应用内「开源许可」页(评审 #7,必做)**:Apache-2.0 §4(d) 要求保留 NOTICE;`mozjpeg` 的 **IJG 许可含命名条款**(不得用 IJG 名义背书),`webp`/`libavif`/`rav1e` 的 **BSD** 要求二进制分发保留版权与免责声明。商店二进制里用户拿不到仓库文件,所以必须在 App 内做一个可滚动的全文「开源致谢」页(嵌入完整 NOTICE/许可文本,非链接)。

### 上架渠道(发布顺序见 ROADMAP P3:Linux 优先)
| 渠道 | 打包 | 沙盒 | 签名 | 文件访问 |
|---|---|---|---|---|
| **Linux(v1)** | `.deb`/`.rpm`/AppImage | 无 | —(可选 GPG)| 自由;⚠️ webkit2gtk 动态系统依赖,各发行版版本不同 |
| **Flathub(v1)** | Flatpak | portal | — | **文件 portal**(拿到的可能是 portal 路径非真实路径,P0.5 验证) |
| **Mac App Store(后续)** | `.pkg` | **App Sandbox 必须** | Apple Distribution + provisioning | security-scoped bookmarks;输出目录走 NSOpenPanel |
| **Microsoft Store(后续)** | **MSIX**(Tauri 非一等)| `runFullTrust` | Partner Center | 较自由 |
| 直接分发(macOS/Win,后续)| `.dmg`/`.msi` | 无 | Developer ID + 公证 / 代码签名 | 自由 |

- ⚠️ **为商店留门(贯穿全程)**:即使 v1 只发 Linux,也要保持**无子进程**、**文件访问抽象成「用户显式授权目录」**(Flatpak portal ↔ 未来 MAS security-scoped bookmark 同一抽象),否则 v1 后上 MAS 会大返工(评审 #6)。
- **MAS 现状(2026-06 实查)**:Tauri 2 + Svelte **可上 MAS**——官方文档完整(App Sandbox + Entitlements.plist + provisioning + Mac Installer Distribution 证书 + `.pkg` + altool),有真实上架案例(如 Simple Invoice & Bill Maker),Svelte 不影响(WKWebView 静态资源)。⚠️ **唯一硬点**:批量目录访问要的 **security-scoped bookmark 在 Tauri 不是一等公民**(核心 issue #3716 自 2022 未解)。`tauri-plugin-dialog` 能返回 bookmark 数据,但 `startAccessingSecurityScopedResource`/`stop` 生命周期**需自写 `objc2` shim**(忘 stop 泄漏内核资源 + 丢越沙盒能力)。→ 这是「用户显式授权目录」抽象的 macOS 落地点,P0.5 文件访问尖刺要把它设计进去。
- **HEVC/HEIC 专利(评审 #9)**:调用系统编解码器是**实务安全垫**(平台已付费),**非法律免责**;按平台能力如实表述,营销勿平铺「支持 HEIC」;商用前请 IP 律师出意见。

---

## 9. 关键文档 / 参考路径

- 参考实现:slimg `crates/slimg-core/src/codec/*`、`dist-workspace.toml`、`about.toml`(MIT,可直接借鉴);Hando `encoder/auto.rs`、`icc.rs`、`metadata.rs`、`docs/bench-results.md`(AGPL,重写);DropWebP `backend/src/command.rs`(系统 HEIC)。
- crate 文档:docs.rs/{mozjpeg, oxipng, webp, libavif-sys, image, fast_image_resize, ssimulacra2}
- Tauri:Channel https://v2.tauri.app/develop/calling-frontend/ · macOS 签名 https://v2.tauri.app/distribute/sign/macos/
- cargo-dist:https://opensource.axo.dev/cargo-dist/ · cargo-about:https://github.com/EmbarkStudios/cargo-about
