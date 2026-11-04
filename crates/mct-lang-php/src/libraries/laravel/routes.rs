//! Route definitions: which controller methods the router invokes.

use std::collections::HashSet;

use mct_core::{Location, RelationKind, RelationTarget, SymbolId};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::is_identifier;
use crate::names::{namespace_alias, ResolvedName};
use crate::{last_segment, literal_string_text, named_children, text, Walker};

const FACADE_NAMESPACE: &str = "Illuminate\\Support\\Facades";

/// `Route` methods whose action is the second argument.
const VERBS: &[&str] = &["get", "post", "put", "patch", "delete", "options", "any"];

const RESOURCE_METHODS: &[&str] = &[
    "index", "create", "store", "show", "edit", "update", "destroy",
];
const API_RESOURCE_METHODS: &[&str] = &["index", "store", "show", "update", "destroy"];
const SINGLETON_METHODS: &[&str] = &["show", "edit", "update"];
const API_SINGLETON_METHODS: &[&str] = &["show", "update"];
/// What `->creatable()` adds to a singleton resource, and to an API one.
const SINGLETON_CREATABLE: &[&str] = &["create", "store", "destroy"];
const API_SINGLETON_CREATABLE: &[&str] = &["store", "destroy"];

impl Walker<'_> {
    /// Whether `scope` names the facade `short` (`Route`, `View`): imported
    /// from `Illuminate\Support\Facades`, or the unimported global alias
    /// Laravel registers for it. A namespaced `Route` that is not imported
    /// is some other class.
    pub(crate) fn is_facade(&self, scope: Node, short: &str) -> bool {
        if last_segment(scope).is_none() {
            return false;
        }
        matches!(
            self.scope.resolve_class(text(scope, self.source)),
            Some(ResolvedName { namespace, name })
                if name == short && (namespace.is_empty() || namespace == FACADE_NAMESPACE)
        )
    }

    /// Hook for every `Name::method(...)` call.
    pub(crate) fn laravel_scoped_call(&mut self, node: Node, owner: SymbolId) {
        let (Some(scope), Some(name)) = (
            node.child_by_field_name("scope"),
            node.child_by_field_name("name"),
        ) else {
            return;
        };
        let method = text(name, self.source);
        let args = argument_values(node);
        if self.is_facade(scope, "View") && method == "make" {
            if let Some(view) = args.first() {
                self.push_view_reference(owner, *view, location(node));
            }
            return;
        }
        if !self.is_facade(scope, "Route") {
            return;
        }
        match method {
            "view" => {
                if let Some(view) = args.get(1) {
                    self.push_view_reference(owner, *view, location(node));
                }
            }
            "match" => self.route_action(owner, args.get(2).copied()),
            "fallback" => self.route_action(owner, args.first().copied()),
            "resource" => self.resource_routes(node, owner, &args, RESOURCE_METHODS, &[]),
            "apiResource" => self.resource_routes(node, owner, &args, API_RESOURCE_METHODS, &[]),
            "singleton" => {
                self.resource_routes(node, owner, &args, SINGLETON_METHODS, SINGLETON_CREATABLE)
            }
            "apiSingleton" => self.resource_routes(
                node,
                owner,
                &args,
                API_SINGLETON_METHODS,
                API_SINGLETON_CREATABLE,
            ),
            "resources" | "softDeletableResources" => {
                self.resource_map(owner, &args, RESOURCE_METHODS, node)
            }
            "apiResources" => self.resource_map(owner, &args, API_RESOURCE_METHODS, node),
            m if VERBS.contains(&m) => self.route_action(owner, args.get(1).copied()),
            _ => {}
        }
    }

    /// Hook for every `$x->method(...)` call: the `withRouting(web: ...)`
    /// of `bootstrap/app.php` includes the route files it names.
    pub(crate) fn laravel_member_call(&mut self, node: Node, owner: SymbolId) {
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        if text(name, self.source) != "withRouting" {
            return;
        }
        let Some(arguments) = node.child_by_field_name("arguments") else {
            return;
        };
        for argument in named_children(arguments) {
            let Some(label) = argument.child_by_field_name("name") else {
                continue;
            };
            if !matches!(
                text(label, self.source),
                "web" | "api" | "commands" | "channels"
            ) {
                continue;
            }
            let Some(value) = argument_value(argument) else {
                continue;
            };
            if value.kind() == "array_creation_expression" {
                for element in named_children(value) {
                    if let Some(path) = element.named_child(0) {
                        self.push_include_target(owner, path, location(path));
                    }
                }
            } else {
                self.push_include_target(owner, value, location(value));
            }
        }
    }

    /// The controller a `Route::controller(X::class)...->group(...)` chain
    /// binds, for the string actions inside its closure.
    pub(crate) fn controller_group_context(&self, group: Node) -> Option<ResolvedName> {
        if text(group.child_by_field_name("name")?, self.source) != "group" {
            return None;
        }
        let mut controller = None;
        let mut current = group.child_by_field_name("object");
        while let Some(call) = current {
            let name = call
                .child_by_field_name("name")
                .map(|n| text(n, self.source));
            if name == Some("controller") && controller.is_none() {
                controller = argument_values(call)
                    .first()
                    .and_then(|arg| self.class_constant(*arg));
            }
            match call.kind() {
                "member_call_expression" => current = call.child_by_field_name("object"),
                "scoped_call_expression" => {
                    let scope = call.child_by_field_name("scope")?;
                    return if self.is_facade(scope, "Route") {
                        controller
                    } else {
                        None
                    };
                }
                _ => return None,
            }
        }
        None
    }

    /// `Name::class` -> the class it names.
    fn class_constant(&self, node: Node) -> Option<ResolvedName> {
        if node.kind() != "class_constant_access_expression" {
            return None;
        }
        let class = node.named_child(0)?;
        if text(node.named_child(1)?, self.source) != "class" || last_segment(class).is_none() {
            return None;
        }
        self.scope.resolve_class(text(class, self.source))
    }

    fn call_into(&mut self, owner: SymbolId, class: &ResolvedName, method: &str, loc: Location) {
        self.push_targeted(
            owner,
            RelationKind::Calls,
            method.to_string(),
            loc,
            Some(RelationTarget {
                qualifier: Some(class.name.clone()),
                target_module: Some(namespace_alias(&class.namespace)),
                ..Default::default()
            }),
        );
    }

    /// A route's action: `[Controller::class, 'method']`, an invokable
    /// `Controller::class`, or a bare method name inside a
    /// `Route::controller(...)->group(...)`. A closure's body is walked like
    /// any other code; `'Controller@method'` strings need the controller
    /// namespace configured at run time, so they stay unrecorded.
    fn route_action(&mut self, owner: SymbolId, action: Option<Node>) {
        let Some(action) = action else { return };
        match action.kind() {
            "array_creation_expression" => {
                let items = named_children(action);
                let [class, method] = items.as_slice() else {
                    return;
                };
                if class.named_child_count() != 1 || method.named_child_count() != 1 {
                    return;
                }
                let (Some(class), Some(method)) = (class.named_child(0), method.named_child(0))
                else {
                    return;
                };
                let Some(name) = literal_string_text(method, self.source) else {
                    return;
                };
                if let Some(class) = self.class_constant(class).filter(|_| is_identifier(&name)) {
                    self.call_into(owner, &class, &name, location(method));
                }
            }
            "class_constant_access_expression" => {
                if let Some(class) = self.class_constant(action) {
                    self.call_into(owner, &class, "__invoke", location(action));
                }
            }
            "string" => {
                let Some(controller) = self.route_controllers.last().cloned() else {
                    return;
                };
                if let Some(name) =
                    literal_string_text(action, self.source).filter(|name| is_identifier(name))
                {
                    self.call_into(owner, &controller, &name, location(action));
                }
            }
            _ => {}
        }
    }

    /// `Route::resource('photos', PhotoController::class)->only([...])`. The
    /// `only`/`except` chained after it narrow the conventional methods; a
    /// modifier that is not a literal list makes the set unknown, and then
    /// nothing is recorded.
    fn resource_routes(
        &mut self,
        node: Node,
        owner: SymbolId,
        args: &[Node],
        conventional: &[&str],
        creatable: &[&str],
    ) {
        let Some(class) = args.get(1).and_then(|a| self.class_constant(*a)) else {
            return;
        };
        let Some(methods) = self.resource_methods(node, conventional, creatable) else {
            return;
        };
        for method in methods {
            self.call_into(owner, &class, method, location(node));
        }
    }

    /// `Route::resources(['photos' => PhotoController::class, ...])`.
    fn resource_map(&mut self, owner: SymbolId, args: &[Node], conventional: &[&str], node: Node) {
        // Per-call options are a second argument; their effect is not read.
        if args.len() != 1 {
            return;
        }
        let Some(map) = args
            .first()
            .filter(|a| a.kind() == "array_creation_expression")
        else {
            return;
        };
        let Some(methods) = self.resource_methods(node, conventional, &[]) else {
            return;
        };
        for element in named_children(*map) {
            let Some(class) = element
                .named_child(1)
                .and_then(|value| self.class_constant(value))
            else {
                continue;
            };
            for method in &methods {
                self.call_into(owner, &class, method, location(element));
            }
        }
    }

    /// The conventional methods left after the `only`/`except` calls chained
    /// onto `node`, or `None` when one of them is not a literal list.
    fn resource_methods<'m>(
        &self,
        node: Node,
        conventional: &[&'m str],
        creatable: &[&'m str],
    ) -> Option<Vec<&'m str>> {
        let mut methods: Vec<&'m str> = conventional.to_vec();
        let mut call = node;
        while let Some(parent) = call.parent() {
            if parent.kind() != "member_call_expression"
                || parent.child_by_field_name("object").map(|o| o.id()) != Some(call.id())
            {
                break;
            }
            let name = parent
                .child_by_field_name("name")
                .map(|n| text(n, self.source));
            match name {
                // A singleton resource gains routes with `creatable()`, and
                // only `destroy` with `destroyable()`.
                Some("creatable") => {
                    for method in creatable {
                        if !methods.contains(method) {
                            methods.push(method);
                        }
                    }
                }
                Some("destroyable") if !methods.contains(&"destroy") && !creatable.is_empty() => {
                    methods.push("destroy");
                }
                _ => {}
            }
            if matches!(name, Some("only" | "except")) {
                let listed = self.literal_names(parent)?;
                if name == Some("only") {
                    methods.retain(|m| listed.contains(*m));
                } else {
                    methods.retain(|m| !listed.contains(*m));
                }
            }
            call = parent;
        }
        Some(methods)
    }

    /// The method names a call lists, as one array or as separate strings;
    /// `None` when any is not a string literal.
    fn literal_names(&self, call: Node) -> Option<HashSet<String>> {
        let mut names = HashSet::new();
        for value in argument_values(call) {
            if value.kind() == "array_creation_expression" {
                for element in named_children(value) {
                    let item = element
                        .named_child(0)
                        .filter(|_| element.named_child_count() == 1)?;
                    names.insert(literal_string_text(item, self.source)?);
                }
            } else {
                names.insert(literal_string_text(value, self.source)?);
            }
        }
        Some(names)
    }
}

/// The value expression of one `argument` (the last named child; a named
/// argument has its label first).
fn argument_value(argument: Node) -> Option<Node> {
    argument.named_child(argument.named_child_count().checked_sub(1)?)
}

fn argument_values(call: Node) -> Vec<Node> {
    call.child_by_field_name("arguments")
        .map(|arguments| {
            named_children(arguments)
                .into_iter()
                .filter_map(argument_value)
                .collect()
        })
        .unwrap_or_default()
}
