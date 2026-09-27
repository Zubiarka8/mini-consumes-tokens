"""Read-only reports over the catalogue: circulation statistics, overdue
lists, inventory and a plain-text/Markdown/CSV renderer."""

from __future__ import annotations

import csv
import io
import statistics
from collections import Counter
from dataclasses import dataclass, field
from datetime import datetime, timedelta
from functools import cached_property, lru_cache, reduce, total_ordering
from typing import Any, Callable, ClassVar, Iterable, Iterator, NamedTuple, Sequence, TypedDict

from . import models as m
from .errors import Timer, ValidationError, format_trail, audit_trail
from .models import Book, Genre, Loan, describe_book, group_by_genre, overdue_fee
from .repository import Catalogue
from .services import LendingService

_RENDERERS: dict[str, Callable[["Report"], str]] = {}
_REPORT_COUNT = 0


def renderer(fmt: str) -> Callable[[Callable[["Report"], str]], Callable[["Report"], str]]:
    """Registers a render function for an output format."""

    def register(fn: Callable[["Report"], str]) -> Callable[["Report"], str]:
        if fmt in _RENDERERS:
            raise ValidationError(f"renderer {fmt!r} registered twice")
        _RENDERERS[fmt] = fn
        return fn

    return register


def fmt_name(*parts: str) -> str:
    return "-".join(p.strip().lower() for p in parts if p)


class Row(NamedTuple):
    label: str
    value: float
    note: str = ""


class GenreStats(TypedDict):
    genre: str
    titles: int
    copies: int
    on_loan: int


@total_ordering
@dataclass
class Report:
    """A titled table of rows plus free-form footnotes."""

    title: str
    columns: Sequence[str]
    rows: list[tuple[Any, ...]] = field(default_factory=list)
    notes: list[str] = field(default_factory=list)
    created: datetime = field(default_factory=datetime.now)

    MAX_ROWS: ClassVar[int] = 500

    class Style:
        """Rendering options, nested in the report class."""

        def __init__(self, width: int = 80, *, borders: bool = True) -> None:
            self.width = width
            self.borders = borders

        def rule(self, char: str = "-") -> str:
            return char * self.width if self.borders else ""

        class Palette:
            header = "bold"
            zebra = ("plain", "dim")

            def pick(self, index: int) -> str:
                return self.zebra[index % len(self.zebra)]

    def add(self, *values: Any) -> "Report":
        if len(values) != len(self.columns):
            raise ValidationError(f"row has {len(values)} values, report has {len(self.columns)} columns")
        if len(self.rows) >= self.MAX_ROWS:
            self.notes.append(f"truncated at {self.MAX_ROWS} rows")
            return self
        self.rows.append(tuple(values))
        return self

    def extend(self, rows: Iterable[Sequence[Any]]) -> "Report":
        for row in rows:
            self.add(*row)
        return self

    def __lt__(self, other: "Report") -> bool:
        return (self.created, self.title) < (other.created, other.title)

    @cached_property
    def widths(self) -> list[int]:
        cells = [self.columns, *[[str(v) for v in row] for row in self.rows]]
        return [max(len(str(c[i])) for c in cells) for i in range(len(self.columns))]

    def render(self, fmt: str = "text") -> str:
        try:
            fn = _RENDERERS[fmt]
        except KeyError:
            raise ValidationError(f"unknown report format {fmt!r}; known: {sorted(_RENDERERS)}") from None
        return fn(self)


@renderer("text")
def render_text(report: Report) -> str:
    style = Report.Style(width=sum(report.widths) + 3 * len(report.widths))
    lines = [report.title, style.rule("=")]
    lines.append(" | ".join(c.ljust(w) for c, w in zip(report.columns, report.widths)))
    lines.append(style.rule())
    for row in report.rows:
        lines.append(" | ".join(str(v).ljust(w) for v, w in zip(row, report.widths)))
    lines.extend(f"* {n}" for n in report.notes)
    return "\n".join(lines)


@renderer("markdown")
def render_markdown(report: Report) -> str:
    head = "| " + " | ".join(report.columns) + " |"
    sep = "|" + "|".join("---" for _ in report.columns) + "|"
    body = ["| " + " | ".join(str(v).replace("|", "\\|") for v in row) + " |" for row in report.rows]
    notes = [f"> {n}" for n in report.notes]
    return "\n".join([f"## {report.title}", "", head, sep, *body, "", *notes])


@renderer(fmt_name("CSV"))
def render_csv(report: Report) -> str:
    buffer = io.StringIO()
    writer = csv.writer(buffer)
    writer.writerow(report.columns)
    writer.writerows(report.rows)
    return buffer.getvalue()


def _next_report_number() -> int:
    global _REPORT_COUNT
    _REPORT_COUNT += 1
    return _REPORT_COUNT


def genre_breakdown(catalogue: Catalogue) -> list[GenreStats]:
    on_loan = {l.barcode for l in catalogue.open_loans()}
    stats: list[GenreStats] = []
    for genre, books in group_by_genre(catalogue.books.values()).items():
        copies = [c for b in books for c in catalogue.copies_of(b.isbn)]
        stats.append(
            GenreStats(
                genre=genre.value,
                titles=len(books),
                copies=len(copies),
                on_loan=sum(1 for c in copies if c.barcode in on_loan),
            )
        )
    return sorted(stats, key=lambda s: (-s["on_loan"], s["genre"]))


