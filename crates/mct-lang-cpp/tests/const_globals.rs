//! A namespace-scope declaration whose value can't change is a `constant`:
//! `const` or `constexpr` with a plain name or an array (`const int A`,
//! `constexpr int B`, `const int C[]`), and a pointer that is itself
//! `const` (`char* const q`). A pointer to const (`const char* p`) can be
//! repointed and a reference (`const int& r`) is a binding, so both stay a
//! `variable`, as does `constinit`, which only fixes the initialization. A
//! class member stays a `field`, whatever its qualifiers.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, SymbolKind};
use mct_lang_cpp::CppParser;

const SOURCE: &str = r#"const int A = 1;
constexpr int B = 2;
const int C[] = {1};
char* const q = nullptr;
const char* p = nullptr;
const int& r = A;
constinit int F = 1;
int v = 3;
namespace shop {
static const int D = 4, E = 5;
struct Cart {
    static const int MAX = 9;
};
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CppParser, "src/shop.cpp", "C++", SOURCE)
}

#[test]
fn an_unchangeable_global_is_a_constant() {
    let rows: Vec<String> = parsed()
        .symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Constant | SymbolKind::Variable | SymbolKind::Field
            )
        })
        .map(|s| format!("{} {:?}", s.name, s.kind))
        .collect();
    assert_eq!(
        rows,
        [
            "A Constant",
            "B Constant",
            "C Constant",
            "q Constant",
            "p Variable",
            "r Variable",
            "F Variable",
            "v Variable",
            "D Constant",
            "E Constant",
            "MAX Field",
        ]
    );
}
