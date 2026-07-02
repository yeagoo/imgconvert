<!-- SPDX-License-Identifier: Apache-2.0 -->
# DEVLOG — 开发记录

> 倒序记录关键进展与决策。详细阶段计划见 [ROADMAP.md](./ROADMAP.md);引擎设计见 [ENGINE.md](./ENGINE.md);许可见 [LEGAL.md](./LEGAL.md)。

---

## 2026-07-02 — 图像管线后续增强:质量 heuristics 第一批 + AVIF lossless guardrail

Codex 继续推进 P2/P3 后的图像管线增强,本批先落地可自动测试、无新增许可风险的守门项:

- **AVIF 真无损负向 guardrail**:`imgconvert-core` 新增 `AVIF_LOSSLESS_SUPPORTED=false` 常量和单测,继续保证 AVIF 不进入 `LOSSLESS_FORMATS` / capabilities lossless。当前 rav1e 后端没有完成像素级可逆验证,因此不会用 `quality=100` 冒充真无损。
- **自动质量耗时上限**:core 新增 `AUTO_QUALITY_MAX_SCORING_EVALUATIONS=7`,并给二分搜索评分次数加测试。JPEG 最坏 6 次 SSIMULACRA2 评分;WebP 额外比较一次 lossless 候选。搜索失败回退最高质量时复用已评估的 max 候选,避免重复编码/评分。
- **假无损 PNG hint**:core 新增 `detect_lossy_artifacts()` 第一版,检测 PNG 中明显 JPEG 8x8 网格痕迹。Tauri 代际损失防护在 `generationLossProtection=true` 时把这类 PNG 当作有损来源处理;该结果只是保守 hint,不改变默认可读/可写格式矩阵。
- **诊断暴露**:`runtime_diagnostics()` 增加自动质量最大评分次数,便于后续 UI/日志展示。

仍未完成:

- 色彩管线 v2(`PixelBuffer { U8, U16, F32 }`、ICC transform、线性 resize、16-bit/HDR)仍未开始。
- 语义级 metadata 模块、HEIC/helper metadata sidecar、AVIF/WebP 平台 benchmark 仍在后续项。

---

## 2026-07-02 — 图像管线后续增强规划 + XMP 透传第一批

Codex 梳理 P2/P3 后的图像管线剩余增强项后,先落地一批低风险、Linux 可验证、无新增许可风险的 metadata fidelity 增强:

- **后续方案拆分**:ROADMAP 新增“图像管线后续增强路线”,把 AVIF 真无损、`8/16/float` 像素表示 + ICC transform/线性 resize、语义级 metadata、HEIC/helper metadata passthrough 和质量 heuristics 拆成独立后续项。当前不把 RGBA8 管线伪装成像素级色彩管理。
- **XMP raw packet 透传**:`imgconvert-core` 的 `ImageData` / `Metadata` 增加 `xmp` 字段。JPEG 支持 APP1 Adobe XMP namespace,PNG 支持未压缩 `iTXt XML:com.adobe.xmp`,WebP 支持 `XMP ` chunk 与 VP8X XMP flag。默认仍剥离;只有 `preserveMetadata=true` 才写回。
- **容器替换语义**:PNG/WebP 写回时会替换旧 XMP chunk,避免重复堆叠;WebP 重建 VP8X 时按当前输出 alpha 与 metadata 重新置位 ICC/EXIF/XMP flags。
- **边界保持清晰**:AVIF 仍只保留 ICC/EXIF,XMP 暂不接入;XMP 只做 raw packet 透传,不解析 XML,不改写 XMP 内可能存在的 orientation/IPTC/编辑历史字段;JPEG extended XMP 分片与 PNG 压缩 `iTXt` XMP 暂不保留。
- **测试覆盖**:core 单测扩展 JPEG/PNG/WebP metadata 默认剥离/开启保留断言,并新增 PNG 源跨 JPEG/PNG/WebP 的 XMP 保留测试。

验证:

- `cargo +1.96.0 fmt --all -- --check`:通过。
- `cargo +1.96.0 test -p imgconvert-core`:通过。
- `cargo +1.96.0 clippy -p imgconvert-core -- -D warnings`:通过。
- `pnpm run format:check`:通过。
- `pnpm run check`:通过。

---

## 2026-07-02 — P3 review 修复:Flatpak 真实运行闭环 + Display P3 ICC 端到端测试

Codex review Flatpak 与色彩保真剩余缺口后补齐两项可复跑护栏:

- **Flatpak 真实运行 smoke**:新增 `scripts/smoke-flatpak-runtime.mjs` 与 `pnpm run release:flatpak:smoke`。脚本会准备 source archive、添加缺失的 Flathub user remote、用 `flatpak-builder --user --install-deps-from=flathub --install` 构建/安装,再分别通过 `flatpak-builder --run` 和安装后的 `flatpak run --user --command=imgconvert` 运行隐藏转换 smoke。Flatpak 主包仍设置 `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1`,不启用 HEIC/helper。
- **review 修复:Flatpak runtime EOL**:本机 smoke 首跑时 Flathub 提示 `org.gnome.Sdk 48` 已于 2026-03-24 EOL;manifest 升到当前可解析的 GNOME `50` runtime,避免发布包建立在过期 runtime 上。
- **review 修复:AppStream metadata license**:真实 Flatpak build 的 `appstreamcli compose` 拒绝 `<metadata_license>Apache-2.0</metadata_license>`;metainfo 改为 AppStream 常用的 `CC0-1.0` 元数据许可,`project_license` 继续保持 `Apache-2.0`。
- **Display P3 / ICC 端到端测试**:`imgconvert-core` 新增自生成 Display P3 ICC fixture,覆盖 P3 PNG 输入在 `preserveMetadata=true` 下转换到 JPEG/PNG/WebP/AVIF 后 ICC 逐字节保留。PNG/WebP lossless 路径同时验证像素不变;JPEG/AVIF 验证尺寸和 ICC 元数据保真。
- **文档清理**:更新 Flatpak README 与 ENGINE,删除“发行版 runtime smoke 仍未覆盖 Debian/Ubuntu/Fedora”的过期表述,明确当前 Display P3 范围是 ICC 元数据保真而非像素级色彩管理。

---

## 2026-07-02 — P3 Linux 发布最后一公里:Flathub source bundle + 包内转换 smoke

Codex review Linux/Flathub 发布链路后补齐两个发布前缺口:

