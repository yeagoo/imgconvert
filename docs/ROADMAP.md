# 开发路线图

> 📍 **当前进度(2026-06-30)**:P0.5 引擎尖刺已通(`imgconvert-core` 跑通 JPEG/PNG/WebP/AVIF,测试全绿)→ **P0「前端整顿」三项已落地**(组件化架构 + core 能力契约 + shadcn 控件/格式选择器)→ **P0.5 批量进度/取消协议最小闭环已落地**。下一步进入 P0.5 文件访问/许可尖刺与 P1 并发批量。详见 [DEVLOG.md](DEVLOG.md)。

> 原则:**UI/UX 优先**——先把界面与交互做出来「看得见」,再逐步接真实功能与高级压缩。
> 参考依据见 [REFERENCES.md](REFERENCES.md),引擎/打包设计见 [ENGINE.md](ENGINE.md)。

> ⚠️ **架构决策(2026-06-29 转向「混合架构」;2026-06-29 二轮:三方评审 + 用户拍板后修订)**:
> - **引擎**:放弃 libvips CLI,改为**进程内宽松许可 Rust 编解码 crate**——JPEG `mozjpeg`、PNG `oxipng`、WebP `webp`(libwebp)、**AVIF `libavif-sys`(codec-rav1e 默认,采 DropWebP 路线)**、解码/容器 `image`。C 编解码器构建期静态链接。**HEIC 走系统原生(macOS ImageIO / Windows WIC),Linux v1 不含 HEIC**。
> - **平台优先级(已改)**:**Linux 优先**(Debian/Ubuntu/Fedora;.deb/.rpm/AppImage + Flathub);macOS、Windows 商店为后续阶段。**但架构必须为商店留门**:无子进程、纯宽松许可、文件访问抽象成「用户显式授权目录」(兼容 macOS security-scoped bookmark / Flatpak portal)。
> - **许可证**:**Apache-2.0**(已从 AGPL 切换完毕);`deny.toml` 禁止 GPL/AGPL/LGPL;`imagequant`(GPL)→ `color_quant`,`dssim`(AGPL)→ `ssimulacra2`。
> - **格式(v1)**:JPEG/PNG/WebP/AVIF(全平台 crate);HEIC 按平台能力(Linux 无);TIFF/JXL 推后。
> - **三方评审已纳入的修正**:见文末「评审修正清单」。

## 阶段总览

| 阶段 | 主题 | 目标 |
|---|---|---|
| **P0** | UI/UX 外壳 | 界面、交互、设计系统全部可见可点(后端可先用 mock 或最小 core) |
| **P0.5** | 技术尖刺(并行) | core crate / C 工具链 / AVIF 后端 / 许可清单 / **文件访问抽象(为商店留门)**,避免返工 |
| **P1** | 拖拽 + 批量 + 真实转换 | 拖拽、并发批量(rav1e `threads=1` 防 oversubscribe)、进度(Channel)、取消 |
| **P2** | 高级压缩与保真 | 自动质量(**仅 JPEG/WebP**)、ICC/EXIF 透传、代际防护 |
| **P3** | 发布(Linux 优先) | **.deb/.rpm/AppImage + Flathub**;macOS(直分发→MAS)、Windows(→MS Store)后续阶段 |

---

## P0 — UI/UX 外壳(先做,先看)

目标:不接(或假接)后端也能完整演示界面与交互。

> ✅ **脚手架已就位**:Svelte 5 + pnpm/Node LTS + shadcn-svelte(Tailwind v4)+ phosphor-svelte(duotone),`src/App.svelte` 已是一个能拖拽/选文件/调参数/调 `convert_image` 的可运行界面。P0 在此基础上完善设计与交互。

> ✅ **2026-06-30 审计后的 P0 第一优先级已完成(详见 [DEVLOG.md](./DEVLOG.md))。** 原先「没接完的重构 + 旧 libvips 引擎契约」已收口为以下三项:

