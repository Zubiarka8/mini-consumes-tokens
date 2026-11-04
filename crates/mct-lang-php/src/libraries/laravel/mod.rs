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
//! Everything the framework decides at run time stays unknown: string route
//! actions (`'Controller@method'`), route middleware aliases, policy and
//! listener auto-discovery (a listener is only ever a possible entry point), `config()` and `route('name')` lookups, model
//! table names, container bindings made with closures and package view
//! namespaces. Nothing here is a heuristic over names or folders: a relation
//! is recorded only for a pattern the framework defines, and every one that
//! names a file or class is resolved by the index against what it actually
//! indexed, so a pattern that finds no target stays unresolved.

pub(crate) mod blade;
pub(crate) mod listeners;
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
