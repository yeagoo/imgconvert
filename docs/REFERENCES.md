# 参考项目调研结论

> ⚠️ **架构已转向(2026-06-29)**:从「libvips CLI 子进程 + AGPL」改为**混合架构**——
> **进程内宽松许可 Rust 编解码 crate + 系统原生 HEIC + 宽松许可证(MIT/Apache)**,目标上架 MAS/MS Store/Flathub。
> 引擎设计见 [ENGINE.md](ENGINE.md)。**下方第一节是新架构的参考调研(img-cankao2),最相关;** 第二节(原 img-cankao 6 项目)的引擎/许可证框架已过时,但其 **UI/UX 与压缩策略思路仍有效**。

---

# 一、混合架构参考(img-cankao2 + DropWebP)—— 现行方向

对 5 个新项目(全 Tauri 2 + 进程内 Rust crate)+ 之前的 DropWebP 的调研(2026-06-29)。

## 横向对比

| 项目 | 栈 | 引擎 crate | HEIC | 许可证 | 上架就绪 | 最高复用价值 |
|---|---|---|---|---|---|---|
| **slimg** | Tauri2 + React/shadcn + Bun | image+mozjpeg+oxipng+ravif+webp+libjxl | ❌ | **MIT** ✅ | ❌ | **蓝图:Codec trait/core/pipeline + cargo-dist C 工具链 + cargo-about** |
| **Hando** | Tauri2 + 裸 TS | mozjpeg+oxipng+**imagequant**+webp+ravif | ❌ | **AGPL + imagequant(GPL)** ⛔ | ❌ | **思路金矿:ssimulacra2 自动质量、ICC/EXIF 手术、bench 默认值、代际防护** |
| **DropWebP** | Tauri2 + Vue3 | libwebp/libavif+rav1e/jxl/oxipng/jpegli | ✅ **macOS 系统解码** | **MIT** ✅ | ❌ | **唯一做系统原生 HEIC;拖拽/批量 composables** |
| **springbok** | Tauri2 + React/shadcn + zustand | imageoptimize(含 imagequant+**dssim**) | ❌ | app Apache,引擎含 **GPL/AGPL** ⛔ | 仅 Developer-ID | **前端整套:队列 store、状态 UI、splashscreen、blake3 备份/撤销** |
| **compressor_tauri** | Tauri2 + React/shadcn | image+mozjpeg+oxipng+webp | ❌ | **Apache** ✅ | ❌ | oxipng/webp 范式、compare-slider 结果页 |
| **tavif** | Tauri2 + Next/React | image(rav1e)+webp | ❌ | **MIT** ✅ | ❌ | CI 的 nasm + mac 双架构矩阵 |

## 被集体验证的共识

1. **引擎 crate 选型已成共识**:`mozjpeg` + `oxipng` + `webp` + **`libavif-sys`(AVIF,采 DropWebP 路线)** + `image`(`default-features=false`),纯宽松、可上架。
2. **AVIF 用 `libavif-sys`**(非裸 `ravif`/`image::AvifEncoder`)——主因是容器元数据/ICC/nclx 控制 + 后端可插拔(详见 ENGINE.md;alpha 三者都能处理)。
3. **NASM 仅 x86 需要**(mozjpeg-sys ARM 用 gas);⚠️ macOS x86 钉 **NASM 2.15.05**(需核实上游是否已修)。
4. **C 工具链有配方**:slimg 的 cargo-dist `[dist.dependencies]` + Windows MSVC 配置。
5. **进程内 = 无子进程**,沙盒友好;**codec 静态链接**(但 Linux GUI 仍动态依赖 webkit2gtk,非完全单一二进制)。
6. **反面教材**:整图 base64 穿 IPC(tavif/compressor/DropWebP)→ 传路径;普遍无取消、进度多用 event → 我们上 Channel + CancellationToken。

## ⚠️ 上架两雷(转宽松后必处理)

- **`imagequant`(GPL)**(有损 PNG)→ 换 `color_quant`(MIT)/`image` 内置/只做 oxipng 无损。
- **`dssim`(AGPL)**(视觉差异)→ 换 `ssimulacra2`(宽松)。

## 复用优先级(详见 [ENGINE.md](ENGINE.md))