- **Flathub source bundle**:`packaging/flatpak/com.ivmm.imgconvert.yml` 不再使用仓库根 `type: dir` source,改为 `type: archive` release source。新增 `pnpm run release:flatpak:prepare`,会生成 `target/flatpak/sources/imgconvert-<version>-source.tar.gz`,把 pnpm 自身通过 Corepack vendor 到 `.flatpak-vendor/corepack.tgz`,把 Cargo 依赖 vendor 到 `.flatpak-vendor/cargo`,把 pnpm 包 fetch 到 `.flatpak-vendor/pnpm-store`,并回写 manifest 的 archive `sha256`。本地/CI 默认写 `path:` source;Flathub PR 可在发布该 archive 后用 `--source-url=https://.../imgconvert-<version>-source.tar.gz` 切换为可下载 `url:` source。
- **Flatpak guardrail 升级**:`release:flatpak:verify` 现在拒绝 `type: dir`、host/home filesystem、HEIC helper/libheif/x265 和在线 `corepack prepare pnpm@...`,并要求 node/rust SDK extension、offline Corepack cache、offline pnpm install、offline Cargo build、合法 archive `path:`/`url:` source 和 store 外部 codec 禁用环境。
- **包内真实转换 smoke**:安装后的 `imgconvert` 二进制新增隐藏入口 `IMGCONVERT_PACKAGE_CONVERT_SMOKE=1`。该入口不启动 GUI,会用真实 `imgconvert-core` 链路把内置 16×16 PNG 转成 JPEG/WebP/PNG/AVIF,验证 magic 与尺寸后退出。
- **Linux package smoke 升级**:`scripts/smoke-linux-package-install.mjs` 增加 `--convert-smoke`;Docker matrix 默认在 GUI 启动 smoke 后继续跑包内真实转换 smoke。AppImage 路径同样使用 `APPIMAGE_EXTRACT_AND_RUN=1`。

限制:

- 当前容器缺 `flatpak-builder`/`flatpak`,因此本批次完成 manifest/source bundle/guardrail 和二进制转换 smoke 的本机验证;真实 Flatpak build/install/portal runtime smoke 仍需在安装 Flatpak 工具的 Linux runner 上跑。

---

## 2026-07-02 — P3 后续平台:Windows 打包与 Store 护栏第一批

Codex review 后补上 Windows 平台发布留门里的实际配置检查,避免 `release:windows:check` 只验证图标和 store 环境开关:

- **Windows 直发配置**:新增 `src-tauri/tauri.windows.conf.json`,面向 `.msi`/NSIS 直发路线。配置锁定 `allowDowngrades=false`、`digestAlgorithm=sha256`、silent `embedBootstrapper` WebView2 安装、最低 WebView2 版本、稳定 WiX `upgradeCode` 和 NSIS current-user 默认安装。
- **guardrail 升级**:`scripts/check-platform-release-guardrails.mjs` 的 Windows direct 分支现在会读取 `tauri.windows.conf.json` 并验证上述安装/升级边界;新增 `release:windows:direct:check` 便于 CI 分批定位。
- **MS Store 留门**:新增 `packaging/windows/README.md`,明确 Store 仍需 Windows runner、MSIX、`runFullTrust`、Partner Center 和真实安装 smoke;`release:windows:store:check` 继续要求 `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1`,防止外部 HEIC helper 自动发现进入商店构建。

验证:

- `pnpm run release:windows:direct:check`:通过。
- `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 pnpm run release:windows:store:check`:通过。
- 未设置 `IMGCONVERT_DISABLE_EXTERNAL_CODECS` 时 `pnpm run release:windows:store:check` 按预期失败。
- `pnpm run release:windows:check`、`pnpm run release:platform:check`、`IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 pnpm run release:store-env:check`:通过。
- `pnpm tauri build --ci --debug --no-bundle --config src-tauri/tauri.windows.conf.json`:通过,确认新增 Windows 配置可被 Tauri CLI 合并/解析。

限制:

- 本批次不等同于完成 Windows 发布;真实 `.msi`/NSIS 构建、代码签名、MSIX 打包、MS Store 提交、WIC HEIC 运行时探测和 Windows 真机 smoke 仍需在 Windows runner/真机上完成。

---

## 2026-07-02 — P3 后续平台:macOS 打包与沙盒护栏第一批

Codex 在 Linux v1 RC 后继续推进后续平台发布留门,先完成 macOS 可静态验证的打包与沙盒配置骨架:

- **macOS 直发配置**:新增 `src-tauri/tauri.macos.conf.json`,Tauri 在 macOS 构建时会自动合并该平台配置;默认使用 hardened runtime 与 `entitlements.macos.direct.plist`,不启用 App Sandbox,面向 `.dmg` / Developer ID / notarization 直发路线。
- **MAS 配置骨架**:新增 `src-tauri/tauri.macos.mas.conf.json` 与 `entitlements.macos.mas.plist`;MAS entitlements 只声明 App Sandbox、用户选择文件读写和 app-scoped bookmarks,不加 broad filesystem、network server 或 temporary exception entitlement。
- **guardrail 升级**:`scripts/check-platform-release-guardrails.mjs` 现在会读取 macOS 平台配置和 entitlements plist,验证直发/MAS 两套权限边界;新增 `release:macos:direct:check`、`release:macos:store:check`、`release:windows:store:check`。
- **文档**:新增 `packaging/macos/README.md`,记录直发 `.dmg` 和 MAS candidate 的本地 preflight/构建命令。MAS build 必须设置 `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1`,保证外部 HEIC helper 自动发现被编译期关闭。

验证:

- `pnpm run release:macos:check`:通过。
- `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 pnpm run release:macos:store:check`:通过。
- 未设置 `IMGCONVERT_DISABLE_EXTERNAL_CODECS` 时 `pnpm run release:macos:store:check` 按预期失败。

限制:

- 本批次不等同于完成 macOS 发布;真实 `.dmg` 构建、Developer ID 签名、公证、MAS provisioning、security-scoped bookmark runtime shim 和 HEIC ImageIO 沙盒实测仍需在 macOS runner/真机上完成。

---

## 2026-07-02 — P3 review 修复 + Linux 发布收尾

Codex review P3 第一批发布链路后继续推进 Linux 发布闭环:

