<!-- SPDX-License-Identifier: Apache-2.0 -->
# DEVLOG — 开发记录

> 倒序记录关键进展与决策。详细阶段计划见 [ROADMAP.md](./ROADMAP.md);引擎设计见 [ENGINE.md](./ENGINE.md);许可见 [LEGAL.md](./LEGAL.md)。

---

## 2026-06-30 — P1 异步缩略图最小闭环

Codex 完成 P1「异步生成缩略图」的第一版可运行闭环:

- **core 缩略图接口**:`imgconvert-core::thumbnail(bytes, max_edge)` 复用现有 JPEG/PNG/WebP/AVIF 解码器,按最长边缩放并输出小 PNG;全透明图片返回 `None`,前端保留格式占位。
- **Tauri 缩略图命令**:新增 `generate_thumbnail(options)`。前端只传已导入的本机路径,后端在 blocking 线程读取文件并返回 `{ mime, width, height, bytes }`;缩略图最大边限制在 `32..512`。
- **前端异步懒加载**:队列卡片进入视口附近才请求缩略图,全局并发固定为 2;返回字节转 Blob URL 展示,移除/清空队列时释放 URL。
- **卡片展示**:原先格式占位升级为稳定尺寸的预览区;缩略图加载中显示小 spinner,失败或全透明时继续显示格式图标,不影响转换状态。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL,第三方许可生成物保持最新。
- `cargo test -p imgconvert-core`:通过(16 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(22 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

限制:

- 当前缩略图不做磁盘缓存;重新导入同一文件会重新生成。
- 缩略图仍需完整解码源图,只是通过懒加载和并发 2 控制 CPU/内存峰值;后续大目录可再接虚拟列表或持久缓存。

## 2026-06-30 — P1 导入元数据 ping 最小闭环

Codex 完成 P1「导入阶段尺寸/DPI ping」的第一版可运行闭环:

- **core 头部探测接口**:`imgconvert-core::probe(bytes)` 返回 `ImageProbe { format, width, height, dpi }`,覆盖 PNG/JPEG/WebP/AVIF。PNG 解析 `pHYs` DPI,JPEG 解析常见 JFIF density;WebP/AVIF 当前返回尺寸,DPI 为空。
- **导入扫描携带元数据**:`scan_import_paths` 对每个候选文件限量读取前 512 KiB 做头部 ping;探测失败不阻断导入,只让 `metadata` 为空。返回 `ImportScanFile { path, key, metadata }`。
- **前端队列展示**:队列项保存导入元数据,卡片在文件路径下显示 `宽×高` 与 DPI;源格式优先使用后端 magic 探测结果,扩展名只作回退。
- **review 修复**:并发批量启动前新增输出路径预检,同一 batch 中重复目标路径会在 worker 启动前报错,避免 overwrite/skip/no-clobber 在文件级并发下出现抢写竞态。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `cargo test -p imgconvert-core`:通过(14 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(20 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo fmt --check`:通过。

限制:

- 当前只做导入元数据 ping,尚未生成缩略图。
- DPI 只覆盖 PNG `pHYs` 与 JPEG JFIF density;EXIF Resolution 标签、WebP/AVIF 容器级 DPI 后续随元数据保留一起扩展。
- Tauri 扫描阶段最多读取每个文件前 512 KiB;极端 JPEG/AVIF 若尺寸信息晚于该范围,会保守返回空 metadata,不影响导入。

## 2026-06-30 — P1 并发批量最小闭环

Codex 完成 P1「Rust 端文件级并发批量」的第一版可运行闭环:

- **受控文件级并发**:`convert_batch` 从串行循环改为全局工作队列 + 固定 worker 上限。默认并发为 `available_parallelism - 1` 后 clamp 到 `1..8`;前端新增「并发」滑块,`0` 表示自动,`1..8` 表示手动上限。
- **Channel 汇聚**:worker 不直接操作 Tauri Channel,而是把单文件事件发送给 Rust coordinator;coordinator 统一按接收顺序发 `fileStarted` / `fileProgress` / `fileFinished` / `fileSkipped` / `fileError` / `finished`,前端既有队列进度处理保持不变。
- **取消语义**:取消仍使用 `CancellationToken`,worker 在文件边界停止领取新任务;正在编码的单文件结束后再汇报。`ask` 覆盖策略仍走前端逐文件确认路径,并发批量覆盖 skip/overwrite 路径。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL,第三方许可生成物保持最新。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(18 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo test -p imgconvert-core`:通过(12 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

限制:

- 尚未实现按图片尺寸/内存预算动态降并发。
- `ask` 覆盖确认尚未纳入后端统一批量协议。

---

## 2026-06-30 — P1 文件导入层最小闭环

Codex 完成 P1「拖拽/文件夹导入」的第一版可运行闭环:

- **Rust 导入扫描命令**:新增 `scan_import_paths(options)`。前端传入用户显式选择/拖拽的文件或目录,后端用 `std::fs` 显式栈扫描目录,由 core 可读格式自行派生扩展名过滤,并以规范化路径去重。扫描结果返回 `ImportScanResult { files: [{ path, key }], skipped, errors, truncated, cancelled, limitReason }`,权限/缺失路径等错误按条目记录,不让单个坏路径中断整批导入。
- **扫描防护与取消**:扫描默认限制为 20k 文件、100k 路径条目、64 层深度(后端硬上限继续兜底);超过限制会返回截断原因。新增 `cancel_import_scan()` + `ImportScanState`,前端导入中可取消扫描,取消结果不把半批文件塞入队列。
- **符号链接边界**:普通符号链接文件可作为文件导入;符号链接目录不递归,避免目录循环和越过用户明确授权边界。
- **前端统一入口**:`Dropzone` 增加「选择文件夹」,文件选择、文件夹选择、Tauri 原生拖拽都统一调用 `importPaths()`,并显示已添加/重复/跳过/错误数量。队列使用后端返回的 canonical `key` 跨批次去重;导入错误可展开查看前几条明细。导入扫描期间禁止清空、移除、转换和设置改动,避免队列竞态。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL,第三方许可生成物保持最新。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(16 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo test -p imgconvert-core`:通过(12 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

限制:

- Flatpak portal 与 macOS security-scoped bookmark 尚未接入;当前只是把输入导入路径收口到可替换的 Rust 边界。
- 剪贴板粘贴、导入尺寸/DPI ping、异步缩略图仍在 P1 待办。

---

## 2026-06-30 — P0.5 许可合规最小闭环

Codex 完成 P0.5「许可清单尖刺」的第一版可运行闭环:

- **Rust 许可闸门**:`src-tauri/deny.toml` 现在可由 `pnpm run license:rust` 调用 `cargo deny check licenses`,继续禁止 GPL/AGPL/LGPL,并显式放行当前依赖树里确认过的宽松 SPDX(IJG / NCSA / Apache-2.0 WITH LLVM-exception)。
- **第三方许可产物**:新增 `src-tauri/about.toml` / `about.hbs` 与 `scripts/generate-third-party-licenses.mjs`;`pnpm run license:third-party` 会用 `cargo-about` 生成 Rust 许可全文,并从已安装 npm 包读取 `LICENSE` / `NOTICE` / `COPYING` 等文件全文,写入根目录和 `public/THIRD_PARTY_LICENSES.md`。
- **npm 许可扫描**:新增 `scripts/check-npm-licenses.mjs`;`pnpm run license:npm` 基于 pnpm 自带 JSON 输出拦截 GPL/AGPL/LGPL,不额外引入 npm 审计依赖。
- **生成物校验**:`pnpm run license:check` 会继续跑 Rust/npm 禁止许可扫描,并通过 `pnpm run license:verify` 检查 `THIRD_PARTY_LICENSES.md` 与 `public/THIRD_PARTY_LICENSES.md` 是否已按当前依赖树更新。
- **应用内许可页**:顶栏新增「开源许可」入口,弹层按需加载 `public/THIRD_PARTY_LICENSES.md` 文本,避免把 1.1 万行许可文本塞进首屏 JS,同时满足二进制用户可见的基础归属要求。

验证:

- `pnpm run license:npm`:通过,未发现 GPL/AGPL/LGPL。
- `pnpm run license:rust`:通过,`cargo deny` licenses ok。
- `pnpm run license:third-party`:通过,生成 `THIRD_PARTY_LICENSES.md`。
- `pnpm run license:verify`:通过,生成物与当前依赖树一致。
- `pnpm run check`:通过(0 errors / 0 warnings)。

限制:

- 仍有少量 npm 包未在安装目录提供 LICENSE/NOTICE 文件;脚本会在生成物中标记这些包,发布前需人工复核。
- `cargo-about` 已覆盖 Rust crate 许可全文,但 C 库 NOTICE/IJG 段仍需在发布候选阶段人工抽查一遍。

---

## 2026-06-30 — P0.5 批量任务协议最小闭环

Codex 完成 P0.5「进度/取消协议」的第一版可运行闭环:

- **Tauri 批量命令**:新增 `convert_batch(options, progress)` 与 `cancel_batch()`。批量任务状态由 `BatchState` 管理,同一时间只允许一个活动 batch;取消使用 `tokio_util::sync::CancellationToken`,当前阶段在文件边界生效。
- **Channel 进度事件**:`BatchProgressEvent` 覆盖 started / fileStarted / fileProgress / fileFinished / fileSkipped / fileError / cancelled / finished。返回 `BatchSummary { total, completed, skipped, failed, cancelled }`。
- **前端接入**:`convertAll()` 在 skip/overwrite 模式走 Rust batch + Channel 更新每个队列项进度;`ask` 覆盖策略保留逐文件确认路径,避免后端单向 Channel 里做交互阻塞。新增「取消 / 取消中」按钮,取消后未开始或运行中的条目回到 pending 并显示「已取消」。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `cargo test -p imgconvert-core`:通过(12 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(4 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。

限制:

- 当前 batch 仍是串行执行,只是协议从“前端循环调用单张命令”前移到 Rust;P1 的文件级并发、信号量、内存预算仍待做。
- 单张 core encode/decode 内部还不可中断;取消会在当前文件结束后生效。

---

## 2026-06-30 — P0 前端整顿落地

Codex 完成 P0「目标契约」三项:

- **组件化 App 接通**:`src/App.svelte` 现在只负责组装 `Topbar` / `Dropzone` / `SettingsBar` / `QueueItem`,队列、设置、引擎状态统一走 `src/lib/state.svelte.ts`;旧 App 内联拖拽/队列/转换逻辑已移除。
- **Tauri 引擎契约换 core**:`src-tauri` 通过 path 依赖接入 `crates/imgconvert-core`;`capabilities()` 返回 JPEG/PNG/WebP/AVIF 可读/可写矩阵、PNG/WebP 真无损矩阵与 `heic:false`;`convert_image` 读文件字节后调用 `imgconvert_core::convert(bytes, Format, EncodeOptions)` 并保留输出目录、覆盖策略、文件名模板等 P0 行为。
- **shadcn 控件 + 格式选择器**:目标格式选择器自行实现搜索、分类、源格式/已选高亮、回车选择第一个匹配项,窄屏以底部抽屉样式展示;质量、无损、覆盖策略切到 `ui/slider` / `ui/switch` / `ui/select`,格式列表由 `capabilities().writable` 驱动。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `cargo test -p imgconvert-core`:通过(10 tests)。
- `cargo fmt --check` 与 `cargo fmt --manifest-path src-tauri/Cargo.toml --check`:通过。
- 本地网页预览 `http://127.0.0.1:1420/`:已用 headless Chromium 截图确认能显示演示队列卡片。
- `cargo check --manifest-path src-tauri/Cargo.toml`:当前环境缺少系统 `dbus-1.pc`(`libdbus-1-dev`/`dbus-devel` + `pkg-config`),在 `libdbus-sys` 构建脚本处阻断;需在完整 Tauri Linux 开发环境重跑。

Review 修复:

- `Ask` 覆盖策略改为前端确认后以 `overwrite` 重试;取消则按跳过处理。
- `preserveMetadata` 在 P2 实现前强制保持关闭,后端收到 `true` 会返回明确错误,避免静默丢元数据。
- 后端写文件改为唯一临时文件;非覆盖路径用 no-clobber 链接落盘,降低 skip/ask 与后续并发下的竞态覆盖风险。
- `convertAll()` 改为按每个队列项的实际目标格式校验,不再用全局格式一票否决单文件转换。

---

## 2026-06-30 — 前端审计:发现「没接完的重构 + 旧引擎契约」

对 `src/` 做了一次盘点,发现三件需要在 P0 优先理顺的事:

1. **真正在跑的是 `App.svelte`(266 行,自包含)**。`main.ts` 挂载它;它只用了 `Button`,把拖拽 / 控制条 / 队列全写在一个文件里,控件是原生 `<select>` / `<input range>` / `<checkbox>`(没用已就位的 shadcn Select/Slider/Switch)。

2. **存在一套更完整但「孤儿」的重构,没被任何入口引用**:
   - `src/lib/state.svelte.ts`(集中 runes 状态 + 主题浅/深/跟随 + 减弱动效 + skip 跳过 + Tauri Store 持久化)
   - 组件 `Topbar` / `Dropzone` / `QueueItem` / `SettingsBar`(均 `import` 自 `$lib/state.svelte`)
   - **缺一个把它们组装起来的 `App.svelte`**,所以这套更好的代码当前是死代码。两套逻辑并存会漂移。

3. **前端 + 后端仍说旧的 libvips 黑话**,与已锁定的进程内混合架构不符:
   - `src-tauri/src/convert.rs` 顶部已自标**「已弃用(2026-06-29)」**,实质是 `vips` CLI 子进程引擎。
   - `engine_info` 返回 `vipsAvailable / vipsVersion / heicEncode`;前端提示「未检测到 libvips,请 brew install vips」。
   - `imgconvert-core` 尚未接入 `src-tauri`(原计划 P1)。今天启动只会显示红色「未检测到 libvips」,点转换会去 shell 调 `vips`。

**决策**:P0 第一件事 = 前端整顿,三项一起做(下方「目标契约」给出落地标准)。交由 Codex 接手(本机有 webkit/桌面环境,可 `pnpm run tauri dev` / 编译 `src-tauri`;本仓库容器无法 `cargo check` src-tauri)。

### 目标契约(Codex 落地标准)

**Tauri 命令(`src-tauri/src/lib.rs` + 新 `convert.rs`,改为调用 `imgconvert-core`,删除所有 `vips` 子进程逻辑):**

- `capabilities() -> Capabilities`(**取代** `engine_info`):由 core 能力推导,形如
  `{ readable: ["jpeg","png","webp","avif"], writable: ["jpeg","png","webp","avif"], lossless: ["png","webp"], heic: false }`(Linux v1 无 HEIC;AVIF 暂不声明真无损)。前端「引擎状态」文案据此改写,不再提 libvips。
- `convert_image(options) -> ConvertResult`:读输入文件字节 → `imgconvert_core::convert(bytes, Format, EncodeOptions{quality,lossless})` → 写输出(沿用 `outDir` / 覆盖·skip / 文件名规则)。`ConvertResult { input, output, inSize, outSize }` 保持不变,前端压缩比展示无需改。
  - 接 core:`src-tauri/Cargo.toml` 加 `imgconvert-core = { path = "../crates/imgconvert-core" }`;注意 src-tauri 仍排除在 workspace 外(见根 `Cargo.toml`),用 path 依赖即可。
- 格式映射:字符串 ↔ `imgconvert_core::Format`(`jpeg`/`png`/`webp`/`avif`)。`heic`/`tiff` 暂标不可写(Linux v1)。

**前端架构(以 `state.svelte.ts` + 4 组件为基准):**

- 写一个真正的 `App.svelte`,组装 `Topbar` + `Dropzone` + `SettingsBar` + 队列(`QueueItem`),状态全部走 `$lib/state.svelte`,**删掉老 App.svelte 里重复的内联逻辑**。
- `state.svelte` 里 `checkEngine()` 改为调 `capabilities()`,`EngineInfo` 类型替换为 `Capabilities`。
- 原生控件换 shadcn:目标格式用 **格式选择器**(复刻 vert FormatDropdown 的交互:搜索 + 分类 + 源格式/已选高亮 + 回车选第一个;**不抄 AGPL 源码**,用 `src/lib/components/ui/select` 自行实现),质量用 `ui/slider`,无损/覆盖用 `ui/switch`。
- 格式列表由 `capabilities().writable` 驱动,别硬编码。

---

## 2026-06(P0.5 技术尖刺)— 进程内编解码核心打通

详见 ROADMAP P0.5(已勾)。要点:

- **core crate**:`crates/imgconvert-core`,`Codec` trait + `ImageData`(RGBA8 / 8-bit SDR / 不变量校验)+ `convert` 管线。接 `mozjpeg`(JPEG)/`oxipng`(PNG 无损)/`webp`(libwebp)/`image`(解码,`default-features=false`)。JPEG 编码 `catch_unwind` 截 Rust panic、alpha 合成白底;WebP 解码 `BitstreamFeatures` 预检拒动图/超尺寸。aarch64 Linux:编译 + clippy(`-D warnings`)+ fmt + 测试全过。
- **AVIF 后端**:`libavif-sys 0.17`(`codec-rav1e` 编码 + `codec-dav1d` 解码,`default-features=false`)。`AvifCodec` 编解码 + magic 检测(`ftyp`/avif·avis)。**arm64 全链编译通过**(rav1e 纯 Rust、dav1d 走 meson/ninja,无需 nasm)。验证 **alpha 往返** + **ICC 逐字节往返**(`avif_preserves_icc`,坐实弃用裸 ravif 的主因)+ convert 管线 PNG↔AVIF。FFI 用 RAII guard 释放 C 资源。可插拔后端点:`avifEncoder.codecChoice`。
- **未决**:N3(`maxThreads=1` 是否压住 rav1e rayon 池,需多核实测)、AVIF 真·无损(需 identity matrix,P2)、speed 默认值 arm64 实测、N5(`webp 0.3` 不暴露 method/near_lossless/sharp_yuv,P2 降到 `libwebp-sys`)。
