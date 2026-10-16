"""Members whose bodies match but whose decorators or enclosing context differ.

Labeled evaluation corpus for the duplicate report (see ../labels.toon).
Parsed by mct-cli, never imported or run.
"""
import functools


# Case py-ctx-routes: two route handlers registered on different endpoints.
@app.get("/users")
def list_users(request):
    page = int(request.args.get("page", 1))
    size = min(int(request.args.get("size", 20)), 100)
    query = User.query.order_by(User.id)
    items = query.offset((page - 1) * size).limit(size).all()
    return jsonify([item.to_dict() for item in items])


@app.get("/teams")
def list_teams(request):
    page = int(request.args.get("page", 1))
    size = min(int(request.args.get("size", 20)), 100)
    query = Team.query.order_by(Team.id)
    items = query.offset((page - 1) * size).limit(size).all()
    return jsonify([item.to_dict() for item in items])


# Case py-ctx-cache: a memoized copy next to an uncached one.
@functools.lru_cache(maxsize=None)
def exchange_rate_cached(source, target):
    table = load_rate_table()
    if source == target:
        return 1.0
    direct = table.get((source, target))
    if direct is not None:
        return direct
    return 1.0 / table[(target, source)]


def exchange_rate(source, target):
    table = load_rate_table()
    if source == target:
        return 1.0
    direct = table.get((source, target))
    if direct is not None:
        return direct
    return 1.0 / table[(target, source)]


# Case py-ctx-static: a static method copy of a module helper.
def slugify(text):
    lowered = text.strip().lower()
    cleaned = "".join(ch if ch.isalnum() else "-" for ch in lowered)
    while "--" in cleaned:
        cleaned = cleaned.replace("--", "-")
    return cleaned.strip("-")


class Article:
    @staticmethod
    def make_slug(title):
        lowered = title.strip().lower()
        cleaned = "".join(ch if ch.isalnum() else "-" for ch in lowered)
        while "--" in cleaned:
            cleaned = cleaned.replace("--", "-")
        return cleaned.strip("-")
