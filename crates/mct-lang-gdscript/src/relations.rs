use mct_core::{
    JoinPoint, Location, RelationKind, RelationTarget, SymbolId, SymbolKind, SymbolRelation,
};
use mct_tree_sitter::{find_child, find_named_child, location};
use tree_sitter::Node;

use crate::{basename, first_named, Walker};

enum Lookup<'a> {
    Scope,
    Receiver(Option<&'a str>),
    SelfMember,
    Base(NodeBase<'a>),
}

enum NodeBase<'a> {
    Global(&'a str),
    Script(String),
}

impl NodeBase<'_> {
    fn method_target(self) -> RelationTarget {
        let mut target = RelationTarget {
            kind: Some(SymbolKind::Method),
            ..Default::default()
        };
        match self {
            Self::Global(class) => {
                target.qualifier = Some(class.rsplit('.').next().unwrap_or(class).into())
            }
            Self::Script(path) => {
                target.module = Some(basename(&path).into());
                target.path = Some(path);
            }
        }
        target
    }
}

pub(super) struct HttpUse<'a> {
    owner: SymbolId,
    receiver: &'a str,
    method: &'static str,
    /// A string literal, or the name of a constant that holds one.
    url: Url<'a>,
    site: Location,
    offset: usize,
}

enum Url<'a> {
    Literal(&'a str),
    Constant(&'a str),
}

pub(super) struct Pending<'a> {
    from: SymbolId,
    name: &'a str,
    kind: RelationKind,
    site: Location,
    offset: usize,
    lookup: Lookup<'a>,
}

impl<'a> Walker<'a> {
    pub(super) fn observe(&mut self, node: Node<'_>, owner: SymbolId, base: Option<Node<'_>>) {
        match node.kind() {
            "extends_statement" => {
                if let Some(target) = first_named(node) {
                    if let Some(path) = self.script_path(target) {
                        // The base is the script's class; the file is imported.
                        self.emit_script_class(owner, RelationKind::Extends, &path, target);
                        self.emit_import(owner, &path, target);
                    } else if target.kind() == "type" {
                        let name = self.text(target);
                        let (qualifier, name) = name
                            .rsplit_once('.')
                            .map_or((None, name), |(parent, name)| (Some(parent.into()), name));
                        self.emit(
                            owner,
                            RelationKind::Extends,
                            name,
                            location(target),
                            RelationTarget {
                                kind: Some(SymbolKind::Class),
                                qualifier,
                                ..Default::default()
                            },
                        );
                    }
                }
            }
            "call" | "attribute_call" | "base_call" => self.call(node, owner, base),
            "identifier" => self.identifier(node, owner),
            "annotation" => {
                if let Some(name) = find_named_child(node, "identifier") {
                    self.emit(
                        owner,
                        RelationKind::References,
                        self.text(name),
                        location(name),
                        RelationTarget {
                            external: true,
                            ..Default::default()
                        },
                    );
                }
            }
            "getter" | "setter" => self.defer(
                owner,
                self.text(node),
                RelationKind::References,
                node,
                Lookup::Scope,
            ),
            "string" => {
                let raw = self.text(node);
                if raw.starts_with("r\"") || raw.starts_with("r'") {
                    if let Some(body) = unquote(raw.get(1..).unwrap_or_default()) {
                        self.literals.push(body, location(node).line);
                    }
                } else if let Some(body) = unquote(raw) {
                    self.literals.push(body, location(node).line);
                }
            }
            _ => {}
        }
    }

