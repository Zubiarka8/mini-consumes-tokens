#![no_main]

use libfuzzer_sys::fuzz_target;
use mct_index::{read_glb_json_chunk, MAX_GLTF_JSON_BYTES};

fuzz_target!(|data: &[u8]| {
    // The real file length, then one the header may lie about.
    let _ = read_glb_json_chunk(&mut &data[..], data.len() as u64);
    let _ = read_glb_json_chunk(&mut &data[..], MAX_GLTF_JSON_BYTES * 2);
});