**🟢 可直接搬代码(MIT/Apache)**
- **slimg core**:`Codec` trait + `ImageData`(RGBA8)+ 各 codec 封装 + `catch_unwind`(仅截 Rust panic;C 崩溃靠输入上限/隔离 worker,见 ENGINE §1)+ pipeline + magic-bytes 检测。
- **slimg 工程**:workspace 分层 + cargo-dist 工具链 + cargo-about 许可清单。
- **CI**:tavif/springbok 的 nasm + mac 双架构矩阵 + capabilities 模板。
- **DropWebP**:拖拽/递归扫描/批量队列/进度协议(去 Vue 化)。

**🟡 思路极佳但 AGPL/GPL → 同款宽松 crate 重写**
- **Hando `auto.rs`**:ssimulacra2 + 二分搜索自动质量,有损/无损竞争取小。
- **Hando `icc.rs`/`metadata.rs`**:ICC/EXIF 透传容器手术(AVIF 走 libavif 已能保 ICC)。
- **Hando `bench-results.md`**:oxipng=4 / AVIF speed=8 / WebP method=4 / JPEG progressive。
- **Hando** 代际损失防护 + EventSink 抽象(用 Channel 落地)。
- **springbok 前端整套** → Svelte 5 + shadcn-svelte 重写。

**🔴 几乎无参考,需自建**
- **HEIC 系统原生**(仅 DropWebP macOS 解码一处)、Windows WIC、HEIC 编码。
- **MAS/MSIX/Flatpak 上架 + 沙盒 entitlements + 签名公证**(无一项目做过真正 MAS)。
- **取消 + Channel 进度**。

## 与蓝图 DropWebP 的差异(我们最对口的 MIT 参考)

> 我们连 AVIF 后端都直接采了 DropWebP 的方案。它是**精简的 WebP/AVIF/JXL 转换器、直接分发**;我们是**更广的批量转换/压缩工具,且为上架商店做了保真、并发、合规加固**。(DropWebP 事实经 `backend/Cargo.toml`/`build.rs`/`command.rs` 实查)

| 维度 | DropWebP | ImgConvert(我们) |
|---|---|---|
| 定位 / 许可 | WebP/AVIF/JXL 转换器,直接分发 / **MIT** | 通用批量转换+压缩,面向商店 / **Apache-2.0** |
| 前端 / 包管理 | Vue 3 + Vite / **pnpm** | Svelte 5 + shadcn-svelte / **pnpm** |
| **AVIF** | **`libavif-sys`(codec-rav1e)** + 可选 vcpkg libaom | **`libavif-sys`(codec-rav1e)** ← 采用其路线 |
| JPEG | **jpegli**(libjxl 系,BSD-3) | **mozjpeg**(IJG/BSD) |
| WebP / PNG | libwebp-sys / oxipng | webp / oxipng + **color_quant**(避 imagequant) |
| JPEG XL | **支持**(jxl-sys) | **砍掉**(libjxl 重型,过早) |
| **HEIC** | 仅 macOS 解码(magic-byte→临时文件→系统解码器) | 按平台:Linux v1 无;mac 后续 ImageIO 读+写;Win 后续 WIC 仅解码 |
| C 库获取 | **vcpkg** + build.rs 探测 | **cargo-dist 声明式 + crate 静态链接**(无 vcpkg) |
| 并发 / IPC | 每图 spawn_blocking(偏串行)/ **整图字节穿 IPC**(易爆内存) | 文件级并发 + **maxThreads=1** / **传文件路径** |
| 进度 / 取消 | progress_callback 事件 + 可取消 | **Channel + CancellationToken** |
| 元数据 / 质量 | 不强调 | **ICC/EXIF 透传、auto-quality(ssimulacra2)、代际防护** |
| 发布 | GitHub Releases | **Linux 优先**(.deb/.rpm/AppImage+Flathub)+ 商店留门 |

