//! PHP name resolution inside one file: the current `namespace` and the
//! `use` imports in effect, applied the way PHP applies them (a fully
//! qualified `\A\B`, a `namespace\C` relative name, a first segment that is an
//! import alias, otherwise the current namespace).
//!
//! The result is the namespace a class lives in plus its short name. The
//! parser publishes a file's own namespaces as aliases of its `Module` symbol
//! (see [`namespace_alias`]) and a reference carries the target namespace as
//! `RelationTarget::target_module`, so the index resolves a reference to the
//! class that declares exactly that namespace, wherever Composer's PSR-4 map
//! happens to put the file. Nothing here reads `composer.json` or the file
//! system.

use std::collections::HashMap;

/// The alias a file's `Module` symbol carries for namespace `namespace`.
///
/// The leading backslash keeps namespaces apart from the class short names
/// the same symbol also carries as aliases; the global namespace is `\`.
pub(crate) fn namespace_alias(namespace: &str) -> String {
    format!("\\{namespace}")
}

/// A class-like name resolved against the namespace and imports in effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedName {
    /// The namespace without a leading backslash; empty for the global one.
    pub namespace: String,
    /// The unqualified name.
    pub name: String,
}

/// What a `use` clause imports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportKind {
    Class,
    Function,
    Constant,
}

#[derive(Debug, Default)]
pub(crate) struct Scope {
    namespace: String,
    classes: HashMap<String, String>,
    functions: HashMap<String, String>,
    constants: HashMap<String, String>,
}

impl Scope {
    pub(crate) fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Enters `namespace ...;` / `namespace ... { }`: the imports of the
    /// previous namespace block do not carry over.
    pub(crate) fn enter_namespace(&mut self, namespace: &str) {
        self.namespace = namespace.trim_matches('\\').to_string();
        self.classes.clear();
        self.functions.clear();
        self.constants.clear();
    }

    /// Registers `use full\Name [as alias]`. Class and namespace aliases
    /// compare case-insensitively in PHP; functions too, constants do not.
    pub(crate) fn import(&mut self, kind: ImportKind, full_name: &str, alias: Option<&str>) {
        let full = full_name.trim_matches('\\').to_string();
        let Some(short) = alias.or_else(|| full.rsplit('\\').next()) else {
            return;
        };
        if short.is_empty() || full.is_empty() {
            return;
        }
        match kind {
            ImportKind::Class => {
                self.classes.insert(short.to_lowercase(), full);
            }
            ImportKind::Function => {
                self.functions.insert(short.to_lowercase(), full);
            }
            ImportKind::Constant => {
                self.constants.insert(short.to_string(), full);
            }
        }
    }

    /// Resolves a class-like name as written (`User`, `Models\User`,
    /// `\App\User`, `namespace\User`).
    pub(crate) fn resolve_class(&self, written: &str) -> Option<ResolvedName> {
        let full = self.qualify(written, &self.classes, true)?;
        Some(split_full_name(&full))
    }

    /// Resolves a function name. An unqualified name that is not imported
    /// falls back to the global function at run time when the namespaced one
    /// does not exist, which a static reading cannot decide: `None`.
    pub(crate) fn resolve_function(&self, written: &str) -> Option<ResolvedName> {
        let written = written.trim();
        if !written.contains('\\') {
            let full = self.functions.get(&written.to_lowercase())?;
            return Some(split_full_name(full));
        }
        let full = self.qualify(written, &self.classes, false)?;
        Some(split_full_name(&full))
    }

    /// `base` supplies the namespace aliases a qualified name starts with;
    /// `unqualified_is_local` says whether an unimported single segment
    /// belongs to the current namespace (true for classes).
    fn qualify(
        &self,
        written: &str,
        base: &HashMap<String, String>,
        unqualified_is_local: bool,
    ) -> Option<String> {
        let written = written.trim();
        if written.is_empty() {
            return None;
        }
        if let Some(absolute) = written.strip_prefix('\\') {
            return non_empty(absolute);
        }
        if let Some(relative) = strip_namespace_keyword(written) {
            return non_empty(&join(&self.namespace, relative));
        }
        let (first, rest) = match written.split_once('\\') {
            Some((first, rest)) => (first, Some(rest)),
            None => (written, None),
        };
        if let Some(target) = base.get(&first.to_lowercase()) {
            return non_empty(&match rest {
                Some(rest) => format!("{target}\\{rest}"),
                None => target.clone(),
            });
        }
        if rest.is_none() && !unqualified_is_local {
            return None;
        }
        non_empty(&join(&self.namespace, written))
    }
}