    fn call(&mut self, node: Node<'_>, owner: SymbolId, base: Option<Node<'_>>) {
        let Some(callee) = first_named(node) else {
            return;
        };
        if callee.kind() != "identifier" && self.text(callee) != "super" {
            return;
        }

        let name = self.text(callee);
        if node.kind() == "call" && matches!(name, "preload" | "load") {
            if let Some((arg, path)) = node
                .child_by_field_name("arguments")
                .and_then(first_named)
                .and_then(|arg| self.resource_path(arg).map(|path| (arg, path)))
            {
                if path.ends_with(".gd") {
                    // A loaded script is a use of its class.
                    self.emit_script_class(owner, RelationKind::References, &path, arg);
                }
                self.emit_import(owner, &path, arg);
            }
            return;
        }

        let lookup = if node.kind() == "base_call" || name == "super" || self.super_receiver(node) {
            let Some(target) = base.and_then(first_named) else {
                return;
            };
            if let Some(path) = self.script_path(target) {
                Lookup::Base(NodeBase::Script(path))
            } else {
                Lookup::Base(NodeBase::Global(self.text(target)))
            }
        } else if node.kind() == "attribute_call" {
            if self.self_receiver(node) {
                Lookup::SelfMember
            } else {
                let receiver = self.simple_receiver(node);
                if let Some(receiver) = receiver.filter(|_| name == "request") {
                    self.observe_http(node, owner, receiver);
                }
                Lookup::Receiver(receiver)
            }
        } else {
            Lookup::Scope
        };
        if name == "super" {
            let Lookup::Base(base) = lookup else { return };
            let Some(method) = std::iter::successors(Some(owner), |id| self.scopes[*id as usize])
                .map(|id| &self.result.symbols[id as usize])
                .find(|symbol| symbol.kind == SymbolKind::Method)
            else {
                return;
            };
            let name = method.name.clone();
            let target = base.method_target();
            self.emit(owner, RelationKind::Calls, &name, location(callee), target);
        } else {
            self.defer(owner, name, RelationKind::Calls, callee, lookup);
        }
    }

    fn self_receiver(&self, member: Node<'_>) -> bool {
        member.prev_named_sibling().is_some_and(|receiver| {
            self.text(receiver) == "self" && receiver.prev_named_sibling().is_none()
        })
    }

    fn super_receiver(&self, member: Node<'_>) -> bool {
        member.prev_named_sibling().is_some_and(|receiver| {
            self.text(receiver) == "super" && receiver.prev_named_sibling().is_none()
        })
    }

    fn identifier(&mut self, node: Node<'_>, owner: SymbolId) {
        let Some(parent) = node.parent() else { return };
        // These identifiers are declarations or are handled by another node.
        if matches!(
            parent.kind(),
            "annotation"
                | "parameters"
                | "typed_parameter"
                | "pattern_binding"
                | "variadic_parameter"
                | "attribute_call"
                | "base_call"
        ) || matches!(
            parent.kind(),
            "typed_default_parameter" | "default_parameter"
        ) && first_named(parent) == Some(node)
            || matches!(parent.kind(), "call" | "attribute_subscript")
                && first_named(parent) == Some(node)
            || matches!(parent.kind(), "for_statement" | "enumerator")
                && parent.child_by_field_name("left") == Some(node)
            || parent.kind() == "pair"
                && parent.child_by_field_name("left") == Some(node)
                && find_child(parent, "=").is_some()
            || in_extends(node)
        {
            return;
        }
        let lookup = if parent.kind() == "attribute"
            && node.prev_named_sibling().is_some()
            && !self.self_receiver(node)
        {
            Lookup::Receiver(self.simple_receiver(node))
        } else if parent.kind() == "attribute" && self.self_receiver(node) {
            Lookup::SelfMember
        } else {
            Lookup::Scope
        };
        self.defer(
            owner,
            self.text(node),
            RelationKind::References,
            node,
            lookup,
        );
    }