- **review 修复:stale artifact**:新增 `scripts/clean-linux-bundles.mjs`,release/debug release 脚本会先清理对应 bundle 目录,避免 artifact verifier 吃到旧包误判通过。
- **review 修复:Linux desktop metadata**:`tauri.conf.json` 补 `publisher`/`homepage`/`licenseFile`/`category`/长短描述/Linux deb section,重新生成的 `.desktop` 已有 `Categories=Graphics;Photography;`,`.deb` 已有 `Homepage`、`Section: graphics` 和长描述。
- **artifact verifier 升级**:`scripts/check-linux-bundle-artifacts.mjs` 不再只校验非空文件;现在会校验版本号、`.deb` 依赖字段、包内 `/usr/bin/imgconvert`、`ImgConvert.desktop`、desktop entry 的 `Name/Type/Exec/Categories`,并对 rpm/AppImage 做基础结构检查。`.deb`/`.rpm` 被选中时必须有 `dpkg-deb`/`rpm` 检查工具,避免“没检查也通过”。
- **release workflow**:新增 `.github/workflows/release-linux.yml`,支持 tag `v*` 和手动触发,在 Linux amd64/arm64 runner 上构建 release `.deb/.rpm/AppImage` 并上传 artifact。
- **安装启动 smoke**:新增 `scripts/smoke-linux-package-install.mjs`。CI/debug `.deb` 构建后会安装 package 并启动一次;脚本支持 Docker 模式,且启动层同时支持 `xvfb-run` 和裸 `Xvfb`,Fedora 容器不再依赖 Debian 专属 wrapper。AppImage smoke 设置 `APPIMAGE_EXTRACT_AND_RUN=1`,避免 Docker/FUSE 缺失导致误报。
- **发行版 runtime matrix**:新增 `scripts/smoke-linux-package-matrix.mjs` 与 `pnpm run release:linux:smoke:docker`,正式 Linux release workflow 会在 amd64/arm64 上跑 Ubuntu `.deb`、Debian `.deb`、Fedora `.rpm`、Ubuntu AppImage smoke。
- **release checksums**:新增 `scripts/generate-linux-release-checksums.mjs`;`pnpm run release:linux` 会生成 `src-tauri/target/release/bundle/SHA256SUMS`,workflow 会随 artifact 上传。
- **Flatpak 第一版**:新增 `packaging/flatpak/com.ivmm.imgconvert.yml`、desktop/metainfo 与 `pnpm run release:flatpak:verify`。manifest 不申请 host/home filesystem,主包设置 `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1`,不捆绑 HEIC/helper;Flathub 正式提交仍需把本地 `dir` source 换成 release tarball + vendored Cargo/npm inputs。
- **review 修复:AppImage 系统库冲突**:发现 Tauri AppImage 会捆入 `libgcrypt.so.20`,在 Ubuntu 24.04 容器中与系统 `libgpg-error` 组合触发符号不匹配。新增 `scripts/scrub-linux-appimage.mjs`,release/debug all 打包后会删除 deny-list 系统库并用 Tauri 缓存的 `linuxdeploy-plugin-appimage.AppImage` 重新打包。
- **review 修复:AppImage symlink 防回归**:repack 复制 AppDir 时使用 `verbatimSymlinks`,避免 `.DirIcon`/`ImgConvert.desktop`/`imgconvert.png` 被改写成宿主机绝对 symlink。artifact verifier 现在会解包 AppImage,拒绝 host-absolute root symlink,拒绝捆绑 `libgcrypt.so.20`,并复用通用 Linux bundle 检查覆盖 `usr/bin/imgconvert`、GLIBC 基线和 `.desktop` 元数据。
- **review 修复:Docker runtime smoke**:AppImage Docker smoke 先复制只读挂载文件再 `chmod`,补装 `libasound2t64`/`libasound2`,并支持 `IMGCONVERT_DOCKER_APT_MIRROR` 覆盖 Ubuntu apt 源以降低本机实测波动。Docker 不可直接访问时脚本会自动尝试 `sudo -n docker`。
- **下一阶段平台发布护栏**:新增 `scripts/check-platform-release-guardrails.mjs` 与 `release:platform:check` / `release:macos:check` / `release:windows:check` / `release:store-env:check`。该脚本静态校验 macOS/Windows 发布元数据、平台图标、Apache-2.0 许可证和“商店构建禁外部 codec/helper”的 build-time 机制;`--require-store-env` 可在实际 store build 前强制要求 `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1`。

验证:

- `pnpm run release:linux:debug`:通过,重新生成 debug `.deb` 并通过增强 artifact verifier。
- `pnpm run release:flatpak:verify`:通过。
- `node scripts/generate-linux-release-checksums.mjs --profile=debug --bundles=deb`:通过。
- `pnpm run release:linux`:通过,生成 release `.deb`/`.rpm`/AppImage 与 `SHA256SUMS`;AppImage scrub 日志确认移除 `libgcrypt.so.20`。
- `pnpm run release:linux:verify`:通过,增强 verifier 校验三类 release artifact。
- `IMGCONVERT_DOCKER_APT_MIRROR=http://mirrors.aliyun.com/ubuntu-ports pnpm run release:linux:smoke:docker -- --timeout=8`:通过,完整覆盖 Ubuntu `.deb`、Debian 13 `.deb`、Fedora `.rpm`、Ubuntu AppImage。
- `pnpm run release:platform:check`:通过。
- `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1 pnpm run release:store-env:check`:通过;未设置该环境变量时按预期失败。
- `dpkg-deb -f`:确认 `Version: 0.1.0`、`Section: graphics`、`Homepage: https://github.com/yeagoo/imgconvert`、运行时依赖 `libwebkit2gtk-4.1-0, libgtk-3-0`。
- `dpkg-deb -x`:确认 `.desktop` 中 `Categories=Graphics;Photography;`。

## 2026-07-01 — P2 review 修复 + P3 Linux 发布第一批

Codex 对 P2 高级压缩收尾做了一轮后端 review,并启动 P3 Linux release 闭环:

- **P2 缓存策略修复**:`result_cache_key()` 升级到 v2,自动质量启用时把 `qualityFloor` 对应的搜索下限纳入 key,避免同一质量上限但不同 floor 复用错误输出。
- **P2 策略一致性修复**:结果缓存命中后也会复用 `skip-if-larger` 与代际损失防护检查,不再绕过当前用户策略。
- **测试覆盖**:Tauri 后端新增缓存 key/floor 差异测试与缓存候选策略检查测试。
- **P3 发布入口**:新增 `pnpm run release:linux` / `release:linux:debug` / `release:linux:debug:all` / `release:linux:verify`;正式 release 入口显式构建并校验 `deb,rpm,appimage` 三类 Linux bundle artifact,debug smoke 默认只打 `.deb` 以避免 debug rpm/AppImage 后处理过慢。
- **P3 CI 第一批**:GitHub Actions 的 Tauri build smoke 改为 Linux `amd64 + arm64` 矩阵,使用原生 runner 构建 debug `.deb` 并上传 artifact。

## 2026-07-01 — P2 收尾:自动质量、代际防护、缓存与高级参数

Codex 完成 P2 剩余高级压缩功能项:

