//! `LanguageParser` implementation for XAML, via `tree-sitter-xml` (XAML is
//! well-formed XML syntactically — there is no dedicated XAML grammar).
//!
//! Nothing here is known to `mct-core`, `mct-index`, or `mct-mcp-server` —
//! this crate is the entire integration surface for XAML support.
//!
//! An element's `x:Name` (checked first) or plain `Name` attribute names its
//! `Element` symbol, nested under its nearest such ancestor — same shape as
//! `mct-lang-html`/`mct-lang-xml`. Unlike generic XML, XAML *does* have a
//! well-defined cross-file convention: a routed-event attribute
//! (`Click="SaveBtn_Click"`, `Loaded="Window_Loaded"`, ...) names a method in
//! the paired code-behind file (`Foo.xaml` -> `Foo.xaml.cs`). Each such
//! attribute whose value looks like a bare identifier (not a `{Binding ...}`
//! expression) emits a `References` relation to that name — resolved purely
//! by name equality against whatever `mct-lang-csharp` indexed for the
//! code-behind file in the same project, exactly like every other
//! cross-language relation in this project. `EVENT_ATTRIBUTE_NAMES` is a
//! bounded, explicit list (WPF/UWP/WinUI/MAUI/Avalonia routed events), not a
//! generic "any capitalized attribute" heuristic — deliberately, to avoid
//! false positives from style/layout properties.

use mct_core::{
    LanguageParser, Location, MAX_TRAVERSAL_DEPTH, ParseError, ParsedFile, RelationKind, SourceFile, SymbolId,
    SymbolKind, SymbolRecord, SymbolRelation,
};
use tree_sitter::{Node, Parser};

/// Common routed/CLR event attribute names across WPF, UWP/WinUI, MAUI, and
/// Avalonia. Not exhaustive by design — see the module doc.
const EVENT_ATTRIBUTE_NAMES: &[&str] = &[
    "Click",
    "DoubleTapped",
    "Tapped",
    "Checked",
    "Unchecked",
    "CheckedChanged",
    "IsCheckedChanged",
    "Toggled",
    "Loaded",
    "Unloaded",
    "Initialized",
    "SelectionChanged",
    "TextChanged",
    "GotFocus",
    "LostFocus",
    "GotKeyboardFocus",
    "LostKeyboardFocus",
    "MouseEnter",
    "MouseLeave",
    "MouseDown",
    "MouseUp",
    "MouseMove",
    "MouseWheel",
    "PreviewMouseDown",
    "PreviewMouseUp",
    "PreviewMouseMove",
    "KeyDown",
    "KeyUp",
    "PreviewKeyDown",
    "PreviewKeyUp",
    "Drop",
    "DragEnter",
    "DragLeave",
    "DragOver",
    "ValueChanged",
    "Closing",
    "Closed",
    "Opened",
    "Activated",
    "Deactivated",
    "CollectionChanged",
    "PropertyChanged",
];

pub struct XamlParser;

impl LanguageParser for XamlParser {
    fn language_id(&self) -> &'static str {
        "xaml"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["xaml"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        #[allow(clippy::expect_used)]
        // SAFETY: `tree_sitter_xml::LANGUAGE_XML` is a statically linked
        // grammar compiled into this binary; `set_language` only fails on an
        // ABI mismatch between the grammar and this `tree-sitter` version,
        // which Cargo.lock pins at build time — it never depends on the
        // content of an indexed repo.
        parser
            .set_language(&tree_sitter_xml::LANGUAGE_XML.into())
            .expect("tree-sitter-xml grammar is statically valid");

        let tree = parser.parse(&file.contents, None).ok_or_else(|| ParseError::Syntax {
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
        let module_id = walker.push_symbol(module_name, SymbolKind::Module, location(root), None);
        walker.visit_children(root, module_id, None, 0);
        Ok(walker.finish())
    }
}

fn module_name_for(relative_path: &str) -> String {
    relative_path.rsplit('/').next().unwrap_or(relative_path).trim_end_matches(".xaml").to_string()
}

fn first_error(node: Node) -> Option<Node> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = first_error(child) {
            return Some(found);
        }
    }
    None
}

fn location(node: Node) -> Location {
    let start = node.start_position();
    let end = node.end_position();
    Location {
        line: start.row as u32 + 1,
        column: start.column as u32 + 1,
        byte_len: (node.end_byte() - node.start_byte()) as u32,
        end_line: Some(end.row as u32 + 1),
    }
}

fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