    fn simple_receiver(&self, member: Node<'_>) -> Option<&'a str> {
        let previous = member.prev_named_sibling()?;
        (previous.kind() == "identifier" && previous.prev_named_sibling().is_none())
            .then(|| self.text(previous))
    }

    pub(super) fn record_binding_type(&mut self, id: SymbolId, node: Node<'_>) {
        if let Some(ty) = node
            .child_by_field_name("type")
            .filter(|n| n.kind() == "type")
        {
            self.receiver_types.insert(id, self.text(ty));
        } else if let Some(class) = node
            .child_by_field_name("type")
            .filter(|n| n.kind() == "inferred_type")
            .and_then(|_| node.child_by_field_name("value"))
            .and_then(|value| self.constructed_class(value))
        {
            // `var x := Name.new()`: the type is fixed at the declaration.
            self.receiver_types.insert(id, class);
        }
        if node.kind() == "const_statement" {
            if let Some(text) = node
                .child_by_field_name("value")
                .filter(|n| n.kind() == "string")
                .and_then(|n| unquote(self.text(n)))
            {
                self.constant_strings.insert(id, text);
            }
            if let Some(value) = node
                .child_by_field_name("value")
                .filter(|n| n.kind() == "call")
            {
                if first_named(value).is_some_and(|callee| self.text(callee) == "preload") {
                    if let Some(path) = value
                        .child_by_field_name("arguments")
                        .and_then(first_named)
                        .and_then(|arg| self.script_path(arg))
                    {
                        self.preloaded_scripts.insert(id, path);
                    }
                }
            }
        }
    }

    /// `Name` in a `Name.new(...)` expression.
    fn constructed_class(&self, value: Node<'_>) -> Option<&'a str> {
        if value.kind() != "attribute" || value.named_child_count() != 2 {
            return None;
        }
        let class = value.named_child(0).filter(|n| n.kind() == "identifier")?;
        let call = value
            .named_child(1)
            .filter(|n| n.kind() == "attribute_call")?;
        let method = first_named(call)?;
        (self.text(method) == "new").then(|| self.text(class))
    }

    fn observe_http(&mut self, node: Node<'_>, owner: SymbolId, receiver: &'a str) {
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let mut cursor = args.walk();
        let mut args = args.named_children(&mut cursor).filter(|n| !n.is_extra());
        let Some(url) = args.next().and_then(|n| match n.kind() {
            "string" => unquote(self.text(n)).map(Url::Literal),
            "identifier" => Some(Url::Constant(self.text(n))),
            _ => None,
        }) else {
            return;
        };
        let method = match args.nth(1).map(|n| self.text(n)) {
            None | Some("HTTPClient.METHOD_GET") => "GET",
            Some("HTTPClient.METHOD_HEAD") => "HEAD",
            Some("HTTPClient.METHOD_POST") => "POST",
            Some("HTTPClient.METHOD_PUT") => "PUT",
            Some("HTTPClient.METHOD_PATCH") => "PATCH",
            Some("HTTPClient.METHOD_DELETE") => "DELETE",
            Some("HTTPClient.METHOD_OPTIONS") => "OPTIONS",
            _ => return,
        };
        self.http_uses.push(HttpUse {
            owner,
            receiver,
            method,
            url,
            site: location(node),
            offset: node.start_byte(),
        });
    }

    /// `Imports` of the file at `path`, a module.
    fn emit_import(&mut self, owner: SymbolId, path: &str, site: Node<'_>) {
        self.emit(
            owner,
            RelationKind::Imports,
            basename(path),
            location(site),
            RelationTarget {
                path: Some(path.to_owned()),
                kind: Some(SymbolKind::Module),
                ..Default::default()
            },
        );
    }

    /// A `kind` relation to the class of the script at `path`, named after
    /// its file (see [`crate::script_class_name`]).
    fn emit_script_class(
        &mut self,
        owner: SymbolId,
        kind: RelationKind,
        path: &str,
        site: Node<'_>,
    ) {
        self.emit(
            owner,
            kind,
            crate::script_class_name(path),
            location(site),
            RelationTarget {
                path: Some(path.to_owned()),
                kind: Some(SymbolKind::Class),
                ..Default::default()
            },
        );
    }

    /// A local `.gd` script path, `res://`-rooted or relative to this file.
    fn script_path(&self, string: Node<'_>) -> Option<String> {
        self.resource_path(string)
            .filter(|path| path.ends_with(".gd"))
    }

    /// A local script, scene (`.tscn`) or text resource (`.tres`) path.
    fn resource_path(&self, string: Node<'_>) -> Option<String> {
        if string.kind() != "string" {
            return None;
        }
        let value = unquote(self.text(string))?;
        let (base, path) = match value.strip_prefix("res://") {
            Some(root_relative) => ("", root_relative),
            None => (
                self.file
                    .relative_path
                    .rsplit_once('/')
                    .map_or("", |(dir, _)| dir),
                value,
            ),
        };
        if !crate::RESOURCE_EXTENSIONS
            .iter()
            .any(|extension| path.ends_with(&format!(".{extension}")))
            || path.starts_with('/')
            || path.contains([':', '\\'])
        {
            return None;
        }
        let mut parts: Vec<_> = base.split('/').filter(|s| !s.is_empty()).collect();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                part => parts.push(part),
            }
        }
        let path = parts.join("/");
        // The index knows which project.godot owns this file.
        Some(if value.starts_with("res://") {
            format!("res://{path}")
        } else {
            path
        })
    }