def circulation_report(catalogue: Catalogue) -> Report:
    report = Report(f"Circulation #{_next_report_number()}", ["genre", "titles", "copies", "on loan", "ratio"])
    for s in genre_breakdown(catalogue):
        ratio = s["on_loan"] / s["copies"] if s["copies"] else 0.0
        report.add(s["genre"], s["titles"], s["copies"], s["on_loan"], f"{ratio:.0%}")
    return report


def overdue_report(lending: LendingService, *, grace_days: int = 0) -> Report:
    report = Report("Overdue loans", ["barcode", "member", "days", "fee"])
    total = 0.0
    with Timer("overdue report"):
        for loan, days in sorted(lending.overdue(grace_days=grace_days), key=lambda pair: -pair[1]):
            fee = overdue_fee(loan)
            total += fee
            report.add(loan.barcode, loan.member_id, days, f"{fee:.2f}")
    report.notes.append(f"total outstanding fees: {total:.2f} €")
    return report


def loan_durations(loans: Iterable[Loan]) -> Iterator[float]:
    for loan in loans:
        if loan.returned is None:
            continue
        yield (loan.returned - loan.start) / timedelta(days=1)


def duration_summary(loans: Sequence[Loan]) -> dict[str, float]:
    durations = list(loan_durations(loans))
    if not durations:
        return {"count": 0, "mean": 0.0, "median": 0.0, "max": 0.0}
    return {
        "count": float(len(durations)),
        "mean": round(statistics.fmean(durations), 2),
        "median": statistics.median(durations),
        "max": max(durations),
    }


@lru_cache(maxsize=256)
def shelf_label(genre: Genre, family_name: str) -> str:
    return f"{genre.value[:3].upper()}-{family_name[:3].upper()}"


def popular_authors(catalogue: Catalogue, *, top: int = 5) -> list[tuple[str, int]]:
    counts = Counter(
        author.display_name
        for loan in catalogue.loans
        for book in [catalogue.books.require(catalogue.copies.require(loan.barcode).isbn)]
        for author in book.authors
    )
    return counts.most_common(top)


def inventory(catalogue: Catalogue) -> Report:
    report = Report("Inventory", ["isbn", "description", "shelf", "copies"])

    class Totals:
        """Running totals, local to this function."""

        def __init__(self) -> None:
            self.titles = 0
            self.copies = 0

        def count(self, n: int) -> None:
            self.titles += 1
            self.copies += n

    totals = Totals()
    for book in sorted(catalogue.books.values(), key=lambda b: b.isbn):
        copies = catalogue.copies_of(book.isbn)
        totals.count(len(copies))
        shelf = shelf_label(book.genre, book.authors[0].family_name) if book.authors else m.classify(book)
        report.add(book.isbn, describe_book(book), shelf, len(copies))
    report.notes.append(f"{totals.titles} titles, {totals.copies} copies")
    return report


def audit_report(actor: str | None = None) -> str:
    return format_trail(audit_trail(actor, failed_only=False))


def total_fees(loans: Iterable[Loan]) -> float:
    return reduce(lambda acc, loan: acc + overdue_fee(loan), loans, 0.0)


def søk(catalogue: Catalogue, spørring: str) -> list[Book]:
    """Norwegian-named search helper: unicode identifiers are valid Python."""
    treff = catalogue.search(spørring, limit=50)
    return [bok for bok in treff if bok.published is None or bok.published > 1900]


def 概要(catalogue: Catalogue) -> dict[str, int]:
    """Summary under a CJK identifier."""
    return {"書籍": catalogue.books.count(), "貸出": len(catalogue.open_loans())}


class ReportBundle:
    """Several reports rendered together, e.g. for the monthly e-mail."""

    def __init__(self, *reports: Report) -> None:
        self.reports = sorted(reports)

    def __iter__(self) -> Iterator[Report]:
        yield from self.reports

    def render_all(self, fmt: str = "markdown") -> str:
        return "\n\n".join(r.render(fmt) for r in self)

    @classmethod
    def monthly(cls, lending: LendingService) -> "ReportBundle":
        catalogue = lending.catalogue
        return cls(
            circulation_report(catalogue),
            overdue_report(lending, grace_days=3),
            inventory(catalogue),
        )


def top_rows(report: Report, n: int, *, key: Callable[[tuple[Any, ...]], Any] | None = None) -> list[Row]:
    rows = sorted(report.rows, key=key or (lambda r: r[0]))[:n]
    return [Row(str(r[0]), float(r[-1]) if str(r[-1]).replace(".", "", 1).isdigit() else 0.0) for r in rows]


def is_periodical(book: m.Book) -> bool:
    return book.genre is m.Genre.PERIODICAL


class ScheduledReport:
    """A report factory plus the weekday it is sent on (0 = Monday)."""

    def __init__(self, name: str, build: Callable[[], Report], weekday: int = 0) -> None:
        self.name = name
        self._build = build
        self._weekday = weekday

    @property
    def weekday(self) -> int:
        return self._weekday

    @weekday.setter
    def weekday(self, value: int) -> None:
        if value not in range(7):
            raise ValidationError(f"weekday must be 0..6, got {value}")
        self._weekday = value

    @weekday.deleter
    def weekday(self) -> None:
        self._weekday = 0

    def due(self, today: datetime) -> bool:
        return today.weekday() == self._weekday

    def run(self, fmt: str = "text") -> str:
        return self._build().render(fmt)
