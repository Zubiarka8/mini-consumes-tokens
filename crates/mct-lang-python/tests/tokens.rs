//! Token extraction for Python (issue #207): kinds, literals, trivia, lines,
//! syntax errors and pathological nesting.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{TokenKind, TokenSource, TokenizedFile};
use mct_lang_python::PythonTokenSource;
use TokenKind::{BlockEnd, BlockStart, Identifier, Keyword, Literal, Punctuation};

fn tokenize(source: &str) -> TokenizedFile {
    PythonTokenSource.tokenize("pkg/mod.py", source).unwrap()
}

fn pairs(file: &TokenizedFile) -> Vec<(TokenKind, &str)> {
    file.tokens
        .iter()
        .map(|token| (token.kind, token.text.as_str()))
        .collect()
}

#[test]
fn language_id_matches_the_parser() {
    use mct_core::LanguageParser;
    assert_eq!(
        PythonTokenSource.language_id(),
        mct_lang_python::PythonParser.language_id()
    );
}

#[test]
fn structural_strings_keep_interpolation_expressions_and_format_expressions() {
    let source = "def format_value(value, width, precision):\n    return f\"{{escaped}} {value + value!r:{width}.{precision}f}\"\n";
    let structural = PythonTokenSource
        .tokenize_structural("sample.py", source)
        .unwrap();
    assert!(structural.error_lines.is_empty(), "{structural:?}");
    for expected in ["+", "width", "precision", "!r"] {
        assert!(
            structural.tokens.iter().any(|token| token.text == expected),
            "{expected}: {structural:?}"
        );
    }
    assert_eq!(
        tokenize(source)
            .tokens
            .iter()
            .filter(|token| token.kind == Literal)
            .count(),
        1
    );
    for token in &structural.tokens {
        assert_eq!(&source[token.start_byte..token.end_byte], token.text);
    }
}

#[test]
fn structural_plain_strings_stay_atomic_and_nested_strings_are_safe() {
    let source = "def plain():\n    return 'escaped\\ntext'\n";
    assert_eq!(
        tokenize(source),
        PythonTokenSource
            .tokenize_structural("sample.py", source)
            .unwrap()
    );
    for source in [
        "x = f'{call(\"a\")}'\n",
        "x = f'{value'\n",
        "x = f'{value:{width}}'\n",
    ] {
        let first = PythonTokenSource
            .tokenize_structural("sample.py", source)
            .unwrap();
        let second = PythonTokenSource
            .tokenize_structural("sample.py", source)
            .unwrap();
        assert_eq!(first, second);
    }
}

#[test]
fn keywords_are_told_apart_from_identifiers() {
    let source =
        "async def add(self, x):\n    if x and not self.done:\n        return await x\n    pass\n";
    let file = tokenize(source);
    assert!(file.error_lines.is_empty());
    let tokens = pairs(&file);
    for keyword in [
        "async", "def", "if", "and", "not", "return", "await", "pass",
    ] {
        assert!(tokens.contains(&(Keyword, keyword)), "{keyword}");
    }
    // `self` is an ordinary name in Python.
    for name in ["add", "self", "x", "done"] {
        assert!(tokens.contains(&(Identifier, name)), "{name}");
    }
    assert!(!tokens.contains(&(Keyword, "add")));
}

#[test]
fn operators_and_punctuation_are_kept() {
    let file = tokenize("def f(a, *b, **c) -> int:\n    return a[0] ** 2 // 3\n");
    let punctuation: Vec<&str> = file
        .tokens
        .iter()
        .filter(|token| token.kind == Punctuation)
        .map(|token| token.text.as_str())
        .collect();
    assert_eq!(
        punctuation,
        ["(", ",", "*", ",", "**", ")", "->", ":", "[", "]", "**", "//"]
    );
}

