//! Interface languages for the dashboard (ADR-018 D9).
//!
//! The Spanish text is the lookup key and is returned as is for [`Lang::Es`];
//! [`Lang::En`] looks it up in `catalog.rs`. A missing English entry falls back
//! to the Spanish text, and a test fails the build of the test suite if any
//! literal passed to `t`/`tf` has no entry, so a gap cannot ship unnoticed.
//! Placeholders are `{0}`, `{1}`… and are filled with already escaped text.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

mod catalog;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) enum Lang {
    #[default]
    Es,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Self::Es => "es",
            Self::En => "en",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "es" => Some(Self::Es),
            "en" => Some(Self::En),
            _ => None,
        }
    }

    /// The language to use for a request: the `mct_lang` cookie wins, then the
    /// first language of `Accept-Language`, then Spanish.
    pub fn from_headers(cookie: Option<&str>, accept_language: Option<&str>) -> Self {
        if let Some(lang) = cookie.and_then(|cookie| {
            cookie
                .split(';')
                .filter_map(|part| part.trim().strip_prefix("mct_lang="))
                .find_map(Self::parse)
        }) {
            return lang;
        }
        accept_language
            .and_then(|header| header.split(',').next())
            .map(|first| first.trim().to_ascii_lowercase())
            .and_then(|first| {
                ["es", "en"]
                    .into_iter()
                    .find(|code| first == *code || first.starts_with(&format!("{code}-")))
            })
            .and_then(Self::parse)
            .unwrap_or_default()
    }
}

fn english() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| catalog::ENTRIES.iter().copied().collect())
}

pub(super) fn t(lang: Lang, spanish: &'static str) -> &'static str {
    match lang {
        Lang::Es => spanish,
        Lang::En => english().get(spanish).copied().unwrap_or(spanish),
    }
}

pub(super) fn tf(lang: Lang, spanish: &'static str, args: &[&dyn Display]) -> String {
    let mut text = t(lang, spanish).to_string();
    for (index, arg) in args.iter().enumerate() {
        text = text.replace(&format!("{{{index}}}"), &arg.to_string());
    }
    text
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    const SOURCES: [(&str, &str); 9] = [
        ("render.rs", include_str!("render.rs")),
        ("view_dead.rs", include_str!("view_dead.rs")),
        ("view_duplicates.rs", include_str!("view_duplicates.rs")),
        ("view_obsidian.rs", include_str!("view_obsidian.rs")),
        ("view_reviews.rs", include_str!("view_reviews.rs")),
        ("view_status.rs", include_str!("view_status.rs")),
        ("reviews.rs", include_str!("reviews.rs")),
        ("stats.rs", include_str!("stats.rs")),
        ("../web.rs", include_str!("../web.rs")),
    ];

    /// String literals passed as the text argument of `t(`/`tf(` calls.
    fn used_keys() -> Vec<(String, String)> {
        let mut keys = Vec::new();
        for (file, text) in SOURCES {
            // Only the non-test part of each file.
            let text = text.split("#[cfg(test)]\nmod tests {").next().unwrap();
            for marker in ["t(", "tf("] {
                let mut rest = text;
                while let Some(at) = rest.find(marker) {
                    let before = rest[..at].chars().last();
                    rest = &rest[at + marker.len()..];
                    if before.is_some_and(|ch| ch.is_alphanumeric() || ch == '_') {
                        continue;
                    }
                    // `lang, "..."` or `req.lang, "..."`: only a language argument may precede the text.
                    let Some(quote) = rest.find('"') else {
                        continue;
                    };
                    if !rest[..quote].chars().all(|ch| {
                        ch.is_alphanumeric() || matches!(ch, '_' | '.' | ',') || ch.is_whitespace()
                    }) {
                        continue;
                    }
                    let mut value = String::new();
                    let mut chars = rest[quote + 1..].chars();
                    while let Some(ch) = chars.next() {
                        match ch {
                            '"' => break,
                            '\\' => match chars.next() {
                                Some('n') => value.push('\n'),
                                Some(other) => value.push(other),
                                None => break,
                            },
                            other => value.push(other),
                        }
                    }
                    keys.push((file.to_string(), value));
                }
            }
        }
        keys
    }

    #[test]
    fn every_used_text_has_an_english_entry_and_no_entry_is_stale() {
        let used = used_keys();
        assert!(used.len() > 100, "the scan found only {} texts", used.len());
        let english = english();
        let missing: Vec<_> = used
            .iter()
            .filter(|(_, key)| !english.contains_key(key.as_str()))
            .collect();
        assert!(missing.is_empty(), "no English entry for: {missing:#?}");
        let used: std::collections::HashSet<&str> =
            used.iter().map(|(_, key)| key.as_str()).collect();
        let stale: Vec<_> = catalog::ENTRIES
            .iter()
            .filter(|(key, _)| !used.contains(key))
            .collect();
        assert!(stale.is_empty(), "entries nothing uses: {stale:#?}");
    }

    #[test]
    fn entries_are_unique_translated_and_keep_their_placeholders() {
        let mut seen = std::collections::HashSet::new();
        for (spanish, english) in catalog::ENTRIES {
            assert!(seen.insert(*spanish), "duplicate key {spanish}");
            assert!(
                !english.trim().is_empty() && english != spanish,
                "untranslated {spanish}"
            );
            for index in 0..6 {
                let placeholder = format!("{{{index}}}");
                assert_eq!(
                    spanish.contains(&placeholder),
                    english.contains(&placeholder),
                    "{spanish}"
                );
            }
            let tags = |text: &str| text.matches('<').count();
            assert_eq!(tags(spanish), tags(english), "markup differs in {spanish}");
        }
    }

    #[test]
    fn language_comes_from_the_cookie_then_accept_language_then_spanish() {
        assert_eq!(Lang::from_headers(None, None), Lang::Es);
        assert_eq!(
            Lang::from_headers(None, Some("en-US,en;q=0.9,es;q=0.8")),
            Lang::En
        );
        assert_eq!(Lang::from_headers(None, Some("es-ES,es")), Lang::Es);
        assert_eq!(Lang::from_headers(None, Some("fr-FR,en")), Lang::Es);
        assert_eq!(
            Lang::from_headers(Some("a=b; mct_lang=en"), Some("es")),
            Lang::En
        );
        assert_eq!(
            Lang::from_headers(Some("mct_lang=zz"), Some("en")),
            Lang::En
        );
    }

    #[test]
    fn placeholders_are_filled_and_unknown_text_falls_back() {
        assert_eq!(
            tf(Lang::Es, "Página {0} de {1} · {2} elementos", &[&1, &3, &9]),
            "Página 1 de 3 · 9 elementos"
        );
        assert_eq!(
            tf(Lang::En, "Página {0} de {1} · {2} elementos", &[&1, &3, &9]),
            "Page 1 of 3 · 9 items"
        );
        assert_eq!(t(Lang::En, "texto sin traducir"), "texto sin traducir");
    }
}
