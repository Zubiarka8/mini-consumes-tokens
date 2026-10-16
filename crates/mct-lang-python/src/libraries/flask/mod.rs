//! Flask application-pattern coverage through the shared Python parser.

use mct_core::{RelationKind, SymbolKind};

use crate::libraries::{corpus, lines, parent};

#[test]
fn flask_routes_hooks_and_method_views() {
    use RelationKind::{Calls, Extends, Imports, References};
    use SymbolKind::{Class, Function, Method};
    let c = corpus();
    let path = "web/flask_app.py";
    // `@bp.route(...)`, `@bp.get(...)`, `@bp.post(...)` reference the
    // decorator's last name from the decorated function.
    c.relation(path, "index", References, "route");
    c.relation(path, "by_genre", References, "get");
    c.relation(path, "logout", References, "post");
    c.relation(path, "format_isbn", References, "app_template_filter");
    // Stacked decorators each reference the function below them.
    for decorator in ["post", "login_required", "staff_only"] {
        c.relation(path, "return_copy", References, decorator);
    }
    // Routes, error handlers and hooks registered inside the app factory
    // are nested functions, not methods, and keep their decorator.
    for (name, decorator) in [
        ("home", "route"),
        ("not_found", "errorhandler"),
        ("start_timer", "before_request"),
        ("close_services", "teardown_appcontext"),
        ("seed", "command"),
    ] {
        assert_eq!(c.symbol(path, name, Function).parent, None, "{name}");
        c.relation(path, name, References, decorator);
    }
    // The decorator factory's inner function is a plain function too.
    c.symbol(path, "checked", Function);
    // `MethodView` subclasses: extends, HTTP verbs as methods.
    c.relation(path, "LoanAPI", Extends, "MethodView");
    assert_eq!(lines(path, "LoanAPI", Class), (209, Some(240)));
    assert_eq!(parent(path, "delete", Method), Some("LoanAPI"));
    c.relation(path, "register_api", Calls, "as_view");
    c.relation(path, "register_api", Calls, "add_url_rule");
    c.relation(path, "create_app", Calls, "register_blueprint");
    // Cross-file into `library/`.
    c.relation(path, "flask_app", Imports, "LendingService");
    c.relation(path, "flask_app", Imports, "normalize_isbn");
    c.relation(path, "detail", Calls, "normalize_isbn");
    c.relation(path, "book_payload", Calls, "classify");
    c.relation(path, "library_error", Calls, "error_response");
}
