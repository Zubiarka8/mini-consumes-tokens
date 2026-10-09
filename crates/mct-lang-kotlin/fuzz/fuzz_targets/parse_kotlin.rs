#![no_main]

use mct_core::{LanguageParser, SourceFile};
use mct_lang_kotlin::KotlinParser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(contents) = std::str::from_utf8(data) else {
        return;
    };
    let file = SourceFile {
        relative_path: "fuzz.kt".to_string(),
        contents: contents.to_string(),
    };
    let _ = KotlinParser.parse(&file);
});