/// An `AttValue` node's raw text includes its surrounding quotes (`"Foo"` or
/// `'Foo'`) — this strips exactly one matching pair, if present.
fn unquote(raw: &str) -> &str {
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' || first == b'\'') && first == last {
            return &raw[1..raw.len() - 1];
        }
    }
    raw
}

/// A plain identifier: `Foo_Click`, not `{Binding SaveCommand}` or `1.0` or
/// text containing spaces — the shape a bare code-behind method name has.
fn looks_like_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

struct Walker<'a> {
    source: &'a str,
    symbols: Vec<SymbolRecord>,
    relations: Vec<SymbolRelation>,
    next_id: SymbolId,
}

impl<'a> Walker<'a> {
    fn new(source: &'a str) -> Self {
        Self { source, symbols: Vec::new(), relations: Vec::new(), next_id: 0 }
    }

    fn push_symbol(
        &mut self,
        name: String,
        kind: SymbolKind,
        location: Location,
        parent: Option<String>,
    ) -> SymbolId {
        let id = self.next_id;
        self.next_id += 1;
        self.symbols.push(SymbolRecord { id, name, kind, location, parent });
        id
    }

    fn push_relation(&mut self, from: SymbolId, kind: RelationKind, to_name: String, loc: Location) {
        if to_name.is_empty() {
            return;
        }
        self.relations.push(SymbolRelation { from, kind, to_name, location: loc });
    }

    fn visit_children(&mut self, node: Node, owner: SymbolId, parent_name: Option<&str>, depth: u32) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.visit(child, owner, parent_name, depth + 1);
        }
    }

    fn visit(&mut self, node: Node, owner: SymbolId, parent_name: Option<&str>, depth: u32) {
        if depth >= MAX_TRAVERSAL_DEPTH {
            return;
        }
        match node.kind() {
            "element" => {
                let tag = find_child(node, "STag").or_else(|| find_child(node, "EmptyElemTag"));
                let Some(tag) = tag else {
                    self.visit_children(node, owner, parent_name, depth + 1);
                    return;
                };
                let attrs = collect_attributes(tag, self.source);
                let key = attr(&attrs, "x:Name")
                    .or_else(|| attr(&attrs, "Name"))
                    .filter(|v| !v.is_empty());

                let (new_owner, new_parent) = match key {
                    Some(name) => {
                        let sym_id = self.push_symbol(
                            name.to_string(),
                            SymbolKind::Element,
                            location(tag),
                            parent_name.map(str::to_string),
                        );
                        (sym_id, Some(name.to_string()))
                    }
                    None => (owner, parent_name.map(str::to_string)),
                };

                for (attr_name, value, attr_node) in &attrs {
                    if EVENT_ATTRIBUTE_NAMES.contains(&attr_name.as_str())
                        && looks_like_identifier(value)
                    {
                        self.push_relation(
                            new_owner,
                            RelationKind::References,
                            value.clone(),
                            location(*attr_node),
                        );
                    }
                }

                if let Some(content) = find_child(node, "content") {
                    self.visit_children(content, new_owner, new_parent.as_deref(), depth + 1);
                }
            }
            _ => self.visit_children(node, owner, parent_name, depth + 1),
        }
    }

    fn finish(self) -> ParsedFile {
        ParsedFile { symbols: self.symbols, relations: self.relations }
    }
}

fn find_child<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find(|child| child.kind() == kind);
    found
}

/// `(exact-case attribute name, unquoted value, the attribute's own node)`
/// triples for an `STag`/`EmptyElemTag` — includes namespaced names
/// (`x:Name`) as one token, since that's how `tree-sitter-xml` tokenizes
/// them. The node is returned (not just the value) so a relation built from
/// it can be located at this specific attribute, not the whole tag — two
/// event attributes naming the same handler (`Click="Foo"
/// DoubleClick="Foo"`) would otherwise emit relations sharing one identical
/// location.
fn collect_attributes<'a>(tag: Node<'a>, source: &str) -> Vec<(String, String, Node<'a>)> {
    let mut out = Vec::new();
    let mut cursor = tag.walk();
    for attribute in tag.named_children(&mut cursor) {
        if attribute.kind() != "Attribute" {
            continue;
        }
        let name = find_child(attribute, "Name").map(|n| text(n, source).to_string());
        let value =
            find_child(attribute, "AttValue").map(|n| unquote(text(n, source)).to_string());
        if let (Some(name), Some(value)) = (name, value) {
            out.push((name, value, attribute));
        }
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String, Node<'a>)], key: &str) -> Option<&'a str> {
    attrs.iter().find(|(n, _, _)| n == key).map(|(_, v, _)| v.as_str())
}
