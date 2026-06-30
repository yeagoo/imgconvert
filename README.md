# ImgConvert

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri)](https://tauri.app)
[![Svelte](https://img.shields.io/badge/Svelte-5-FF3E00?logo=svelte&logoColor=white)](https://svelte.dev)
[![pnpm](https://img.shields.io/badge/pnpm-10-F69220?logo=pnpm&logoColor=white)](https://pnpm.io)

**本地优先**的跨平台图片批量转换与压缩工具。**Linux 优先**(Debian/Ubuntu/Fedora),后续扩展 macOS / Windows(arm64 + amd64)。
面向开发者、设计师、站长与内容创作者:拖拽即转,优先支持 AVIF/WebP/JPEG/PNG 与有损/无损压缩,**所有处理默认在本机完成,不上传图片**。

## ✨ 功能

> 状态图例:✅ 已实现 · 🚧 计划中(详见 [docs/ROADMAP.md](docs/ROADMAP.md))

- ✅ 拖拽导入(Tauri 原生 `onDragDropEvent`,拿到真实文件路径)
- ✅ 格式转换:AVIF / WebP / JPEG / PNG(由进程内 Rust core 提供)
- ✅ 有损 / 无损压缩(各格式按其特性映射)
- ✅ 自定义输出目录、质量、覆盖策略、文件名模板
- ✅ 批量转换(**当前为前端串行**,逐个文件;P1 改为 Rust 端并发)
- 🚧 HEIC:Linux v1 不含;macOS/Windows 后续走系统原生能力
- 🚧 并发批量 + 进度/取消(Rust 端,Channel 上报)
- 🚧 高级压缩(自动质量、多候选取最小、ICC/EXIF 保真)
- 🚧 上架 Mac App Store / Microsoft Store / Flathub

> ✅ **架构已切换(2026-06-30)**:引擎从 libvips CLI 改为**进程内宽松许可 Rust 编解码 crate + 后续平台系统 HEIC**(混合架构),许可证为 **Apache-2.0**,目标上架三大商店。详见 [docs/ROADMAP.md](docs/ROADMAP.md) / [docs/ENGINE.md](docs/ENGINE.md)。

## 当前状态

- ✅ **P0 UI 外壳**:组件化 Svelte 5 界面、shadcn 控件、格式选择器、网页预览与 Tauri 串行转换胶水已接通。
- ✅ **混合架构 core**:进程内 core crate(mozjpeg/oxipng/webp/libavif-sys/image)已跑通 JPEG/PNG/WebP/AVIF。
- 🚧 **Release MVP**:并发/进度/取消、签名、沙盒、三商店上架 —— 尚未开始。

## 🛠️ 技术栈

- **前端**:Tauri 2 + Svelte 5(runes)+ shadcn-svelte / Tailwind v4 + phosphor-svelte(duotone 图标);包管理用 **pnpm**,运行时用 Node LTS
- **引擎(混合架构)**:进程内宽松许可 Rust 编解码 crate(`mozjpeg`/`oxipng`/`webp`/`libavif-sys`/`image`),构建期静态链接为单一二进制
- **HEIC**:Linux v1 不含;后续 macOS ImageIO / Windows WIC 走**系统原生**,不随包分发 x265(详见 [docs/LEGAL.md](docs/LEGAL.md))

> 具体版本以仓库锁定文件为准(`package.json#packageManager`、`pnpm-lock.yaml`、`src-tauri/rust-toolchain.toml`、`Cargo.lock`);完整版本表与决策见 [docs/ROADMAP.md](docs/ROADMAP.md)。

## 🚀 快速开始

> Tauri GUI 运行/打包需要平台 WebView 与系统开发包。前端 `pnpm run check` / `pnpm run build` 可在任意平台跑;若 Linux 上 `cargo check` 报 `dbus-1.pc` 缺失,先安装 `libdbus-1-dev`/`dbus-devel` 与 `pkg-config`。

前置依赖:[Rust](https://rustup.rs/)(版本由 `rust-toolchain.toml` 钉定)、Node LTS + [pnpm](https://pnpm.io/)、Xcode Command Line Tools、**NASM**(rav1e/mozjpeg 汇编;macOS 建议钉 2.15.05)+ cmake + meson/ninja(详见 [docs/ENGINE.md](docs/ENGINE.md) §4)。

```bash
pnpm install           # 前端依赖
pnpm run tauri dev     # 开发
pnpm run tauri build   # 打 .app / .dmg
pnpm run check         # svelte-check 类型检查
```

加 shadcn-svelte 组件:`pnpm dlx shadcn-svelte@latest add <component>`(配置见 `components.json`)。

## 📚 文档

- [docs/ROADMAP.md](docs/ROADMAP.md) —— 路线图、技术栈版本、开发优先级
- [docs/ENGINE.md](docs/ENGINE.md) —— 混合架构引擎:core/crate 参数、系统 HEIC、C 工具链、打包/签名
- [docs/LEGAL.md](docs/LEGAL.md) —— Apache-2.0、依赖许可红线、HEVC 专利、第三方组件合规
- [docs/REFERENCES.md](docs/REFERENCES.md) —— 参考开源项目调研与可复用点

## 📜 许可证

以 **[Apache-2.0](LICENSE)** 发布(宽松许可,可商用、可上架商店;不要求 CLA,贡献走 DCO,见 [CONTRIBUTING.md](CONTRIBUTING.md))。

依赖许可红线(禁 GPL/AGPL/LGPL)、HEVC 专利规避、第三方组件合规的完整说明见 [docs/LEGAL.md](docs/LEGAL.md)。
