#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, SourceFile, MAX_LITERALS_PER_FILE, MAX_LITERAL_CHARS};
use mct_lang_md::MarkdownParser;

#[test]
fn paragraphs_and_table_rows_are_literals() {
    let source = "---\ntitle: Front matter title here\n---\n\
# Heading text only\n\
\n\
A paragraph of prose\nwrapped over two lines.\n\
\n\
| What | Duration |\n\
| ---- | -------- |\n\
| register extensions & spawn host | 381 |\n\
\n\
```\nnot indexed code block text\n```\n";
    let file = SourceFile {
        relative_path: "note.md".to_string(),
        contents: source.to_string(),
    };
    let parsed = MarkdownParser.parse(&file).unwrap();
    let got: Vec<(&str, u32)> = parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect();
    assert_eq!(
        got,
        vec![
            ("A paragraph of prose wrapped over two lines.", 6),
            ("| What | Duration |", 9),
            ("| register extensions & spawn host | 381 |", 11),
        ]
    );
}

#[test]
fn nested_prose_is_kept_but_both_code_block_forms_are_excluded() {
    let source = r#"- A list item with prose

> A quoted paragraph with prose

    Indented code should be excluded

~~~text
Fenced code should be excluded
~~~
"#;
    let parsed = MarkdownParser
        .parse(&SourceFile {
            relative_path: "nested.md".into(),
            contents: source.into(),
        })
        .unwrap();
    let got: Vec<_> = parsed
        .literals
        .iter()
        .map(|literal| (literal.text.as_str(), literal.line))
        .collect();
    assert_eq!(
        got,
        vec![
            ("A list item with prose", 1),
            ("A quoted paragraph with prose", 3),
        ]
    );
}

#[test]
fn shared_filter_deduplication_and_unicode_truncation_apply() {
    let long = "ñandú ".repeat(100);
    let source = format!(
        "Repeated prose message\n\nRepeated   prose message\n\n\
         | Identifier | Time |\n| --- | --- |\n\
         | code/agentHost/didConnect | 1791 |\n\
         | https://example.com | 493 |\n\n\
         Authorization: Bearer 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b\n\n{long}\n"
    );
    let parsed = MarkdownParser
        .parse(&SourceFile {
            relative_path: "filtered.md".into(),
            contents: source,
        })
        .unwrap();
    assert_eq!(parsed.literals.len(), 3);
    assert_eq!(parsed.literals[0].text, "Repeated prose message");
    assert_eq!(parsed.literals[0].line, 1);
    assert_eq!(parsed.literals[1].text, "| Identifier | Time |");
    assert_eq!(parsed.literals[2].text.chars().count(), MAX_LITERAL_CHARS);
    assert!(parsed.literals[2].text.starts_with("ñandú ñandú"));
}

#[test]
fn literal_cap_does_not_stop_relation_extraction() {
    let mut source = String::new();
    for i in 0..MAX_LITERALS_PER_FILE + 5 {
        source.push_str(&format!("Distinct prose message {i}\n\n"));
    }
    source.push_str("See [[TargetNote]] after the literal cap.\n");
    let parsed = MarkdownParser
        .parse(&SourceFile {
            relative_path: "capped.md".into(),
            contents: source,
        })
        .unwrap();
    assert_eq!(parsed.literals.len(), MAX_LITERALS_PER_FILE);
    assert_eq!(
        parsed.literals.last().unwrap().line as usize,
        2 * MAX_LITERALS_PER_FILE - 1
    );
    assert!(parsed.relations.iter().any(|r| r.to_name == "TargetNote"));
}
