//! Laravel: the patterns whose meaning is fixed by the framework and
//! readable from source alone, without running the application.
//!
//! Scope (Laravel 11, 12 and 13 application skeletons, from the official
//! documentation; no application was run):
//! - [`routes`]: `Route::get(..., [Controller::class, 'method'])`, invokable
//!   `Controller::class` actions, `Route::controller(...)->group(...)`,
//!   `Route::resource`/`apiResource`/`resources`/`apiResources` with literal
//!   `only`/`except`, and `withRouting(web: __DIR__.'/../routes/web.php')`.
//! - [`views`]: `view('a.b')`, `View::make('a.b')` and `Route::view` as file
//!   dependencies on `resources/views/a/b.blade.php`.
//! - [`blade`]: a Blade template's own dependencies (`@extends`, `@include`,
//!   `@each`, `@component`, `@inject`, `@use`, `<x-...>` components).
//! - [`migrations`]: the `up`/`down` methods of an anonymous migration class.
//! - [`listeners`]: a *possible* entry point hint for a class under
//!   `app/Listeners/` that event discovery could register, shown by
//!   `find_dead_code` without hiding the candidate.
//!
//! Additional static extraction includes named routes, middleware aliases,
//! configuration and translation keys, named commands, explicit bindings,
//! policy registrations and model table references. Unresolved targets are
//! retained as such. See docs/01-architecture/library-support.md for coverage
//! and limits.

pub(crate) mod blade;
mod console;
mod container;
mod eloquent;
mod keys;
pub(crate) mod listeners;
mod macros;
pub(crate) mod migrations;
pub(crate) mod routes;
pub(crate) mod views;

/// Whether `path` is a Blade template.
pub(crate) fn is_blade_template(path: &str) -> bool {
    path.ends_with(".blade.php")
}

/// Whether `name` is usable as a method or view segment: a plain identifier
/// as PHP and Blade spell one.
pub(crate) fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Read a class's direct, resolved base and simple literal property defaults.
impl crate::Walker<'_> {
    fn extends_framework(&self, class: tree_sitter::Node, namespace: &str, name: &str) -> bool {
        self.framework_base(class)
            .is_some_and(|base| base.namespace == namespace && base.name == name)
    }

    fn framework_base(&self, class: tree_sitter::Node) -> Option<crate::ResolvedName> {
        let clause = crate::find_child(class, "base_clause")?;
        let base = crate::named_children(clause)
            .into_iter()
            .find(|node| crate::last_segment(*node).is_some())?;
        self.scope.resolve_class(crate::text(base, self.source))
    }

    fn property_default<'tree>(
        &self,
        class: tree_sitter::Node<'tree>,
        name: &str,
    ) -> Option<tree_sitter::Node<'tree>> {
        let body = class.child_by_field_name("body")?;
        crate::named_children(body)
            .into_iter()
            .filter(|node| node.kind() == "property_declaration")
            .flat_map(crate::named_children)
            .filter(|node| node.kind() == "property_element")
            .find(|node| {
                node.child_by_field_name("name")
                    .is_some_and(|field| crate::text(field, self.source) == name)
            })?
            .child_by_field_name("default_value")
    }
}
