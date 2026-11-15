//! The "possible entry point" hint for exports the Next.js App Router may
//! load by file convention (`src/nextjs.rs`): what is marked, what is left
//! alone, and what the hint claims.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{EntryPointHint, LanguageParser, ParsedFile, SourceFile};
use mct_lang_js_ts::JsTsParser;

fn parse(path: &str, src: &str) -> ParsedFile {
    JsTsParser
        .parse(&SourceFile {
            relative_path: path.to_string(),
            contents: src.to_string(),
        })
        .expect("valid source should parse")
}

/// `(symbol name, rule)` of every hint, in source order.
fn marked(path: &str, src: &str) -> Vec<(String, String)> {
    let parsed = parse(path, src);
    parsed
        .entry_points
        .iter()
        .map(|hint| {
            let symbol = parsed
                .symbols
                .iter()
                .find(|s| s.id == hint.symbol)
                .expect("a hint names a symbol of its file");
            (symbol.name.clone(), hint.rule.clone())
        })
        .collect()
}

fn one(path: &str, src: &str) -> EntryPointHint {
    let mut found = parse(path, src).entry_points;
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

fn pair(name: &str, rule: &str) -> (String, String) {
    (name.to_string(), rule.to_string())
}

#[test]
fn a_page_default_export_is_a_possible_entry_point() {
    let hint = one(
        "app/(home)/page.tsx",
        "export default function HomePage() { return null; }\n",
    );
    assert_eq!(hint.framework, "next.js");
    assert_eq!(hint.rule, "app-router-file-convention");
    assert!(
        hint.evidence
            .contains("default export of page.tsx under app/"),
        "{}",
        hint.evidence
    );
    let requires = hint.requires.expect("the hint depends on the project");
    assert_eq!(requires.manifest_path, "package.json");
    assert_eq!(requires.package, "next");
    assert_eq!(requires.min_major, 13);
}

#[test]
fn route_handlers_and_build_functions_are_marked_by_their_export_name() {
    let src = "export async function GET() { return new Response(''); }\n\
               export const POST = async () => new Response('');\n\
               export function generateStaticParams() { return []; }\n\
               export function helper() {}\n";
    assert_eq!(
        marked("app/api/search/route.ts", src),
        vec![
            pair("GET", "app-router-file-convention"),
            pair("POST", "app-router-file-convention"),
            pair("generateStaticParams", "app-router-file-convention"),
        ]
    );
}

#[test]
fn a_layout_marks_its_default_export_and_its_metadata_function() {
    let src = "export function generateMetadata() { return {}; }\n\
               export default function Layout({ children }) { return children; }\n\
               export function GET() {}\n";
    assert_eq!(
        marked("app/docs/layout.tsx", src),
        vec![
            pair("generateMetadata", "app-router-file-convention"),
            pair("Layout", "app-router-file-convention"),
        ]
    );
}

#[test]
fn the_requirement_follows_the_application_root() {
    let src = "export default function Page() { return null; }\n";
    for (path, manifest) in [
        (
            "website/app/docs/[[...slug]]/page.tsx",
            "website/package.json",
        ),
        ("apps/web/src/app/page.jsx", "apps/web/package.json"),
        ("src/app/page.js", "package.json"),
    ] {
        let requires = one(path, src).requires.unwrap();
        assert_eq!(requires.manifest_path, manifest, "{path}");
    }
}

#[test]
fn files_outside_the_conventions_are_left_alone() {
    let src = "export default function Page() { return null; }\n\
               export function GET() {}\n";
    for path in [
        // Not a convention file name.
        "app/docs/page-header.tsx",
        "app/components/button.tsx",
        // Not under an `app` directory.
        "pages/page.tsx",
        "lib/route.ts",
        // A private folder is excluded from routing.
        "app/_internal/page.tsx",
        // Not an extension the App Router reads.
        "app/route.mjs",
    ] {
        assert!(parse(path, src).entry_points.is_empty(), "{path}");
    }
}

#[test]
fn only_exported_declarations_are_marked() {
    let src = "function GET() {}\nexport { GET as POST };\nexport function GET2() {}\n";
    assert!(marked("app/route.ts", src).is_empty());
}