**直接借鉴**:① AVIF=libavif-sys(codec-rav1e)及其「rav1e 构建可靠、规避 libaom NASM 坑」的理由;② HEIC magic-byte 检测 + 系统原生解码(`command.rs:55-80`,全网唯一参考);③ 每图 spawn_blocking + 进度回调 + 可取消批处理状态机思路。
**刻意分道**:Apache 替 MIT(专利授权);mozjpeg 替 jpegli(避 libjxl 重依赖);**传路径不传字节**(修其内存反面教材);真并发 + maxThreads=1;补元数据保真 + 自动质量;上架工程化(cargo-about + 应用内许可页 + 商店留门);Svelte/pnpm + cargo-dist 静态链接(它 Vue/pnpm + vcpkg)。

---

# 二、早期参考(img-cankao 6 项目)——【历史/禁止照做:引擎+许可框架已废】

> ⚠️⚠️ **本节按旧的 libvips + AGPL 方案写,引擎/许可证/参数全部已废**。文中出现的「我们选 vips CLI」「传路径给 vips」「ravif 直连」「可直接复用 AGPL/GPL 源码」等均**作废**,以第一节 + ENGINE/LEGAL 为准。**保留本节仅为其 UI/UX 与压缩策略思路**;**所有 AGPL/GPL 项的代码一律不可拷贝**(Apache-2.0 下传染),只借鉴交互思路、自行重写。

## 横向对比(⚠️「代码可复用」列已按 Apache-2.0 现状重判)

| 项目 | 类型/栈 | 图像引擎 | 许可证 | 代码可复用(现状) | 思路借鉴 | 定位 |
|---|---|---|---|---|---|---|
| **DropWebP** | Tauri2 + Vue3 桌面 | 进程内 Rust crate(libwebp/libavif+rav1e/jxl/oxipng/jpegli) | **MIT** | ✅ 4/5(最高) | 高 | 与我们同构,可直接借鉴代码 |
| **squoosh** | Preact + WASM web | 各 codec → WASM | **Apache-2.0** | ✅ 1/5 | 3.5/5 | 参数体系 + 编解码契约范本 |
| **vert** | SvelteKit web | ImageMagick wasm | **AGPL-3.0** | ⛔ 0(代码禁止,仅借鉴交互) | **5/5(UI 思路)** | UI/UX 范本 |
| **ImageOptim** | ObjC macOS | 编排多个 CLI 压缩器 | **GPL-2.0** | ⛔ 0(禁止) | 4/5 | 压缩效果策略金矿 |
| **Converseen** | Qt + Magick++ | ImageMagick | **GPL-3.0** | ⛔ 0(禁止) | 4/5 | 成熟同品类,产品规格 |
| **caesium** | Qt + libcaesium | libcaesium(Rust) | app GPL3 / 库 Apache2 | ⛔ app 禁止;库 Apache 可用 | 3.5/5 | 压缩策略思路 |

## 许可证矩阵(⚠️ 已随转 Apache-2.0 更新)

> ⚠️ **本项目现为 Apache-2.0(宽松)+ 静态链接 + 上架**——**不能再复用 AGPL/GPL/LGPL 源码**,只能借鉴思路、自行重写。下表的 copyleft 项**仅供借鉴**,**严禁拷贝代码**。

- ✅ **可直接复用源码(宽松许可,保留对方许可与署名)**:DropWebP(MIT)、squoosh(Apache-2.0)、oxipng(MIT)、mozjpeg(IJG/BSD)、zopfli(Apache-2.0)、resvg/usvg(MPL-2.0)。
- ⛔ **只可借鉴思路、不可拷贝代码(copyleft,与 Apache-2.0 不兼容)**:vert(AGPL-3.0)、Converseen(GPL-3.0)、caesium-app(GPL-3.0)、ImageOptim(GPL-2.0)、`imagequant`(GPL-3.0)、`gifsicle`(GPL-2.0)。
- ⛔ **被禁依赖需替换**:`imagequant`(GPL)→ `color_quant`(MIT);`dssim`(AGPL)→ `ssimulacra2`;libvips/libheif(LGPL)、x265(GPL)→ 整体放弃,HEIC 走系统原生。
- ⚠️ **cargo-deny 现禁止 GPL/AGPL/LGPL**,只放行宽松;不覆盖 npm / 系统调用 —— 详见 [LEGAL.md](LEGAL.md)。

---

## 一、UI/UX(主要来自 vert、DropWebP)

