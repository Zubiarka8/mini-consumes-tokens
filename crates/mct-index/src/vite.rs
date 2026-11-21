//! `resolve.alias` of a Vite config, read literally (issue #563, ADR-050).
//!
//! Only the `@` alias is read, since `@/…` is the one alias form the
//! JS/TS plugin records. Its replacement must be a single relative path
//! literal, alone or wrapped in `path.resolve(__dirname, …)`,
//! `path.join(__dirname, …)` or `fileURLToPath(new URL(…, import.meta.url))`.
//! Anything computed is skipped.

/// File names Vite loads its config from.
pub(crate) fn is_vite_config(file_name: &str) -> bool {
    file_name
        .strip_prefix("vite.config.")
        .is_some_and(|ext| matches!(ext, "js" | "ts" | "mjs" | "mts" | "cjs" | "cts"))
}

/// Identifiers allowed around the replacement literal.
const ALLOWED: [&str; 11] = [
    "path",
    "resolve",
    "join",
    "__dirname",
    "fileURLToPath",
    "URL",
    "new",
    "import",
    "meta",
    "url",
    "node",
];

/// The directory the `@` alias names, relative to the config's directory
/// and without `./` or a trailing `/` (`""` for the directory itself).
pub(crate) fn at_alias(contents: &str) -> Option<String> {
    let code = strip_comments(contents);
    let mut from = 0;
    while let Some(at) = code[from..].find("alias") {
        let start = from + at;
        from = start + "alias".len();
        let before = code[..start].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.') {
            continue;
        }
        let rest = code[from..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(body) = balanced(rest) else {
            continue;
        };
        let found = if rest.starts_with('{') {
            object_entry(body)
        } else if rest.starts_with('[') {
            array_entry(body)
        } else {
            None
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// `{ '@': value, … }`: the value of key `@` (or `@/`).
fn object_entry(body: &str) -> Option<String> {
    for entry in split_top_level(body) {
        let Some((key, value)) = split_key(entry) else {
            continue;
        };
        if is_at(key) {
            return replacement(value);
        }
    }
    None
}

/// `[{ find: '@', replacement: value }, …]`.
fn array_entry(body: &str) -> Option<String> {
    for item in split_top_level(body) {
        let item = item.trim();
        let Some(inner) = item.starts_with('{').then(|| balanced(item)).flatten() else {
            continue;
        };
        let mut find = None;
        let mut value = None;
        for field in split_top_level(inner) {
            match split_key(field) {
                Some(("find", v)) => find = Some(v.trim()),
                Some(("replacement", v)) => value = Some(v),
                _ => {}
            }
        }
        if find.is_some_and(is_at) {
            return replacement(value?);
        }
    }
    None
}

fn is_at(key: &str) -> bool {
    let key = key.trim();
    matches!(key, "'@'" | "\"@\"" | "'@/'" | "\"@/\"")
}

/// The single relative path literal of `value`, normalized.
fn replacement(value: &str) -> Option<String> {
    let strings = literals(value)?;
    let [path] = strings.as_slice() else {
        return None;
    };
    let without_strings = remove_strings(value);
    let identifiers_ok = without_strings
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|word| {
            word.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        })
        .all(|word| ALLOWED.contains(&word));
    if !identifiers_ok {
        return None;
    }
    // Vite resolves `/src` from its root, the config's directory by default.
    let path = path.strip_prefix('/').unwrap_or(path);
    let path = path
        .strip_prefix("./")
        .unwrap_or(path)
        .trim_end_matches('/');
    if path.starts_with("..") || path.contains("//") {
        return None;
    }
    Some(if path == "." {
        String::new()
    } else {
        path.to_string()
    })
}

/// The string literals of `text`, or `None` if one is a template with
/// a substitution or is unterminated.
fn literals(text: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if matches!(c, '\'' | '"' | '`') {
            let mut value = String::new();
            loop {
                match chars.next()? {
                    '\\' => value.push(chars.next()?),
                    '$' if c == '`' => return None,
                    end if end == c => break,
                    other => value.push(other),
                }
            }
            out.push(value);
        }
    }
    Some(out)
}

fn remove_strings(text: &str) -> String {
    let mut out = String::new();
    let mut quote = None;
    let mut escaped = false;
    for c in text.chars() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                    out.push(' ');
                }
            }
            None if matches!(c, '\'' | '"' | '`') => quote = Some(c),
            None => out.push(c),
        }
    }
    out
}

/// `key: value` of an object entry; `key` keeps its quotes.
fn split_key(entry: &str) -> Option<(&str, &str)> {
    let entry = entry.trim();
    let mut quote = None;
    for (i, c) in entry.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if matches!(c, '\'' | '"' | '`') => quote = Some(c),
            None if c == ':' => return Some((entry[..i].trim(), &entry[i + 1..])),
            None => {}
        }
    }
    None
}

/// The text inside the bracket `text` starts with, up to its match.
fn balanced(text: &str) -> Option<&str> {
    let open = text.chars().next()?;
    if !matches!(open, '{' | '[' | '(') {
        return None;
    }
    let mut depth = 0_usize;
    let mut quote = None;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => quote = Some(c),
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[1..i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// `body` split on commas outside brackets and strings.
fn split_top_level(body: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0_usize;
    let mut quote = None;
    let mut escaped = false;
    let mut start = 0;
    for (i, c) in body.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => quote = Some(c),
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    parts.into_iter().filter(|p| !p.trim().is_empty()).collect()
}

/// `contents` with `//` and `/* */` comments blanked, strings kept.
fn strip_comments(contents: &str) -> String {
    let mut out = String::with_capacity(contents.len());
    let mut chars = contents.chars().peekable();
    let mut quote = None;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('/', Some('/')) => {
                for next in chars.by_ref() {
                    if next == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = ' ';
                for next in chars.by_ref() {
                    if last == '*' && next == '/' {
                        break;
                    }
                    last = next;
                }
                out.push(' ');
            }
            ('\'' | '"' | '`', _) => {
                quote = Some(c);
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_common_at_alias_forms() {
        for (config, expected) in [
            ("export default { resolve: { alias: { '@': '/src' } } }", "src"),
            (
                "import path from 'path';\nexport default defineConfig({ resolve: { alias: { \"@\": path.resolve(__dirname, './src') } } })",
                "src",
            ),
            (
                "resolve: { alias: { '@': fileURLToPath(new URL('./src/app/', import.meta.url)) } }",
                "src/app",
            ),
            ("resolve: { alias: [{ find: '@', replacement: path.join(__dirname, 'lib') }] }", "lib"),
            ("resolve: { alias: { '~': './x', '@/': './web' } }", "web"),
            ("resolve: { alias: { '@': '.' } }", ""),
        ] {
            assert_eq!(at_alias(config).as_deref(), Some(expected), "{config}");
        }
    }

    #[test]
    fn skips_computed_or_escaping_replacements() {
        for config in [
            "resolve: { alias: { '@': path.resolve(root, 'src') } }",
            "resolve: { alias: { '@': `${dir}/src` } }",
            "resolve: { alias: { '@': '../shared' } }",
            "resolve: { alias: { '~': './src' } }",
            "// resolve: { alias: { '@': './src' } }",
            "resolve: { alias: aliases }",
        ] {
            assert_eq!(at_alias(config), None, "{config}");
        }
    }

    #[test]
    fn knows_vite_config_names() {
        assert!(is_vite_config("vite.config.ts"));
        assert!(is_vite_config("vite.config.mjs"));
        assert!(!is_vite_config("vitest.config.ts"));
        assert!(!is_vite_config("vite.config.json"));
    }
}
