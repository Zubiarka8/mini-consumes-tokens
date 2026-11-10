//! Which ECMAScript globals a file can only mean as the platform's: names
//! nothing in the file binds, so `Math.max(…)` or `parseInt(…)` there can
//! never reach a same-named definition elsewhere in the repository.
//!
//! Every binding counts file-wide, whatever its real scope: a parameter
//! `JSON` in one function makes `JSON` unprovable everywhere in the file.
//! That loses some external answers but never claims a wrong one.

use std::collections::{HashMap, HashSet};

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

use super::{is_binding, text};

/// Global objects of ECMAScript 2025 (ECMA-262 §19, the global object's
/// value properties that are functions or namespaces). Host globals
/// (`window`, `document`, `process`, `fetch`…) are left out: a project may
/// define those itself, and the language does not guarantee them.
const GLOBALS: &[&str] = &[
    "AggregateError",
    "Array",
    "ArrayBuffer",
    "Atomics",
    "BigInt",
    "BigInt64Array",
    "BigUint64Array",
    "Boolean",
    "DataView",
    "Date",
    "Error",
    "EvalError",
    "FinalizationRegistry",
    "Float32Array",
    "Float64Array",
    "Function",
    "Int16Array",
    "Int32Array",
    "Int8Array",
    "Intl",
    "Iterator",
    "JSON",
    "Map",
    "Math",
    "Number",
    "Object",
    "Promise",
    "Proxy",
    "RangeError",
    "ReferenceError",
    "Reflect",
    "RegExp",
    "Set",
    "SharedArrayBuffer",
    "String",
    "Symbol",
    "SyntaxError",
    "TypeError",
    "URIError",
    "Uint16Array",
    "Uint32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "WeakMap",
    "WeakRef",
    "WeakSet",
    "decodeURI",
    "decodeURIComponent",
    "encodeURI",
    "encodeURIComponent",
    "eval",
    "isFinite",
    "isNaN",
    "parseFloat",
    "parseInt",
];

/// Declarations whose `name` field binds a name in the enclosing scope.
const DECLARATIONS: &[&str] = &[
    "function_declaration",
    "generator_function_declaration",
    "function_signature",
    "function_expression",
    "class_declaration",
    "abstract_class_declaration",
    "class",
    "enum_declaration",
    "internal_module",
    "module",
    "interface_declaration",
    "type_alias_declaration",
];

#[derive(Debug, Default)]
pub(super) struct FileGlobals {
    /// Every name the file binds anywhere.
    bound: HashSet<String>,
    /// A direct `eval(…)` or a `with` statement: any name may be bound by
    /// something the syntax doesn't show.
    opaque: bool,
    /// What each name an `import` binds stands for; `None` when imports
    /// bind it to different things or its module names no file.
    imports: HashMap<String, Option<Import>>,
    /// Names bound by anything other than an `import`.
    rebound: HashSet<String>,
}

/// What an `import` binds a local name to, with the module's name: `fmt`
/// for `"./fmt.ts"`, `legacy` for `"./legacy/index.js"`, `left-pad` for a
/// package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Import {
    /// `import { f } from "m"`, `import f from "m"`: a definition of `m`.
    Named(String),
    /// `import * as m from "m"`: the module itself.
    Namespace(String),
}

impl FileGlobals {
    pub(super) fn collect(root: Node, source: &str) -> Self {
        let mut globals = Self::default();
        globals.visit(root, source, 0);
        globals
    }

    /// Whether `name`, written bare in this file, can only be the
    /// ECMAScript global.
    pub(super) fn is_global(&self, name: &str) -> bool {
        !self.opaque && !self.bound.contains(name) && GLOBALS.binary_search(&name).is_ok()
    }

    /// The import a name is bound by, when nothing else binds it.
    pub(super) fn import_of(&self, name: &str) -> Option<&Import> {
        if self.opaque || self.rebound.contains(name) {
            return None;
        }
        self.imports.get(name)?.as_ref()
    }

    fn bind(&mut self, name: &str) {
        self.bound.insert(name.to_string());
        self.rebound.insert(name.to_string());
    }

    fn bind_import(&mut self, name: &str, import: Option<Import>) {
        self.bound.insert(name.to_string());
        match self.imports.get_mut(name) {
            Some(existing) if *existing != import => *existing = None,
            Some(_) => {}
            None => {
                self.imports.insert(name.to_string(), import);
            }
        }
    }