#[test]
fn every_kind_of_literal_is_a_single_token() {
    let source = concat!(
        "x = [1_000, 0x1F, 1.5e3, 2j, True, False, None, ...]\n",
        "y = ('cc', \"es\\n\\\"c\", r'raw\\d', b\"by\", f\"hi {a!r:>{w}} {{lit}}\")\n",
    );
    let file = tokenize(source);
    assert!(file.error_lines.is_empty());
    let literals: Vec<&str> = file
        .tokens
        .iter()
        .filter(|token| token.kind == Literal)
        .map(|token| token.text.as_str())
        .collect();
    assert_eq!(
        literals,
        [
            "1_000",
            "0x1F",
            "1.5e3",
            "2j",
            "True",
            "False",
            "None",
            "...",
            "'cc'",
            "\"es\\n\\\"c\"",
            "r'raw\\d'",
            "b\"by\"",
            "f\"hi {a!r:>{w}} {{lit}}\"",
        ]
    );
    // The interpolation's names are inside the f-string literal, not tokens.
    assert!(!file.tokens.iter().any(|token| token.text == "w"));
}

#[test]
fn implicit_concatenation_keeps_one_literal_per_string() {
    let file = tokenize("x = 'a' 'b'\n");
    assert_eq!(
        pairs(&file),
        [
            (Identifier, "x"),
            (Punctuation, "="),
            (Literal, "'a'"),
            (Literal, "'b'")
        ]
    );
}

#[test]
fn a_multi_line_string_is_one_token_spanning_its_lines() {
    let source = "def f():\n    \"\"\"doc\n    string\"\"\"\n    return 1\n";
    let file = tokenize(source);
    let doc = file
        .tokens
        .iter()
        .find(|token| token.kind == Literal && token.text.starts_with("\"\"\""))
        .unwrap();
    assert_eq!((doc.line, doc.end_line), (2, 3));
    assert_eq!(&source[doc.start_byte..doc.end_byte], doc.text);
    let one = file.tokens.iter().find(|token| token.text == "1").unwrap();
    assert_eq!((one.line, one.end_line), (4, 4));
}

#[test]
fn comments_line_continuations_and_whitespace_produce_no_tokens() {
    let source = "#!/usr/bin/env python\n# note\nx = (1 +  # why\n     2) \\\n    + 3\n\n\n";
    let file = tokenize(source);
    assert!(file.error_lines.is_empty());
    assert_eq!(
        pairs(&file),
        [
            (Identifier, "x"),
            (Punctuation, "="),
            (Punctuation, "("),
            (Literal, "1"),
            (Punctuation, "+"),
            (Literal, "2"),
            (Punctuation, ")"),
            (Punctuation, "+"),
            (Literal, "3"),
        ]
    );
    // `#` inside a string is part of the literal.
    let file = tokenize("s = '# not a comment'\n");
    assert!(file
        .tokens
        .iter()
        .any(|token| token.kind == Literal && token.text == "'# not a comment'"));
}

#[test]
fn indentation_width_does_not_change_the_tokens() {
    let four = tokenize("if a:\n    b()\n");
    let eight = tokenize("if a:\n        b()\n");
    assert_eq!(pairs(&four), pairs(&eight));
}

#[test]
fn every_block_is_bracketed_by_markers_around_its_statements() {
    let file = tokenize("if a:\n    b()\nc()\n");
    assert_eq!(
        pairs(&file),
        [
            (Keyword, "if"),
            (Identifier, "a"),
            (Punctuation, ":"),
            (BlockStart, ""),
            (Identifier, "b"),
            (Punctuation, "("),
            (Punctuation, ")"),
            (BlockEnd, ""),
            (Identifier, "c"),
            (Punctuation, "("),
            (Punctuation, ")"),
        ]
    );
    let lines: Vec<(TokenKind, u32)> = file
        .tokens
        .iter()
        .filter(|token| token.kind.is_block_marker())
        .map(|token| (token.kind, token.line))
        .collect();
    assert_eq!(lines, [(BlockStart, 2), (BlockEnd, 2)]);
}

