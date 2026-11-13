//! Declaration details (ADR-034) read from a JS/TS declaration: its header,
//! parameters, return type, generics, and a class member's visibility and
//! modifiers. A TS annotation is `syntax`; without one, a JSDoc `@param` or
//! `@returns` type naming the same parameter is `doc`. Nothing is inferred.

use mct_core::{
    Modifier, Parameter, ParameterKind, Provenance, SymbolDetail, SymbolId, TypeText, Visibility,
};
use tree_sitter::Node;

/// A function-shaped symbol: `decl` is what declares it (a
/// `function_declaration`, `method_definition`, field or `variable_declarator`)
/// and `function` holds its parameters and body (`decl` itself, or the
/// arrow function / function expression assigned to it).
pub(super) fn function_detail(
    decl: Node,
    function: Node,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let end = function
        .child_by_field_name("body")
        .map_or(function.end_byte(), |b| b.start_byte());
    let prefix = match decl.parent() {
        Some(p) if decl.kind() == "variable_declarator" => p
            .child_by_field_name("kind")
            .map(|k| format!("{} ", slice(k, source)))
            .unwrap_or_default(),
        _ => String::new(),
    };
    let doc = JsDoc::of(decl, source);
    let mut modifiers = modifiers(decl);
    if function.id() != decl.id() {
        modifiers.extend(modifiers_of(function));
    }
    let parameters = match function.child_by_field_name("parameters") {
        Some(list) => {
            let mut cursor = list.walk();
            list.named_children(&mut cursor)
                .filter_map(|p| parameter(p, source, &doc))
                .collect()
        }
        // `x => …`
        None => function
            .child_by_field_name("parameter")
            .and_then(|p| parameter(p, source, &doc))
            .into_iter()
            .collect(),
    };
    SymbolDetail {
        symbol,
        signature: header(source, start(decl), end, &prefix),
        parameters,
        parameters_known: true,
        return_type: function
            .child_by_field_name("return_type")
            .map(|t| TypeText::syntax(annotation(t, source)))
            .or_else(|| doc.returns.clone()),
        visibility: member_visibility(decl, source),
        modifiers,
        generics: function
            .child_by_field_name("type_parameters")
            .or_else(|| decl.child_by_field_name("type_parameters"))
            .map(|g| slice(g, source).to_string()),
    }
}

/// A class, interface, type alias or enum: its header without the body.
pub(super) fn type_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |b| b.start_byte());
    SymbolDetail {
        symbol,
        signature: header(source, start(node), end, ""),
        modifiers: modifiers(node),
        generics: node
            .child_by_field_name("type_parameters")
            .map(|g| slice(g, source).to_string()),
        ..Default::default()
    }
}

/// A class field holding no function: its header without the initializer.
pub(super) fn field_detail(node: Node, symbol: SymbolId, source: &str) -> SymbolDetail {
    let end = node
        .child_by_field_name("value")
        .map_or(node.end_byte(), |v| v.start_byte());
    SymbolDetail {
        symbol,
        signature: header(source, start(node), end, ""),
        visibility: member_visibility(node, source),
        modifiers: modifiers(node),
        ..Default::default()
    }
}

/// A module-level binding (`const MAX: number = 5`): its `keyword` and
/// declarator without the value.
pub(super) fn binding_detail(
    declarator: Node,
    keyword: &str,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let end = declarator
        .child_by_field_name("value")
        .map_or(declarator.end_byte(), |v| v.start_byte());
    SymbolDetail {
        symbol,
        signature: header(source, declarator.start_byte(), end, &format!("{keyword} ")),
        ..Default::default()
    }
}

