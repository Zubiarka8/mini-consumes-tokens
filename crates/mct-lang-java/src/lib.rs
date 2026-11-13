//! `LanguageParser` implementation for Java, via `tree-sitter-java`.
//!
//! Known limits: an anonymous class's methods (`new Runnable() { … }`)
//! attach to the enclosing type (an enum constant's body is the constant's,
//! since it has a name); a sealed type's
//! `permits` list is no relation (it names subtypes, not supertypes).

use mct_core::{
    LanguageParser, Location, ParseError, ParsedFile, RelationKind, RelationTarget, SourceFile,
    SymbolId, SymbolKind, SymbolRecord, SymbolRelation, MAX_TRAVERSAL_DEPTH,
};
use mct_tree_sitter::{find_child, first_error, location, module_location, Supertype};
use tree_sitter::{Node, Parser};

mod details;
mod jdk;
mod receivers;
use receivers::Bindings;

pub struct JavaParser;

impl LanguageParser for JavaParser {
    fn language_id(&self) -> &'static str {
        "java"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["java"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_java::LANGUAGE` is a statically linked grammar
        // compiled into this binary; `set_language` only fails on an ABI
        // mismatch between the grammar and this `tree-sitter` version, which
        // Cargo.lock pins at build time — it never depends on the content of
        // an indexed repo.
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .expect("tree-sitter-java grammar is statically valid");

        let tree = parser
            .parse(&file.contents, None)
            .ok_or_else(|| ParseError::Syntax {
                path: file.relative_path.clone(),
                line: 1,
                message: "tree-sitter produced no parse tree".to_string(),
            })?;

        let root = tree.root_node();
        if root.has_error() {
            let error_node = first_error(root).unwrap_or(root);
            return Err(ParseError::Syntax {
                path: file.relative_path.clone(),
                line: error_node.start_position().row as u32 + 1,
                message: "syntax error".to_string(),
            });
        }

        let module_name = module_name_for(&file.relative_path);
        let mut walker = Walker::new(&file.contents);
        walker.jdk = jdk::JdkImports::of(root, &file.contents);
        let module_id =
            walker.push_symbol(module_name, SymbolKind::Module, module_location(root), None);
        walker.visit_children(root, module_id, None, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .unwrap_or(relative_path)
        .trim_end_matches(".java")
        .to_string()
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

#[derive(Default)]
struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    relation_targets: Vec<RelationTarget>,
    next_id: SymbolId,
    /// Declared types of the enclosing method's names (see `receivers`).
    locals: Bindings,
    /// Declared types of each enclosing type's fields, innermost last.
    fields: Vec<Bindings>,
    /// Type variables in scope, which never qualify a call.
    generics: Vec<String>,
    /// Declaration details of the symbols recorded so far (ADR-034).
    details: Vec<mct_core::SymbolDetail>,
    /// Direct supertypes of each enclosing type body, innermost last: what
    /// an `@Override` method there can override (D8).
    supertypes: Vec<Vec<Supertype>>,
    /// Type names that a single-type JDK import binds in this file (D9).
    jdk: jdk::JdkImports,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }

    fn push_symbol(
        &mut self,
        name: String,
        kind: SymbolKind,
        location: Location,
        parent: Option<String>,
    ) -> SymbolId {
        mct_tree_sitter::push_symbol(
            &mut self.symbols,
            &mut self.next_id,
            name,
            kind,
            location,
            parent,
        )
    }

    fn push_relation(
        &mut self,
        from: SymbolId,
        kind: RelationKind,
        to_name: String,
        loc: Location,
    ) {
        mct_tree_sitter::push_relation(&mut self.relations, from, kind, to_name, loc);
    }

    /// `relation` from `from` to the type `type_id` names (a supertype or
    /// an annotation type), which can only be a type of `target_kind`:
    /// never a constructor or method spelled like it.
    fn push_type_relation(
        &mut self,
        from: SymbolId,
        relation: RelationKind,
        target_kind: SymbolKind,
        type_id: Node,
    ) {
        self.relation_targets.push(RelationTarget {
            relation: self.relations.len(),
            kind: Some(target_kind),
            ..Default::default()
        });
        self.push_relation(
            from,
            relation,
            text(type_id, self.source).to_string(),
            location(type_id),
        );
    }

    /// A `references` relation from `from` to the annotation type each
    /// annotation on `declaration` names (JLS §9.7): `Audited` for `@Audited`
    /// or `@shop.meta.Audited(…)`. Annotation types are indexed as
    /// interfaces, so a same-named method or class is never the target.
    fn push_annotations(&mut self, declaration: Node, from: SymbolId) {
        let Some(modifiers) = find_child(declaration, "modifiers") else {
            return;
        };
        let mut cursor = modifiers.walk();
        let names: Vec<Node> = modifiers
            .children(&mut cursor)
            .filter(|m| matches!(m.kind(), "marker_annotation" | "annotation"))
            .filter_map(|m| m.child_by_field_name("name").and_then(last_identifier))
            .collect();
        for name in names {
            self.push_type_relation(from, RelationKind::References, SymbolKind::Interface, name);
        }
    }

    /// Each component of `record R(T a, U b)` declares an implicit accessor
    /// method `a()` (JLS §8.10.3), indexed as a `method` of `R` at the
    /// component. A component whose accessor the body declares explicitly
    /// (a method of that name with no parameters) is skipped, since that
    /// declaration is indexed on its own.
    fn push_record_accessors(&mut self, record: Node, record_name: &str) {
        let Some(params) = record.child_by_field_name("parameters") else {
            return;
        };
        let mut explicit = Vec::new();
        if let Some(body) = record.child_by_field_name("body") {
            let mut cursor = body.walk();
            for member in body.named_children(&mut cursor) {
                let takes_none = member
                    .child_by_field_name("parameters")
                    .is_some_and(|p| p.named_child_count() == 0);
                if member.kind() == "method_declaration" && takes_none {
                    if let Some(n) = member.child_by_field_name("name") {
                        explicit.push(text(n, self.source));
                    }
                }
            }
        }
        let mut cursor = params.walk();
        for param in params.named_children(&mut cursor) {
            if param.kind() != "formal_parameter" {
                continue;
            }
            let Some(name_node) = param.child_by_field_name("name") else {
                continue;
            };
            let name = text(name_node, self.source);
            if !explicit.contains(&name) {
                self.push_symbol(
                    name.to_string(),
                    SymbolKind::Method,
                    location(param),
                    Some(record_name.to_string()),
                );
            }
        }
    }

    fn scan_locals(&self, node: Node) -> Bindings {
        let mut map = Bindings::new();
        receivers::locals(node, self.source, &self.generics, &mut map, 0);
        map
    }

    /// The declared type of a name the current method can see: its locals
    /// shadow the innermost type's fields. `None` when not provable.
    fn declared_type(&self, name: &str) -> Option<Option<String>> {
        self.locals
            .get(name)
            .or_else(|| self.fields.last().and_then(|f| f.get(name)))
            .cloned()
    }

    /// `a.b.C` read as a type name: only identifiers, the last capitalized,
    /// and the first not a variable the current method can see.
    fn names_a_type(&self, chain: Node) -> bool {
        let Some(last) = chain.child_by_field_name("field") else {
            return false;
        };
        if last.kind() != "identifier" || !text(last, self.source).starts_with(char::is_uppercase) {
            return false;
        }
        let mut head = chain;
        while head.kind() == "field_access" {
            match head.child_by_field_name("object") {
                Some(object) => head = object,
                None => return false,
            }
        }
        head.kind() == "identifier" && self.declared_type(text(head, self.source)).is_none()
    }

    /// `(qualifier, member)` for a call on `object`: its declared type, a
    /// type name for a static call (`Money.of()`), or a member call whose
    /// receiver type is not proven. `this.m()` and `super.m()` are left as
    /// bare calls, since the method may be inherited.
    fn receiver(&self, object: Node) -> (Option<String>, bool) {
        let typed = |ty: Option<String>| {
            let member = ty.is_none();
            (ty, member)
        };
        match object.kind() {
            "this" | "super" => (None, false),
            "identifier" => {
                let name = text(object, self.source);
                match self.declared_type(name) {
                    Some(ty) => typed(ty),
                    // Not a variable in scope: by Java's naming convention
                    // a capitalized name is a type. A misread inherited
                    // constant qualifies by a type that declares nothing,
                    // so the call stays unresolved, never wrong.
                    None if name.starts_with(char::is_uppercase) => (Some(name.to_string()), false),
                    None => (None, true),
                }
            }
            "field_access" => {
                let on_this = object
                    .child_by_field_name("object")
                    .is_some_and(|o| o.kind() == "this");
                let field = object.child_by_field_name("field");
                match (on_this, field) {
                    (true, Some(field)) => typed(
                        self.fields
                            .last()
                            .and_then(|f| f.get(text(field, self.source)))
                            .cloned()
                            .flatten(),
                    ),
                    // `Outer.Inner.m()` or `pkg.Outer.Inner.m()`: a chain of
                    // names whose first one is no variable in scope, ending
                    // in a capitalized name, is a nested type.
                    (false, Some(field)) if self.names_a_type(object) => {
                        (Some(text(field, self.source).to_string()), false)
                    }
                    _ => (None, true),
                }
            }
            _ => (None, true),
        }
    }

    /// `owner` is the innermost enclosing method/module (calls/imports
    /// attach to it); `type_name` is the innermost enclosing
    /// class/interface/enum name, used as the `parent` of members declared
    /// directly inside it. Each `method_declaration` — including every
    /// overload — gets its own `SymbolRecord` row (same name, same parent,
    /// different location), so overloads are never collapsed into one
    /// symbol; `find_symbol` naturally returns all of them.
    fn visit_children(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, owner, type_name, depth + 1);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, type_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "class_declaration"
            | "record_declaration"
            | "interface_declaration"
            | "annotation_type_declaration"
            | "enum_declaration" => {
                let kind = match node.kind() {
                    "class_declaration" | "record_declaration" => SymbolKind::Class,
                    "interface_declaration" | "annotation_type_declaration" => {
                        SymbolKind::Interface
                    }
                    _ => SymbolKind::Enum,
                };
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(
                    name.clone(),
                    kind,
                    location(node),
                    type_name.map(str::to_string),
                );
                self.details
                    .push(details::type_detail(node, id, self.source));
                self.push_annotations(node, id);

                // JLS §8.1.4, §8.1.5, §9.1.3: a class extends a class,
                // implements interfaces, and an interface extends interfaces.
                let supertypes = [
                    (
                        node.child_by_field_name("superclass"),
                        RelationKind::Extends,
                        SymbolKind::Class,
                    ),
                    (
                        node.child_by_field_name("interfaces"),
                        RelationKind::Implements,
                        SymbolKind::Interface,
                    ),
                    (
                        find_child(node, "extends_interfaces"),
                        RelationKind::Extends,
                        SymbolKind::Interface,
                    ),
                ];
                for (clause, relation, target_kind) in supertypes {
                    for type_id in clause.map(find_type_identifiers).unwrap_or_default() {
                        self.push_type_relation(id, relation, target_kind, type_id);
                    }
                }
                if node.kind() == "record_declaration" {
                    self.push_record_accessors(node, &name);
                }
                self.supertypes.push(self.direct_supertypes(node));
                let generics_len = self.generics.len();
                self.generics
                    .extend(receivers::type_parameters(node, self.source));
                self.fields
                    .push(receivers::fields(node, self.source, &self.generics));
                let outer_locals = std::mem::take(&mut self.locals);
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, owner, Some(&name), depth + 1);
                }
                self.locals = outer_locals;
                self.fields.pop();
                self.supertypes.pop();
                self.generics.truncate(generics_len);
            }
            "enum_constant" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                self.push_symbol(
                    name.clone(),
                    SymbolKind::Field,
                    location(node),
                    type_name.map(str::to_string),
                );
                // `NEW { Set<State> next() { … } }`: the constant's body is
                // its own subclass of the enum (javac's `State$1`), so its
                // methods have the constant as their parent and the enum as
                // their supertype. Their override of `State.next` then has
                // one candidate, the enum's own method, never themselves.
                if let Some(body) = node.child_by_field_name("body") {
                    self.supertypes.push(
                        type_name
                            .map(|enum_name| Supertype {
                                name: enum_name.to_string(),
                                external: false,
                            })
                            .into_iter()
                            .collect(),
                    );
                    self.visit_children(body, owner, Some(&name), depth + 1);
                    self.supertypes.pop();
                }
            }
            "method_declaration" | "constructor_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .map(|n| text(n, self.source).to_string())
                    .unwrap_or_default();
                let id = self.push_symbol(
                    name,
                    SymbolKind::Method,
                    location(node),
                    type_name.map(str::to_string),
                );
                self.details
                    .push(details::method_detail(node, id, self.source));
                self.push_annotations(node, id);
                if node.kind() == "method_declaration" && has_override_annotation(node, self.source)
                {
                    self.push_overrides(node, id);
                }
                let generics_len = self.generics.len();
                self.generics
                    .extend(receivers::type_parameters(node, self.source));
                let scanned = self.scan_locals(node);
                let outer_locals = std::mem::replace(&mut self.locals, scanned);
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.visit_children(params, id, type_name, depth + 1);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    self.visit_children(body, id, type_name, depth + 1);
                }
                self.locals = outer_locals;
                self.generics.truncate(generics_len);
            }
            "field_declaration" => {
                let mut cursor = node.walk();
                for declarator in node.children(&mut cursor) {
                    if declarator.kind() == "variable_declarator" {
                        if let Some(name_node) = declarator.child_by_field_name("name") {
                            let id = self.push_symbol(
                                text(name_node, self.source).to_string(),
                                SymbolKind::Field,
                                location(declarator),
                                type_name.map(str::to_string),
                            );
                            self.details.push(details::field_detail(
                                node,
                                declarator,
                                id,
                                self.source,
                            ));
                            self.push_annotations(node, id);
                        }
                        // The initializer runs like an initializer block:
                        // its calls and anonymous classes are the type's.
                        if let Some(value) = declarator.child_by_field_name("value") {
                            let scanned = self.scan_locals(value);
                            let outer_locals = std::mem::replace(&mut self.locals, scanned);
                            self.visit(value, owner, type_name, depth + 1);
                            self.locals = outer_locals;
                        }
                    }
                }
            }
            "import_declaration" => {
                if let Some(path) = node.named_child(0) {
                    self.push_type_import(owner, text(path, self.source), location(node));
                    if let Some(last) = last_identifier(path) {
                        self.push_relation(
                            owner,
                            RelationKind::Imports,
                            text(last, self.source).to_string(),
                            location(node),
                        );
                    }
                }
            }
            "method_invocation" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    // Java can't invoke a field or a variable (a lambda is
                    // called as `f.apply()`), so the target is a method.
                    let (qualifier, member) = node
                        .child_by_field_name("object")
                        .map_or((None, false), |object| self.receiver(object));
                    // A receiver typed by a JDK import can only be the JDK's
                    // (D9): the single-type import shadows the package's own
                    // types, so no repository method is the target.
                    let external = qualifier.as_deref().is_some_and(|q| self.jdk.is_jdk(q));
                    self.relation_targets.push(RelationTarget {
                        relation: self.relations.len(),
                        kind: Some(SymbolKind::Method),
                        qualifier: if external { None } else { qualifier },
                        member: member && !external,
                        external,
                        ..Default::default()
                    });
                    // location(name_node), not location(node): a chained call
                    // (`a.f(x).f(y)`) has its outer and inner method_invocation
                    // both start at `a`, which would make two same-named
                    // chained calls collide into one indistinguishable row.
                    self.push_relation(
                        owner,
                        RelationKind::Calls,
                        text(name_node, self.source).to_string(),
                        location(name_node),
                    );
                }
                if let Some(object) = node.child_by_field_name("object") {
                    self.visit(object, owner, type_name, depth + 1);
                }
                if let Some(arguments) = node.child_by_field_name("arguments") {
                    self.visit_children(arguments, owner, type_name, depth + 1);
                }
            }
            "object_creation_expression" => {
                // `new Foo(…)` calls Foo's constructor: a call to the type's
                // name (`HashMap` for `new java.util.HashMap<K, V>()`).
                if let Some(type_id) = node
                    .child_by_field_name("type")
                    .and_then(|t| find_type_identifiers(t).into_iter().next())
                {
                    // `new HashMap<>()` under `import java.util.HashMap;`
                    // can only build the JDK's type (D9). A qualified name
                    // (`new java.util.ArrayList<>()`) is left to resolution.
                    let unqualified = type_id
                        .parent()
                        .is_some_and(|p| p.kind() != "scoped_type_identifier");
                    if unqualified && self.jdk.is_jdk(text(type_id, self.source)) {
                        self.relation_targets.push(RelationTarget {
                            relation: self.relations.len(),
                            external: true,
                            ..Default::default()
                        });
                    }
                    self.push_relation(
                        owner,
                        RelationKind::Calls,
                        text(type_id, self.source).to_string(),
                        location(type_id),
                    );
                }
                // `new T() { … }` is an anonymous subclass of `T`: its one
                // direct supertype.
                let anonymous = find_child(node, "class_body").is_some();
                if anonymous {
                    let supertype = node
                        .child_by_field_name("type")
                        .and_then(|t| find_type_identifiers(t).into_iter().next())
                        .map(|t| Supertype {
                            name: text(t, self.source).to_string(),
                            external: false,
                        });
                    self.supertypes.push(supertype.into_iter().collect());
                }
                self.visit_children(node, owner, type_name, depth + 1);
                if anonymous {
                    self.supertypes.pop();
                }
            }
            // An initializer block has its own locals, like a method body.
            "static_initializer" | "block"
                if node.kind() == "static_initializer"
                    || node.parent().is_some_and(|p| p.kind() == "class_body") =>
            {
                let scanned = self.scan_locals(node);
                let outer_locals = std::mem::replace(&mut self.locals, scanned);
                self.visit_children(node, owner, type_name, depth + 1);
                self.locals = outer_locals;
            }
            _ => self.visit_children(node, owner, type_name, depth + 1),
        }
    }

    /// The direct supertypes `node`'s header names (`extends`, `implements`,
    /// an interface's `extends`). A type that extends no class also has the
    /// JDK root, external: `Object` (`Enum` for an enum, `Record` for a
    /// record), whose overridable methods are [`OBJECT_METHODS`].
    fn direct_supertypes(&self, node: Node) -> Vec<Supertype> {
        let mut named: Vec<Supertype> = Vec::new();
        let clauses = [
            node.child_by_field_name("superclass"),
            node.child_by_field_name("interfaces"),
            find_child(node, "extends_interfaces"),
        ];
        for clause in clauses.into_iter().flatten() {
            for type_id in find_type_identifiers(clause) {
                named.push(Supertype {
                    name: text(type_id, self.source).to_string(),
                    external: false,
                });
            }
        }
        let root = match node.kind() {
            "enum_declaration" => Some("Enum"),
            "record_declaration" => Some("Record"),
            _ if node.child_by_field_name("superclass").is_none() => Some("Object"),
            _ => None,
        };
        named.extend(root.map(|name| Supertype {
            name: name.to_string(),
            external: true,
        }));
        named
    }

    /// D8: an `@Override` method overrides a same-named method of one of
    /// its type's direct supertypes; the JDK root only one it declares.
    fn push_overrides(&mut self, method: Node, id: SymbolId) {
        let Some(name_node) = method.child_by_field_name("name") else {
            return;
        };
        let supertypes = self.supertypes.last().cloned().unwrap_or_default();
        mct_tree_sitter::push_overrides(
            &mut self.relations,
            &mut self.relation_targets,
            id,
            text(name_node, self.source),
            location(name_node),
            &supertypes,
            &OBJECT_METHODS,
        );
    }

    fn finish(self) -> ParsedFile {
        ParsedFile {
            symbols: self.symbols,
            relations: self.relations,
            relation_targets: self.relation_targets,
            symbol_details: self.details,
            ..Default::default()
        }
    }

    /// `import a.b.Type;` / `import static a.b.Type.member;` depend on the
    /// file declaring `Type` (a public Java type lives in `Type.java`): an
    /// `Imports` relation to that file's `Module` symbol, scoped to a path
    /// under the package segment `b`. A JDK package is external. A package
    /// wildcard names a directory, not a file, and records nothing.
    fn push_type_import(&mut self, owner: SymbolId, path: &str, loc: Location) {
        let segments: Vec<&str> = path.split('.').map(str::trim).collect();
        let Some(at) = segments
            .iter()
            .position(|s| s.starts_with(|c: char| c.is_ascii_uppercase()))
        else {
            return;
        };
        let external = matches!(segments.first(), Some(&"java" | &"javax"));
        let relation = self.relations.len();
        self.push_relation(owner, RelationKind::Imports, segments[at].to_string(), loc);
        self.relation_targets.push(RelationTarget {
            relation,
            module: at
                .checked_sub(1)
                .map(|i| segments[i].to_string())
                .filter(|m| !m.is_empty()),
            kind: Some(SymbolKind::Module),
            external,
            ..Default::default()
        });
    }
}

/// `extends`/`implements` clauses wrap their `type_identifier`(s) in
/// grammar nodes (`superclass`, `super_interfaces` → `type_list`) with no
/// per-entry field name, so entries are found by kind rather than field.
/// One name per supertype: the last segment of `java.io.Serializable`, and
/// never the type arguments of `Comparable<Money>`.
fn find_type_identifiers(node: Node) -> Vec<Node> {
    let mut out = Vec::new();
    collect_type_identifiers(node, &mut out);
    out
}

fn collect_type_identifiers<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {
    match node.kind() {
        "type_identifier" => out.push(node),
        "type_arguments" | "annotation" | "marker_annotation" => {}
        "scoped_type_identifier" => {
            let mut cursor = node.walk();
            if let Some(last) = node
                .named_children(&mut cursor)
                .filter(|c| c.kind() == "type_identifier")
                .last()
            {
                out.push(last);
            }
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_type_identifiers(child, out);
            }
        }
    }
}

/// Last `identifier` in a possibly-dotted import path
/// (`scoped_identifier` chains), i.e. the imported class/member name.
fn last_identifier(node: Node) -> Option<Node> {
    match node.kind() {
        "identifier" => Some(node),
        "scoped_identifier" => node.child_by_field_name("name").and_then(last_identifier),
        "asterisk" => None,
        _ => {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            children.into_iter().rev().find_map(last_identifier)
        }
    }
}

/// The methods of `java.lang.Object` a subclass can override; every other
/// `Object` method is `final`.
const OBJECT_METHODS: [&str; 5] = ["equals", "hashCode", "toString", "clone", "finalize"];

/// Whether `method`'s modifiers carry `@Override` (or `@java.lang.Override`).
fn has_override_annotation(method: Node, source: &str) -> bool {
    let Some(modifiers) = find_child(method, "modifiers") else {
        return false;
    };
    let mut cursor = modifiers.walk();
    let found = modifiers.children(&mut cursor).any(|m| {
        matches!(m.kind(), "marker_annotation" | "annotation")
            && m.child_by_field_name("name").is_some_and(|n| {
                let name = text(n, source);
                name == "Override" || name == "java.lang.Override"
            })
    });
    found
}
