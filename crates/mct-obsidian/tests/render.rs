#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_index::SymbolListEntry;
use mct_obsidian::{render_index, FileEntry};

fn symbol(path: &str, name: &str, kind: &str, line: u32, parent: Option<&str>) -> SymbolListEntry {
    SymbolListEntry {
        name: name.into(),
        kind: kind.into(),
        relative_path: path.into(),
        line,
        end_line: Some(line + 2),
        parent: parent.map(String::from),
        language: "rust".into(),
        level: None,
    }
}

#[test]
fn files_are_grouped_by_directory_with_their_symbols() {
    let a = [symbol("src/a.rs", "run", "function", 3, None)];
    let b = [
        symbol("b.rs", "B", "struct", 1, None),
        symbol("b.rs", "go", "method", 5, Some("B")),
    ];
    let text = render_index(&[
        FileEntry {
            path: "src/a.rs",
            language: "rust",
            symbols: &a,
            imports: &["b.rs".into()],
        },
        FileEntry {
            path: "b.rs",
            language: "rust",
            symbols: &b,
            imports: &[],
        },
    ]);
    assert!(
        text.contains("## (root)") || text.contains("## `(root)`"),
        "{text}"
    );
    assert!(text.contains("## `src`"), "{text}");
    assert!(text.contains("Imports: `b.rs`"), "{text}");
    assert!(
        text.contains("- *method* `go` · lines 5-7 · in `B`"),
        "{text}"
    );
    assert!(text.starts_with("# Project index"));
}

#[test]
fn assets_and_symbolless_files_are_left_out() {
    let one = [symbol("logo.png", "logo.png", "asset", 1, None)];
    let text = render_index(&[
        FileEntry {
            path: "logo.png",
            language: "asset",
            symbols: &one,
            imports: &[],
        },
        FileEntry {
            path: "empty.rs",
            language: "rust",
            symbols: &[],
            imports: &[],
        },
    ]);
    assert!(
        !text.contains("logo.png") && !text.contains("empty.rs"),
        "{text}"
    );
    assert!(text.contains("0 files, 0 symbols"), "{text}");
}

#[test]
fn hostile_names_cannot_break_out_of_their_code_span() {
    let s = [symbol("a.rs", "evil`\n# heading", "function", 1, None)];
    let text = render_index(&[FileEntry {
        path: "a.rs",
        language: "rust",
        symbols: &s,
        imports: &[],
    }]);
    assert!(text.contains("`evil # heading`"), "{text}");
    assert!(!text.contains("\n# heading"), "{text}");
}

#[test]
fn rendering_is_deterministic() {
    let s = [symbol("a.rs", "x", "function", 1, None)];
    let f = || {
        render_index(&[FileEntry {
            path: "a.rs",
            language: "rust",
            symbols: &s,
            imports: &[],
        }])
    };
    assert_eq!(f(), f());
}

#[test]
fn file_frontmatter_is_yaml_with_exact_paths_and_bounded_deduplicated_lists() {
    use mct_obsidian::{render_file_note, MAX_IMPORT_LINKS, MAX_SYMBOLS};
    use std::collections::BTreeMap;
    let path = "src/a\"[x]:\u{0001}.rs";
    let symbols: Vec<_> = (0..MAX_SYMBOLS + 7)
        .map(|i| symbol(path, &format!("s{i}"), "function", i as u32 + 1, None))
        .collect();
    let mut imports: Vec<String> = (0..MAX_IMPORT_LINKS + 3)
        .map(|i| format!("src/dep{i}.rs"))
        .collect();
    imports.push(imports[0].clone());
    let mut paths: BTreeMap<_, _> = imports
        .iter()
        .map(|p| (p.clone(), format!("files/{}.md", p.replace('/', "_"))))
        .collect();
    paths.insert(path.into(), "files/a.md".into());
    let file = FileEntry {
        path,
        language: "rust",
        symbols: &symbols,
        imports: &imports,
    };
    let text = render_file_note(&file, "hash", &paths, false);
    let frontmatter = text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n")
        .unwrap()
        .0;
    let fields: BTreeMap<_, _> = frontmatter
        .lines()
        .map(|line| line.split_once(": ").unwrap())
        .collect();
    // JSON double-quoted scalars are also valid YAML scalars.
    for key in ["title", "path", "language", "content_hash"] {
        let _: String = serde_json::from_str(fields[key]).unwrap();
    }
    assert_eq!(
        serde_json::from_str::<String>(fields["path"]).unwrap(),
        path
    );
    assert_eq!(
        fields["symbol_count"].parse::<usize>().unwrap(),
        MAX_SYMBOLS + 7
    );
    assert!(text.contains("7 symbols omitted (limit: 100)"), "{text}");
    assert!(text.contains("3 imports omitted (limit: 50)"), "{text}");
    assert_eq!(text.matches("- **`function`**").count(), MAX_SYMBOLS);
    assert_eq!(text.matches("- [src/dep").count(), MAX_IMPORT_LINKS);
    assert!(text.contains("Directory: [src](../directories/"));
}