- **自动质量**:core 新增 `convert_auto_quality()`。仅 JPEG/WebP 启用,用 `ssimulacra2`(BSD-2-Clause,`default-features=false`)按 step≈4 二分搜索达到目标分的最低质量;自动质量不低于格式质量下限,小于 8×8 的图片回退固定质量。WebP 会把 lossless 候选纳入比较,若更小则选 lossless。
- **代际损失防护**:Tauri 对 JPEG/AVIF/lossy WebP 源再次输出有损格式时,按 source bpp 分级要求最低体积收益(默认 2%/3%/5%/8%);收益不足计 skipped,避免有损源无意义重压。VP8L lossless WebP 不触发该保护。
- **结果缓存**:新增 `blake3` 设置哈希 + 源文件哈希缓存。缓存默认开启,只记录已有输出的 hash/size,命中时直接返回结果;不把图片内容写入缓存目录。
- **PNG 实验性限色**:新增 `color_quant`(MIT)限色路径,默认关闭;开启后先 NeuQuant 映射 RGBA,再输出普通 PNG 并继续走 oxipng。继续禁止 `imagequant`/GPL。
- **高级参数面板**:core/Tauri/前端贯通 AVIF subsample(4:4:4/4:2:0)、WebP near-lossless/sharp YUV、MozJPEG trellis scans,设置持久化并纳入多候选去重与缓存 key。
- **CI 进阶**:GitHub Actions 增加 Tauri Linux debug build smoke;既有 npm license/audit、cargo-deny、cargo-about 生成校验继续作为 P2/P3 guardrail。

验证:

- `cargo test -p imgconvert-core`:通过(32 tests)。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(78 tests)。
- `pnpm run check`:通过。
- `pnpm run test`:通过(4 tests)。

---

## 2026-07-01 — P1.5 HEIC 平台收尾 review 修复

Codex 修复 P1.5 HEIC 平台 review 提出的边界问题:

- **平台开关收窄**:外部 HEIC helper 自动发现从 `Unix + Windows` 收敛为 `Linux + Windows`;macOS 不再进入 XDG/Library helper 探测,继续留给 P3 的 ImageIO / App Sandbox 验证。
- **Windows 临时目录**:HEIC helper 解码 PNG 的工作目录改到 `%LOCALAPPDATA%\ImgConvert\Temp\heic\imgconvert-heic-*`,不再使用全局 temp 根目录;Unix 仍使用 0700 私有临时目录。
- **Windows 覆盖**:新增 Windows-only 单测覆盖 `.exe` helper 要求、LocalAppData codecs PATH 探测、大小写不敏感且分段安全的路径前缀判断、工作目录落点;CI 新增 `windows-latest` 的 `external_codecs` 定向测试。

---

## 2026-07-01 — P1.5 HEIC 平台收尾:Windows 外部 helper

Codex 补齐 P1.5 HEIC 平台剩余的外部 helper 路径:

- **Windows helper 启用**:外部 codec/helper 发现从 Unix 扩展到 Windows;Windows 可通过用户手动选择 `imgconvert-heic-helper.exe`、manifest provider 或受信任 PATH 目录激活 decode-only HEIC 导入。
- **Windows 信任边界**:manifest 自动发现加入 `%LOCALAPPDATA%\ImgConvert\codecs` 与 `%PROGRAMDATA%\ImgConvert\codecs`;自动发现只接受 canonical 后位于 Program Files、ProgramData/ImgConvert/codecs 或 LocalAppData/ImgConvert/codecs 下的目录。用户显式选择的 helper 可在其它位置,但仍必须是普通 `.exe` 文件,且调用不经过 shell。
- **helper 名称**:系统/插件 PATH 探测新增 `imgconvert-heic-helper`,同时保留 Linux `heif-convert` / `heif-dec`。主程序仍不链接 libheif/libde265/x265,不把 LGPL/GPL 组件放入主依赖树。
- **诊断 UI**:插件诊断文案增加 Windows 免费 helper 路线,强调 helper/provider 是 decode-only、单独分发、可被商店/Flatpak 构建禁用。
- **阶段边界**:Windows WIC + HEIF/HEVC 扩展探测仍属于 P3 平台发布项;本批次只完成免费外部 helper 协议与信任模型,不承诺 HEIC 开箱即用。

---

## 2026-07-01 — P2 第五批:ICC/EXIF 元数据保真

Codex 完成 P2 元数据保真的第一版容器闭环:

- **默认隐私语义不变**:`preserveMetadata=false` 时 JPEG/PNG/WebP/AVIF 重新编码后仍显式剥离 ICC/EXIF;只有用户开启“保留元数据”才写回。
- **core 容器手术**:JPEG 提取/写入 APP1 EXIF 与 APP2 ICC(含 1-based 分块重组);PNG 在 oxipng 之后 splice `iCCP`/`eXIf`;WebP 重写 RIFF chunk,必要时插入/更新 `VP8X` 并写 `ICCP`/`EXIF`;AVIF 走 libavif 的 ICC/EXIF metadata API。
- **EXIF orientation 防双旋**:JPEG/PNG 解码经 image crate 真旋正像素后,保留的 EXIF orientation 改写为 1;WebP/AVIF 当前未做几何 transform,因此只保留原始 EXIF payload。
- **Tauri/前端接通**:`ConvertOptions.preserveMetadata` 不再报“未实现”,经 `EncodeOptions.preserve_metadata` 传入 core;设置栏“保留元数据”开关启用并持久化。
- **测试覆盖**:core 新增 JPEG/PNG/WebP/AVIF 元数据默认剥离、开启后逐字节保留、JPEG ICC 分块与管线跨格式保留测试;Tauri 映射测试覆盖 preserve flag。

验证:

