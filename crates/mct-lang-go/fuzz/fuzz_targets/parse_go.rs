#![no_main]

use mct_core::{LanguageParser, SourceFile};
use mct_lang_go::GoParser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(contents) = std::str::from_utf8(data) else {
        return;
    };
    let file = SourceFile {
        relative_path: "fuzz.go".to_string(),
        contents: contents.to_string(),
    };
    let _ = GoParser.parse(&file);
});
