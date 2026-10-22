//! Project statistics derived once from the navigation snapshot (ADR-018 D6).
//!
//! Counting rules, shown next to the numbers in the UI:
//! - a **file** is an indexed file that is not an `asset`, counted under the
//!   language the indexer assigned (a mixed-language file counts once);
//! - a **function** is a `function` symbol with no parent; a **method** is a
//!   `method` symbol, or a `function` symbol that has a parent;
//! - a **class** is a `class` symbol; **types** are `struct`, `enum`, `trait`,
//!   `interface` and `type_alias`; every other kind is **other**;
//! - a directory counts when any counted file lies below it, at any depth.

use std::collections::{BTreeMap, BTreeSet};

use mct_index::NavigationSnapshot;

use super::i18n::{t, Lang};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Bucket {
    Function,
    Method,
    Class,
    Type,
    Other,
}

impl Bucket {
    pub const ALL: [Bucket; 5] = [
        Bucket::Function,
        Bucket::Method,
        Bucket::Class,
        Bucket::Type,
        Bucket::Other,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|bucket| bucket.value() == value)
    }
    pub fn value(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Class => "class",
            Self::Type => "type",
            Self::Other => "other",
        }
    }
    pub fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Function => t(lang, "Funciones"),
            Self::Method => t(lang, "Métodos"),
            Self::Class => t(lang, "Clases"),
            Self::Type => t(lang, "Estructuras, enums, traits e interfaces"),
            Self::Other => t(lang, "Otros símbolos"),
        }
    }
    pub fn short_label(self, lang: Lang) -> &'static str {
        match self {
            Self::Function => t(lang, "Funciones"),
            Self::Method => t(lang, "Métodos"),
            Self::Class => t(lang, "Clases"),
            Self::Type => t(lang, "Tipos"),
            Self::Other => t(lang, "Otros"),
        }
    }
}

pub(super) fn bucket(kind: &str, has_parent: bool) -> Bucket {
    match kind {
        "function" if has_parent => Bucket::Method,
        "function" => Bucket::Function,
        "method" => Bucket::Method,
        "class" => Bucket::Class,
        "struct" | "enum" | "trait" | "interface" | "type_alias" => Bucket::Type,
        _ => Bucket::Other,
    }
}

#[derive(Debug, Default, Clone)]
pub(super) struct Language {
    pub files: usize,
    pub symbols: [usize; 5],
}

impl Language {
    pub fn count(&self, bucket: Bucket) -> usize {
        self.symbols[bucket as usize]
    }
}

#[derive(Debug, Default)]
pub(super) struct Stats {
    pub files: usize,
    pub directories: usize,
    pub symbols: [usize; 5],
    pub imports: usize,
    pub languages: BTreeMap<String, Language>,
}

impl Stats {
    pub fn count(&self, bucket: Bucket) -> usize {
        self.symbols[bucket as usize]
    }

    pub fn total_symbols(&self) -> usize {
        self.symbols.iter().sum()
    }

    /// `navigation.files` must already exclude assets.
    pub fn compute(navigation: &NavigationSnapshot) -> Self {
        let mut stats = Self {
            files: navigation.files.len(),
            imports: navigation.imports.len(),
            ..Self::default()
        };
        let mut directories = BTreeSet::new();
        for file in &navigation.files {
            stats
                .languages
                .entry(file.language.clone())
                .or_default()
                .files += 1;
            let mut path = file.relative_path.as_str();
            while let Some((parent, _)) = path.rsplit_once('/') {
                if !directories.insert(parent) {
                    break;
                }
                path = parent;
            }
        }
        stats.directories = directories.len();
        let listed: BTreeSet<&str> = navigation
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect();
        for symbol in &navigation.symbols {
            if !listed.contains(symbol.relative_path.as_str()) {
                continue;
            }
            let Some(language) = stats.languages.get_mut(&symbol.language) else {
                continue;
            };
            let bucket = bucket(&symbol.kind, symbol.parent.is_some()) as usize;
            language.symbols[bucket] += 1;
            stats.symbols[bucket] += 1;
        }
        stats
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use mct_index::{IndexedFile, SymbolListEntry};

    fn file(path: &str, language: &str) -> IndexedFile {
        IndexedFile {
            relative_path: path.into(),
            language: language.into(),
            content_hash: "h".into(),
        }
    }

    fn symbol(path: &str, language: &str, kind: &str, parent: Option<&str>) -> SymbolListEntry {
        SymbolListEntry {
            name: "n".into(),
            kind: kind.into(),
            language: language.into(),
            relative_path: path.into(),
            line: 1,
            end_line: None,
            parent: parent.map(String::from),
            level: None,
        }
    }

    #[test]
    fn counts_follow_the_documented_rules() {
        let navigation = NavigationSnapshot {
            files: vec![
                file("a/b/c.rs", "rust"),
                file("a/d.rs", "rust"),
                file("x.py", "python"),
            ],
            symbols: vec![
                symbol("a/b/c.rs", "rust", "function", None),
                symbol("a/b/c.rs", "rust", "function", Some("Thing")),
                symbol("a/b/c.rs", "rust", "method", Some("Thing")),
                symbol("a/b/c.rs", "rust", "struct", None),
                symbol("a/d.rs", "rust", "trait", None),
                symbol("a/d.rs", "rust", "module", None),
                symbol("x.py", "python", "class", None),
                symbol("gone.rs", "rust", "function", None),
            ],
            imports: Vec::new(),
        };
        let stats = Stats::compute(&navigation);
        assert_eq!((stats.files, stats.directories), (3, 2));
        let rust = &stats.languages["rust"];
        assert_eq!(rust.files, 2);
        assert_eq!(rust.count(Bucket::Function), 1);
        assert_eq!(rust.count(Bucket::Method), 2);
        assert_eq!(rust.count(Bucket::Type), 2);
        assert_eq!(rust.count(Bucket::Other), 1);
        assert_eq!(stats.languages["python"].count(Bucket::Class), 1);
        assert_eq!(
            stats.total_symbols(),
            7,
            "symbols of unlisted files are skipped"
        );
        assert_eq!(
            Bucket::ALL
                .map(|bucket| stats.count(bucket))
                .iter()
                .sum::<usize>(),
            stats.total_symbols()
        );
    }
}