/// A constructor parameter property (`private readonly store: Store`): its
/// text without the default value, its accessibility keyword (public when
/// only `readonly` is written) and its modifiers.
pub(super) fn parameter_property_detail(
    param: Node,
    symbol: SymbolId,
    source: &str,
) -> SymbolDetail {
    let end = param
        .child_by_field_name("value")
        .map_or(param.end_byte(), |v| v.start_byte());
    SymbolDetail {
        symbol,
        signature: header(source, start(param), end, ""),
        visibility: Some(accessibility(param, source).unwrap_or(Visibility::Public)),
        modifiers: modifiers_of(param),
        ..Default::default()
    }
}

/// Where the declaration's own text starts: after its decorators.
fn start(node: Node) -> usize {
    let mut cursor = node.walk();
    let first = node
        .children(&mut cursor)
        .find(|c| !matches!(c.kind(), "decorator" | "comment"));
    first.map_or(node.start_byte(), |c| c.start_byte())
}

fn header(source: &str, start: usize, end: usize, prefix: &str) -> Option<String> {
    let text = source.get(start..end)?.trim_end();
    let text = text.strip_suffix(';').unwrap_or(text).trim_end();
    let text = text.strip_suffix('=').unwrap_or(text).trim_end();
    (!text.is_empty()).then(|| format!("{prefix}{text}"))
}

/// A class member's visibility: its TS accessibility keyword, `#name`
/// (private), or public by default. `None` outside a class body.
fn member_visibility(decl: Node, source: &str) -> Option<Visibility> {
    if decl.parent().map(|p| p.kind()) != Some("class_body") {
        return None;
    }
    if let Some(visibility) = accessibility(decl, source) {
        return Some(visibility);
    }
    let private_name = decl
        .child_by_field_name("name")
        .or_else(|| decl.child_by_field_name("property"))
        .is_some_and(|n| n.kind() == "private_property_identifier");
    Some(if private_name {
        Visibility::Private
    } else {
        Visibility::Public
    })
}

/// The visibility a TS accessibility keyword on `node` writes, if any.
fn accessibility(node: Node, source: &str) -> Option<Visibility> {
    let mut cursor = node.walk();
    let keyword = node
        .children(&mut cursor)
        .find(|c| c.kind() == "accessibility_modifier")?;
    Some(match slice(keyword, source) {
        "private" => Visibility::Private,
        "protected" => Visibility::Protected,
        _ => Visibility::Public,
    })
}

fn modifiers(node: Node) -> Vec<Modifier> {
    let mut found = modifiers_of(node);
    if node.kind() == "abstract_class_declaration" {
        found.push(Modifier::Abstract);
    }
    found
}

fn modifiers_of(node: Node) -> Vec<Modifier> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter_map(|c| match c.kind() {
            "async" => Some(Modifier::Async),
            "static" => Some(Modifier::Static),
            "readonly" => Some(Modifier::Readonly),
            "abstract" => Some(Modifier::Abstract),
            "override_modifier" | "override" => Some(Modifier::Override),
            "const" => Some(Modifier::Const),
            "*" => Some(Modifier::Generator),
            _ => None,
        })
        .collect()
}

fn parameter(param: Node, source: &str, doc: &JsDoc) -> Option<Parameter> {
    let (pattern, written_type, default) = match param.kind() {
        "required_parameter" | "optional_parameter" => (
            param.child_by_field_name("pattern")?,
            param
                .child_by_field_name("type")
                .map(|t| annotation(t, source)),
            param.child_by_field_name("value"),
        ),
        "assignment_pattern" => (
            param.child_by_field_name("left")?,
            None,
            param.child_by_field_name("right"),
        ),
        "identifier" | "rest_pattern" | "object_pattern" | "array_pattern" => (param, None, None),
        _ => return None,
    };
    let (name, kind) = match pattern.kind() {
        "identifier" => (Some(slice(pattern, source)), ParameterKind::Positional),
        "this" => (Some("this"), ParameterKind::Receiver),
        "rest_pattern" => {
            let mut cursor = pattern.walk();
            let inner = pattern
                .named_children(&mut cursor)
                .find(|c| c.kind() == "identifier")
                .map(|n| slice(n, source));
            (inner, ParameterKind::Variadic)
        }
        _ => (None, ParameterKind::Positional),
    };
    let type_text = written_type
        .map(TypeText::syntax)
        .or_else(|| name.and_then(|n| doc.param(n)));
    Some(Parameter {
        name: name.map(str::to_string),
        type_text,
        default: default.map(|d| slice(d, source).to_string()),
        kind,
    })
}

