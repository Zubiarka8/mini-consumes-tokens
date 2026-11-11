//! Which Lua standard globals a file can only mean as the host's: names
//! nothing in the file binds, so `setmetatable(…)` or `math.floor(…)` there
//! never reaches a same-named definition elsewhere in the repository. A
//! `local setmetatable = setmetatable` in another file is that file's own
//! local, invisible here.
//!
//! Every binding counts file-wide, whatever its real scope: a parameter
//! `type` in one function makes `type` unprovable everywhere in the file,
//! and so does a global assignment or `function type()` anywhere in it.
//! A file that touches the environment itself (`_ENV`, `_G`, `setfenv`) is
//! opaque: nothing in it is marked. That loses some external answers but
//! never claims a wrong one.

use std::collections::HashSet;

use mct_core::MAX_TRAVERSAL_DEPTH;
use tree_sitter::Node;

/// Lua 5.4 basic functions (manual §6.1) and standard library tables
/// (§6.2–§6.10). Names only Lua 5.1 had (`unpack`, `loadstring`,
/// `module`, `setfenv`) are left out: a project often defines those itself
/// as globals (`unpack = unpack or table.unpack`) for portability.
const GLOBALS: &[&str] = &[
    "assert",
    "collectgarbage",
    "coroutine",
    "debug",
    "dofile",
    "error",
    "getmetatable",
    "io",
    "ipairs",
    "load",
    "loadfile",
    "math",
    "next",
    "os",
    "package",
    "pairs",
    "pcall",
    "print",
    "rawequal",
    "rawget",
    "rawlen",
    "rawset",
    "require",
    "select",
    "setmetatable",
    "string",
    "table",
    "tonumber",
    "tostring",
    "type",
    "utf8",
    "warn",
    "xpcall",
];

/// Names that reach or replace the global environment itself.
const ENVIRONMENT: &[&str] = &["_ENV", "_G", "setfenv"];

/// The file-wide facts [`StdGlobals::is_std`] needs, gathered before the walk.
pub(crate) struct StdGlobals {
    bound: HashSet<String>,
    opaque: bool,
}

impl StdGlobals {
    pub(crate) fn of(root: Node, source: &str) -> Self {
        let mut globals = Self {
            bound: HashSet::new(),
            opaque: false,
        };
        globals.collect(root, source, 0);
        globals
    }

    /// `name` here can only be the Lua standard global of that name.
    pub(crate) fn is_std(&self, name: &str) -> bool {
        !self.opaque && GLOBALS.contains(&name) && !self.bound.contains(name)
    }

    fn collect(&mut self, node: Node, source: &str, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            // A subtree the walk can't see could bind anything.
            self.opaque = true;
            return;
        }
        if node.kind() == "identifier" {
            let name = text(node, source);
            if ENVIRONMENT.contains(&name) {
                self.opaque = true;
            }
            if is_binding(node) {
                self.bound.insert(name.to_string());
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect(child, source, depth + 1);
        }
    }
}

/// An identifier that names what it binds or assigns: a `local` name, a
/// parameter, a `for` variable, a declared function's name, or a bare
/// assignment target.
fn is_binding(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    match parent.kind() {
        // `local a, b`, `a, b = …`, `for k, v in …`: the names sit directly
        // in the list; `t.a = …` puts `t` under the index expression.
        "variable_list" | "parameters" => true,
        "for_numeric_clause" | "function_declaration" => parent
            .child_by_field_name("name")
            .is_some_and(|n| n.id() == node.id()),
        _ => false,
    }
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}