### 来自 vert(AGPL-3.0,⛔ **代码不可拷贝,仅借鉴思路**)—— UI 范本 5/5
> ⚠️ 本项目为 Apache-2.0,vert 是 AGPL,**源码不可搬入**。以下只作交互/视觉**思路**参考,用 shadcn-svelte 自行实现。
- **全屏拖拽**:监听挂在页面根元素,拖入时全屏彩色模糊覆盖层 + 颜色循环动画。
- **剪贴板粘贴导入**(window `paste` 事件)+ 点击上传三合一。
- **格式选择器 `FormatDropdown`**(全项目最值得复刻):搜索框 + 分类标签(image/video/audio/doc)+ 3 列网格;输入即搜、回车选第一个、源格式高亮、已选高亮;桌面下拉浮层(自动判断向左/中/右避免溢出)、移动端变底部抽屉。
- **队列卡片网格**:每文件一张卡(缩略图 + 进度条 + 格式选择 + 转换/下载按钮),顶部工具条「全部转换 / 打包 zip 下载 / 清空 / 统一设格式」。
- **按文件类型染色**:图片蓝 / 音频紫 / 视频红 / 文档绿,贯穿图标、按钮、背景渐变。
- **设计系统**:CSS 变量集中管理颜色/圆角/阴影,light/dark + 变体统一;自定义弹性 `linear()` easing;**可一键关闭所有动效**(无障碍/性能)。
- **氛围**:单文件时把缩略图做成全屏模糊背景。
- **文件名模板**:`%name%` / `%date%` / `%extension%`。

### 来自 DropWebP(MIT,**可直接改写为 TS**)
- Tauri `tauri://drag-drop` 拖拽监听拿绝对路径(`useDragAndDrop.ts:8-25`)。
- 递归目录扫描 / 路径归一化(`useFileSystem.ts:61-98`)。
- 批处理状态机 + 可取消循环(`useImageConversionController.ts:50-121`):跳过已存在 / 扩展名过滤 / 可选删原图。
- 进度事件协议 `{percent, stage, status}`(`progress.rs:26-41`)。
- 完成/失败的**声音 + 系统通知**反馈。
- **6 语言参数 tooltip 文案**(含简繁中):参考并**自行重写**;若直接复制(来自 MIT 项目)须纳入 `THIRD_PARTY_LICENSES`/`NOTICE`。

### 架构抽象(vert + squoosh)
- **`Converter` 抽象 + 能力声明**:每格式用 `FormatInfo(name, fromSupported, toSupported)` 标注**输入/输出方向**,由能力**反向生成 UI 的格式分类**。新增格式只改一处。
- 引擎 `status` 生命周期(not-ready/downloading/ready/error)+ 超时 + cancel(杀进程)。

---

## 二、转换引擎 & 参数(UI 默认值思路 —— ⚠️ 旧 vips 映射已废,精确参数见 ENGINE §2)

> ⚠️ 本节原含 libvips saver 参数映射,**已全部作废**(项目不再用 vips;真实 crate 参数见 [ENGINE.md](ENGINE.md) §2)。这里只保留 **squoosh 的 UI 默认值/滑块范围**作交互设计参考,**不是任何后端的参数名**。

**AVIF UI 默认**:quality 50(0–100,99+=无损 checkbox)、speed/effort 滑块、subsample(0=4:0:0…3=4:4:4,无损强制 4:4:4)、tune(auto/psnr/ssim)。⚠️ 后端参数到 `libavif-sys` 的映射见 ENGINE §2。
**WebP UI 默认**:quality 75、lossless 开关、near_lossless、method 4(0–6)、alpha_quality、sharp_yuv。⚠️ **需核实 `webp` crate 是否暴露 method/near_lossless/sharp_yuv**(见 ENGINE 待验证项)。
**MozJPEG UI 默认**:quality 75、progressive、optimize_coding、quant_table、chroma_subsample、trellis。
**OxiPNG UI 默认**:level(0–6,甜点 4)、interlace false;有损量化走 `color_quant`(非 imagequant)。

**运行时动态枚举可用格式**(借 Converseen 思路):格式选择器由 **core 的支持矩阵**驱动(可读/可写分开、写前二次校验),别硬编码。

---

## 三、压缩效果策略(主要来自 ImageOptim,GPL → 借思路)

