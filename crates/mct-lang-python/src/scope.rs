//! What one module binds, so a bare name's call site can be proven to reach
//! `builtins` (nothing in the file binds it) or this same file (only a
//! `def`/`class` here binds it).
//!
//! Every binding counts file-wide, whatever its real scope: a parameter
//! `len` in one function makes `len` unprovable everywhere in the file. That
//! loses some external answers but never claims a wrong one.

use std::collections::{HashMap, HashSet};

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

/// The modules that export the PEP 698 `override` decorator.
pub(crate) const TYPING_MODULES: &[&str] = &["typing", "typing_extensions"];

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
    /// What each name an import binds stands for; `None` when imports bind
    /// it to different things or the module is not named (`from . import`).
    imports: HashMap<String, Option<Import>>,
    /// Names bound by anything other than an import.
    rebound: HashSet<String>,
    /// Names an import binds to `override` of `typing` or
    /// `typing_extensions` (`from typing import override [as o]`).
    typing_override: HashSet<String>,
    /// Names some other import binds.
    other_imports: HashSet<String>,
}

/// What an import binds a local name to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Import {
    /// `from a.b import f [as g]`: a name defined in module `b`.
    From(String),
    /// `import a.b as m` (`a.b`) or `import a.b` (`a`): a module, whose
    /// dotted path the name stands for.
    Module(String),
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

    /// Whether a bare `name` can only be `typing.override`: every binding of
    /// it is an import of `override` from `typing` or `typing_extensions`
    /// (`try: from typing import … except ImportError: from
    /// typing_extensions import …` binds it twice, to the same decorator).
    pub(crate) fn is_typing_override(&self, name: &str) -> bool {
        !self.opaque
            && self.typing_override.contains(name)
            && !self.other_imports.contains(name)
            && !self.functions.contains(name)
            && !self.rebound.contains(name)
    }

    /// Whether a bare `name` can only be a `def`/`class` of this file.
    pub(crate) fn is_local_function(&self, name: &str) -> bool {
        !self.opaque && self.functions.contains(name) && !self.other.contains(name)
    }

    /// The module a bare `name` was imported from (`util` for `from
    /// app.util import parse`), when an import is its only binding.
    pub(crate) fn imported_from(&self, name: &str) -> Option<&str> {
        match self.import_of(name)? {
            Import::From(module) => Some(module),
            Import::Module(_) => None,
        }
    }

    /// The module path a name stands for (`app.legacy` for `import
    /// app.legacy as lg`, `app` for `import app.util`), when an import is
    /// its only binding.
    pub(crate) fn module_alias(&self, name: &str) -> Option<&str> {
        match self.import_of(name)? {
            Import::Module(path) => Some(path),
            Import::From(_) => None,
        }
    }

    fn import_of(&self, name: &str) -> Option<&Import> {
        if self.opaque || self.functions.contains(name) || self.rebound.contains(name) {
            return None;
        }
        self.imports.get(name)?.as_ref()
    }

    fn bind(&mut self, name: &str) {
        if !name.is_empty() {
            self.other.insert(name.to_string());
            self.rebound.insert(name.to_string());
        }
    }

    fn mark_override(&mut self, name: &str, typing_override: bool) {
        if typing_override {
            self.typing_override.insert(name.to_string());
        } else {
            self.other_imports.insert(name.to_string());
        }
    }

    fn bind_import(&mut self, name: &str, import: Option<Import>) {
        if name.is_empty() {
            return;
        }
        self.other.insert(name.to_string());
        match self.imports.get_mut(name) {
            Some(existing) if *existing != import => *existing = None,
            Some(_) => {}
            None => {
                self.imports.insert(name.to_string(), import);
            }
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
                        self.rebound.insert(name.clone());
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
        let module_node = node.child_by_field_name("module_name");
        let module = module_node.map(|n| n.id());
        let from = node.kind() == "import_from_statement";
        // `from a.b import f` and `from .b import f`: module `b`. A bare
        // `from . import f` names no module, so it proves nothing.
        let from_module = module_node.and_then(|m| last_module_segment(m, source));
        let from_typing =
            from && module_node.is_some_and(|m| TYPING_MODULES.contains(&text(m, source)));
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if Some(child.id()) == module {
                continue;
            }
            match child.kind() {
                "wildcard_import" => self.opaque = true,
                "aliased_import" => {
                    if let Some(alias) = child.child_by_field_name("alias") {
                        let import = if from {
                            from_module.clone().map(Import::From)
                        } else {
                            child
                                .child_by_field_name("name")
                                .map(|n| Import::Module(text(n, source).to_string()))
                        };
                        let original = child.child_by_field_name("name").map(|n| text(n, source));
                        self.mark_override(
                            text(alias, source),
                            from_typing && original == Some("override"),
                        );
                        self.bind_import(text(alias, source), import);
                    }
                }
                "dotted_name" => {
                    let segment = if from {
                        child.named_child(child.named_child_count().saturating_sub(1))
                    } else {
                        child.named_child(0)
                    };
                    if let Some(segment) = segment {
                        let name = text(segment, source);
                        let import = if from {
                            // `from m import a.b` is not valid Python; only
                            // a single name is a definition of `m`.
                            (child.named_child_count() == 1)
                                .then(|| from_module.clone().map(Import::From))
                                .flatten()
                        } else {
                            Some(Import::Module(name.to_string()))
                        };
                        self.mark_override(
                            name,
                            from_typing && child.named_child_count() == 1 && name == "override",
                        );
                        self.bind_import(name, import);
                    }
                }
                _ => {}
            }
        }
    }
}

/// The last segment of an import's module (`b` in `a.b`, `.b` or `..a.b`),
/// `None` for a bare relative prefix (`.`, `..`).
fn last_module_segment(module: Node, source: &str) -> Option<String> {
    let dotted = if module.kind() == "dotted_name" {
        Some(module)
    } else {
        let mut cursor = module.walk();
        let found = module
            .named_children(&mut cursor)
            .find(|c| c.kind() == "dotted_name");
        found
    }?;
    let last = dotted.named_child(dotted.named_child_count().checked_sub(1)?)?;
    Some(text(last, source).to_string())
}

#[cfg(test)]
mod tests {
    use super::BUILTINS;

    #[test]
    fn the_builtins_list_is_sorted_for_binary_search() {
        assert!(BUILTINS.windows(2).all(|w| w[0] < w[1]));
    }
}
