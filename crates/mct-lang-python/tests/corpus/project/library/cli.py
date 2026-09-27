"""Command-line front end: ``python -m library.cli <command> [options]``.

Every command is a small class with ``configure`` (argparse wiring) and
``run`` (the work), dispatched from :func:`main`.
"""

from __future__ import annotations

import argparse
import json
import os.path
import sys
from pathlib import Path
from typing import Any, Callable, Optional, Sequence

from . import services as svc
from .errors import LibraryError, audit, clear_audit, error_response, safe_call
from .models import Author, Book, Copy, Genre, Member, MembershipTier, is_valid_isbn, normalize_isbn
from .reports import *  # noqa: F403 - re-exported report builders
from .reports import ReportBundle, circulation_report, overdue_report
from .repository import Catalogue, load_fixture, open_database, SqliteRepository as SqlRepo

try:
    import tomllib as toml_reader
except ImportError:  # pragma: no cover - Python < 3.11
    toml_reader = None

EXIT_OK = 0
EXIT_USAGE = 2
EXIT_ERROR = 1

_COMMANDS: dict[str, type["Command"]] = {}


def command(name: str, *aliases: str) -> Callable[[type["Command"]], type["Command"]]:
    """Class decorator registering a command under one or more names."""

    def register(cls: type["Command"]) -> type["Command"]:
        for key in (name, *aliases):
            _COMMANDS[key] = cls
        cls.name = name
        return cls

    return register


class Context:
    """State shared by every command of one invocation."""

    def __init__(self, catalogue: Catalogue, *, out=sys.stdout, verbose: bool = False) -> None:
        self.catalogue = catalogue
        self.out = out
        self.verbose = verbose
        self.lending, self.reminders = svc.build_services(catalogue)

    def echo(self, *parts: Any) -> None:
        print(*parts, file=self.out)

    def debug(self, message: str) -> None:
        if self.verbose:
            self.echo(f"[debug] {message}")

    def emit_json(self, payload: Any) -> None:
        self.echo(json.dumps(payload, ensure_ascii=False, indent=2, default=str))


class Command:
    """Base class of every sub-command."""

    name = "?"
    help = ""

    def configure(self, parser: argparse.ArgumentParser) -> None:
        parser.add_argument("--json", action="store_true", help="machine-readable output")

    def run(self, ctx: Context, args: argparse.Namespace) -> int:
        raise NotImplementedError(self.name)

    def __call__(self, ctx: Context, args: argparse.Namespace) -> int:
        ctx.debug(f"running {self.name}")
        try:
            return self.run(ctx, args)
        except LibraryError as exc:
            status, body = error_response(exc)
            ctx.emit_json(body) if getattr(args, "json", False) else ctx.echo(f"error ({status}): {exc}")
            return EXIT_ERROR


@command("add-book", "ab")
class AddBookCommand(Command):
    help = "register a new title"

    def configure(self, parser: argparse.ArgumentParser) -> None:
        super().configure(parser)
        parser.add_argument("isbn")
        parser.add_argument("title")
        parser.add_argument("--author", action="append", default=[], metavar="FAMILY[,GIVEN]")
        parser.add_argument("--genre", default="fiction", type=Genre.parse)
        parser.add_argument("--year", type=int)

    @staticmethod
    def parse_author(text: str) -> Author:
        family, _, given = text.partition(",")
        return Author(family.strip(), given.strip())

    def run(self, ctx: Context, args: argparse.Namespace) -> int:
        if not is_valid_isbn(args.isbn):
            ctx.echo(f"warning: {args.isbn} has a bad checksum")
        book = Book(
            isbn=normalize_isbn(args.isbn),
            title=args.title,
            authors=[self.parse_author(a) for a in args.author],
            genre=args.genre,
            published=args.year,
        )
        ctx.catalogue.add_book(book)
        audit("cli", "add-book", book.isbn)
        ctx.emit_json({"isbn": book.isbn}) if args.json else ctx.echo(f"added {book.byline}: {book.title}")
        return EXIT_OK


@command("add-copy")
class AddCopyCommand(Command):
    help = "register a physical copy of a title"

    def configure(self, parser: argparse.ArgumentParser) -> None:
        super().configure(parser)
        parser.add_argument("isbn")
        parser.add_argument("barcode")
        parser.add_argument("--shelf", default="IN")

    def run(self, ctx: Context, args: argparse.Namespace) -> int:
        copy = ctx.catalogue.add_copy(Copy(barcode=args.barcode, isbn=normalize_isbn(args.isbn), shelf=args.shelf))
        ctx.echo(f"copy {copy.barcode} on shelf {copy.shelf}")
        return EXIT_OK


@command("join")
class JoinCommand(Command):
    help = "register a member"

    def configure(self, parser: argparse.ArgumentParser) -> None:
        super().configure(parser)
        parser.add_argument("member_id", type=int)
        parser.add_argument("name")
        parser.add_argument("email")
        parser.add_argument("--tier", choices=[t.name.lower() for t in MembershipTier], default="basic")

    def run(self, ctx: Context, args: argparse.Namespace) -> int:
        member = Member(args.member_id, args.name, args.email, tier=MembershipTier[args.tier.upper()])
        ctx.catalogue.members.put(member.member_id, member)
        ctx.echo(f"welcome, {member.name} ({member.tier.name.title()})")
        return EXIT_OK


