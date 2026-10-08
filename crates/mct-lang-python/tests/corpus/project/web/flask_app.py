"""Flask front end for the lending library (issue #114).

Scenario: a Flask 3.0 app (``flask`` is an external dependency, not in the
corpus) read by ``mct-lang-python``. Owned code: an application factory,
blueprints, route decorators with and without arguments, the HTTP-method
shortcut decorators, error handlers, request hooks, a class-based
``MethodView`` registered through ``add_url_rule``, CLI commands and
template filters. Handlers call into ``library``.

Expected relations: decorators reference their last name (``route``,
``get``, ``errorhandler``…), ``LoanAPI`` extends ``MethodView``, calls and
imports of ``library`` names. ``render_template("books/index.html")`` is
only a call to ``render_template``: no edge to the template is extracted.
"""

from __future__ import annotations

import functools
import logging
from http import HTTPStatus
from typing import Any, Callable, TypeVar

from flask import (
    Blueprint,
    Flask,
    abort,
    current_app,
    g,
    jsonify,
    redirect,
    render_template,
    request,
    session,
    url_for,
)
from flask.views import MethodView

from library.errors import LibraryError, NotFoundError, ValidationError, error_response
from library.models import Book, Genre, classify, describe_book, normalize_isbn
from library.repository import Catalogue, load_fixture
from library.services import LendingService, build_services

log = logging.getLogger(__name__)

F = TypeVar("F", bound=Callable[..., Any])

DEFAULT_PAGE_SIZE = 20
MAX_PAGE_SIZE = 100


# ---------------------------------------------------------------------------
# Helpers and decorators
# ---------------------------------------------------------------------------


def login_required(view: F) -> F:
    """Redirects anonymous visitors to the login page."""

    @functools.wraps(view)
    def guarded(*args: Any, **kwargs: Any) -> Any:
        if g.get("member_id") is None:
            return redirect(url_for("auth.login", next=request.path))
        return view(*args, **kwargs)

    return guarded  # type: ignore[return-value]


def staff_only(role: str = "librarian") -> Callable[[F], F]:
    """Decorator factory: only members holding ``role`` may continue."""

    def decorate(view: F) -> F:
        @functools.wraps(view)
        def checked(*args: Any, **kwargs: Any) -> Any:
            if role not in session.get("roles", ()):
                abort(HTTPStatus.FORBIDDEN)
            return view(*args, **kwargs)

        return checked  # type: ignore[return-value]

    return decorate


def page_args() -> tuple[int, int]:
    """Reads ``?page=`` and ``?size=`` with sane bounds."""
    page = request.args.get("page", default=1, type=int)
    size = request.args.get("size", default=DEFAULT_PAGE_SIZE, type=int)
    return max(page, 1), min(max(size, 1), MAX_PAGE_SIZE)


def lending_service() -> LendingService:
    """One service per request, cached on ``g``."""
    if "lending" not in g:
        g.lending = build_services(current_app.config["DATABASE"])
    return g.lending


def book_payload(book: Book) -> dict[str, Any]:
    return {
        "isbn": book.isbn,
        "title": book.title,
        "genre": classify(book).value,
        "summary": describe_book(book),
    }


# ---------------------------------------------------------------------------
# Blueprints
# ---------------------------------------------------------------------------

auth = Blueprint("auth", __name__, url_prefix="/auth")
catalogue_bp = Blueprint("catalogue", __name__, url_prefix="/books")
api = Blueprint("api", __name__, url_prefix="/api/v1")


@auth.route("/login", methods=["GET", "POST"])
def login():
    if request.method == "POST":
        member_id = request.form.get("member_id", "").strip()
        if not member_id:
            return render_template("login.html", error="Member id required"), 400
        session["member_id"] = member_id
        return redirect(request.args.get("next") or url_for("catalogue.index"))
    return render_template("login.html")


@auth.post("/logout")
def logout():
    session.clear()
    return redirect(url_for("auth.login"))


@auth.before_app_request
def load_member() -> None:
    g.member_id = session.get("member_id")


@catalogue_bp.route("/")
def index():
    page, size = page_args()
    books = lending_service().catalogue.page(page=page, size=size)
    return render_template("books/index.html", books=books, page=page)


@catalogue_bp.route("/<isbn>")
def detail(isbn: str):
    book = lending_service().catalogue.find(normalize_isbn(isbn))
    if book is None:
        abort(404)
    return render_template("books/detail.html", book=book)


@catalogue_bp.get("/genre/<genre>")
def by_genre(genre: str):
    try:
        wanted = Genre[genre.upper()]
    except KeyError:
        abort(404, description=f"Unknown genre {genre!r}")
    books = [b for b in lending_service().catalogue.all() if classify(b) is wanted]
    return render_template("books/index.html", books=books, genre=wanted)


@catalogue_bp.post("/<isbn>/checkout")
@login_required
def checkout(isbn: str):
    loan = lending_service().checkout(g.member_id, normalize_isbn(isbn))
    log.info("checkout %s by %s", loan.copy_id, g.member_id)
    return redirect(url_for("catalogue.detail", isbn=isbn))


@catalogue_bp.post("/<isbn>/return")
@login_required
@staff_only("desk")
def return_copy(isbn: str):
    lending_service().return_copy(normalize_isbn(isbn))
    return redirect(url_for(".detail", isbn=isbn))


@catalogue_bp.app_template_filter("isbn")
def format_isbn(value: str) -> str:
    digits = normalize_isbn(value)
    return "-".join([digits[:3], digits[3:4], digits[4:9], digits[9:12], digits[12:]])