/// A `type_annotation` without its leading `:`.
fn annotation<'a>(node: Node, source: &'a str) -> &'a str {
    let text = slice(node, source);
    text.strip_prefix(':').unwrap_or(text).trim()
}

/// The `@param`/`@returns` types of the `/** … */` comment right above a
/// declaration (or above the `export`/`const` statement holding it).
#[derive(Default)]
struct JsDoc {
    params: Vec<(String, String)>,
    returns: Option<TypeText>,
}

impl JsDoc {
    fn of(decl: Node, source: &str) -> Self {
        let mut statement = decl;
        if decl.kind() == "variable_declarator" {
            if let Some(parent) = decl.parent() {
                statement = parent;
            }
        }
        if let Some(parent) = statement
            .parent()
            .filter(|p| p.kind() == "export_statement")
        {
            statement = parent;
        }
        let Some(comment) = statement.prev_sibling().filter(|c| {
            c.kind() == "comment"
                && c.end_position().row + 1 >= statement.start_position().row
                && slice(*c, source).starts_with("/**")
        }) else {
            return Self::default();
        };
        Self::parse(slice(comment, source))
    }

    fn parse(comment: &str) -> Self {
        let mut doc = Self::default();
        let mut rest = comment;
        while let Some(at) = rest.find('@') {
            rest = &rest[at + 1..];
            let tag: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
            let after = rest[tag.len()..].trim_start();
            let Some((ty, after)) = braced(after) else {
                continue;
            };
            match tag.as_str() {
                "param" | "arg" | "argument" => {
                    let name: String = after
                        .trim_start()
                        .chars()
                        .take_while(|c| !c.is_whitespace())
                        .collect();
                    let name = name.trim_start_matches('[').trim_start_matches("...");
                    let name = name.split(['=', ']']).next().unwrap_or_default();
                    if !name.is_empty() {
                        doc.params.push((name.to_string(), ty.to_string()));
                    }
                }
                "returns" | "return" if doc.returns.is_none() => {
                    doc.returns = Some(TypeText {
                        text: ty.to_string(),
                        provenance: Provenance::Doc,
                    });
                }
                _ => {}
            }
        }
        doc
    }

    fn param(&self, name: &str) -> Option<TypeText> {
        self.params
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, ty)| TypeText {
                text: ty.clone(),
                provenance: Provenance::Doc,
            })
    }
}

/// `{type} rest` split into the trimmed type and `rest`, braces balanced.
fn braced(text: &str) -> Option<(&str, &str)> {
    let body = text.strip_prefix('{')?;
    let mut depth = 1usize;
    for (i, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let ty = body[..i].trim();
                    return (!ty.is_empty()).then(|| (ty, &body[i + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

/// `node`'s source text, or "" when its range is not on char boundaries.
fn slice<'a>(node: Node, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::JsDoc;

    #[test]
    fn jsdoc_tags_keep_nested_braces_optional_and_rest_names() {
        let doc = JsDoc::parse(
            "/**\n * @param {{ a: {b: 1} }} [opts={}] x\n * @param {...number} ...nums\n * @return {void}\n */",
        );
        assert_eq!(
            doc.param("opts").map(|t| t.text).as_deref(),
            Some("{ a: {b: 1} }")
        );
        assert_eq!(
            doc.param("nums").map(|t| t.text).as_deref(),
            Some("...number")
        );
        assert!(doc.param("x").is_none());
        assert_eq!(doc.returns.map(|t| t.text).as_deref(), Some("void"));
    }
}