    /// `import a, { b as c }, * as d from "m"`: each local name and what it
    /// stands for. A TS `import x = require(…)` is a plain binding.
    fn import(&mut self, node: Node, source: &str) {
        let module = node
            .child_by_field_name("source")
            .and_then(|s| module_name(text(s, source)));
        let mut cursor = node.walk();
        let clause = node
            .named_children(&mut cursor)
            .find(|c| c.kind() == "import_clause");
        let Some(clause) = clause else {
            return;
        };
        let mut cursor = clause.walk();
        for part in clause.named_children(&mut cursor) {
            match part.kind() {
                "identifier" => {
                    self.bind_import(text(part, source), module.clone().map(Import::Named));
                }
                "namespace_import" => {
                    if let Some(name) = part.named_child(0) {
                        let import = module.clone().map(Import::Namespace);
                        self.bind_import(text(name, source), import);
                    }
                }
                "named_imports" => {
                    let mut specs = part.walk();
                    for spec in part.named_children(&mut specs) {
                        let local = spec
                            .child_by_field_name("alias")
                            .or_else(|| spec.child_by_field_name("name"));
                        if let Some(local) = local.filter(|l| l.kind() == "identifier") {
                            let import = module.clone().map(Import::Named);
                            self.bind_import(text(local, source), import);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn visit(&mut self, node: Node, source: &str, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            // Too deep to see every binding: prove nothing.
            self.opaque = true;
            return;
        }
        match node.kind() {
            "identifier" | "shorthand_property_identifier_pattern" if is_binding(node) => {
                self.bind(text(node, source));
            }
            "with_statement" => self.opaque = true,
            "call_expression"
                if node
                    .child_by_field_name("function")
                    .is_some_and(|f| f.kind() == "identifier" && text(f, source) == "eval") =>
            {
                self.opaque = true;
            }
            "assignment_expression" | "augmented_assignment_expression" => {
                // `Math = …` in sloppy code rebinds the global itself.
                if let Some(left) = node
                    .child_by_field_name("left")
                    .filter(|l| l.kind() == "identifier")
                {
                    self.bind(text(left, source));
                }
            }
            "import_statement" => {
                if node
                    .named_children(&mut node.walk())
                    .any(|c| c.kind() == "import_require_clause")
                {
                    self.names(node, source, depth + 1);
                } else {
                    self.import(node, source);
                }
                return;
            }
            "import_alias" => {
                self.names(node, source, depth + 1);
                return;
            }
            kind if DECLARATIONS.contains(&kind) => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.names(name, source, depth + 1);
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, source, depth + 1);
        }
    }

    /// Every identifier under `node`, skipping string literals (an import's
    /// module specifier).
    fn names(&mut self, node: Node, source: &str, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            self.opaque = true;
            return;
        }
        match node.kind() {
            "identifier" | "type_identifier" => self.bind(text(node, source)),
            "string" => {}
            _ => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.names(child, source, depth + 1);
                }
            }
        }
    }
}

/// The module a specifier names, as a path component a definition can be
/// declared under: its last segment without extension, or the directory
/// for an `index` file. `-` is written `_`, as the index compares a
/// candidate's path with `-` read as `_`. `None` for `.`/`..` and an empty
/// specifier.
fn module_name(specifier: &str) -> Option<String> {
    let path = specifier.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    let mut segments = path.rsplit('/').filter(|s| !s.is_empty());
    let mut last = segments.next()?;
    let stem = |s: &str| {
        const EXTENSIONS: &[&str] = &[
            ".d.ts", ".tsx", ".ts", ".mts", ".cts", ".jsx", ".js", ".mjs", ".cjs",
        ];
        EXTENSIONS
            .iter()
            .find_map(|e| s.strip_suffix(e))
            .unwrap_or(s)
            .to_string()
    };
    if stem(last) == "index" {
        last = segments.next()?;
    }
    let name = stem(last);
    (!name.is_empty() && name != "." && name != "..").then(|| name.replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::{module_name, GLOBALS};

    #[test]
    fn a_specifier_names_its_file_or_directory() {
        assert_eq!(module_name("\"./fmt.ts\"").as_deref(), Some("fmt"));
        assert_eq!(module_name("\"./fmt\"").as_deref(), Some("fmt"));
        assert_eq!(
            module_name("'./legacy/index.js'").as_deref(),
            Some("legacy")
        );
        assert_eq!(module_name("\"left-pad\"").as_deref(), Some("left_pad"));
        assert_eq!(module_name("\"@scope/pkg\"").as_deref(), Some("pkg"));
        assert_eq!(module_name("\"..\""), None);
        assert_eq!(module_name("\"./index\""), None);
    }

    #[test]
    fn the_globals_list_is_sorted_for_binary_search() {
        assert!(GLOBALS.windows(2).all(|w| w[0] < w[1]));
    }
}
