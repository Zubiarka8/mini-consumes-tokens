//! Deterministic, cross-platform note paths for generated vault content.

use std::collections::{BTreeMap, BTreeSet};

const FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*', '[', ']', '#'];

fn component(value: &str) -> String {
    let mut cleaned: String = value
        .chars()
        .map(|ch| {
            if ch.is_control() || FORBIDDEN.contains(&ch) {
                '_'
            } else {
                ch
            }
        })
        .collect();
    cleaned = cleaned.trim_end_matches([' ', '.']).to_string();
    if cleaned.is_empty() || cleaned == "." || cleaned == ".." {
        cleaned = "_".to_string();
    }

    let device_name = cleaned
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    let reserved = matches!(device_name.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            device_name.strip_prefix(prefix).is_some_and(|suffix| {
                suffix.len() == 1 && matches!(suffix.as_bytes()[0], b'1'..=b'9')
            })
        });
    if reserved {
        cleaned.insert(0, '_');
    }
    cleaned
}

fn safe_relative(path: &str) -> String {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(component)
        .collect::<Vec<_>>()
        .join("/")
}

fn note_path(folder: &str, source: &str) -> String {
    // A bounded ASCII label plus the exact source path's digest avoids case,
    // normalization, component/file conflicts and platform path-length limits.
    let label: String = safe_relative(source)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    format!(
        "{folder}/{}_{}.md",
        label.trim_start_matches('.').trim_end_matches([' ', '.']),
        crate::hash_note(source)
    )
}

/// Maps source paths to safe note paths under `files/`. Sorting before
/// collision resolution makes the result stable across filesystems.
pub(crate) fn file_note_paths<'a>(
    paths: impl IntoIterator<Item = &'a str>,
) -> BTreeMap<String, String> {
    let mut paths: Vec<&str> = paths.into_iter().collect();
    paths.sort_unstable();
    paths.dedup();

    paths
        .into_iter()
        .map(|source| (source.to_string(), note_path("files", source)))
        .collect()
}

/// Maps every non-root source directory to a safe note path under
/// `directories/`.
pub(crate) fn directory_note_paths<'a>(
    paths: impl IntoIterator<Item = &'a str>,
) -> BTreeMap<String, String> {
    let mut directories = BTreeSet::new();
    for path in paths {
        let mut prefix = String::new();
        let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            directories.insert(prefix.clone());
        }
    }

    directories
        .into_iter()
        .map(|source| {
            let note = note_path("directories", &source);
            (source, note)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_reserved_unicode_and_case_only_paths_are_safe_and_stable() {
        let paths = [
            "CON.rs",
            "con.rs",
            "AUX.txt",
            "com1",
            "LPT9",
            "a. ",
            "a<>:\\|?*[x]#.rs",
            "../../x.rs",
            "/absolute",
            "A/a.rs",
            "a/A.rs",
            "a.md/child.rs",
            "a/child.rs",
            "é.rs",
            "é.rs",
            "COM¹.txt",
            ".hidden.rs",
        ];
        let notes = file_note_paths(paths);
        let mut seen = BTreeSet::new();
        for (source, note) in &notes {
            assert!(note.starts_with("files/"));
            let name = note.trim_start_matches("files/");
            assert!(name.is_ascii() && name.len() < 160);
            assert!(!name.starts_with('.'));
            assert!(!name.chars().any(|ch| FORBIDDEN.contains(&ch)));
            assert!(seen.insert(note.to_lowercase()));
            assert_eq!(file_note_paths([source.as_str()]).get(source), Some(note));
        }
        assert_eq!(component("NUL.txt"), "_NUL.txt");
        assert_eq!(component("COM9"), "_COM9");
        assert_eq!(component("bad. "), "bad");
        let long = "長".repeat(400);
        assert!(note_path("files", &long).len() < 170);
        assert_eq!(notes, file_note_paths(paths.into_iter().rev()));
    }
}
