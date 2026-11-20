//! Lexical resolution of a path written in source (`import x from
//! "./a.glb"`, `useGLTF("/models/robot.glb")`) to a repository-relative
//! path. Pure: no filesystem access, so any `LanguageParser` can call it.
//! The index then only resolves the result against canonical in-root
//! `files` rows, which covers missing files and symlinks (ADR-003 D3).

/// Resolves `spec`, as written in `from_file` (repository-relative, forward
/// slashes), to a repository-relative path, or `None` when the spec is not
/// a local path this function can prove.
///
/// - `?…` and `#…` suffixes are dropped (`./tex.png?url`).
/// - Empty specs, backslashes, schemes (`http:`, `data:`), drive paths
///   (`C:/`) and protocol-relative `//host` give `None`.
/// - A leading `/` is relative to `<app root>/public/`, where the app root is
///   the prefix before `from_file`'s first `src` segment, or else
///   `from_file`'s own directory (Astro/Vite default `publicDir`).
/// - `./` and `../` are relative to `from_file`'s directory; a `..` that
///   climbs above the repository root gives `None`.
/// - Bare specifiers (`three`, `@/components/X`) give `None`.
pub fn resolve_reference_path(from_file: &str, spec: &str) -> Option<String> {
    match classify(from_file, spec) {
        Lexical::Inside(path) => Some(path),
        Lexical::Outside | Lexical::Unproven => None,
    }
}

/// Whether `spec`, a `./` or `../` path written in `from_file`, climbs
/// above the repository root (`../../other-repo/x.css` from `web/a.html`).
/// Such a reference is `external`: it names a file, but none this index
/// may store a path for (issue #567). A root-relative `/…` never leaves:
/// a browser clamps `..` at the web root.
pub fn reference_leaves_repository(from_file: &str, spec: &str) -> bool {
    matches!(classify(from_file, spec), Lexical::Outside)
}

/// [`reference_leaves_repository`] with the URL reading of
/// [`resolve_url_reference_path`].
pub fn url_reference_leaves_repository(from_file: &str, spec: &str) -> bool {
    reference_leaves_repository(from_file, &url_spec(spec))
}

/// What a written path lexically proves.
enum Lexical {
    /// A repository-relative path.
    Inside(String),
    /// A relative path whose `..` climbs above the repository root.
    Outside,
    /// No local path this module can prove.
    Unproven,
}

fn classify(from_file: &str, spec: &str) -> Lexical {
    let spec = spec.split(['?', '#']).next().unwrap_or_default();
    let first_segment = spec.split('/').next().unwrap_or_default();
    if spec.is_empty() || spec.contains('\\') || first_segment.contains(':') {
        return Lexical::Unproven;
    }
    let dir: Vec<&str> = match from_file.rsplit_once('/') {
        Some((dir, _)) => dir.split('/').collect(),
        None => Vec::new(),
    };
    let root_relative = spec.starts_with('/');
    let (mut base, rest) = if let Some(rest) = spec.strip_prefix('/') {
        if rest.starts_with('/') {
            return Lexical::Unproven;
        }
        let mut base: Vec<&str> = match dir.iter().position(|s| *s == "src") {
            Some(i) => dir[..i].to_vec(),
            None => dir,
        };
        base.push("public");
        (base, rest)
    } else if first_segment == "." || first_segment == ".." {
        (dir, spec)
    } else {
        return Lexical::Unproven;
    };
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if base.pop().is_none() {
                    return if root_relative {
                        Lexical::Unproven
                    } else {
                        Lexical::Outside
                    };
                }
            }
            s => base.push(s),
        }
    }
    let resolved = base.join("/");
    if resolved.is_empty() {
        Lexical::Unproven
    } else {
        Lexical::Inside(resolved)
    }
}

/// [`resolve_reference_path`] for a spec that is a URL reference (an HTML
/// `href`/`src`, a CSS `@import`, a JS/TS module specifier, a XAML `Source`):
/// as a browser or bundler reads it, `\` is a path separator and `%XX`
/// escapes are decoded once, before the shared rules run. A shell `source`,
/// a C/PHP `include` or a PowerShell path is not a URL and keeps the strict
/// rules: there `\` and `%` are literal file-name characters or escapes of
/// that language.
pub fn resolve_url_reference_path(from_file: &str, spec: &str) -> Option<String> {
    resolve_reference_path(from_file, &url_spec(spec))
}

/// A URL spec as a plain path: suffix dropped, `\` as `/`, `%XX` decoded once.
fn url_spec(spec: &str) -> String {
    let spec = spec.split(['?', '#']).next().unwrap_or_default();
    percent_decode(&spec.replace('\\', "/"))
}

