//! View names as file dependencies.

use mct_core::{Location, RelationKind, RelationTarget, SymbolId, SymbolKind};

use crate::{literal_string_text, Walker};

/// Directories a Laravel application keeps next to `resources/`. The first
/// one on a path marks the application root that `resources/views` lives
/// under, which is what lets a `view('a.b')` in `app/Http/...` name a file.
const APPLICATION_DIRECTORIES: &[&str] = &[
    "app",
    "bootstrap",
    "config",
    "database",
    "public",
    "resources",
    "routes",
    "storage",
    "tests",
];

/// The application root of the file at `relative_path` (forward slashes),
/// `Some("")` for the repository root, `None` when the path is outside every
/// conventional application directory.
pub(crate) fn application_root(relative_path: &str) -> Option<String> {
    let mut segments: Vec<&str> = relative_path.split('/').collect();
    segments.pop()?;
    let at = segments
        .iter()
        .position(|segment| APPLICATION_DIRECTORIES.contains(segment))?;
    Some(segments[..at].join("/"))
}

/// The template file `view_name` (`orders.index`) names for the application
/// the file at `relative_path` belongs to. A package-namespaced view
/// (`mail::message`) or a name that is not a plain dotted path has no file
/// that source alone can name.
pub(crate) fn view_file(relative_path: &str, view_name: &str) -> Option<String> {
    let root = application_root(relative_path)?;
    blade_file(&root, "resources/views", view_name)
}

/// `<root>/<directory>/<dotted.name as a path>.blade.php`.
pub(crate) fn blade_file(root: &str, directory: &str, dotted_name: &str) -> Option<String> {
    let segments: Vec<&str> = dotted_name.split('.').collect();
    let plain = segments.iter().all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    });
    if !plain {
        return None;
    }
    let tail = segments.join("/");
    Some(if root.is_empty() {
        format!("{directory}/{tail}.blade.php")
    } else {
        format!("{root}/{directory}/{tail}.blade.php")
    })
}

/// The name a template's `Module` symbol carries: its file name without the
/// trailing `.php`.
pub(crate) fn module_name_of(file: &str) -> String {
    let name = file.rsplit('/').next().unwrap_or(file);
    name.trim_end_matches(".php").to_string()
}

impl Walker<'_> {
    /// An `Imports` relation from `owner` to the template file `path`, pinned
    /// to that exact path so no other file can ever be its target.
    pub(crate) fn push_template_import(&mut self, owner: SymbolId, path: String, loc: Location) {
        self.push_template_relation(owner, RelationKind::Imports, path, loc);
    }

    /// A relation of `kind` to the template file `path`, pinned to it.
    pub(crate) fn push_template_relation(
        &mut self,
        owner: SymbolId,
        kind: RelationKind,
        path: String,
        loc: Location,
    ) {
        let name = module_name_of(&path);
        let relation = self.relations.len();
        self.push_relation(owner, kind, name, loc);
        if self.relations.len() == relation {
            return;
        }
        self.relation_targets.push(RelationTarget {
            relation,
            path: Some(path),
            kind: Some(SymbolKind::Module),
            ..Default::default()
        });
    }

    /// `view('a.b')`, `View::make('a.b')`, `Route::view('/x', 'a.b')`: the
    /// view named by the string literal `argument`.
    pub(crate) fn push_view_reference(
        &mut self,
        owner: SymbolId,
        argument: tree_sitter::Node,
        loc: Location,
    ) {
        let Some(name) = literal_string_text(argument, self.source) else {
            return;
        };
        if let Some(path) = view_file(self.path, &name) {
            self.push_template_import(owner, path, loc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_application_root_is_whatever_precedes_the_first_application_directory() {
        assert_eq!(
            application_root("app/Http/Controllers/X.php").as_deref(),
            Some("")
        );
        assert_eq!(application_root("routes/web.php").as_deref(), Some(""));
        assert_eq!(
            application_root("apps/shop/app/Http/X.php").as_deref(),
            Some("apps/shop")
        );
        assert_eq!(application_root("src/Billing/X.php"), None);
        assert_eq!(application_root("index.php"), None);
    }

    #[test]
    fn a_dotted_view_name_is_a_path_under_resources_views() {
        assert_eq!(
            view_file("app/Http/Controllers/O.php", "orders.index").as_deref(),
            Some("resources/views/orders/index.blade.php")
        );
        assert_eq!(
            view_file("apps/shop/routes/web.php", "welcome").as_deref(),
            Some("apps/shop/resources/views/welcome.blade.php")
        );
    }

    #[test]
    fn package_namespaced_and_computed_view_names_name_no_file() {
        for name in ["mail::message", "a..b", ".a", "a.", "a b", "a/b", ""] {
            assert_eq!(view_file("app/X.php", name), None, "{name:?}");
        }
        assert_eq!(view_file("src/X.php", "welcome"), None);
    }
}
