<!-- SPDX-License-Identifier: Apache-2.0 -->

# Fuzzing

This directory contains `cargo-fuzz` targets for `imgconvert-core`.

Use:

```bash
pnpm run fuzz:prepare
pnpm run fuzz:check
pnpm run fuzz:replay
cargo fuzz run decode_pipeline
cargo fuzz run convert_pipeline
cargo fuzz run metadata_semantics
```

`fuzz/corpus/` is local-only and intentionally ignored. Do not commit third-party
or private camera images into this repository; put them under `corpus/real/` or
point `IMGCONVERT_REAL_CORPUS_DIRS` at local directories, then run
`pnpm run fuzz:prepare`.

`pnpm run fuzz:replay` replays prepared seeds and `fuzz/artifacts/<target>/`
crash inputs through normal `imgconvert-core` paths and writes
`target/fuzz-corpus/replay-report.json`.