#### 🔧 前端整顿(P0 优先,三项一起)
- [x] **① 合并到组件化架构**:以 `src/lib/state.svelte.ts` + `Topbar`/`Dropzone`/`SettingsBar`/`QueueItem` 为基准,写真正组装它们的 `App.svelte`,删除老 `App.svelte` 的内联重复逻辑(两套并存会漂移)。
- [x] **② 引擎契约换新架构**:`src-tauri` 删除 `vips` 子进程逻辑,以 path 依赖接入 `imgconvert-core`;`engine_info`→`capabilities()`(可读/可写/无损格式矩阵,Linux 无 HEIC);`convert_image` 走 core 的 `convert(bytes, Format, EncodeOptions)`;前端「引擎状态」文案 + 类型随之更新(目标契约见 DEVLOG)。
- [x] **③ 控件升级 shadcn + 格式选择器**:原生 `<select>/<range>/<checkbox>` 换成 `ui/select`(做成可搜索分类的格式选择器,复刻 vert FormatDropdown 交互,**不抄 AGPL 源码**)/`ui/slider`/`ui/switch`;格式列表由 `capabilities().writable` 驱动,别硬编码。

### 设计系统(基座已有,借鉴 vert 完善)
- [x] 设计 token(`src/app.css`,shadcn-svelte slate 主题,oklch)+ light/dark 变量
- [x] 主题手动切换(`.dark` class 切换)+ 持久化
- [x] 自定义弹性 easing 过渡 + **「关闭动效」开关**(`prefers-reduced-motion`)
- [x] 按文件类型染色体系(图片蓝为主,预留音视频/文档色)
- [x] 图标统一 phosphor duotone(`IconContext`)

### 主界面布局
- [x] 顶栏(标题 + 引擎状态 + 设置入口)
- [x] **拖拽区 + 全屏拖拽彩色模糊覆盖反馈**(拖入时整窗高亮)
- [x] **队列卡片网格**:每文件卡片(缩略图占位 / 文件名 / 进度条 / 单文件格式选择 / 状态徽章 / 移除)
- [x] 顶部工具条:全部转换 / 清空 / 统一设格式 / 选输出目录
- [x] 空状态、批量计数、整体进度条(文件多时出现)

### 格式选择器(复刻 vert 的 FormatDropdown,**最值得做的单点**)
- [x] 搜索 + 分类标签 + 网格布局
- [x] 输入即搜、回车选第一个、源格式/已选高亮
- [x] 桌面下拉浮层(防溢出)/ 窄屏底部抽屉

### 设置面板
- [x] 质量滑块 + 无损开关(随格式启用/禁用)
- [x] 输出目录 / 同目录、覆盖策略(Ask/Overwrite/Skip)
- [x] 文件名模板(`%name%`/`%date%`/`%extension%`)
- [x] 保留/剥离元数据开关
- [x] 设置持久化(Tauri store)

### 交付物
- 一个能拖入文件、看到卡片队列、调参数、点「转换」(此时可调用现有 `convert_image` 或 mock)的完整界面。

> 框架已定:**Svelte 5**(脚手架已就位)。⚠️ 项目为 Apache-2.0,**不可直接搬 vert(AGPL)源码**;FormatDropdown / 全屏拖拽 / 设计 token 只借鉴其交互/视觉思路,用 shadcn-svelte 自行实现;可直接复用的是 DropWebP(MIT)的前端逻辑。

---

## P0.5 — 技术尖刺(与 P0 并行,降风险)

> 高风险点:core crate / AVIF 后端构建 / 文件访问抽象(决定 P1 设计,且关系到日后商店沙盒)/ 许可合规。先各做最小验证。
>
> 🚦 **强制门槛**:进入 P3 打包前必须通过——① 干净 Linux(Debian/Ubuntu/Fedora)上转换跑通;② 依赖树**不含 GPL/AGPL/LGPL**(无 imagequant/dssim/x265);③ `THIRD_PARTY_LICENSES` 可自动生成 + **应用内「开源许可」页**可见;④ 文件访问只走「用户显式授权目录」抽象(为 Flatpak portal / 未来 MAS bookmark 留门)。