- `cargo test -p imgconvert-core`:通过(28 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(72 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `pnpm run check`:通过。
- `pnpm run deadcode`:通过。
- `pnpm run test`:通过(4 tests)。
- `pnpm run build`:通过。
- `pnpm run e2e`:通过(Chromium Web 预览 smoke 1 test)。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL,第三方许可证清单未过期。
- `cargo deny check bans sources`:通过;Tauri 依赖树重复版本仍只输出 warning。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`pnpm run format:check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:启动链路到达 `Running target/debug/imgconvert` 后按预期由 timeout 结束。

---

## 2026-07-01 — P0.5 技术风险闭环:工具链/授权路径/并发诊断

Codex 收口 P0.5 剩余技术风险的最小可验证闭环:

- **原生工具链预检**:新增 `scripts/check-native-toolchain.mjs` 与 `pnpm run toolchain:check`,检查 cmake / meson / ninja,并仅在 x86/x86_64 检查 NASM;脚本接入 `quality:rust`,缺依赖时给明确安装提示。
- **授权路径边界**:新增 `src-tauri/src/access.rs`,把用户选择路径、输出目录和剪贴板临时文件统一收口为授权路径 grant。该层不强制 canonicalize,为 Flatpak portal 映射路径和 macOS security-scoped bookmark 生命周期留接口。
- **并发诊断**:`imgconvert-core` 新增 `AVIF_ENCODER_MAX_THREADS=1` 常量并用于 libavif encoder;Tauri 新增 `runtime_diagnostics()` 命令暴露默认并发、内存预算、RGBA 工作集倍率和 AVIF 内部线程上限。
- **测试稳定性**:HEIC selected-helper 相关测试增加串行锁,避免并发测试共享全局 helper 白名单导致偶发 provider 判定漂移。
- **阶段边界**:P0.5 已完成本机可验证 guardrail;Debian/Ubuntu/Fedora × amd64/arm64、Flatpak portal runtime smoke、macOS bookmark shim 和 Apple Silicon AVIF speed 仍属于 P3/平台阶段实测。

验证:

- `pnpm run toolchain:check`:通过(linux/arm64;cmake 3.31.6,meson 1.7.0,ninja 1.12.1;NASM 按 arm64 跳过)。
- `pnpm run quality:frontend`:通过。
- `pnpm run quality:rust`:通过(core 23 tests;src-tauri 71 tests)。
- `cargo test -p imgconvert-core`:通过(23 tests)。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(71 tests)。
- `pnpm run e2e`:通过(Chromium Web 预览 smoke 1 test)。
- `pnpm run quality:security`:通过;`cargo deny check bans sources` 仍有 Tauri 依赖树重复版本 warning,但不阻断。
- `pnpm run format:check`:通过。
- `git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:启动链路到达 `Running target/debug/imgconvert` 后按预期由 timeout 结束。

---

## 2026-07-01 — P2 第四批:质量下限阈值

Codex 完成全局有损/无损开关与每格式质量下限的最小闭环:

- **前端设置**:保留现有全局“无损压缩”开关,继续仅对 PNG/WebP 生效;JPEG/WebP/AVIF 在格式参数区新增“最低质量”滑块。
- **阈值语义**:质量下限按 `30..=100` 生效,低于 30 视为关闭;默认下限为 30,避免极低质量误操作,但后端旧 IPC 缺字段时按关闭处理。
- **Tauri 映射**:`ConvertOptions` 新增 `quality_floor`(camelCase:`qualityFloor`);进入 core 前对 JPEG/WebP/AVIF 的有损质量做 `max(quality, floor)` clamp。PNG 和 WebP 无损模式不应用有损质量下限。
- **自动质量留门**:后续 `ssimulacra2` 二分搜索必须以该下限作为最低质量边界。

验证:

- `pnpm run check`:通过。
- `pnpm run test`:通过(4 tests)。
- `pnpm run quality:frontend`:通过。
- `pnpm run quality:rust`:通过(core 23 tests;src-tauri 68 tests)。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(68 tests)。
- `cargo test -p imgconvert-core`:通过(23 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `pnpm run e2e`:通过(Chromium Web 预览 smoke 1 test)。
- `pnpm run format:check`:通过。
- `git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:启动链路到达 `Running target/debug/imgconvert` 后按预期由 timeout 结束。

---

## 2026-07-01 — P2 第三批:多候选取最小

Codex 完成多候选取最小的第一版:

- **core 能力**:新增 `convert_best_of(input, target, options)`。输入只解码一次,再用多个 `EncodeOptions` 编码候选竞争,返回体积最小的候选;原 `convert()` 保持单候选兼容。
- **Tauri 候选生成**:`multi_candidate`(camelCase:`multiCandidate`)默认开启。JPEG 在 baseline/progressive 间竞争;PNG 在用户级别、相邻级别、默认 4、最高 6 间去重竞争;WebP 在用户 method、4、6 间竞争;AVIF 暂不加候选,避免慢编码成倍放大。
- **语义边界**:候选不会偷偷改变 quality、lossless、目标格式或 AVIF speed;需要精确按单一参数输出时可在 UI 关闭“多候选取最小”。
- **测试修复**:HEIC manifest helper 执行增加短暂重试,规避测试 helper 刚写入后 Linux 偶发 `Text file busy`。

验证:

- `cargo test -p imgconvert-core`:通过(23 tests)。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(67 tests)。
- `pnpm run quality:frontend`:通过。
- `pnpm run quality:rust`:通过(core 23 tests;src-tauri 67 tests)。
- `pnpm run quality:security`:通过;`cargo deny check bans sources` 仍有 Tauri 依赖树重复版本 warning,但不阻断。
- `pnpm run e2e`:通过(Chromium Web 预览 smoke 1 test)。
- `timeout 25s xvfb-run -a pnpm tauri dev`:启动链路到达 `Running target/debug/imgconvert` 后按预期由 timeout 结束。

---

## 2026-07-01 — P2 第二批:skip-if-larger / 永不变差

Codex 完成 P2 的输出防变大闭环:

- **Tauri 策略**:`ConvertOptions` 新增 `skip_if_larger`(camelCase:`skipIfLarger`),默认开启;core 编码完成后、写文件前比较候选输出大小与源文件大小,若候选不小于源文件则直接跳过写入。
- **覆盖保护**:即使用户选择 overwrite,变大候选也不会替换已有输出文件;原地优化同样受保护。
- **批量协议**:skip-if-larger 命中时通过既有 `FileSkipped` 事件计入 skipped,不会记为 failed;前端队列显示跳过原因和源/候选字节数。
- **前端设置**:`SettingsBar` 新增默认开启的“跳过变大输出” switch;需要强制格式迁移时用户可以关闭。
- **边界说明**:本批次只做单候选大小防护,不做多候选竞争;自动质量阶段的“省不到 2% 也跳过”阈值后续再接。

验证:

- `pnpm run quality:frontend`:通过。
- `pnpm run quality:rust`:通过(core 21 tests;src-tauri 66 tests)。
- `pnpm run quality:security`:通过;`cargo deny check bans sources` 仍有 Tauri 依赖树重复版本 warning,但不阻断。
- `pnpm run e2e`:通过(Chromium Web 预览 smoke 1 test)。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

---

## 2026-07-01 — P2 第一批:格式级编码参数

Codex 启动 P2 保真/压缩阶段,先完成可见且低风险的 per-format 参数闭环:

- **core 参数扩展**:`EncodeOptions` 新增 `jpeg_progressive`、`png_oxipng_level`、`webp_method`、`avif_speed`,默认值锁为 JPEG progressive=true、oxipng=4、WebP method=4、AVIF speed=8。
- **编码器落地**:JPEG 可切 baseline/progressive;PNG 从固定 oxipng preset 改为用户级别 0..6;WebP 改用 `webp::WebPConfig` + `encode_advanced()` 传入 method/lossless/quality;AVIF `libavif-sys` encoder speed 改为来自参数,继续保持 `maxThreads=1`。
- **Tauri 协议**:`ConvertOptions` 增加对应 camelCase IPC 字段并保留 serde default,旧前端/旧配置缺字段时仍按 P2 默认值运行;`convert_image` / `convert_batch` 统一经 `encode_options_for()` 传入 core。
- **前端设置**:`SettingsBar` 新增“格式参数”区,JPEG 显示 progressive switch,PNG/WebP/AVIF 显示 shadcn slider;设置持久化和归一化会 clamp 到合法范围。
- **测试覆盖**:core 新增默认值、JPEG SOF marker 和参数 clamp 测试;Tauri 新增 `ConvertOptions -> EncodeOptions` 映射测试。

验证:

- `cargo test -p imgconvert-core`:通过(21 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(64 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run format:check`:通过。
- `pnpm run test`:通过。
- `pnpm run build`:通过。
- `pnpm run e2e`:通过(Chromium Web 预览 smoke 1 test)。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

---

## 2026-07-01 — 质量体系 v1

Codex 按当前 Tauri 2 + Svelte 5 + Rust core 架构补齐第一版可执行质量门:

- **前端质量门**:新增 `typecheck`(`tsc --noEmit` + `svelte-check`)、`lint`(ESLint flat config + Svelte/TS rules)、`format:check`(Prettier + Svelte plugin)、`deadcode`(Knip 文件/依赖扫描)、`test`(Vitest) 与 `e2e`(Playwright Web 预览 smoke)。
- **测试基线**:新增 `tests/state.test.ts`,覆盖格式能力映射、队列去重/跳过与大小格式化;新增 `e2e/app.spec.ts`,验证 Web 预览 shell 能加载。
- **工程入口**:新增 `quality:frontend` / `quality:rust` / `quality:security` 聚合脚本与 `Makefile`;默认 security gate 保持本机可重复,只跑 license、deny bans、deny sources,在线 RustSec 检查保留在 `audit:rust` 与 CI。
- **CI**:新增 `.github/workflows/ci.yml`,分层跑 frontend、Rust core、Tauri backend、security/license 与 Playwright Web preview E2E。
- **格式基线**:补 `.prettierrc.json` / `.prettierignore`,并把 shadcn button 的 module exports 拆到 `button.ts`,让裸 `tsc --noEmit` 能稳定检查。

验证:

- `pnpm run quality:frontend`:通过。
- `pnpm run quality:rust`:通过(core 18 tests;src-tauri 63 tests)。
- `pnpm run quality:security`:通过;`cargo deny check bans sources` 仍会输出 Tauri 依赖树重复版本 warning,但不阻断。
- `pnpm run e2e`:通过(Chromium Web 预览 smoke 1 test)。
- `pnpm run audit:npm`:通过,无 prod npm 漏洞。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。
- `git diff --check`:通过。

---

## 2026-07-01 — P1.5 收尾:许可边界与手动 helper 白名单

Codex 在 review 后继续推进 P1.5 收尾:

- **review 修复**:剪贴板临时文件清理改为后端状态登记,`cleanup_imported_temp_file()` 只会删除本次运行中由 `import_clipboard_image()` 创建并登记的文件,不再仅凭 `/tmp/imgconvert-clipboard-*` 路径前缀判断;paste 导入同时读取 `DataTransfer.items`,覆盖部分 WebView/桌面环境截图不出现在 `DataTransfer.files` 的情况。
- **渠道禁用开关**:新增 `IMGCONVERT_DISABLE_EXTERNAL_CODECS=1|true|yes|on`,支持运行时或构建时禁用外部 codec/helper 自动发现。禁用后 HEIC manifest 与系统 helper 都不会激活,诊断 UI 会显示禁用原因。
- **诊断字段**:`codec_diagnostics().heic` 新增 `externalCodecsEnabled` 与 `disabledReason`,用于区分“未安装 helper”和“渠道/构建主动禁用”。
- **用户显式 helper 白名单**:新增 `set_selected_heic_helper(path|null)` 命令,前端在插件诊断弹层中选择/清除本机 HEIC helper,设置路径随用户配置持久化;能力检测时先同步该白名单。
- **provider 优先级**:HEIC provider 激活顺序调整为手动 helper → manifest provider → 系统 PATH helper。手动 helper 有效时保存 canonical 可执行文件路径;失效路径只进入诊断状态,显示为不可用但不会执行。
- **许可/专利文案**:插件诊断 UI 增加“许可与渠道边界”,明确主程序不内置 HEIC codec、不链接 libheif、不分发 x265;HEIC 仅通过用户环境里的可选 decode-only provider 导入,插件许可/NOTICE/专利风险需单独处理。
- **前端文案**:引擎状态与诊断标题统一使用“HEIC 可选导入”,避免写成开箱即用的 HEIC 支持。
- **review 修复 2**:外部 helper/manifest 发现改为 canonical 路径后再校验目录信任、文件写权限和执行权限;manifest 读取限制为 64 KiB 并新增超限错误码;剪贴板导入区分 scan/clipboard 模式,取消按钮可中断剪贴板循环并清理未入队临时文件;临时文件清理改为用已登记 canonical 路径验证和删除,避免 alias 路径导致登记丢失。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(63 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo test -p imgconvert-core`:通过(18 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

---

## 2026-07-01 — P1 剪贴板导入最小闭环

Codex 完成 P1 最后一个导入小项:

- **粘贴入口**:Dropzone 新增「粘贴导入」按钮,主窗口监听 `Ctrl+V`/系统 paste;图片 Blob 直接导入,不影响文件名模板等输入框里的普通文本粘贴。
- **路径兼容**:剪贴板文本支持 `file://`、`text/uri-list`、GNOME `x-special/gnome-copied-files` 与绝对路径,统一复用既有 `scan_import_paths` 扫描/过滤/去重。
- **图片 Blob 落盘**:新增 `import_clipboard_image()` Tauri 命令,把 PNG/JPEG/WebP/AVIF 剪贴板图片写入私有临时目录,返回现有 `ImportScanFile` 元数据,后续缩略图、批量转换、取消协议不另开分支。
- **清理边界**:剪贴板临时图片标记为 `temporary`,队列移除/清空时调用 `cleanup_imported_temp_file()`;后端只清理本应用创建的 `imgconvert-clipboard-*` 私有临时目录下文件。
- **安全与限制**:单张剪贴板图片上限 128 MiB;Linux 文件管理器复制文件时依赖剪贴板是否暴露 `file://`/路径文本,不尝试从 WebView 猜测不可见本机路径。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(54 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo test -p imgconvert-core`:通过(18 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

---

## 2026-07-01 — P1.5 插件诊断 UI

Codex 完成插件诊断 UI 的第一版:

- **后端诊断命令**:新增 `codec_diagnostics()` Tauri 命令,返回 HEIC 是否启用、active provider、manifest 搜索目录、每个 manifest 的 accepted/rejected 状态与系统 helper 探测结果。
- **诊断信息粒度**:manifest 目录会区分 missing / empty / ready / rejected / untrusted / unreadable;manifest 文件返回具体拒绝原因,包括许可、协议、可写 HEIC、路径逃逸、helper 不可执行等错误码前缀。
- **前端入口**:顶栏新增「插件诊断」按钮;弹层展示 HEIC 状态、active provider 执行文件与 argv、探测摘要、系统 helper 列表和 manifest 搜索明细。
- **运行边界**:网页预览返回空诊断;Tauri 桌面端执行本机只读探测,不启动 helper、不解码文件。
- **review 修复**:helper stdout 直接丢弃,stderr 走管道并限制为 64 KiB;helper 生成的临时 PNG 读取限制为 512 MiB,避免异常 helper 填满磁盘或内存。非 Unix 平台在尚未实现平台信任模型前不启用外部 HEIC helper/manifest 自动发现;诊断弹层每次打开都会刷新,顶栏引擎文案在窄屏截断。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(51 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo test -p imgconvert-core`:通过(18 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

---

## 2026-07-01 — P1.5 HEIC manifest 插件协议最小闭环

Codex 完成 HEIC 插件 manifest v1 的第一版可运行闭环:

- **manifest 发现**:新增 `IMGCONVERT_CODEC_PLUGIN_DIRS`、XDG user data、XDG system data 三层搜索;优先读取 `imgconvert-codec-heic.json`,再读取同目录其它 `imgconvert-codec-*.json`。
- **协议校验**:v1 manifest 要求 `protocol:1`、`mode:"external-process"`、`decode.kind:"heic-to-png-file"`、`output:"png"`;`readable` 只能声明 `heic/heif/hif` 且必须含 `heic`;`writable` 必须为空;拒绝 GPL/AGPL 许可。
- **安全边界**:manifest helper 可用 manifest 目录内相对路径或受信任目录绝对路径;相对路径禁止 `..`,解析后不能逃出 manifest 目录;`args` 只做 argv 模板替换,`{input}` / `{output}` 必须是独立 argv entry,不执行 shell。
- **能力矩阵**:`capabilities()` 新增 `codecProviders`,HEIC manifest provider 会以 `{ kind:"manifest", license, readable, writable }` 形式返回;系统 `heif-convert` fallback 以 `kind:"system-helper"` 返回;前端引擎文案区分「插件」与「系统 helper」。
- **兼容路径**:manifest provider 优先于系统 PATH helper;未安装 manifest 时仍保持 `heif-convert` / `heif-dec` 系统 helper fallback。

验证:

- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(47 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo test -p imgconvert-core`:通过(18 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

---

## 2026-06-30 — P1.5 HEIC 系统 helper 导入最小闭环

Codex 完成 HEIC decode-only 的第一批实现,主程序依赖树仍保持 Apache-2.0 / 禁 GPL-AGPL-LGPL:

- **外部进程边界**:新增 `src-tauri/src/external_codecs.rs`,运行时探测系统 `heif-convert` / `heif-dec`,通过 argv + 临时 PNG 文件调用,不使用 shell 拼接,不链接 `libheif`。
- **能力矩阵合并**:`capabilities()` 在检测到 helper 时把 `heic` 加入 `readable`,但不加入 `writable`;前端只把 HEIC 作为源格式/导入能力展示,目标格式下拉仍只来自 `capabilities.writable`。
- **导入与转换路径**:`scan_import_paths` 仅在 helper 可用时接受 `heic/heif/hif`;`convert_image` / `convert_batch` 对 HEIC 输入先经 helper 解码为 PNG 字节,再进入现有 `imgconvert-core::convert()` 管线;异步缩略图同样复用该 decode 路径。
- **安全与错误边界**:调用 helper 前验证 HEIF/HEIC `ftyp` 文件头;HEIC 临时目录在 Unix 下以 `0700` 创建并在 Drop 中清理;helper PATH 目录及其祖先不能 world-writable;helper 缺失、超时、输出缺失、stderr 诊断都会返回到前端错误信息。
- **该批次限制**:当时尚未实现 manifest 插件协议、用户显式选择 helper、Windows WIC/helper 路线;真实 HEIC 样张 smoke 已在安装 `libheif-examples` 后补跑通过。

验证:

- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(43 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo test -p imgconvert-core`:通过(18 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。
- `libheif-examples` 1.19.8 真实 helper smoke:用 `heif-enc` 生成临时 `.heic`,再调用当前 `external_codecs::decode_heic_to_png()` 成功读回 PNG 字节。

---

## 2026-06-30 — HEIC 插件策略文档化

Codex 记录 HEIC 可选插件路线,并按许可/平台依赖做了一轮方案 review:

- **主程序边界**:ImgConvert 主程序继续 Apache-2.0,依赖树继续禁止 GPL/AGPL/LGPL;HEIC 不作为主包内置 codec。
- **插件形态**:HEIC 作为独立进程 helper/plugin,单独 LGPL 分发,用户显式安装后激活;主程序只做 manifest 发现、能力合并和受控进程调用。
- **Linux 差异**:Debian/Ubuntu 可提示 `libheif-examples` 等系统工具;Fedora 的 HEVC-encoded HEIC 可能需要 RPM Fusion `libheif-freeworld`;`heif-gdk-pixbuf`/`heif-thumbnailer` 只影响 GTK/文件管理器,不能等同于 core 能力。
- **Windows 免费路线**:系统路线仍是 WIC + HEIF/HEVC 扩展;若要避免要求用户购买 Microsoft Store HEVC 扩展,可另做 decode-only `imgconvert-heic-helper.exe`。注意不能直接整包带现成 MSYS2 `libheif` 发行物,因为依赖组合可能包含 `x265`/GPL;需要自建 decode-only 动态包并单独审计。
- **功能范围**:插件第一版只声明 HEIC/HEIF 输入,不提供 HEIC 输出,不承诺 HEIC 开箱即用。
- **渠道边界**:外部 helper 只作为主包外直发/用户安装增强;商店/Flathub 构建默认禁用,避免破坏现有上架前提。

Review 结论:

- 方案不破坏当前主程序 Apache-2.0/禁 LGPL 规则,前提是保持**独立进程 + 独立分发 + decode-only + 不打包 x265**。
- 最大剩余风险是 HEVC 专利与各发行版 codec 组件可用性,因此需要运行时探测和平台化错误提示,不能写营销式“支持 HEIC”。

## 2026-06-30 — P1 文件可靠性:EXIF 旋正、内存预算与失败提示

Codex 完成 P1 剩余文件可靠性三项的第一版可运行闭环:

- **EXIF orientation 真旋正**:`imgconvert-core` 的 image-crate 解码路径读取 `ImageDecoder::orientation()` 并在 RGBA 中间图上执行 `apply_orientation()`;JPEG→任意目标和异步缩略图共享同一旋正行为。当前 re-encode 不透传 EXIF blob,因此不会把旧 orientation tag 再写出去造成二次旋转。
- **导入尺寸驱动批量降并发**:前端把导入阶段探测到的 `sourceWidth/sourceHeight` 随转换参数传给 Tauri;后端按 `width*height*4*3` 估算单任务工作集,在 768 MiB 保守预算下把实际 worker 数压低。无尺寸提示的任务按 128 MiB 估算,core 仍保留真实尺寸上限校验。
- **失败路径与半成品提示细化**:输入读取、输出目录创建、临时文件创建、输出写入/替换等错误现在包含具体路径;写入失败会尝试清理临时文件或失败输出,并把「已清理半成品 / 清理失败」写进错误消息。
- **JPEG probe 显示尺寸同步**:`probe()` 解析 JPEG APP1 EXIF orientation 后返回旋正后的显示宽高,导入元数据与内存预算使用同一显示尺寸语义。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL,第三方许可生成物保持最新。
- `cargo test -p imgconvert-core`:通过(18 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(33 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

限制:

- 批量内存预算是保守启发式,不是 OS 级内存 cgroup;极端编码器内部峰值仍可能高于估算。
- ICC/EXIF 完整透传仍留在 P2「高级压缩与保真」;当前行为是像素旋正后重新编码,不保留原始元数据 blob。

## 2026-06-30 — P1 文件可靠性:目录结构与时间戳

Codex 完成 P1「保留目录结构 + 保留源文件修改时间」的第一版可运行闭环:

- **导入相对目录**:`scan_import_paths` 对目录递归导入的文件返回 `relativeDir`,表示从用户选择的目录根到文件父目录的相对路径;直接导入单文件时为空。
- **输出目录结构**:前端把队列项 `relativeDir` 传入转换参数;后端仅在设置了 `outDir` 时把安全相对目录拼到输出目录下,从而把目录导入结果输出为同样的子目录结构。
- **路径穿越防护**:后端拒绝绝对路径、`..` 等非法相对目录片段,并对目录片段做文件名级清理,避免前端参数被篡改后写出授权输出目录外。
- **保留修改时间**:转换开始前读取源文件 modified time,写出成功后 best-effort 设置到输出文件;覆盖写也先捕获源时间,避免同路径覆盖时丢失原始 mtime。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL,第三方许可生成物保持最新。
- `cargo test -p imgconvert-core`:通过(16 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(28 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

限制:

- 当前保留的是修改时间(mtime),不是访问时间/创建时间。
- 若目标文件系统不支持设置时间戳,转换不会因为时间戳设置失败而失败;后续如需要严格模式可把失败作为 warning 暴露给前端。

## 2026-06-30 — P1 ask 覆盖统一协议最小闭环

Codex 完成 P1「ask 覆盖策略纳入批量协议」的第一版可运行闭环:

- **转换规划命令**:新增 `plan_conversions(options)`。后端复用既有输出路径规则,按 batch 下标返回 `{ index, input, output, exists, error }`,供前端在转换开始前判断哪些文件需要覆盖确认。
- **ask 决策前置**:`settings.overwrite === "ask"` 不再走前端逐文件 `convert_image` 串行分支;前端先根据规划结果逐项确认,确认覆盖的 job 改成 `overwrite`,取消覆盖的 job 改成 `skip`。
- **转换统一走 batch**:ask 模式收集完决策后同样调用 `convert_batch` + Tauri Channel,因此进度、取消、并发、单文件跳过/错误汇总与 skip/overwrite 模式保持一致。
- **竞态语义**:规划时不存在、转换前新出现的输出文件按 `skip` 处理,走 no-clobber 写入路径,避免 ask 模式在竞态下意外覆盖。

验证:

- `pnpm run check`:通过(0 errors / 0 warnings)。
- `pnpm run build`:通过。
- `pnpm run license:check`:通过,未发现 GPL/AGPL/LGPL,第三方许可生成物保持最新。
- `cargo test -p imgconvert-core`:通过(16 tests)。
- `cargo clippy -p imgconvert-core -- -D warnings`:通过。
- `cargo +1.96.0 test --manifest-path src-tauri/Cargo.toml`:通过(24 tests)。
- `cargo +1.96.0 clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`:通过。
- `cargo fmt --check`、`cargo +1.96.0 fmt --manifest-path src-tauri/Cargo.toml --check`、`git diff --check`:通过。
- `timeout 25s xvfb-run -a pnpm tauri dev`:按预期由 timeout 结束;启动链路到达 `Running target/debug/imgconvert`。

限制:

- 覆盖确认仍是逐项系统 dialog,尚未做批量冲突列表弹窗。
- 后端 Channel 仍是单向进度通道;用户交互决策保持在前端开始批量转换前完成。

## 2026-06-30 — P1 异步缩略图最小闭环

Codex 完成 P1「异步生成缩略图」的第一版可运行闭环:

- **core 缩略图接口**:`imgconvert-core::thumbnail(bytes, max_edge)` 复用现有 JPEG/PNG/WebP/AVIF 解码器,按最长边缩放并输出小 PNG;全透明图片返回 `None`,前端保留格式占位。
- **Tauri 缩略图命令**:新增 `generate_thumbnail(options)`。前端只传已导入的本机路径,后端在 blocking 线程读取文件并返回 `{ mime, width, height, bytes }`;缩略图最大边限制在 `32..512`。
- **前端异步懒加载**:队列卡片进入视口附近才请求缩略图,全局并发固定为 2;返回字节转 Blob URL 展示,移除/清空队列时释放 URL。
- **卡片展示**:原先格式占位升级为稳定尺寸的预览区;缩略图加载中显示小 spinner,失败或全透明时继续显示格式图标,不影响转换状态。
- **review 修复**:缩略图命令生成前先检查文件元数据,超过 256 MiB 的源文件直接跳过预览,避免后台预览读取巨大伪图片;清空/移除队列时同步清理未执行的缩略图等待队列。

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
