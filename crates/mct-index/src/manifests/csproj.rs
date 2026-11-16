//! .NET (MSBuild) dependency declarations.

use super::ManifestDependency;

/// The `<PackageReference Include="Name" Version="1.0" />` items of a
/// `.csproj`. The version is its `Version` attribute, or else a `<Version>`
/// child element; a central-package-management reference has neither and
/// keeps `None`. A reference inside an XML comment is not a dependency.
/// `<ProjectReference>` and `<Reference>` are not packages.
pub(super) fn parse_csproj(contents: &str) -> Vec<ManifestDependency> {
    let text = strip_comments(contents);
    let mut deps = Vec::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find("<PackageReference") {
        rest = &rest[at + "<PackageReference".len()..];
        // `<PackageReferenceFoo` is another element.
        if !rest.starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/') {
            continue;
        }
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        let self_closing = tag.ends_with('/');
        let name = attribute(tag, "Include").or_else(|| attribute(tag, "Update"));
        if let Some(name) = name.filter(|n| !n.is_empty()) {
            let version = attribute(tag, "Version").or_else(|| {
                if self_closing {
                    return None;
                }
                let body = &rest[end + 1..];
                let body = &body[..body.find("</PackageReference>")?];
                let start = body.find("<Version>")? + "<Version>".len();
                let value = &body[start..start + body[start..].find("</Version>")?];
                Some(value.trim().to_string())
            });
            deps.push(ManifestDependency {
                name,
                version: version.filter(|v| !v.is_empty()),
            });
        }
        rest = &rest[end + 1..];
    }
    deps
}

/// The double- or single-quoted value of `name="…"` in a tag's attributes.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(found) = tag[from..].find(name) {
        let start = from + found;
        from = start + name.len();
        let before_ok = tag[..start].ends_with(|c: char| c.is_whitespace());
        let after = tag[from..].trim_start();
        let Some(after) = after.strip_prefix('=') else {
            continue;
        };
        if !before_ok {
            continue;
        }
        let after = after.trim_start();
        let quote = after.chars().next().filter(|q| *q == '"' || *q == '\'')?;
        let value = &after[1..];
        return value.find(quote).map(|e| value[..e].trim().to_string());
    }
    None
}

fn strip_comments(contents: &str) -> String {
    let mut out = String::with_capacity(contents.len());
    let mut rest = contents;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + "-->".len()..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}