**核心哲学:不信任单一编码器,多候选取最小 + 有损→无损链式叠加。**

- **PNG**:先有损量化(`color_quant`,非 GPL 的 pngquant)→ 再 oxipng 无损优化,**只有更小才替换**。
- **JPEG**:mozjpeg `-optimize` 渐进式(无损重排);有损时要求**至少省 5% 才采纳**,否则丢弃。
- **机制**:维护「当前最优」`wipInput`,每个工具基于它再压,谁更小谁胜出;有副作用的有损工具串行先跑,无损工具并行后跑。
- **`--skip-if-larger`** + 「更小才替换」防止越压越大。
- **结果缓存**:(设置哈希 + 文件内容哈希)跳过已优化文件。
- **大小自适应**:大文件降迭代/换快 filter,小文件可更激进。
- **全局「有损/无损」开关 + 每格式质量下限阈值**(30–100,低于视作禁用)。

### Rust crate 落地(⚠️ 仅宽松,可静态链接进 Apache-2.0 项目并上架)
| 用途 | crate | 许可证 |
|---|---|---|
| PNG 无损 | `oxipng` | MIT |
| PNG 有损量化 | ⛔ ~~`imagequant`(GPL)~~ → `color_quant`(NeuQuant)/`image` 内置 | MIT |
| JPEG | `mozjpeg`(mozjpeg-sys) | IJG/BSD |
| WebP | `webp`(libwebp) | Apache/MIT;BSD |
| AVIF | `libavif-sys`(codec-rav1e)/libavif | BSD-2 |
| 质量判定 | ⛔ ~~`dssim`(AGPL)~~ → `ssimulacra2`(宽松,待核) | — |
| 通用解码/缩放/TIFF | `image` + `fast_image_resize` | MIT/Apache |

---

## 四、批量 / 输出 / 文件处理(Converseen + caesium + DropWebP)

- **并发**:Converseen 与 DropWebP **都是串行(反面教材)**。我们用 Rust 文件级并发(信号量 `(num_cpus-1).clamp(1,8)`,**编码器内部 maxThreads=1 防 oversubscribe**)。
- **传参**:DropWebP 把整图字节穿过 IPC(大图批量内存爆炸,反面教材)→ 我们**传文件路径**给 core。
- **批处理三态模型**:成功/跳过/错误,单张失败不中断,末尾汇总 + 「打开输出目录」。
- **输出命名三模式**:保持原名 / 前后缀模板(`#` 占位)/ 递增编号。
- **覆盖策略三选项**:Ask / Overwrite / Skip。
- **不破坏原文件的细节**(caesium,值得照搬):**原子写**(临时文件再替换)、**保留源文件时间戳**、**保留源目录结构**、保留/剥离元数据开关。
- **导入只 ping header 取尺寸/DPI**(`image` reader 的 header 读取),避免全解码。
- **缩略图**异步生成(canvas 缩到 ~180px,检测全透明跳过)。
- **ZIP 智能处理**:zip 内同类→整体转,否则解包成独立条目。

---

## 五、关键文件索引(参考项目内)

- DropWebP:`frontend/src/composables/useDragAndDrop.ts`、`useFileSystem.ts:61-98`、`useImageConversionController.ts:50-121`、`backend/src/encoder/progress.rs`、`backend/src/command.rs:55-85`(HEIC OS 解码)。
- squoosh:`src/features/encoders/*/shared/meta.ts`(参数默认)、`avif/client/index.tsx`(范围+映射)、`lib/feature-plugin.js`(自动注册)、`src/features/README.md`(契约)。
- vert:`src/routes/+layout.svelte`(全屏拖拽)、`src/lib/components/...FormatDropdown.svelte`、`src/lib/converters/converter.svelte.ts`(抽象基类)、`src/lib/css/app.scss`(设计 token)。
- ImageOptim:`imageoptim/Backend/Job.m`(编排 529-651 / 竞速 138-208)、`Workers/*Worker.m`(各工具参数)。
- Converseen:`src/formats.cpp:34-120`(动态枚举)、`src/converter.cpp`、`src/mainwindowimpl.cpp:689-719`(参数映射)。
- caesium:`src/models/CImage.cpp`(原子写/时间戳/目录结构)、`src/include/libcaesium.h`(FFI)。