#[test]
fn moving_a_statement_out_of_a_block_changes_the_tokens() {
    let inside = tokenize("if a:\n    b()\n    c()\n");
    let outside = tokenize("if a:\n    b()\nc()\n");
    let texts = |file: &TokenizedFile| -> Vec<String> {
        file.tokens
            .iter()
            .filter(|token| !token.kind.is_block_marker())
            .map(|token| token.text.clone())
            .collect()
    };
    // Same source tokens, different nesting: only the markers tell them apart.
    assert_eq!(texts(&inside), texts(&outside));
    assert_ne!(pairs(&inside), pairs(&outside));
}

#[test]
fn a_one_line_suite_matches_its_indented_form() {
    let one_line = tokenize("for x in y: f(x)\n");
    let indented = tokenize("for x in y:\n    f(x)\n");
    assert_eq!(pairs(&one_line), pairs(&indented));
}

#[test]
fn a_functions_markers_stay_within_its_own_lines() {
    let source = "def f(a):\n    if a:\n        return 1\n    return 2\n\n\ny = 3\n";
    let file = tokenize(source);
    let markers: Vec<(TokenKind, u32)> = file
        .tokens
        .iter()
        .filter(|token| token.kind.is_block_marker())
        .map(|token| (token.kind, token.line))
        .collect();
    assert_eq!(
        markers,
        [
            (BlockStart, 2),
            (BlockStart, 3),
            (BlockEnd, 3),
            (BlockEnd, 4)
        ]
    );
}

#[test]
fn token_offsets_are_in_order_and_index_the_source() {
    let source = "def \u{e9}t\u{e9}():\n    s = '\u{1f980}'\n";
    let file = tokenize(source);
    for token in &file.tokens {
        assert_eq!(&source[token.start_byte..token.end_byte], token.text);
    }
    assert!(file
        .tokens
        .windows(2)
        .all(|pair| pair[0].end_byte <= pair[1].start_byte));
    assert!(file
        .tokens
        .iter()
        .any(|token| token.kind == Identifier && token.text == "\u{e9}t\u{e9}"));
}

#[test]
fn a_syntax_error_is_reported_by_line_without_panicking() {
    let source = "def ok():\n    pass\n\ndef broken(:\n    pass\n\ndef also_ok():\n    pass\n";
    let file = tokenize(source);
    assert!(
        file.error_lines
            .iter()
            .any(|range| range.start <= 4 && range.end >= 4),
        "{:?}",
        file.error_lines
    );
    assert!(file.tokens.iter().any(|token| token.text == "ok"));
    assert!(tokenize("def ok():\n    pass\n").error_lines.is_empty());
}

#[test]
fn garbage_and_empty_input_never_panic() {
    assert!(tokenize("").tokens.is_empty());
    let file = tokenize("))) \u{0} $$ ?? \"unterminated\n  if\n");
    assert!(!file.error_lines.is_empty());
}

#[test]
fn pathologically_deep_nesting_does_not_overflow_the_stack() {
    let depth = 10_000;
    let source = format!("x = {}1{}\n", "(".repeat(depth), ")".repeat(depth));
    let handle = std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(move || tokenize(&source).tokens.len())
        .unwrap();
    assert!(handle.join().unwrap() > 2 * depth);
}

fn roles(file: &TokenizedFile) -> Vec<(&str, mct_core::TokenRole)> {
    file.tokens
        .iter()
        .filter(|token| matches!(token.kind, Identifier | Literal))
        .map(|token| (token.text.as_str(), token.role))
        .collect()
}

#[test]
fn identifier_roles_are_syntactic_positions() {
    use mct_core::TokenRole::{Callee, Declaration, Member, Parameter, Type, Unclassified};
    let source = "\
def load(user, limit: int, *rest, mode=None, flag: bool = True) -> Result:
    local = user.email
    send(local, retries=limit)
    user.mailer.notify(local)
    value: typing.Optional[int] = None
    return lambda item: item
";
    let file = tokenize(source);
    let roles = roles(&file);
    for expected in [
        ("load", Declaration),
        ("user", Parameter),
        ("limit", Parameter),
        ("mode", Parameter),
        ("flag", Parameter),
        ("int", Type),
        ("bool", Type),
        ("Result", Type),
        ("email", Member),
        ("notify", Member),
        ("retries", Member),
        ("send", Callee),
        ("typing", Type),
        ("Optional", Type),
        ("item", Parameter),
        ("rest", Parameter),
        // Locals and references are not classified.
        ("local", Unclassified),
        ("value", Unclassified),
    ] {
        assert!(roles.contains(&expected), "{expected:?} in {roles:?}");
    }
    // A later reference to a parameter is a reference, not a declaration.
    assert!(roles.contains(&("user", Unclassified)), "{roles:?}");
}

