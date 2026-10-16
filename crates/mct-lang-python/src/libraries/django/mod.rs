//! Django application-pattern coverage through the shared Python parser.

use mct_core::{RelationKind, SymbolKind};

use crate::libraries::{corpus, parent};

#[test]
fn django_models_admin_views_signals_and_urls() {
    use RelationKind::{Calls, Extends, Imports, References};
    use SymbolKind::{Class, Method};
    let c = corpus();
    let app = "web/django_site/lending/";
    let models = &format!("{app}models.py");
    let admin = &format!("{app}admin.py");
    let forms = &format!("{app}forms.py");
    let views = &format!("{app}views.py");
    let signals = &format!("{app}signals.py");
    let urls = &format!("{app}urls.py");
    // Models: dotted base, abstract base chain, inner `Meta` and choices.
    c.relation(models, "TimeStampedModel", Extends, "Model");
    c.relation(models, "BookRecord", Extends, "TimeStampedModel");
    c.relation(models, "Status", Extends, "TextChoices");
    assert_eq!(parent(models, "Status", Class), Some("CopyRecord"));
    let metas = c.symbols_named("Meta");
    for (path, owner) in [
        (models, "TimeStampedModel"),
        (models, "AuthorRecord"),
        (models, "BookRecord"),
        (models, "LoanRecord"),
        (forms, "BookForm"),
    ] {
        assert!(
            metas
                .iter()
                .any(|(p, s)| p == path && s.parent.as_deref() == Some(owner)),
            "{owner}.Meta"
        );
    }
    // Field declarations are calls owned by the model class.
    c.relation(models, "BookRecord", Calls, "CharField");
    c.relation(models, "BookRecord", Calls, "ManyToManyField");
    c.relation(models, "BookRecord", Calls, "as_manager");
    c.relation(models, "is_overdue", References, "property");
    // Admin: class decorator, method decorators.
    c.relation(admin, "BookAdmin", References, "register");
    c.relation(admin, "BookAdmin", Extends, "ModelAdmin");
    c.relation(admin, "copy_count", References, "display");
    c.relation(admin, "mark_lost", References, "action");
    assert_eq!(parent(admin, "mark_lost", Method), Some("BookAdmin"));
    // Class-based views list every mixin as a base.
    for base in [
        "LoginRequiredMixin",
        "PermissionRequiredMixin",
        "CreateView",
    ] {
        c.relation(views, "BookCreateView", Extends, base);
    }
    assert_eq!(parent(views, "get_queryset", Method), Some("BookListView"));
    // Function views under decorator stacks; signal receivers.
    for decorator in ["login_required", "permission_required", "require_POST"] {
        c.relation(views, "waive_fee", References, decorator);
    }
    c.relation(views, "overdue", References, "cache_page");
    c.relation(signals, "mark_copy_on_loan", References, "receiver");
    // `urlpatterns` and the pattern tuples are module-level calls.
    for callee in ["path", "re_path", "include", "as_view"] {
        c.relation(urls, "urls", Calls, callee);
    }
    // The app's modules import each other relatively…
    c.relation(admin, "admin", Imports, "BookRecord");
    c.relation(forms, "forms", Imports, "BookRecord");
    c.relation(views, "views", Imports, "BookForm");
    c.relation(signals, "signals", Imports, "LoanRecord");
    c.relation(urls, "urls", Imports, "checkout");
    c.relation(&format!("{app}apps.py"), "ready", Imports, "signals");
    // …and call into `library/`.
    c.relation(models, "models", Imports, "overdue_fee");
    c.relation(models, "fee", Calls, "overdue_fee");
    c.relation(models, "clean", Calls, "is_valid_isbn");
    c.relation(forms, "clean_isbn", Calls, "normalize_isbn");
    c.relation(views, "checkout", Calls, "build_services");
    c.relation(signals, "audit_book_delete", Calls, "audit");
}
