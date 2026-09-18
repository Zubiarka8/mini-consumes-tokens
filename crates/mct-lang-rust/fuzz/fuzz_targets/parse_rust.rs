#![no_main]

use mct_core::{LanguageParser, SourceFile};
use mct_lang_rust::RustParser;
use libfuzzer_sys::fuzz_target;

// RustParser::parse must never panic on arbitrary bytes — it runs over
// untrusted third-party source. A syntax error must come back as
// ParseError::Syntax, never a crash.
fuzz_target!(|data: &[u8]| {
    let Ok(contents) = std::str::from_utf8(data) else {
        return;
    };
    let file = SourceFile {
        relative_path: "fuzz.rs".to_string(),
        contents: contents.to_string(),
    };
    let _ = RustParser.parse(&file);
});
