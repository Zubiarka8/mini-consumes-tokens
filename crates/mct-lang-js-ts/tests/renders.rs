//! A component used as a JSX/TSX element or in an Astro template is a
//! `renders` relation, resolved like any reference; an Astro file's
//! frontmatter is TypeScript at its own lines.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_index::Index;
use mct_lang_js_ts::JsTsParser;

const FILES: &[(&str, &str)] = &[
    (
        "components/Button.tsx",
        "export function Button() {\n  return <button />;\n}\n",
    ),
    ("ui.tsx", "export function Card() {\n  return <div />;\n}\n"),
    (
        "App.tsx",
        "import { Button } from './components/Button';\nimport * as UI from './ui';\n\
         export function App() {\n  return <div><Button /><UI.Card /></div>;\n}\n",
    ),
    (
        "pages/index.astro",
        "---\nimport Layout from '../layouts/Layout.astro';\n\
         import { Button } from '../components/Button';\n\
         const title = shout('home');\n\
         function shout(s: string) {\n  return s.toUpperCase();\n}\n---\n\
         <Layout title={title}>\n  <h1>{title}</h1>\n  <Button />\n</Layout>\n",
    ),
    (
        "layouts/Layout.astro",
        "---\nconst { title } = Astro.props;\n---\n<html><title>{title}</title><slot /></html>\n",
    ),
];

fn indexed() -> Index {
    let root = std::env::temp_dir().join(format!("mct-renders-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, contents) in FILES {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }
    mct_corpus::open_indexed(&root, Arc::new(JsTsParser), FILES.len(), "renders fixture")
}

/// `name`'s `renders` relations as `from@path:line resolution target`.
fn renders(index: &Index, name: &str) -> Vec<String> {
    let mut rows: Vec<String> = index
        .find_references(name)
        .unwrap()
        .into_iter()
        .filter(|hit| hit.kind == "renders")
        .map(|hit| {
            let target = hit
                .target_id
                .map(|id| {
                    let row = &index.symbols_by_ids(&[id]).unwrap()[0];
                    format!(" {}:{}", row.relative_path, row.line)
                })
                .unwrap_or_default();
            format!(
                "{}@{}:{} {}{target}",
                hit.from_symbol,
                hit.relative_path,
                hit.line,
                hit.resolution.as_str()
            )
        })
        .collect();
    rows.sort();
    rows
}

#[test]
fn a_jsx_tag_renders_its_imported_component() {
    let index = indexed();
    assert_eq!(
        renders(&index, "Button"),
        [
            "App@App.tsx:4 resolved components/Button.tsx:1",
            "index@pages/index.astro:11 resolved components/Button.tsx:1",
        ]
    );
    // A member tag keeps its qualifier, as a reference did before: it is
    // never pinned to an unrelated bare `Card`.
    assert_eq!(renders(&index, "UI.Card"), ["App@App.tsx:4 unresolved"]);
}

#[test]
fn an_astro_template_renders_an_astro_component() {
    let index = indexed();
    assert_eq!(
        renders(&index, "Layout"),
        ["index@pages/index.astro:9 resolved layouts/Layout.astro:1"]
    );
    // An HTML element is not a component.
    assert!(index
        .find_references("h1")
        .unwrap()
        .iter()
        .all(|hit| hit.kind != "renders"));
}

#[test]
fn an_astro_frontmatter_is_typescript_at_its_own_lines() {
    let index = indexed();
    let shout = index.find_symbol("shout").unwrap();
    assert_eq!(shout.len(), 1, "{shout:?}");
    assert_eq!(
        (
            shout[0].relative_path.as_str(),
            shout[0].line,
            shout[0].kind.as_str()
        ),
        ("pages/index.astro", 5, "function")
    );
    let callers = index.find_callers("shout").unwrap();
    assert_eq!(callers.len(), 1, "{callers:?}");
    assert_eq!(
        (callers[0].line, callers[0].resolution.as_str()),
        (4, "resolved")
    );
}
