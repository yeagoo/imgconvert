// SPDX-License-Identifier: Apache-2.0

#![no_main]

use std::time::Duration;

use imgconvert_core::{
    convert_best_of_with_color_policy_timeout, probe, AvifSubsample, ColorManagementPolicy,
    EncodeOptions, Format,
};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_CONVERT_PIXELS: u64 = 1_000_000;

fuzz_target!(|data: &[u8]| {
    if data.len() < 8 || data.len() > MAX_INPUT_BYTES {
        return;
    }

    let Ok(info) = probe(data) else {
        return;
    };
    if u64::from(info.width) * u64::from(info.height) > MAX_CONVERT_PIXELS {
        return;
    }

    let target = match data[0] & 0b11 {
        0 => Format::Png,
        1 => Format::Jpeg,
        2 => Format::WebP,
        _ => Format::Avif,
    };
    let options = EncodeOptions {
        quality: 82,
        lossless: matches!(target, Format::Png | Format::WebP) && (data[1] & 1 == 1),
        png_oxipng_level: 0,
        webp_method: 0,
        avif_speed: 10,
        avif_subsample: AvifSubsample::Yuv444,
        preserve_metadata: data[2] & 1 == 1,
        ..EncodeOptions::default()
    };

    if let Ok(encoded) = convert_best_of_with_color_policy_timeout(
        data,
        target,
        &[options],
        None,
        ColorManagementPolicy::PreserveEmbeddedProfile,
        Duration::from_secs(2),
    ) {
        assert_eq!(Format::from_magic(&encoded), Some(target));
        let _ = probe(&encoded);
    }
});
