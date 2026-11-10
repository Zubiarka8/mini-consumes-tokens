//! What one module binds, so a bare name's call site can be proven to reach
//! `builtins` (nothing in the file binds it) or this same file (only a
//! `def`/`class` here binds it).
//!
//! Every binding counts file-wide, whatever its real scope: a parameter
//! `len` in one function makes `len` unprovable everywhere in the file. That
//! loses some external answers but never claims a wrong one.

use std::collections::HashSet;

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

use crate::text;

/// Names of `builtins` (CPython 3.14, `python -I -S -c "import builtins;
/// print(dir(builtins))"`), without the `_`-prefixed names, the keyword
/// constants and the names `site` adds (`help`, `exit`, `quit`…).
const BUILTINS: &[&str] = &[
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BaseException",
    "BaseExceptionGroup",
    "BlockingIOError",
    "BrokenPipeError",
    "BufferError",
    "BytesWarning",
    "ChildProcessError",
    "ConnectionAbortedError",
    "ConnectionError",
    "ConnectionRefusedError",
    "ConnectionResetError",
    "DeprecationWarning",
    "EOFError",
    "EncodingWarning",
    "EnvironmentError",
    "Exception",
    "ExceptionGroup",
    "FileExistsError",
    "FileNotFoundError",
    "FloatingPointError",
    "FutureWarning",
    "GeneratorExit",
    "IOError",
    "ImportError",
    "ImportWarning",
    "IndentationError",
    "IndexError",
    "InterruptedError",
    "IsADirectoryError",
    "KeyError",
    "KeyboardInterrupt",
    "LookupError",
    "MemoryError",
    "ModuleNotFoundError",
    "NameError",
    "NotADirectoryError",
    "NotImplementedError",
    "OSError",
    "OverflowError",
    "PendingDeprecationWarning",
    "PermissionError",
    "ProcessLookupError",
    "PythonFinalizationError",
    "RecursionError",
    "ReferenceError",
    "ResourceWarning",
    "RuntimeError",
    "RuntimeWarning",
    "StopAsyncIteration",
    "StopIteration",
    "SyntaxError",
    "SyntaxWarning",
    "SystemError",
    "SystemExit",
    "TabError",
    "TimeoutError",
    "TypeError",
    "UnboundLocalError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeError",
    "UnicodeTranslateError",
    "UnicodeWarning",
    "UserWarning",
    "ValueError",
    "Warning",
    "ZeroDivisionError",
    "abs",
    "aiter",
    "all",
    "anext",
    "any",
    "ascii",
    "bin",
    "bool",
    "breakpoint",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "compile",
    "complex",
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
    "filter",
    "float",
    "format",
    "frozenset",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "hex",
    "id",
    "input",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "list",
    "locals",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "vars",
    "zip",
];

/// Calls that can bind a module name at run time (`globals()["f"] = …`,
/// `exec("f = …")`, `vars()[…]`).
const DYNAMIC_BINDERS: &[&str] = &["globals", "exec", "vars"];

#[derive(Debug, Default)]
pub(crate) struct ModuleScope {
    /// `def`/`class` names outside a class body (module level or nested in
    /// a function): a bare call can only reach these in this file.
    functions: HashSet<String>,
    /// Every other binding: methods, parameters, assignment, loop, `with`,
    /// `except`, walrus and `match` targets, `global`/`nonlocal` names,
    /// `type` aliases and the local names imports bind.
    other: HashSet<String>,
    /// A `from m import *`, `globals()`, `exec` or `vars()`, or a write
    /// through `sys.modules`: any name may be bound by something unseen.
    opaque: bool,
}

impl ModuleScope {
    pub(crate) fn collect(root: Node, source: &str) -> Self {
        let mut scope = Self::default();
        scope.visit(root, source, false, 0);
        scope
    }

    /// Whether a bare `name` can only be the `builtins` one.
    pub(crate) fn is_builtin(&self, name: &str) -> bool {
        BUILTINS.binary_search(&name).is_ok()
            && !self.opaque
            && !self.functions.contains(name)
            && !self.other.contains(name)
    }

    /// Whether a bare `name` can only be a `def`/`class` of this file.
    pub(crate) fn is_local_function(&self, name: &str) -> bool {
        !self.opaque && self.functions.contains(name) && !self.other.contains(name)
    }

    fn bind(&mut self, name: &str) {
        if !name.is_empty() {
            self.other.insert(name.to_string());
        }
    }

