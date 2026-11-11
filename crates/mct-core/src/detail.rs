//! Declaration details a parser can prove from syntax (ADR-034): the
//! signature as written, parameters, return type, visibility, modifiers and
//! generics of a symbol. Sparse: a symbol without a [`SymbolDetail`] has no
//! known details, and a missing field means "unknown", never "none".

use crate::SymbolId;

/// Longest stored signature, in bytes; longer ones are cut on a char boundary.
pub const MAX_SIGNATURE_BYTES: usize = 512;
/// Longest stored type or generic list, in bytes.
pub const MAX_TYPE_BYTES: usize = 256;
/// Longest stored default value, in bytes.
pub const MAX_DEFAULT_BYTES: usize = 64;

/// What the syntax proves about one declaration of the same parsed file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SymbolDetail {
    pub symbol: SymbolId,
    /// The declaration header as written, normalised to one line, without
    /// the body, attributes or doc comment: `pub fn run(&self, n: u8) -> u8`.
    pub signature: Option<String>,
    /// In declaration order. Meaningful only when `parameters_known`.
    pub parameters: Vec<Parameter>,
    /// Whether the parser modelled this symbol's parameters: `false` with an
    /// empty list is "unknown", `true` with an empty list is "none".
    pub parameters_known: bool,
    pub return_type: Option<TypeText>,
    pub visibility: Option<Visibility>,
    /// Sorted and deduplicated by [`SymbolDetail::normalized`].
    pub modifiers: Vec<Modifier>,
    /// The generic parameter list as written (`<T: Clone, const N: usize>`).
    pub generics: Option<String>,
}

impl SymbolDetail {
    /// This detail with every text collapsed to one line and capped, and
    /// modifiers sorted and deduplicated — what the index stores.
    pub fn normalized(mut self) -> Self {
        self.signature = self
            .signature
            .map(|s| cap(&one_line(&s), MAX_SIGNATURE_BYTES));
        self.generics = self.generics.map(|g| cap(&one_line(&g), MAX_TYPE_BYTES));
        self.return_type = self.return_type.map(TypeText::normalized);
        for p in &mut self.parameters {
            p.type_text = p.type_text.take().map(TypeText::normalized);
            p.default = p
                .default
                .take()
                .map(|d| cap(&one_line(&d), MAX_DEFAULT_BYTES));
        }
        self.modifiers.sort();
        self.modifiers.dedup();
        self
    }
}

/// One declared parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// `None` for `_`, a destructuring pattern, or an unnamed prototype slot.
    pub name: Option<String>,
    pub type_text: Option<TypeText>,
    /// The default value's source text, capped.
    pub default: Option<String>,
    pub kind: ParameterKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ParameterKind {
    Positional,
    /// `self`, `this`, a Go method receiver.
    Receiver,
    /// `...args`, `*args`, `params T[]`.
    Variadic,
    KeywordOnly,
    /// `**kwargs`.
    KeywordVariadic,
}

impl ParameterKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Positional => "positional",
            Self::Receiver => "receiver",
            Self::Variadic => "variadic",
            Self::KeywordOnly => "keyword_only",
            Self::KeywordVariadic => "keyword_variadic",
        }
    }
}

/// A type as written in the source, with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeText {
    pub text: String,
    pub provenance: Provenance,
}

impl TypeText {
    /// A type written in the declaration's own syntax.
    pub fn syntax(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            provenance: Provenance::Syntax,
        }
    }

    fn normalized(self) -> Self {
        Self {
            text: cap(&one_line(&self.text), MAX_TYPE_BYTES),
            provenance: self.provenance,
        }
    }
}

/// Where a type came from. No parser may emit an inferred type until an ADR
/// defines how, so there is no variant for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provenance {
    /// The declaration's own syntax (`x: u8`).
    Syntax,
    /// A documentation comment (JSDoc, PHPDoc, a docstring, a Lua annotation).
    Doc,
}

impl Provenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Doc => "doc",
        }
    }
}

/// Language-neutral visibility; each parser documents its mapping in
/// `docs/02-crates/parsers/symbol-coverage.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Visibility {
    Public,
    Package,
    /// Rust `pub(crate)`.
    Crate,
    /// Rust `pub(super)`, `pub(in path)`; C# `protected internal`, `private protected`.
    Restricted,
    Protected,
    Internal,
    Private,
    FilePrivate,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Package => "package",
            Self::Crate => "crate",
            Self::Restricted => "restricted",
            Self::Protected => "protected",
            Self::Internal => "internal",
            Self::Private => "private",
            Self::FilePrivate => "file_private",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Modifier {
    Abstract,
    Async,
    Const,
    Extern,
    Final,
    Generator,
    Inline,
    Open,
    Override,
    Readonly,
    Sealed,
    Static,
    Unsafe,
    Virtual,
}

impl Modifier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Abstract => "abstract",
            Self::Async => "async",
            Self::Const => "const",
            Self::Extern => "extern",
            Self::Final => "final",
            Self::Generator => "generator",
            Self::Inline => "inline",
            Self::Open => "open",
            Self::Override => "override",
            Self::Readonly => "readonly",
            Self::Sealed => "sealed",
            Self::Static => "static",
            Self::Unsafe => "unsafe",
            Self::Virtual => "virtual",
        }
    }
}

/// Whitespace runs (newlines included) collapsed to one space, trimmed.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` cut to at most `max` bytes on a char boundary.
fn cap(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_collapses_lines_caps_on_a_char_boundary_and_sorts_modifiers() {
        let long = "é".repeat(MAX_SIGNATURE_BYTES); // 2 bytes each
        let detail = SymbolDetail {
            signature: Some(format!("pub fn run(\n    a: u8,\n) {long}")),
            modifiers: vec![Modifier::Unsafe, Modifier::Async, Modifier::Unsafe],
            ..Default::default()
        }
        .normalized();
        let signature = detail.signature.unwrap_or_default();
        assert!(
            signature.starts_with("pub fn run( a: u8, ) é"),
            "{signature}"
        );
        assert!(signature.len() <= MAX_SIGNATURE_BYTES);
        assert_eq!(detail.modifiers, [Modifier::Async, Modifier::Unsafe]);
    }
}