    fn defer(
        &mut self,
        from: SymbolId,
        name: &'a str,
        kind: RelationKind,
        node: Node<'_>,
        lookup: Lookup<'a>,
    ) {
        self.pending.push(Pending {
            from,
            name,
            kind,
            site: location(node),
            offset: node.start_byte(),
            lookup,
        });
    }

    pub(super) fn finish_relations(&mut self) {
        for http in std::mem::take(&mut self.http_uses) {
            let binding = self.find_binding(http.owner, http.receiver, http.offset, false);
            let url = match http.url {
                Url::Literal(url) => Some(url),
                Url::Constant(name) => self
                    .find_binding(http.owner, name, http.offset, false)
                    .and_then(|id| self.constant_strings.get(&id).copied()),
            };
            if binding.and_then(|id| self.receiver_types.get(&id)) == Some(&"HTTPRequest") {
                if let Some(point) = url
                    .and_then(|url| JoinPoint::http_uses(http.owner, http.method, url, http.site))
                {
                    self.result.join_points.push(point);
                }
            }
        }
        for pending in std::mem::take(&mut self.pending) {
            let Pending {
                from,
                name,
                kind,
                site,
                offset,
                lookup,
            } = pending;
            let mut target = RelationTarget::default();
            let mut target_name = name;
            match lookup {
                Lookup::Receiver(receiver) => {
                    let binding = receiver.and_then(|r| self.find_binding(from, r, offset, false));
                    if let Some(path) = binding.and_then(|id| self.preloaded_scripts.get(&id)) {
                        target.module = Some(basename(path).into());
                        target.path = Some(path.clone());
                        if name == "new" && kind == RelationKind::Calls {
                            target_name = "_init";
                            target.kind = Some(SymbolKind::Method);
                        }
                    } else if let Some(ty) = binding.and_then(|id| self.receiver_types.get(&id)) {
                        // A type named by a preload constant is that script.
                        match self
                            .find_binding(from, ty, offset, false)
                            .and_then(|id| self.preloaded_scripts.get(&id))
                        {
                            Some(path) => {
                                target.module = Some(basename(path).into());
                                target.path = Some(path.clone());
                            }
                            None => target.qualifier = Some((*ty).into()),
                        }
                    } else if let Some(receiver) = receiver.filter(|_| {
                        binding.is_none() && name == "new" && kind == RelationKind::Calls
                    }) {
                        target_name = receiver;
                        target.kind = Some(SymbolKind::Class);
                    } else if let Some(class) = binding
                        .map(|id| &self.result.symbols[id as usize])
                        .filter(|symbol| {
                            matches!(symbol.kind, SymbolKind::Class | SymbolKind::Enum)
                        })
                    {
                        target.path = Some(self.file.relative_path.clone());
                        if class.kind == SymbolKind::Class
                            && name == "new"
                            && kind == RelationKind::Calls
                        {
                            target_name = receiver.unwrap_or(name);
                            target.kind = Some(SymbolKind::Class);
                            target.qualifier.clone_from(&class.parent);
                        } else {
                            target.qualifier = Some(class.name.clone());
                        }
                    } else if let Some(global) =
                        receiver.filter(|r| binding.is_none() && is_global_name(r))
                    {
                        // An unbound PascalCase receiver names a `class_name`
                        // global, an autoload or an engine class. The index
                        // binds it to that type and its ancestors only.
                        target.qualifier = Some(global.into());
                    } else {
                        target.member = true;
                    }
                }
                Lookup::Base(base) => target = base.method_target(),
                Lookup::Scope | Lookup::SelfMember => {
                    if let Some(binding) =
                        self.find_binding(from, name, offset, matches!(lookup, Lookup::SelfMember))
                    {
                        let symbol = &self.result.symbols[binding as usize];
                        target.kind = Some(symbol.kind);
                        target.qualifier.clone_from(&symbol.parent);
                        target.path = Some(self.file.relative_path.clone());
                    } else if kind == RelationKind::References && matches!(name, "self" | "super") {
                        continue;
                    } else if is_type_name(name) {
                        // A PascalCase bare name is a `class_name` global, an
                        // autoload or an engine class.
                        target.kind = Some(SymbolKind::Class);
                    } else if let Some(class) = self.enclosing_class(from) {
                        // Any other unbound name is an inherited member or an
                        // engine one. Qualified by its class, with no path,
                        // the index finds it on the class's ancestors only.
                        if kind == RelationKind::Calls {
                            target.kind = Some(SymbolKind::Method);
                        }
                        target.qualifier = Some(self.result.symbols[class as usize].name.clone());
                    }
                }
            }
            self.emit(from, kind, target_name, site, target);
        }
    }

