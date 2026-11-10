//! Which ECMAScript globals a file can only mean as the platform's: names
//! nothing in the file binds, so `Math.max(…)` or `parseInt(…)` there can
//! never reach a same-named definition elsewhere in the repository.
//!
//! Every binding counts file-wide, whatever its real scope: a parameter
//! `JSON` in one function makes `JSON` unprovable everywhere in the file.
//! That loses some external answers but never claims a wrong one.

use std::collections::HashSet;

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

    fn visit(&mut self, node: Node, source: &str, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            // Too deep to see every binding: prove nothing.
            self.opaque = true;
            return;
        }
        match node.kind() {
            "identifier" | "shorthand_property_identifier_pattern" if is_binding(node) => {
                self.bound.insert(text(node, source).to_string());
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
                    self.bound.insert(text(left, source).to_string());
                }
            }
            "import_statement" | "import_alias" => {
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
            "identifier" | "type_identifier" => {
                self.bound.insert(text(node, source).to_string());
            }
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

#[cfg(test)]
mod tests {
    use super::GLOBALS;

    #[test]
    fn the_globals_list_is_sorted_for_binary_search() {
        assert!(GLOBALS.windows(2).all(|w| w[0] < w[1]));
    }
}