/// Every role a name takes, in source order.
fn roles_of(roles: &[(&str, mct_core::TokenRole)], name: &str) -> Vec<mct_core::TokenRole> {
    roles
        .iter()
        .filter(|(text, _)| *text == name)
        .map(|(_, role)| *role)
        .collect()
}

#[test]
fn splat_parameters_are_parameters_but_unpacking_and_splat_arguments_are_not() {
    use mct_core::TokenRole::{Parameter, Unclassified};
    let source = "\
def forward(first, *args, key, **kwargs):
    head, *tail = args
    for *init, last in pairs:
        pass
    pick = lambda *xs, **kw: xs
    return first(*args, **kwargs)


def typed(*values: int, **named: str):
    pass
";
    let structural = PythonTokenSource
        .tokenize_structural("pkg/mod.py", source)
        .unwrap();
    for file in [tokenize(source), structural] {
        let roles = roles(&file);
        // Declared, then read, then splatted into a call.
        assert_eq!(
            roles_of(&roles, "args"),
            [Parameter, Unclassified, Unclassified]
        );
        assert_eq!(roles_of(&roles, "kwargs"), [Parameter, Unclassified]);
        assert_eq!(roles_of(&roles, "key"), [Parameter]);
        assert_eq!(roles_of(&roles, "xs"), [Parameter, Unclassified]);
        assert_eq!(roles_of(&roles, "kw"), [Parameter]);
        assert_eq!(roles_of(&roles, "values"), [Parameter]);
        assert_eq!(roles_of(&roles, "named"), [Parameter]);
        // The same splat patterns as unpacking targets are locals.
        for local in ["head", "tail", "init", "last"] {
            assert_eq!(roles_of(&roles, local), [Unclassified], "{local}");
        }
    }
}

#[test]
fn literal_categories_include_interpolation_and_docstrings_in_both_modes() {
    use mct_core::LiteralCategory::{Docstring, InterpolatedString, Null, Number, String};
    use mct_core::TokenRole::Literal as Lit;
    let source = "\
def f(x):
    \"\"\"Doc.\"\"\"
    a = f\"{x + 1}\"
    b = \"plain\"
    return 2 if a else None
";
    let broad = tokenize(source);
    let roles = roles(&broad);
    for expected in [
        ("\"\"\"Doc.\"\"\"", Lit(Docstring)),
        ("f\"{x + 1}\"", Lit(InterpolatedString)),
        ("\"plain\"", Lit(String)),
        ("2", Lit(Number)),
        ("None", Lit(Null)),
    ] {
        assert!(roles.contains(&expected), "{expected:?} in {roles:?}");
    }
    // The structural stream expands the f-string; its fragments are strings
    // and its expression keeps ordinary roles.
    let structural = PythonTokenSource
        .tokenize_structural("pkg/mod.py", source)
        .unwrap();
    let roles = self::roles(&structural);
    assert!(
        !roles
            .iter()
            .any(|(_, role)| *role == Lit(InterpolatedString)),
        "{roles:?}"
    );
    assert!(roles.contains(&("1", Lit(Number))), "{roles:?}");
}

fn contexts(file: &TokenizedFile) -> Vec<(String, Vec<String>, Vec<String>)> {
    file.callables
        .as_ref()
        .unwrap()
        .iter()
        .map(|span| {
            (
                span.name.clone(),
                span.decorators.clone(),
                span.enclosing.clone(),
            )
        })
        .collect()
}