/// Decodes each `%XX` (two hex digits) once; any other `%` is kept, and a
/// sequence that decodes to invalid UTF-8 leaves the spec as written.
fn percent_decode(spec: &str) -> String {
    let bytes = spec.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (
            bytes[i],
            bytes.get(i + 1).copied().and_then(hex),
            bytes.get(i + 2).copied().and_then(hex),
        ) {
            (b'%', Some(hi), Some(lo)) => {
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            (b, _, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| spec.to_string())
}

#[cfg(test)]
mod tests {
    use super::resolve_reference_path as r;
    use super::resolve_url_reference_path as url;

    #[test]
    fn relative_paths_resolve_from_the_file_directory() {
        assert_eq!(
            r("src/c/Robot.tsx", "./robot.glb").as_deref(),
            Some("src/c/robot.glb")
        );
        assert_eq!(r("Robot.tsx", "./robot.glb").as_deref(), Some("robot.glb"));
        assert_eq!(
            r("src/c/Robot.tsx", "./a/./b//c.png").as_deref(),
            Some("src/c/a/b/c.png")
        );
    }

    #[test]
    fn leading_slash_resolves_under_the_app_public_dir() {
        assert_eq!(
            r("apps/web/src/x/Scene.tsx", "/models/robot.glb").as_deref(),
            Some("apps/web/public/models/robot.glb")
        );
        assert_eq!(
            r("src/Scene.tsx", "/a.png").as_deref(),
            Some("public/a.png")
        );
        // No `src` segment: the file's own directory is the app root.
        assert_eq!(
            r("site/page.astro", "/a.png").as_deref(),
            Some("site/public/a.png")
        );
        // A segment merely containing "src" is not the `src` segment.
        assert_eq!(
            r("srcs/x.tsx", "/a.png").as_deref(),
            Some("srcs/public/a.png")
        );
    }

    #[test]
    fn query_and_fragment_suffixes_are_stripped() {
        assert_eq!(
            r("src/x.ts", "./tex.png?url").as_deref(),
            Some("src/tex.png")
        );
        assert_eq!(
            r("src/x.ts", "./m.gltf#node").as_deref(),
            Some("src/m.gltf")
        );
        assert_eq!(r("src/x.ts", "?url"), None);
    }

    #[test]
    fn parent_segments_are_allowed_inside_the_root_only() {
        assert_eq!(
            r("src/c/Robot.tsx", "../textures/a.png").as_deref(),
            Some("src/textures/a.png")
        );
        assert_eq!(
            r("src/c/Robot.tsx", "../../a.png").as_deref(),
            Some("a.png")
        );
        assert_eq!(r("src/c/Robot.tsx", "../../../a.png"), None);
        assert_eq!(r("x.tsx", "../a.png"), None);
        assert_eq!(r("src/x.tsx", "/../../a.png"), None);
    }

    #[test]
    fn absolute_drive_and_scheme_paths_are_rejected() {
        for spec in [
            "",
            "C:/models/a.glb",
            "c:a.glb",
            "..\\a.png",
            "C:\\a.png",
            "//cdn.example.com/a.glb",
            "http://example.com/a.glb",
            "https://example.com/a.glb",
            "data:image/png;base64,AAAA",
            "blob:https://example.com/1",
        ] {
            assert_eq!(r("src/x.tsx", spec), None, "{spec}");
        }
    }

    #[test]
    fn bare_specifiers_are_not_paths() {
        for spec in [
            "three",
            "@/components/X",
            "@react-three/drei",
            "a.png",
            ".hidden/a",
        ] {
            assert_eq!(r("src/x.tsx", spec), None, "{spec}");
        }
    }

    #[test]
    fn only_a_relative_path_climbing_above_the_root_leaves_the_repository() {
        use super::{reference_leaves_repository as leaves, url_reference_leaves_repository};
        assert!(leaves("web/a.html", "../../other-repo/x.css"));
        assert!(leaves("a.html", "../x.css"));
        assert!(url_reference_leaves_repository(
            "web/a.html",
            "..\\..\\other\\x.css"
        ));
        assert!(url_reference_leaves_repository(
            "web/a.html",
            "..%2F..%2Fx.css"
        ));
        // Inside the repository, root-relative (clamped at the web root),
        // bare, scheme and home paths do not leave it.
        for spec in [
            "../x.css",
            "./x.css",
            "/../../x.css",
            "x.css",
            "https://h/x",
            "~/x.css",
        ] {
            assert!(!leaves("web/a.html", spec), "{spec}");
        }
        assert_eq!(r("web/a.html", "../../other-repo/x.css"), None);
    }

    #[test]
    fn a_url_is_percent_decoded_once() {
        assert_eq!(
            url("site/index.html", "./my%20file.css").as_deref(),
            Some("site/my file.css")
        );
        // Decoded once: `%2520` is a literal `%20` in the file name.
        assert_eq!(
            url("site/index.html", "./a%2520b.css").as_deref(),
            Some("site/a%20b.css")
        );
        // A `%` with no two hex digits after it is kept as written.
        assert_eq!(
            url("site/index.html", "./100%.css").as_deref(),
            Some("site/100%.css")
        );
        // An encoded separator or parent segment never climbs out.
        assert_eq!(url("index.html", "./%2E%2E/a.css"), None);
        assert_eq!(url("index.html", "..%2Fa.css"), None);
    }

    #[test]
    fn a_url_backslash_is_a_separator() {
        assert_eq!(
            url("site/pages/index.html", "..\\css\\site.css").as_deref(),
            Some("site/css/site.css")
        );
        assert_eq!(
            url("site/index.html", ".\\app.js").as_deref(),
            Some("site/app.js")
        );
        assert_eq!(url("site/index.html", "C:\\a.css"), None);
        assert_eq!(url("site/index.html", "\\\\host\\a.css"), None);
    }

    #[test]
    fn a_plain_path_is_neither_decoded_nor_normalized() {
        assert_eq!(
            r("lib/run.sh", "./my%20file.sh").as_deref(),
            Some("lib/my%20file.sh")
        );
        assert_eq!(r("lib/run.sh", ".\\x.sh"), None);
    }
}
