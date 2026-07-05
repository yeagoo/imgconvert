// SPDX-License-Identifier: Apache-2.0

#![no_main]

use imgconvert_core::{codec_for, detect_lossy_artifacts, probe, thumbnail, Format};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_DECODE_PIXELS: u64 = 16_000_000;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() || data.len() > MAX_INPUT_BYTES {
        return;
    }

    let Some(format) = Format::from_magic(data) else {
        return;
    };
    let Ok(info) = probe(data) else {
        return;
    };
    if u64::from(info.width) * u64::from(info.height) > MAX_DECODE_PIXELS {
        return;
    }

    let _ = thumbnail(data, 256);
    if format == Format::Png {
        let _ = detect_lossy_artifacts(data);
    }
    let _ = codec_for(format).decode(data);
});
