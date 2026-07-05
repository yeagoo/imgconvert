// SPDX-License-Identifier: Apache-2.0

#![no_main]

use imgconvert_core::{inspect_metadata_semantics, normalize_metadata_semantics, RawMetadata};
use libfuzzer_sys::fuzz_target;

const MAX_METADATA_FUZZ_BYTES: usize = 1024 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_METADATA_FUZZ_BYTES {
        return;
    }

    let first = data.len() / 3;
    let second = first * 2;
    let metadata = RawMetadata {
        icc: None,
        exif: Some(data[..first].to_vec()),
        xmp: Some(data[first..second].to_vec()),
        iptc: Some(data[second..].to_vec()),
    };

    let _ = inspect_metadata_semantics(&metadata);
    let normalized = normalize_metadata_semantics(metadata);
    let _ = inspect_metadata_semantics(&normalized);
});