    /// The nearest class enclosing `from`, itself included.
    fn enclosing_class(&self, from: SymbolId) -> Option<SymbolId> {
        let mut scope = Some(from);
        while let Some(owner) = scope {
            if self.result.symbols[owner as usize].kind == SymbolKind::Class {
                return Some(owner);
            }
            scope = self.scopes[owner as usize];
        }
        None
    }

    fn find_binding(
        &self,
        from: SymbolId,
        name: &str,
        offset: usize,
        self_member: bool,
    ) -> Option<SymbolId> {
        let mut scope = Some(from);
        while let Some(owner) = scope {
            if self_member && self.result.symbols[owner as usize].kind != SymbolKind::Class {
                scope = self.scopes[owner as usize];
                continue;
            }
            if let Some(id) = self.bindings.get(&(owner, name)).and_then(|ids| {
                ids.iter().rev().find(|id| {
                    self.local_ranges
                        .get(id)
                        .is_none_or(|range| range.contains(&offset))
                })
            }) {
                return Some(*id);
            }
            if self_member {
                return None;
            }
            scope = self.scopes[owner as usize];
        }
        None
    }

    fn emit(
        &mut self,
        from: SymbolId,
        kind: RelationKind,
        name: &str,
        mut site: Location,
        mut target: RelationTarget,
    ) {
        site.end_line = None;
        target.relation = self.result.relations.len();
        self.result.relation_targets.push(target);
        self.result.relations.push(SymbolRelation {
            from,
            kind,
            to_name: name.into(),
            location: site,
        });
    }
}

/// Whether `node` is part of an `extends` base, already an `Extends`.
fn in_extends(node: Node<'_>) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current.filter(|p| matches!(p.kind(), "type" | "attribute")) {
        current = parent.parent();
    }
    current.is_some_and(|p| p.kind() == "extends_statement")
}

/// Godot names classes, autoloads and engine singletons in PascalCase.
fn is_global_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// A PascalCase type name, not an `UPPER_SNAKE` constant.
fn is_type_name(name: &str) -> bool {
    is_global_name(name) && name.contains(|c: char| c.is_ascii_lowercase())
}

fn unquote(text: &str) -> Option<&str> {
    ["\"\"\"", "'''", "\"", "'"]
        .into_iter()
        .find_map(|quote| text.strip_prefix(quote)?.strip_suffix(quote))
}