#[test]
fn callables_carry_decorators_and_enclosing_constructs() {
    let source = "\
@cache
@app.route(
    \"/x\",   # comment inside
)
def top(): pass

try:
    def guarded(): pass
except ImportError:
    def fallback(): pass

@dataclass
class Box:
    def method(self):
        def inner(): pass
    if DEBUG:
        def debug(self): pass
    else:
        with lock:
            def locked(self): pass
";
    for file in [
        tokenize(source),
        PythonTokenSource
            .tokenize_structural("pkg/mod.py", source)
            .unwrap(),
    ] {
        let strings = |items: &[&str]| -> Vec<String> {
            items.iter().map(|item| (*item).to_string()).collect()
        };
        assert_eq!(
            contexts(&file),
            [
                (
                    "top".to_string(),
                    strings(&["@cache", "@app.route( \"/x\", )"]),
                    strings(&[])
                ),
                ("guarded".to_string(), strings(&[]), strings(&["try"])),
                (
                    "fallback".to_string(),
                    strings(&[]),
                    strings(&["try", "except"])
                ),
                (
                    "method".to_string(),
                    strings(&[]),
                    strings(&["@dataclass class"])
                ),
                (
                    "inner".to_string(),
                    strings(&[]),
                    strings(&["@dataclass class", "function"])
                ),
                (
                    "debug".to_string(),
                    strings(&[]),
                    strings(&["@dataclass class", "if"])
                ),
                (
                    "locked".to_string(),
                    strings(&[]),
                    strings(&["@dataclass class", "if", "else", "with"])
                ),
            ]
        );
    }
}

fn body_shapes(file: &TokenizedFile) -> Vec<(String, Option<(usize, usize)>)> {
    file.callables
        .as_ref()
        .unwrap()
        .iter()
        .map(|span| {
            (
                span.name.clone(),
                span.body.map(|body| (body.statements, body.flat_calls)),
            )
        })
        .collect()
}

#[test]
fn body_shapes_count_flat_call_statements_without_reading_callee_names() {
    let source = "\
def logs(msg):
    \"\"\"Docstring, not a statement.\"\"\"
    log.info(msg)  # comment
    print(f\"{msg!r} done\", sep=\"\")
    assert_called(msg, *rest, key=VALUE)

def nested(msg):
    log.info(compute(msg))
    log.info(f\"{compute(msg)}\")
    print(lambda: msg)
    print([m for m in msg])
    print(a if msg else b)
    log.info(msg).flush()

def other(msg):
    await_me = log.info(msg)
    return print(msg)
    await notify(msg)
    if msg: print(msg)
    pass

def empty():
    \"\"\"Only a docstring.\"\"\"
";
    for file in [
        tokenize(source),
        PythonTokenSource
            .tokenize_structural("pkg/mod.py", source)
            .unwrap(),
    ] {
        assert_eq!(
            body_shapes(&file),
            [
                ("logs".to_string(), Some((3, 3))),
                ("nested".to_string(), Some((6, 0))),
                ("other".to_string(), Some((5, 0))),
                ("empty".to_string(), Some((0, 0))),
            ]
        );
    }
}

#[test]
fn primitive_spellings_require_complete_direct_annotations() {
    let source = r#"
def sample(a: int, b: str = "", c: Optional[int] = None,
           d: list[str] = [], e: int | str = 1, f: "int" = 1,
           g: pkg.int = 1, h: (int) = 1, i: Custom = 1) -> bool:
    local: float = 1.0
    return True
"#;
    for file in [
        tokenize(source),
        PythonTokenSource
            .tokenize_structural("t.py", source)
            .unwrap(),
    ] {
        assert!(file.error_lines.is_empty(), "{:?}", file.error_lines);
        let recognized: Vec<_> = file
            .tokens
            .iter()
            .filter(|token| token.primitive_spelling)
            .map(|token| token.text.as_str())
            .collect();
        assert_eq!(recognized, ["int", "str", "bool", "float"]);
        assert!(file
            .tokens
            .iter()
            .filter(|token| token.primitive_spelling)
            .all(|token| token.role == mct_core::TokenRole::Type));
    }
}