fn strip_namespace_keyword(written: &str) -> Option<&str> {
    let head = written.get(..10)?;
    if head.eq_ignore_ascii_case("namespace\\") {
        written.get(10..)
    } else {
        None
    }
}

fn join(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}\\{name}")
    }
}

fn non_empty(full: &str) -> Option<String> {
    let full = full.trim_matches('\\');
    if full.is_empty() {
        None
    } else {
        Some(full.to_string())
    }
}

fn split_full_name(full: &str) -> ResolvedName {
    match full.rsplit_once('\\') {
        Some((namespace, name)) => ResolvedName {
            namespace: namespace.to_string(),
            name: name.to_string(),
        },
        None => ResolvedName {
            namespace: String::new(),
            name: full.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> Scope {
        let mut scope = Scope::default();
        scope.enter_namespace("App\\Http");
        scope.import(ImportKind::Class, "App\\Models\\User", None);
        scope.import(ImportKind::Class, "Acme\\Billing\\Invoice", Some("Bill"));
        scope.import(ImportKind::Class, "Illuminate\\Support\\Facades", None);
        scope.import(ImportKind::Function, "App\\helper", None);
        scope
    }

    fn resolved(namespace: &str, name: &str) -> Option<ResolvedName> {
        Some(ResolvedName {
            namespace: namespace.to_string(),
            name: name.to_string(),
        })
    }

    fn assert_resolutions(cases: &[(&str, &str, &str)]) {
        let scope = scope();
        for (input, namespace, name) in cases {
            assert_eq!(
                scope.resolve_class(input),
                resolved(namespace, name),
                "{input}"
            );
        }
    }

    #[test]
    fn an_unqualified_name_belongs_to_the_current_namespace_unless_imported() {
        assert_resolutions(&[
            ("Request", r"App\Http", "Request"),
            ("User", r"App\Models", "User"),
            ("user", r"App\Models", "User"),
        ]);
    }

    #[test]
    fn an_alias_resolves_to_the_imported_class_not_to_its_spelling() {
        assert_eq!(
            scope().resolve_class("Bill"),
            resolved("Acme\\Billing", "Invoice")
        );
    }

    #[test]
    fn a_qualified_name_resolves_its_first_segment_through_the_imports() {
        let scope = scope();
        assert_eq!(
            scope.resolve_class("Facades\\Cache"),
            resolved("Illuminate\\Support\\Facades", "Cache")
        );
        assert_eq!(
            scope.resolve_class("Sub\\Thing"),
            resolved("App\\Http\\Sub", "Thing")
        );
    }

    #[test]
    fn fully_qualified_and_namespace_relative_names_ignore_the_imports() {
        assert_resolutions(&[
            (r"\User", "", "User"),
            (r"\App\User", "App", "User"),
            (r"namespace\Thing", r"App\Http", "Thing"),
        ]);
    }

    #[test]
    fn a_new_namespace_block_drops_the_previous_imports() {
        let mut scope = scope();
        scope.enter_namespace("Other");
        assert_eq!(scope.resolve_class("User"), resolved("Other", "User"));
    }

    #[test]
    fn a_function_is_resolved_only_when_imported_or_qualified() {
        let scope = scope();
        assert_eq!(scope.resolve_function("helper"), resolved("App", "helper"));
        assert_eq!(scope.resolve_function("strlen"), None);
        assert_eq!(scope.resolve_function("\\strlen"), resolved("", "strlen"));
    }

    #[test]
    fn namespace_aliases_cannot_be_confused_with_class_names() {
        assert_eq!(namespace_alias(""), "\\");
        assert_eq!(namespace_alias("App\\Models"), "\\App\\Models");
    }
}
