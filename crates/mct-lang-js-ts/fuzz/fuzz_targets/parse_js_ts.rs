#![no_main]

use mct_core::{LanguageParser, SourceFile};
use mct_lang_js_ts::JsTsParser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    let Ok(contents) = std::str::from_utf8(rest) else {
        return;
    };
    // One byte of the input selects which of the three grammars
    // (`tree-sitter-javascript`, `LANGUAGE_TYPESCRIPT`, `LANGUAGE_TSX`) this
    // run exercises, so a single corpus/campaign covers all of them rather
    // than just whichever extension a fixed filename would pick.
    let relative_path = match selector % 3 {
        0 => "Fuzz.js",
        1 => "Fuzz.ts",
        _ => "Fuzz.tsx",
    };
    let file = SourceFile {
        relative_path: relative_path.to_string(),
        contents: contents.to_string(),
    };
    let _ = JsTsParser.parse(&file);
});