    fn visit(&mut self, node: Node, source: &str, in_class: bool, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            // Too deep to see every binding: prove nothing.
            self.opaque = true;
            return;
        }
        let mut child_in_class = false;
        match node.kind() {
            "function_definition" | "class_definition" => {
                if let Some(name) = node.child_by_field_name("name") {
                    let name = text(name, source).to_string();
                    if in_class {
                        self.other.insert(name);
                    } else {
                        self.functions.insert(name);
                    }
                }
                child_in_class = node.kind() == "class_definition";
            }
            "parameters" | "lambda_parameters" => self.parameters(node, source),
            "assignment" | "augmented_assignment" | "for_statement" | "for_in_clause" => {
                if let Some(left) = node.child_by_field_name("left") {
                    self.targets(left, source, depth + 1);
                }
            }
            "named_expression" => {
                if let Some(name) = node.child_by_field_name("name") {
                    self.bind(text(name, source));
                }
            }
            "as_pattern" | "except_clause" => {
                if let Some(alias) = node.child_by_field_name("alias") {
                    self.targets(alias, source, depth + 1);
                }
            }
            "case_pattern" | "global_statement" | "nonlocal_statement" => {
                self.identifiers(node, source, depth + 1);
            }
            "type_alias_statement" => {
                if let Some(left) = node.child_by_field_name("left") {
                    self.identifiers(left, source, depth + 1);
                }
            }
            "import_statement" | "import_from_statement" => self.import(node, source),
            "call" => {
                let callee = node.child_by_field_name("function");
                if callee.is_some_and(|f| {
                    f.kind() == "identifier" && DYNAMIC_BINDERS.contains(&text(f, source))
                }) {
                    self.opaque = true;
                }
            }
            "attribute" if text(node, source) == "sys.modules" => self.opaque = true,
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            // Only a class's own body is a class body; a `def` inside a
            // method is a function again.
            let in_body = child_in_class && child.kind() == "block";
            self.visit(child, source, in_body, depth + 1);
        }
    }

    fn parameters(&mut self, node: Node, source: &str) {
        let mut cursor = node.walk();
        for param in node.named_children(&mut cursor) {
            match param.kind() {
                "identifier" => self.bind(text(param, source)),
                "default_parameter" | "typed_default_parameter" => {
                    if let Some(name) = param.child_by_field_name("name") {
                        self.targets(name, source, 0);
                    }
                }
                "typed_parameter" | "list_splat_pattern" | "dictionary_splat_pattern" => {
                    let mut inner = param.walk();
                    for part in param.named_children(&mut inner) {
                        if part.kind() == "identifier" {
                            self.bind(text(part, source));
                        } else if part.kind() != "type" {
                            self.targets(part, source, 0);
                        }
                    }
                }
                "tuple_pattern" => self.targets(param, source, 0),
                _ => {}
            }
        }
    }

    /// The names an assignment-like target binds: identifiers, through
    /// tuples, lists and splats, but not `a.b`/`a[i]`, which bind nothing.
    fn targets(&mut self, node: Node, source: &str, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            self.opaque = true;
            return;
        }
        match node.kind() {
            "identifier" => self.bind(text(node, source)),
            "attribute" | "subscript" => {}
            _ => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    self.targets(child, source, depth + 1);
                }
            }
        }
    }

    /// Every identifier under `node` (a `match` pattern can capture any of
    /// its names; over-collecting only proves less).
    fn identifiers(&mut self, node: Node, source: &str, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            self.opaque = true;
            return;
        }
        if node.kind() == "identifier" {
            self.bind(text(node, source));
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.identifiers(child, source, depth + 1);
        }
    }

    /// `import a.b` binds `a`, `import a.b as x` binds `x`, `from m import
    /// f [as g]` binds `f` (`g`), and `from m import *` binds anything.
    fn import(&mut self, node: Node, source: &str) {
        let module = node.child_by_field_name("module_name").map(|n| n.id());
        let from = node.kind() == "import_from_statement";
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if Some(child.id()) == module {
                continue;
            }
            match child.kind() {
                "wildcard_import" => self.opaque = true,
                "aliased_import" => {
                    if let Some(alias) = child.child_by_field_name("alias") {
                        self.bind(text(alias, source));
                    }
                }
                "dotted_name" => {
                    let segment = if from {
                        child.named_child(child.named_child_count().saturating_sub(1))
                    } else {
                        child.named_child(0)
                    };
                    if let Some(segment) = segment {
                        self.bind(text(segment, source));
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BUILTINS;

    #[test]
    fn the_builtins_list_is_sorted_for_binary_search() {
        assert!(BUILTINS.windows(2).all(|w| w[0] < w[1]));
    }
}
