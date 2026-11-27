#![no_main]

use mct_core::{LanguageParser, SourceFile};
use mct_lang_gdscript::GdscriptParser;
use libfuzzer_sys::fuzz_target;

// GdscriptParser::parse must never panic on arbitrary bytes, valid UTF-8 or not —
// it runs over untrusted third-party source. A syntax error must come back
// as ParseError::Syntax, never a crash.
fuzz_target!(|data: &[u8]| {
    let Ok(contents) = std::str::from_utf8(data) else {
        return;
    };
    let file = SourceFile {
        relative_path: "fuzz.gd".to_string(),
        contents: contents.to_string(),
    };
    let _ = GdscriptParser.parse(&file);
});
