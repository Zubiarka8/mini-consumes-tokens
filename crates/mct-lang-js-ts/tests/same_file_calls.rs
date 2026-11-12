//! A bare call `f()` reaches the binding lexically in scope: when nothing
//! around it binds `f` (no parameter or local, no import, no global) and
//! the file declares `f` itself, the target is in this file. The same holds
//! for a same-file function used as a value. A local the file does not
//! also declare at its own level is in this file too, with no symbol, so
//! the call stays unresolved instead of matching another file's name. A
//! local shadowing a file-level name, an import, a global or a name the
//! file never declares gives no such evidence. An import names the module
//! its specifier ends in, as a call through it does.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, RelationTarget};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"import { imported } from "./m";
function helper() {}
const handler = () => helper();
let state = make();
helper();
state();
imported();
parseInt("1");
function outer(helper) {
  helper();
}
unknown();
[1].map(helper);
"#;

#[test]
fn a_bare_call_to_a_name_the_file_declares_targets_this_file() {
    let parsed = mct_corpus::parse_source(&JsTsParser, "src/a.ts", "TypeScript", SOURCE);
    let targets: Vec<(RelationKind, String, Option<String>)> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r.kind, RelationKind::Calls | RelationKind::References))
        .map(|(i, r)| {
            let path = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t: &RelationTarget| t.path.clone());
            (r.kind, r.to_name.clone(), path)
        })
        .collect();
    let here = Some("src/a.ts".to_string());
    let row = |kind, name: &str, path: &Option<String>| (kind, name.to_string(), path.clone());
    assert_eq!(
        targets,
        [
            row(RelationKind::Calls, "helper", &here),
            row(RelationKind::Calls, "make", &None),
            row(RelationKind::Calls, "helper", &here),
            row(RelationKind::Calls, "state", &here),
            row(RelationKind::Calls, "imported", &None),
            row(RelationKind::Calls, "parseInt", &None),
            row(RelationKind::Calls, "helper", &None),
            row(RelationKind::Calls, "unknown", &None),
            row(RelationKind::Calls, "map", &None),
            row(RelationKind::References, "helper", &here),
        ]
    );
}

const LOCALS: &str = r#"function check() {
  const cleanup = make();
  cleanup();
}
"#;

#[test]
fn a_bare_call_to_a_local_only_targets_this_file() {
    let parsed = mct_corpus::parse_source(&JsTsParser, "src/b.ts", "TypeScript", LOCALS);
    let targets: Vec<(String, Option<String>)> = parsed
        .relations
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let path = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.path.clone());
            (r.to_name.clone(), path)
        })
        .collect();
    assert_eq!(
        targets,
        [
            ("make".to_string(), None),
            ("cleanup".to_string(), Some("src/b.ts".to_string())),
        ]
    );
}

const IMPORTS: &str = r#"import Def, { named, orig as alias } from "@scope/pkg";
import * as ns from "./util.js";
import { store } from "../core/index.ts";
"#;

#[test]
fn an_import_names_the_module_of_its_specifier() {
    let parsed = mct_corpus::parse_source(&JsTsParser, "src/c.ts", "TypeScript", IMPORTS);
    let bindings: Vec<(String, Option<String>)> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.kind == RelationKind::Imports
                && ["Def", "named", "alias", "ns", "store"].contains(&r.to_name.as_str())
        })
        .map(|(i, r)| {
            let module = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.module.clone());
            (r.to_name.clone(), module)
        })
        .collect();
    let row = |name: &str, module: &str| (name.to_string(), Some(module.to_string()));
    assert_eq!(
        bindings,
        [
            row("Def", "pkg"),
            row("named", "pkg"),
            row("alias", "pkg"),
            row("ns", "util"),
            row("store", "core"),
        ]
    );
}
