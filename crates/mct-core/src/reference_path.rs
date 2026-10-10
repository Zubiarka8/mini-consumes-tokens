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
    let spec = spec.split(['?', '#']).next().unwrap_or_default();
    let first_segment = spec.split('/').next().unwrap_or_default();
    if spec.is_empty() || spec.contains('\\') || first_segment.contains(':') {
        return None;
    }
    let dir: Vec<&str> = match from_file.rsplit_once('/') {
        Some((dir, _)) => dir.split('/').collect(),
        None => Vec::new(),
    };
    let (mut base, rest) = if let Some(rest) = spec.strip_prefix('/') {
        if rest.starts_with('/') {
            return None;
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
        return None;
    };
    for segment in rest.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                base.pop()?;
            }
            s => base.push(s),
        }
    }
    let resolved = base.join("/");
    (!resolved.is_empty()).then_some(resolved)
}

#[cfg(test)]
mod tests {
    use super::resolve_reference_path as r;

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
}
