//! Literal Laravel keys, scoped to their application so monorepo apps do not
//! resolve one another's route names, middleware aliases or config keys.

use mct_core::{Location, RelationKind, RelationTarget, SymbolId, SymbolKind, MAX_TRAVERSAL_DEPTH};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::routes::argument_values;
use super::views::application_root;
use crate::{literal_string_text, named_children, text, Walker};

#[derive(Clone, Copy)]
pub(super) enum KeyKind {
    Route,
    Middleware,
    Config,
    Translation,
    Table,
    Command,
    Binding,
    Policy,
    Morph,
}

impl KeyKind {
    fn parent(self, root: &str) -> String {
        let kind = match self {
            Self::Route => "route",
            Self::Middleware => "middleware",
            Self::Config => "config",
            Self::Translation => "translation",
            Self::Table => "table",
            Self::Command => "command",
            Self::Binding => "binding",
            Self::Policy => "policy",
            Self::Morph => "morph",
        };
        format!("laravel:{kind}:{root}")
    }
}

impl Walker<'_> {
    pub(super) fn laravel_key_declaration(
        &mut self,
        kind: KeyKind,
        name: String,
        loc: Location,
    ) -> Option<SymbolId> {
        let root = key_root(self.path)?;
        if name.is_empty() {
            return None;
        }
        Some(self.push_symbol(name, SymbolKind::Constant, loc, Some(kind.parent(&root))))
    }

    pub(super) fn laravel_key_reference(
        &mut self,
        owner: SymbolId,
        kind: KeyKind,
        name: &str,
        loc: Location,
    ) {
        if let Some(root) = key_root(self.path) {
            if !name.is_empty() {
                self.push_targeted(
                    owner,
                    RelationKind::References,
                    name.to_string(),
                    loc,
                    Some(RelationTarget {
                        qualifier: Some(kind.parent(&root)),
                        kind: Some(SymbolKind::Constant),
                        ..Default::default()
                    }),
                );
            }
        }
    }

    pub(crate) fn laravel_function_key(&mut self, call: Node, function: Node, owner: SymbolId) {
        let written = text(function, self.source);
        let name = written.strip_prefix('\\').unwrap_or(written);
        if self.declared_functions.contains(name)
            || (written == name && self.scope.resolve_function(name).is_some())
        {
            return;
        }
        let kind = match name {
            "route" => KeyKind::Route,
            "config" => KeyKind::Config,
            "app" | "resolve" => KeyKind::Binding,
            "__" | "trans" | "trans_choice" => KeyKind::Translation,
            _ => return,
        };
        if let Some(argument) = argument_values(call).first().copied() {
            self.laravel_literal_key(owner, kind, argument);
        }
    }

    pub(super) fn laravel_literal_key(&mut self, owner: SymbolId, kind: KeyKind, argument: Node) {
        let key = match kind {
            KeyKind::Binding | KeyKind::Policy => self.binding_name(argument),
            _ => literal_string_text(argument, self.source),
        };
        if let Some(key) = key {
            self.laravel_key_reference(owner, kind, &key, location(argument));
        }
    }

    pub(crate) fn laravel_file_keys(&mut self, root: Node) {
        let Some((kind, group)) = file_key_group(self.path) else {
            return;
        };
        for statement in named_children(root) {
            if statement.kind() != "return_statement" {
                continue;
            }
            let Some(array) = statement
                .named_child(0)
                .filter(|value| value.kind() == "array_creation_expression")
            else {
                continue;
            };
            if matches!(kind, KeyKind::Config) {
                self.laravel_key_declaration(kind, group.clone(), location(array));
            }
            self.laravel_array_keys(array, kind, &group, 0);
        }
    }

    fn laravel_array_keys(&mut self, array: Node, kind: KeyKind, prefix: &str, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        for element in named_children(array) {
            if element.kind() != "array_element_initializer" || element.named_child_count() != 2 {
                continue;
            }
            let (Some(key), Some(value)) = (element.named_child(0), element.named_child(1)) else {
                continue;
            };
            let Some(segment) = literal_string_text(key, self.source).filter(|key| !key.is_empty())
            else {
                continue;
            };
            let name = format!("{prefix}.{segment}");
            if value.kind() == "array_creation_expression" {
                self.laravel_array_keys(value, kind, &name, depth + 1);
            }
            self.laravel_key_declaration(kind, name, location(key));
        }
    }

    /// A typed Middleware configuration callback in bootstrap/app.php.
    pub(super) fn laravel_middleware_aliases(&mut self, call: Node) {
        if !self.path.ends_with("/bootstrap/app.php") && self.path != "bootstrap/app.php" {
            return;
        }
        let Some(receiver) = call.child_by_field_name("object") else {
            return;
        };
        let mut ancestors = call.parent();
        let mut callback = None;
        while let Some(node) = ancestors {
            if matches!(node.kind(), "anonymous_function" | "arrow_function") {
                callback = Some(node);
                break;
            }
            ancestors = node.parent();
        }
        let Some(callback) = callback else { return };
        let Some(registration) = callback
            .parent()
            .and_then(|arg| arg.parent())
            .and_then(|args| args.parent())
        else {
            return;
        };
        if registration
            .child_by_field_name("name")
            .map(|name| text(name, self.source))
            != Some("withMiddleware")
        {
            return;
        }
        let Some(parameter) = callback
            .child_by_field_name("parameters")
            .and_then(|p| p.named_child(0))
        else {
            return;
        };
        if parameter
            .child_by_field_name("name")
            .map(|name| text(name, self.source))
            != Some(text(receiver, self.source))
        {
            return;
        }
        let Some(ty) = parameter
            .child_by_field_name("type")
            .and_then(|ty| self.scope.resolve_class(text(ty, self.source)))
        else {
            return;
        };
        if ty.namespace != r"Illuminate\Foundation\Configuration" || ty.name != "Middleware" {
            return;
        }
        let Some(aliases) = argument_values(call)
            .first()
            .copied()
            .filter(|arg| arg.kind() == "array_creation_expression")
        else {
            return;
        };
        for element in named_children(aliases) {
            if element.kind() != "array_element_initializer" || element.named_child_count() != 2 {
                continue;
            }
            if let Some(key) = element.named_child(0) {
                if let Some(alias) = literal_string_text(key, self.source) {
                    self.laravel_key_declaration(KeyKind::Middleware, alias, location(key));
                }
            }
        }
    }
}

fn key_root(path: &str) -> Option<String> {
    let (directory, _) = path.rsplit_once('/')?;
    if let Some((before_locale, _)) = directory.rsplit_once('/') {
        for lang in ["resources/lang", "lang"] {
            if before_locale == lang {
                return Some(String::new());
            }
            if let Some(root) = before_locale.strip_suffix(&format!("/{lang}")) {
                return Some(root.to_string());
            }
        }
    }
    application_root(path)
}

fn file_key_group(path: &str) -> Option<(KeyKind, String)> {
    let root = key_root(path)?;
    let local = if root.is_empty() {
        path
    } else {
        path.strip_prefix(&root)?.strip_prefix('/')?
    };
    if let Some(file) = local
        .strip_prefix("config/")
        .and_then(|p| p.strip_suffix(".php"))
    {
        return (!file.contains('/')).then(|| (KeyKind::Config, file.to_string()));
    }
    let (_, file) = local
        .strip_prefix("lang/")
        .or_else(|| local.strip_prefix("resources/lang/"))?
        .rsplit_once('/')?;
    Some((KeyKind::Translation, file.strip_suffix(".php")?.to_string()))
}
