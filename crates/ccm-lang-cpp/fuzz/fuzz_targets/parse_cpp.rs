#![no_main]

use ccm_core::{LanguageParser, SourceFile};
use ccm_lang_cpp::CppParser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(contents) = std::str::from_utf8(data) else {
        return;
    };
    let file = SourceFile {
        relative_path: "Fuzz.cpp".to_string(),
        contents: contents.to_string(),
    };
    let _ = CppParser.parse(&file);
});