@command("checkout", "co")
class CheckoutCommand(Command):
    help = "lend a copy of a title to a member"

    def configure(self, parser: argparse.ArgumentParser) -> None:
        super().configure(parser)
        parser.add_argument("member_id", type=int)
        parser.add_argument("isbn", nargs="+")

    def run(self, ctx: Context, args: argparse.Namespace) -> int:
        if len(args.isbn) == 1:
            loan = ctx.lending.checkout(args.member_id, normalize_isbn(args.isbn[0]))
            ctx.echo(f"{loan.barcode} due {loan.due:%Y-%m-%d}")
            return EXIT_OK
        loans, errors = ctx.lending.bulk_checkout(args.member_id, map(normalize_isbn, args.isbn))
        for loan in loans:
            ctx.echo(f"{loan.barcode} due {loan.due:%Y-%m-%d}")
        for err in errors:
            ctx.echo(f"skipped: {err}")
        return EXIT_OK if not errors else EXIT_ERROR


@command("return", "ret")
class ReturnCommand(Command):
    help = "take back a copy"

    def configure(self, parser: argparse.ArgumentParser) -> None:
        super().configure(parser)
        parser.add_argument("barcode")

    def run(self, ctx: Context, args: argparse.Namespace) -> int:
        fee = ctx.lending.return_copy(args.barcode)
        ctx.echo(f"returned; fee {fee:.2f} €" if fee else "returned on time")
        return EXIT_OK


@command("report")
class ReportCommand(Command):
    help = "print a report"

    KINDS: dict[str, Callable[[Context], Report]] = {  # noqa: F405 - Report comes from the wildcard import
        "circulation": lambda ctx: circulation_report(ctx.catalogue),
        "overdue": lambda ctx: overdue_report(ctx.lending, grace_days=0),
        "inventory": lambda ctx: inventory(ctx.catalogue),  # noqa: F405
    }

    def configure(self, parser: argparse.ArgumentParser) -> None:
        super().configure(parser)
        parser.add_argument("kind", choices=[*self.KINDS, "monthly"])
        parser.add_argument("--format", default="text", choices=["text", "markdown", "csv"])

    def run(self, ctx: Context, args: argparse.Namespace) -> int:
        if args.kind == "monthly":
            ctx.echo(ReportBundle.monthly(ctx.lending).render_all(args.format))
            return EXIT_OK
        report = self.KINDS[args.kind](ctx)
        ctx.echo(report.render(args.format))
        return EXIT_OK


@command("remind")
class RemindCommand(Command):
    help = "send overdue reminders"

    def configure(self, parser: argparse.ArgumentParser) -> None:
        super().configure(parser)
        parser.add_argument("--grace", type=int, default=0)

    def run(self, ctx: Context, args: argparse.Namespace) -> int:
        sent = ctx.reminders.run(grace_days=args.grace)
        ctx.echo(f"{sent} reminder(s) sent")
        return EXIT_OK


def load_config(path: Optional[Path]) -> dict[str, Any]:
    """Reads ``library.toml`` (or JSON as a fallback) when present."""
    if path is None or not path.exists():
        return {}
    if path.suffix == ".json" or toml_reader is None:
        return json.loads(path.read_text(encoding="utf-8"))
    with path.open("rb") as fh:
        return toml_reader.load(fh)


def open_catalogue(config: dict[str, Any]) -> Catalogue:
    db = config.get("database")
    if db:
        conn = open_database(os.path.expanduser(db))
        return Catalogue(books=SqlRepo(conn, "books", "isbn"))  # type: ignore[arg-type]
    fixture = config.get("fixture")
    if fixture:
        return load_fixture(Path(fixture))
    return Catalogue()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="library", description="Lending library administration")
    parser.add_argument("--config", type=Path, default=Path("library.toml"))
    parser.add_argument("-v", "--verbose", action="store_true")
    sub = parser.add_subparsers(dest="command", required=True)
    seen: set[type[Command]] = set()
    for name, cls in sorted(_COMMANDS.items()):
        if cls in seen:
            continue
        seen.add(cls)
        aliases = [k for k, v in _COMMANDS.items() if v is cls and k != cls.name]
        cmd_parser = sub.add_parser(cls.name, aliases=aliases, help=cls.help)
        cls().configure(cmd_parser)
    return parser


def main(argv: Optional[Sequence[str]] = None) -> int:
    parser = build_parser()
    try:
        args = parser.parse_args(argv)
    except SystemExit as exc:
        return int(exc.code or EXIT_USAGE)
    config = safe_call(load_config, args.config, default={})
    ctx = Context(open_catalogue(config), verbose=args.verbose)
    handler = _COMMANDS[args.command]()
    status = handler(ctx, args)
    if args.verbose:
        from .errors import audit_trail, format_trail

        ctx.echo(format_trail(audit_trail()))
    clear_audit()
    return status


def repl(ctx: Context, *, prompt: str = "library> ") -> None:
    """Interactive loop reading one command per line."""
    import shlex

    parser = build_parser()
    while True:
        try:
            line = input(prompt)
        except EOFError:
            break
        if not (words := shlex.split(line)):
            continue
        if words[0] in {"quit", "exit"}:
            break
        try:
            args = parser.parse_args(words)
        except SystemExit:
            continue
        _COMMANDS[args.command]()(ctx, args)


if __name__ == "__main__":
    sys.exit(main())