- [x] **core crate 尖刺**(2026-06):建 `crates/imgconvert-core`,`Codec` trait + `ImageData`(RGBA8,8-bit SDR,带不变量校验)+ pipeline,接 `mozjpeg`/`oxipng`/`webp`/`image`,跑通 JPEG/PNG/WebP 一轮转换 + 跨格式转换;`image` 已 `default-features=false`(避免带回 AVIF/ravif/rayon/nasm);JPEG 编码 `catch_unwind` 截 Rust panic、alpha 合成白底;WebP 解码 `BitstreamFeatures` 预检拒动图/超尺寸。aarch64 Linux 编译 + clippy(`-D warnings`)+ fmt + 10 测试全过。⚠️ **N5 待办**:`webp 0.3` 仅暴露 `encode_simple(lossless, quality)`,**未暴露 method/near_lossless/sharp_yuv**——P2 高级参数需降级到 `libwebp-sys` 自填 `WebPConfig`。
- [x] **AVIF 后端尖刺**(2026-06):`libavif-sys 0.17`(`codec-rav1e` 编码 + `codec-dav1d` 解码,`default-features=false`)接入 core,`AvifCodec` 实现编解码 + magic 检测(`ftyp`/avif·avis)。**aarch64 Linux 全链编译通过**(rav1e 纯 Rust、dav1d 走 meson/ninja,arm64 无需 nasm,首次 ~1min)。验证:**alpha 往返**(YUV444 + RGBA)、**ICC 逐字节往返**(`avif_preserves_icc` 测试,证实弃用裸 ravif 的主因已解决,评审 #8)、**convert 管线 PNG↔AVIF**。FFI 用 RAII guard(`ImageGuard`/`EncoderGuard`/`DecoderGuard`/`RwDataGuard`)保证各返回路径释放 C 资源。**后端可插拔点**:`avifEncoder.codecChoice`(当前 `AVIF_CODEC_CHOICE_RAV1E`;切 aom/svt 只需改该枚举 + Cargo feature)。⚠️ 待办:(a) **N3** `maxThreads=1` 已设但未证实压住 rav1e 内部 rayon 池(需 macOS/多核机实测线程数);(b) AVIF 真·无损需 identity matrix,当前不在 `capabilities().lossless` 声明;(c) speed=8 默认值待 arm64 实测(评审 #2)。
- [ ] **C 工具链尖刺**:Linux 三发行版 + arm64 各编译通过——NASM(x86)+ cmake/meson/ninja;**NASM 装好后加版本检测,失败给明确错误**。⚠️ **arm64 用原生 runner,别交叉**(Claude N4:dav1d meson cross-file + cmake toolchain 是出名的坑,无 Linux-arm64 交叉先例);若必须交叉则列为高风险待验证。
- [ ] **文件访问抽象尖刺(新增,前移)**:把「读输入目录 / 写输出目录」抽象成显式授权模型,先在 **Flatpak portal** 下跑通(拿到的可能是 portal 路径,非真实路径);确保该抽象**日后能换 macOS security-scoped bookmark**。← 决定 P1 文件 API,不能拖到 P3(评审一致)。
- [ ] **并发尖刺**:文件级信号量并发下不 oversubscribe / 不 OOM(评审 #4)。⚠️ **验收(Claude N3)**:设 libavif `maxThreads=1` 后**实测 rav1e 是否仍另起自己的 rayon 全局池**(`libavif-sys` 的 maxThreads 控的是 libavif tile 线程,未必压住 rav1e)——若是,需显式设 rav1e 线程池或关其 threading 特性。
- [x] **许可清单尖刺**:`cargo-about` 生成 `THIRD_PARTY_LICENSES`;**做应用内「开源许可」页**(全文,含 IJG/BSD/Apache NOTICE);`cargo deny` 禁 GPL/AGPL/LGPL。npm 侧读取已安装包的 LICENSE/NOTICE/COPYING 文件并纳入生成物;少量缺失文件的 npm 包在生成物中标记,发布前人工复核。
- [x] **进度/取消协议**:Tauri **Channel** + `CancellationToken` 最小闭环。当前 `convert_batch` 串行执行,取消在文件边界生效;P1 再接文件级并发/信号量/内存预算。
- [ ] **最小 CI**:`cargo fmt --check`、`cargo clippy`、`cargo test`、`cargo deny check`、`pnpm run check`、`pnpm run build`。
- [ ] **(macOS 阶段)系统 HEIC 尖刺**:`objc2`/`core-graphics` 调 ImageIO 读写 HEIC,**且必须在 App Sandbox 内验证编码成功**(评审:沙盒内 HEVC 编码能否用需实测)。
- [ ] **(macOS 阶段)rav1e arm64 实测**:M 系列上 benchmark AVIF speed 8/10,对比 ImageIO/svt-av1,再锁默认值(评审 #2,最重要的待实测项)。
- [ ] **(macOS 阶段)security-scoped bookmark shim**:**Tauri 无内建支持**(核心 issue [#3716](https://github.com/tauri-apps/tauri/issues/3716) 自 2022 至今未解)→ 需自写 `objc2` 的 `startAccessingSecurityScopedResource`/`stop` 生命周期(忘 stop 会泄漏内核资源并丢失越沙盒能力)。这是「用户显式授权目录」抽象的 macOS 落地。`tauri-plugin-dialog` 能返回 bookmark 数据,但 start/stop 自理。MAS 本身已验证可行(官方文档 + 真实上架案例)。

## P1 — 拖拽 + 批量 + 真实转换

- [ ] Tauri `tauri://drag-drop` 原生拖拽(拿绝对路径,改写自 DropWebP MIT)
- [ ] 剪贴板粘贴导入
- [ ] 递归目录扫描 + 扩展名过滤 + 去重(改写自 DropWebP)
- [ ] **Rust 端并发批量**(全局任务队列 + 信号量限并发)——替换当前串行
  - 外层全局并发上限(默认 `(num_cpus-1).clamp(1,8)`,信号量控流)+ 内存预算 + 用户可调并发;大图场景降并发。
  - 进程内无子进程,不存在 vips「多进程×多线程」过度并发问题,但 `libavif`(rav1e)/`oxipng` 本身吃内存且内部多线程(设 maxThreads=1),仍需控流。
- [ ] **进度/取消统一走 Tauri Channel**(有序、低延迟、按调用作用域;`{index, percent, stage, status}`)。取消 = `CancellationToken`(见 ENGINE.md §7)。P0.5 已落 `convert_batch`/`cancel_batch` 最小闭环;P1 还需并发接入后把 `ask` 覆盖策略也纳入统一协议。
- [ ] 批处理三态(成功/跳过/错误)+ 单张失败不中断 + 末尾汇总
- [ ] **格式选择器由 core 支持矩阵驱动**(core 暴露可读/可写格式),别硬编码
- [ ] 导入 ping 尺寸/DPI(`image` reader);异步生成缩略图
- [ ] **原子写 + 保留时间戳 + 保留目录结构**(借鉴 caesium)
- [ ] HEIC:⚠️ **v1(Linux)不含 HEIC**(Codex:此处与 Linux-first 冲突已修正);macOS/Windows 的系统原生 HEIC 是**后续平台阶段**任务(见 P3 + ENGINE.md §3)
- [ ] **文件可靠性**:失败清理半成品;同名冲突策略;**EXIF orientation 真旋正**;ICC/元数据保留或显式剥离(见 ENGINE.md §5);超大图内存上限;符号链接/权限错误友好处理;传参**传路径不传字节**。

---

## P2 — 高级压缩与保真

- [ ] **per-format 参数**(在 core 已接 mozjpeg/oxipng/webp/libavif-sys 之上):quality、progressive、oxipng level、AVIF speed、WebP method;默认值取自 Hando bench(oxipng=4 / AVIF speed=8 / WebP method=4,**arm64 待实测**)
- [ ] 全局有损/无损开关 + 每格式质量下限阈值(TIFF 为 **P2 可选**,非 v1 承诺,与顶部「TIFF 推后」一致)
- [ ] **有损 PNG 量化用宽松库**(`color_quant`/`image` 内置,⚠️ **不用 imagequant/GPL**)
- [ ] **「多候选取最小 + skip-if-larger / 永不变差」**(借鉴 ImageOptim + Hando keep_bar)
- [ ] **自动质量(仅 JPEG/WebP)**:`ssimulacra2`(宽松)感知打分 + 二分搜索压到目标分(借鉴 Hando `auto.rs`,重写)
- [ ] **ICC/EXIF 透传容器手术**(JPEG APP2 分块 / WebP RIFF / PNG eXIf 排序;**AVIF 走 libavif 已能保留 ICC**;借鉴 Hando `icc.rs`/`metadata.rs`,重写)
- [ ] **代际损失防护**(bpp 分级,已有损源不重压;借鉴 Hando `auto.rs`)
- [ ] 结果缓存(设置哈希 + 文件 blake3 哈希)跳过已优化(借鉴 springbok)
- [ ] 高级参数面板(AVIF speed/subsample、WebP near_lossless/sharp_yuv、MozJPEG trellis 等)
- [ ] ⚠️ **不做**:JPEG XL(评审一致,过早);有损 PNG 量化(`color_quant`)仅标「实验性」,PNG 默认 oxipng 无损
- [ ] **CI 进阶**:npm license audit、Tauri build smoke test、`cargo-about` 自动生成、依赖树 GPL/AGPL/LGPL 拦截、干净机器冒烟。

---

## P3 — 发布(Linux 优先,商店留门)

> 评审一致:个人/小团队不要四渠道并行。先 Linux 直发,验证需求后逐步上 macOS / Windows 商店。

**v1(Linux):**
- [ ] CI 矩阵:Linux × **amd64 + arm64**;C 工具链 NASM + cmake/meson/ninja(见 ENGINE.md §4)
- [ ] 打包:**.deb(Debian/Ubuntu)+ .rpm(Fedora)+ AppImage**;注意各发行版 **webkit2gtk / glibc 版本差异**
- [ ] **Flathub**:Flatpak manifest + **文件 portal**(P0.5 已验证目录授权抽象);离线构建用 cargo vendor + 预构建前端产物;**Flathub 版不含 HEIC**
- [ ] 自动更新(AppImage 用 updater;Flatpak 由 Flathub 托管)

**后续阶段(留门,不阻塞 v1):**
- [ ] **macOS**:直分发 `.dmg`(Developer ID + notarytool 公证)起步 → 验证后再 MAS(App Sandbox + security-scoped bookmarks + Apple Distribution + provisioning + `.pkg`);HEIC 完整;AVIF 跑 arm64 实测后定后端
- [ ] **Windows**:直分发 `.msi/.exe` 起步 → 后 MS Store(MSIX + `runFullTrust`);HEIC **仅解码**(WIC + 运行时探测 HEVC 扩展,缺失则引导安装,**不承诺开箱即用**)
- [ ] ⚠️ **架构前提(全程保持)**:无子进程、Apache-2.0、依赖树无 GPL/AGPL/LGPL、文件访问走显式授权抽象 → 这样 v1 之后上商店不返工

---

## 评审修正清单(2026-06-29 三方外部评审纳入)

> 三份独立模型评审 + crates.io 实查后确认并采纳。**驳回的 FUD**:ssimulacra2 实为 BSD-2-Clause(非 WTFPL/GPL),干净可用。

| # | 评审发现 | 处理 | 落点 |
|---|---|---|---|
| 1 | 禁 LGPL 与 Linux libheif 自相矛盾 | **Linux v1 不含 HEIC** | 本文件 / LEGAL |
| 2 | rav1e 无 arm64 汇编,Apple Silicon AVIF 慢 | Linux x86 v1 不咬人;macOS 阶段实测后定后端 | P0.5 / ENGINE |
| 3 | ssimulacra2 二分搜索 × AVIF 不可用;PNG 无 quality 可搜 | 自动质量**仅 JPEG/WebP** | P2 |
| 4 | 文件级并发 × rav1e 内部线程 oversubscribe | rav1e `threads=1` | P0.5 / P1 |
| 5 | Windows HEIC 仅解码、需 HEVC 扩展 | 运行时探测 + 不承诺开箱即用,不编码 | P3 |
| 6 | 沙盒约束反向决定文件 API,晚验证会返工 | 文件访问抽象**前移 P0.5** | P0.5 |
| 7 | NOTICE/IJG/BSD 归属 | **应用内「开源许可」页** | P0.5 |
| 8 | ravif/image 容器元数据(ICC/EXIF/nclx)控制弱(alpha 其实能处理) | **AVIF 改用 libavif-sys**(DropWebP 路线) | ENGINE |
| 9 | HEVC 专利:系统 API ≠ 法律免责 | 文案按平台能力如实写;商用前找 IP 律师 | LEGAL |
| 10 | color_quant 质量 < imagequant | PNG 默认无损;有损量化标实验性 | P2 |
| 11 | NASM 2.15.05 钉版本脆弱 | build 加版本检测 + 明确报错 | P0.5 / ENGINE |
| 12 | JPEG XL 过早 | 删除 | P2 |

---

## 技术栈版本(2026-06-29 核对并锁定)

| 组件 | 锁定版本 | 备注 |
|---|---|---|
| Rust | **1.96.0**(stable) | `src-tauri/rust-toolchain.toml`(Rust 无 LTS) |
| Tauri CLI / api | **2.11.4** / **2.11.1** | crate `tauri` 2.11.x |
| Vite | **8.1.0** | ⚠️ Vite 8 默认 Rolldown,较新 |
| TypeScript | **6.0.3** | 6.0 是面向 TS 7(原生编译器)的过渡版 |
| 图像引擎 | **进程内 Rust crate**(混合架构) | `mozjpeg`/`oxipng`/`webp`/**`libavif-sys`(codec-rav1e)**/`image` + 按平台系统 HEIC;**取代 libvips**;详见 ENGINE.md |
| C 构建工具链 | **NASM**(macOS 钉 2.15.05;装后做版本检测)+ cmake + meson/ninja | rav1e/mozjpeg/libavif/libwebp 静态链接需要;Linux 上 webkit2gtk 仍为动态系统库 |
| 许可证 | **Apache-2.0** ✅ | 为上架从 AGPL 转;含专利授权;禁 GPL/AGPL/LGPL 依赖(deny.toml) |
| 包管理/运行时 | **pnpm 10 + Node LTS** ✅ | `package.json#packageManager` + `pnpm-lock.yaml`;Rust 后端不受影响 |
| 前端框架 | **Svelte 5(runes)** ✅ | `@sveltejs/vite-plugin-svelte` 7.x |
| UI 组件库 | **shadcn-svelte 1.3 + Tailwind v4** ✅ | `components.json` 已配;`pnpm dlx shadcn-svelte add <c>` 加组件 |
| 图标 | **phosphor-svelte 3.1(duotone)** ✅ | 全局 `IconContext weight="duotone"` |

> 已写入 `package.json` / `rust-toolchain.toml`,并经 **`pnpm run check`(0 错)+ `pnpm run build`** 验证通过。
> ⚠️ **Fallback**:TS 6 / Vite 8 都较新(TS 6 是面向 TS 7 的过渡版,Vite 8 用 Rolldown)。若与 Tauri / Svelte 插件链出现兼容问题,短期回退到 **TS 5.9 / Vite 7**。

## 已定技术决策

- **前端框架:Svelte 5(runes)** —— 轻、快、组件化省事。⚠️ **注意**:转宽松许可后,**不能再直接搬 vert(AGPL)源码**;只借鉴其交互/布局思路,自行实现。
- **包管理:pnpm + Node LTS** —— Tauri 桌面复杂度主要在 Rust/Cargo/WebView/系统依赖/签名打包;前端不依赖 Bun runtime。`beforeDevCommand`/`beforeBuildCommand` 用 `pnpm run`,锁文件只保留 `pnpm-lock.yaml`。
- **UI 库:shadcn-svelte 1.3 + Tailwind v4** —— 组件即代码(复制进仓库,可改),`components.json` 已配;`src/app.css` 是设计 token 单一出处(改色/圆角只此一处)。已内置 `Button`,其余用 `pnpm dlx shadcn-svelte add <component>`。
- **图标:phosphor-svelte 3.1,默认 duotone** —— 在 `App.svelte` 用 `<IconContext values={{ weight: "duotone" }}>` 全局设默认。

### 当前脚手架文件结构
```
index.html              # 挂载点 #app
src/main.ts             # Svelte mount()
src/App.svelte          # 主界面(已接 convert_image 后端)
src/app.css             # Tailwind v4 + shadcn 设计 token(改主题在此)
src/lib/utils.ts        # cn() 助手
src/lib/components/ui/   # shadcn-svelte 组件(button 已加)
components.json          # shadcn-svelte CLI 配置
svelte.config.js / vite.config.ts / tsconfig.json
```

> ⚠️ 复用提示(已随转宽松更新):**vert 是 AGPL,源码不可直接搬入宽松项目**——FormatDropdown、全屏拖拽遮罩等只借鉴交互/视觉思路,用 shadcn-svelte 自行实现。可直接复用的前端是 **DropWebP(MIT)** 的拖拽/批量逻辑(去 Vue 化)。