@catalogue_bp.app_context_processor
def inject_genres() -> dict[str, Any]:
    return {"genres": list(Genre)}


# ---------------------------------------------------------------------------
# JSON API: function views and a MethodView
# ---------------------------------------------------------------------------


@api.get("/books")
def list_books():
    page, size = page_args()
    books = lending_service().catalogue.page(page=page, size=size)
    return jsonify(items=[book_payload(b) for b in books], page=page)


@api.route("/books/<isbn>", methods=["GET"])
def get_book(isbn: str):
    book = lending_service().catalogue.find(normalize_isbn(isbn))
    if book is None:
        raise NotFoundError("book", isbn)
    return jsonify(book_payload(book))


class LoanAPI(MethodView):
    """``/api/v1/loans`` and ``/api/v1/loans/<loan_id>``."""

    init_every_request = False
    decorators = [login_required]

    def __init__(self, lending: LendingService | None = None) -> None:
        self.lending = lending

    def _service(self) -> LendingService:
        return self.lending or lending_service()

    def get(self, loan_id: int | None = None):
        if loan_id is None:
            loans = self._service().open_loans(g.member_id)
            return jsonify([loan.as_dict() for loan in loans])
        loan = self._service().loan(loan_id)
        if loan is None:
            abort(404)
        return jsonify(loan.as_dict())

    def post(self):
        body = request.get_json(silent=True) or {}
        isbn = body.get("isbn")
        if not isbn:
            raise ValidationError("isbn is required")
        loan = self._service().checkout(g.member_id, normalize_isbn(isbn))
        return jsonify(loan.as_dict()), HTTPStatus.CREATED

    def delete(self, loan_id: int):
        self._service().return_loan(loan_id)
        return "", HTTPStatus.NO_CONTENT


class GenreAPI(MethodView):
    """Read-only genre listing, registered on the app directly."""

    methods = ["GET"]

    def get(self):
        counts: dict[str, int] = {}
        for book in lending_service().catalogue.all():
            name = classify(book).name
            counts[name] = counts.get(name, 0) + 1
        return jsonify(counts)


def register_api(bp: Blueprint, view: type[MethodView], endpoint: str, url: str) -> None:
    """The documented pattern: one view function, several URL rules."""
    view_func = view.as_view(endpoint)
    bp.add_url_rule(url, defaults={"loan_id": None}, view_func=view_func, methods=["GET"])
    bp.add_url_rule(url, view_func=view_func, methods=["POST"])
    bp.add_url_rule(f"{url}/<int:loan_id>", view_func=view_func, methods=["GET", "DELETE"])


register_api(api, LoanAPI, "loans", "/loans")


# ---------------------------------------------------------------------------
# Application factory
# ---------------------------------------------------------------------------


def create_app(config: dict[str, Any] | None = None) -> Flask:
    app = Flask(__name__, instance_relative_config=True)
    app.config.from_mapping(
        SECRET_KEY="dev",
        DATABASE="library.sqlite3",
        JSON_SORT_KEYS=False,
    )
    if config:
        app.config.update(config)

    app.register_blueprint(auth)
    app.register_blueprint(catalogue_bp)
    app.register_blueprint(api)
    app.add_url_rule("/genres", view_func=GenreAPI.as_view("genres"))

    @app.route("/")
    def home():
        return redirect(url_for("catalogue.index"))

    @app.route("/health")
    def health():
        return {"status": "ok", "books": len(lending_service().catalogue.all())}

    @app.errorhandler(404)
    def not_found(exc):
        if request.path.startswith("/api/"):
            return jsonify(error="not found"), 404
        return render_template("404.html"), 404

    @app.errorhandler(LibraryError)
    def library_error(exc: LibraryError):
        body, status = error_response(exc)
        return jsonify(body), status

    @app.before_request
    def start_timer() -> None:
        g.started = current_app.config.get("CLOCK", lambda: 0.0)()

    @app.after_request
    def add_headers(response):
        response.headers["X-Library"] = "mct-corpus"
        return response

    @app.teardown_appcontext
    def close_services(exc: BaseException | None) -> None:
        lending = g.pop("lending", None)
        if lending is not None:
            lending.close()

    @app.cli.command("seed")
    def seed() -> None:
        """flask --app web.flask_app seed"""
        catalogue: Catalogue = load_fixture(app.config["DATABASE"])
        for book in catalogue.all():
            log.info("seeded %s", describe_book(book))

    @app.shell_context_processor
    def shell_context() -> dict[str, Any]:
        return {"lending_service": lending_service, "Book": Book}

    return app


# ---------------------------------------------------------------------------
# Module-level app for `flask run`, plus a tiny route table check
# ---------------------------------------------------------------------------


app = create_app()


@app.route("/about", endpoint="about_page")
def about():
    return render_template("about.html", version=current_app.config.get("VERSION", "0"))


@app.route("/search", methods=("GET",), strict_slashes=False)
def search():
    query = request.args.get("q", "").strip()
    if not query:
        return redirect(url_for("catalogue.index"))
    hits = [b for b in lending_service().catalogue.all() if query.lower() in b.title.lower()]
    return render_template("books/index.html", books=hits, query=query)


def route_table(flask_app: Flask) -> list[tuple[str, str]]:
    """Every (rule, endpoint) pair, sorted — used by the smoke test."""
    return sorted((rule.rule, rule.endpoint) for rule in flask_app.url_map.iter_rules())


if __name__ == "__main__":
    for rule, endpoint in route_table(app):
        print(f"{rule:40} {endpoint}")
    app.run(debug=True)
