//! Route definitions: which controller methods the router invokes.

use std::collections::HashSet;

use mct_core::{Location, RelationKind, RelationTarget, SymbolId};
use mct_tree_sitter::location;
use tree_sitter::Node;

use super::{is_identifier, keys::KeyKind};
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
        self.laravel_morph_map(scope, method, &args, owner);
        if self.is_facade(scope, "View") && matches!(method, "make" | "first" | "composer") {
            if let Some(view) = args.first() {
                if method == "make" {
                    self.push_view_reference(owner, *view, location(node));
                } else {
                    self.laravel_view_candidates(owner, *view);
                }
            }
            return;
        }
        if let Some(argument) = args.first() {
            match method {
                "create" if self.is_facade(scope, "Schema") => {
                    self.laravel_schema_table(*argument);
                }
                "command" if self.is_facade(scope, "Artisan") => {
                    self.laravel_command(*argument, None);
                }
                "command" if self.is_facade(scope, "Schedule") => {
                    self.laravel_command(*argument, Some(owner));
                }
                "call" | "callSilent" | "queue" if self.is_facade(scope, "Artisan") => {
                    self.laravel_command(*argument, Some(owner));
                }
                _ => {}
            }
        }
        if self.laravel_service_registration(scope, method, &args, owner) {
            return;
        }
        let key = match method {
            "get" if self.is_facade(scope, "Config") => Some(KeyKind::Config),
            "get" | "choice" if self.is_facade(scope, "Lang") => Some(KeyKind::Translation),
            "route" if self.is_facade(scope, "URL") || self.is_facade(scope, "Redirect") => {
                Some(KeyKind::Route)
            }
            _ => None,
        };
        if let Some(key) = key {
            if let Some(argument) = args.first() {
                self.laravel_literal_key(owner, key, *argument);
            }
            return;
        }
        if self.is_facade(scope, "Route") {
            self.laravel_route_call(node, method, owner, &args);
        }
    }

    fn laravel_route_call(&mut self, node: Node, method: &str, owner: SymbolId, args: &[Node]) {
        if matches!(
            method,
            "view" | "redirect" | "permanentRedirect" | "match" | "fallback"
        ) || VERBS.contains(&method)
        {
            self.laravel_route_name(node);
            self.laravel_route_middleware(node, owner);
        }
        match method {
            "view" => {
                if let Some(view) = args.get(1) {
                    self.push_view_reference(owner, *view, location(node));
                }
            }
            "middleware" => self.laravel_middleware_arguments(node, owner),
            "match" => self.route_action(owner, args.get(2).copied()),
            "fallback" => self.route_action(owner, args.first().copied()),
            "resource" => self.resource_routes(node, owner, args, RESOURCE_METHODS, &[]),
            "apiResource" => self.resource_routes(node, owner, args, API_RESOURCE_METHODS, &[]),
            "singleton" => {
                self.resource_routes(node, owner, args, SINGLETON_METHODS, SINGLETON_CREATABLE)
            }
            "apiSingleton" => self.resource_routes(
                node,
                owner,
                args,
                API_SINGLETON_METHODS,
                API_SINGLETON_CREATABLE,
            ),
            "resources" | "softDeletableResources" => {
                self.resource_map(owner, args, RESOURCE_METHODS, node)
            }
            "apiResources" => self.resource_map(owner, args, API_RESOURCE_METHODS, node),
            m if VERBS.contains(&m) => self.route_action(owner, args.get(1).copied()),
            _ => {}
        }
    }

    fn laravel_route_name(&mut self, route: Node) {
        let mut name = String::new();
        let mut loc = None;
        for call in route_modifiers(route) {
            if method_name(call, self.source) == Some("name") {
                let Some(argument) = argument_values(call).first().copied() else {
                    return;
                };
                let Some(part) = literal_string_text(argument, self.source) else {
                    return;
                };
                name.push_str(&part);
                loc = Some(location(argument));
            }
        }
        let Some(loc) = loc else { return };
        let mut prefixes = Vec::new();
        let mut ancestor = route.parent();
        while let Some(node) = ancestor {
            if method_name(node, self.source) == Some("group")
                && matches!(
                    node.kind(),
                    "scoped_call_expression" | "member_call_expression"
                )
            {
                let Some(prefix) = self.route_group_prefix(node) else {
                    return;
                };
                prefixes.push(prefix);
            }
            ancestor = node.parent();
        }
        let full_name: String = prefixes.into_iter().rev().chain([name]).collect();
        self.laravel_key_declaration(KeyKind::Route, full_name, loc);
    }

    fn route_group_prefix(&self, mut call: Node) -> Option<String> {
        // RouteRegistrar keeps the last name/as value for each group.
        let arguments = argument_values(call);
        let [body] = arguments.as_slice() else {
            return None;
        };
        if !matches!(body.kind(), "anonymous_function" | "arrow_function") {
            return None;
        }
        let mut prefix = None;
        loop {
            if prefix.is_none() && matches!(method_name(call, self.source), Some("name" | "as")) {
                let argument = argument_values(call).first().copied()?;
                prefix = Some(literal_string_text(argument, self.source)?);
            }
            match call.kind() {
                "member_call_expression" => call = call.child_by_field_name("object")?,
                "scoped_call_expression" => {
                    return self
                        .is_facade(call.child_by_field_name("scope")?, "Route")
                        .then(|| prefix.unwrap_or_default());
                }
                _ => return None,
            }
        }
    }

    fn laravel_route_middleware(&mut self, route: Node, owner: SymbolId) {
        for call in route_modifiers(route) {
            if method_name(call, self.source) == Some("middleware") {
                self.laravel_middleware_arguments(call, owner);
            }
        }
    }

    fn laravel_middleware_arguments(&mut self, call: Node, owner: SymbolId) {
        for argument in argument_values(call) {
            let items = if argument.kind() == "array_creation_expression" {
                named_children(argument)
                    .into_iter()
                    .filter(|item| item.named_child_count() == 1)
                    .filter_map(|item| item.named_child(0))
                    .collect()
            } else {
                vec![argument]
            };
            for item in items {
                if let Some(value) = literal_string_text(item, self.source) {
                    let name = value.split(':').next().unwrap_or(&value);
                    self.laravel_key_reference(owner, KeyKind::Middleware, name, location(item));
                }
            }
        }
    }

    /// Hook for every `$x->method(...)` call: the `withRouting(web: ...)`
    /// of `bootstrap/app.php` includes the route files it names.
    pub(crate) fn laravel_member_call(&mut self, node: Node, owner: SymbolId) {
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        if node
            .child_by_field_name("object")
            .filter(|receiver| receiver.kind() == "variable_name")
            .and_then(|receiver| self.locals.get(text(receiver, self.source)))
            .and_then(|name| self.scope.resolve_class(name))
            .is_some_and(|class| class.namespace == r"Illuminate\Routing" && class.name == "Router")
        {
            self.laravel_route_call(node, text(name, self.source), owner, &argument_values(node));
        }
        self.laravel_provider_binding(node, text(name, self.source));
        if text(name, self.source) == "alias" {
            self.laravel_middleware_aliases(node);
        }
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
    pub(super) fn class_constant(&self, node: Node) -> Option<ResolvedName> {
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
    /// any other code. Fully qualified `'Namespace\Controller@method'`
    /// strings are also explicit enough to resolve.
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
            "string" | "encapsed_string" => {
                let Some(value) = literal_string_text(action, self.source) else {
                    return;
                };
                if let Some((class, method)) = value.split_once('@') {
                    let full = class.strip_prefix('\\').unwrap_or(class);
                    if let Some((namespace, name)) = full.rsplit_once('\\') {
                        if full.split('\\').all(is_identifier) && is_identifier(method) {
                            let controller = ResolvedName {
                                namespace: namespace.to_string(),
                                name: name.to_string(),
                            };
                            self.call_into(owner, &controller, method, location(action));
                        }
                    }
                } else if is_identifier(&value) {
                    if let Some(controller) = self.route_controllers.last().cloned() {
                        self.call_into(owner, &controller, &value, location(action));
                    }
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

pub(super) fn argument_values(call: Node) -> Vec<Node> {
    call.child_by_field_name("arguments")
        .map(|arguments| {
            named_children(arguments)
                .into_iter()
                .filter_map(argument_value)
                .collect()
        })
        .unwrap_or_default()
}

fn method_name<'a>(call: Node, source: &'a str) -> Option<&'a str> {
    call.child_by_field_name("name").map(|n| text(n, source))
}

fn route_modifiers(route: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    let mut receiver = route;
    std::iter::from_fn(move || {
        let call = receiver.parent()?;
        if call.kind() != "member_call_expression"
            || call.child_by_field_name("object") != Some(receiver)
        {
            return None;
        }
        receiver = call;
        Some(call)
    })
}
