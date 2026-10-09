#![no_main]

use mct_core::{LanguageParser, SourceFile};
use mct_lang_html::HtmlParser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(contents) = std::str::from_utf8(data) else {
        return;
    };
    let file = SourceFile {
        relative_path: "fuzz.html".to_string(),
        contents: contents.to_string(),
    };
    let _ = HtmlParser.parse(&file);
});
