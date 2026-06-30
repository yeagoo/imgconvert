# 贡献指南

感谢你对 ImgConvert 的兴趣!

## 许可证

本项目以 **Apache-2.0** 发布。提交贡献即表示你同意你的贡献也在该许可证下发布。

## 贡献的授权方式(DCO,无需 CLA)

贡献遵循 **inbound = outbound** 原则:你提交的贡献即以与项目相同的 **Apache-2.0** 许可证授权,贡献者保留自己的著作权(Apache-2.0 §5 已含贡献条款)。

我们使用 **DCO(Developer Certificate of Origin)** 这一轻量机制,代替 CLA:

- 提交时加上 `Signed-off-by: 你的名字 <你的邮箱>`(用 `git commit -s` 自动添加)。
- 这表示你声明:你有权提交该贡献,且同意以项目许可证发布。DCO 全文见 https://developercertificate.org/。

## ⚠️ 依赖与代码复用红线(宽松 + 上架)

本项目要静态链接编解码库并上架应用商店,**严禁引入 copyleft**:

- ⛔ 不得引入 **GPL / AGPL / LGPL** 依赖(CI 用 `cargo deny` 拦截);典型被禁:`imagequant`(GPL)、`dssim`(AGPL)、libvips/libheif(LGPL)。
- ⛔ 不得直接拷贝 **GPL/AGPL** 项目源码(如 vert、Hando);只能借鉴思路、自行实现。
- ✅ 复用宽松许可(MIT/BSD/Apache-2.0)源码时,保留其许可证 + 版权声明,并在 `THIRD_PARTY_LICENSES` / `NOTICE` 列出来源。
- 详见 [docs/LEGAL.md](docs/LEGAL.md)。

## 开发

见 [README.md](README.md) 的「开发」一节。提交前请确保:

- `pnpm run check` 通过
- 新增源文件带 `// SPDX-License-Identifier: Apache-2.0` 头(HTML/Svelte 用 `<!-- -->`)
- 不引入 GPL/AGPL/LGPL 依赖(CI 会用 `cargo deny` 校验,见 `src-tauri/deny.toml`)
