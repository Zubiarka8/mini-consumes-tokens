"""Domain model for the lending library: books, members, loans and holds.

Everything here is plain data plus validation; persistence lives in
``repository`` and business rules in ``services``.
"""

from __future__ import annotations

import enum
import re
from dataclasses import dataclass, field
from datetime import date, datetime, timedelta
from typing import ClassVar, Generic, Iterable, Iterator, Optional, TypeVar

from .errors import ValidationError, LibraryError

ISBN_PATTERN = re.compile(r"^(97[89])?\d{9}[\dX]$")
DEFAULT_LOAN_DAYS = 21
MAX_RENEWALS = 3

T = TypeVar("T")
K = TypeVar("K")


class Genre(enum.Enum):
    """Top-level shelf a book is filed under."""

    FICTION = "fiction"
    NON_FICTION = "non-fiction"
    CHILDREN = "children"
    REFERENCE = "reference"
    PERIODICAL = "periodical"

    @property
    def loanable(self) -> bool:
        return self is not Genre.REFERENCE

    @classmethod
    def parse(cls, text: str) -> "Genre":
        normalized = text.strip().lower().replace("_", "-")
        for member in cls:
            if member.value == normalized:
                return member
        raise ValidationError(f"unknown genre: {text!r}")


class MembershipTier(enum.IntEnum):
    BASIC = 1
    PLUS = 2
    PATRON = 3

    def max_loans(self) -> int:
        return {MembershipTier.BASIC: 3, MembershipTier.PLUS: 8, MembershipTier.PATRON: 20}[self]


def normalize_isbn(raw: str) -> str:
    """Strips hyphens/spaces and upper-cases a trailing ``x``."""
    cleaned = re.sub(r"[\s-]", "", raw).upper()
    if not ISBN_PATTERN.match(cleaned):
        raise ValidationError(f"not an ISBN: {raw!r}")
    return cleaned


def isbn10_checksum(digits: str) -> str:
    total = sum((10 - i) * int(d) for i, d in enumerate(digits[:9]))
    check = (11 - total % 11) % 11
    return "X" if check == 10 else str(check)


def isbn13_checksum(digits: str) -> str:
    total = sum(int(d) * (1 if i % 2 == 0 else 3) for i, d in enumerate(digits[:12]))
    return str((10 - total % 10) % 10)


def is_valid_isbn(raw: str) -> bool:
    try:
        isbn = normalize_isbn(raw)
    except ValidationError:
        return False
    if len(isbn) == 10:
        return isbn10_checksum(isbn) == isbn[-1]
    return isbn13_checksum(isbn) == isbn[-1]


@dataclass(frozen=True)
class Author:
    """A person credited on a book. Names may contain any script."""

    family_name: str
    given_names: str = ""
    born: Optional[int] = None

    @property
    def display_name(self) -> str:
        if not self.given_names:
            return self.family_name
        return f"{self.given_names} {self.family_name}"

    def sort_key(self) -> tuple[str, str]:
        return (self.family_name.casefold(), self.given_names.casefold())


@dataclass
class Book:
    """One title in the catalogue; physical copies are tracked separately."""

    isbn: str
    title: str
    authors: list[Author] = field(default_factory=list)
    genre: Genre = Genre.FICTION
    published: Optional[int] = None
    tags: set[str] = field(default_factory=set)

    registry: ClassVar[dict[str, "Book"]] = {}

    def __post_init__(self) -> None:
        self.isbn = normalize_isbn(self.isbn)
        if not self.title.strip():
            raise ValidationError("book title must not be empty")
        Book.registry[self.isbn] = self

    @property
    def byline(self) -> str:
        names = [a.display_name for a in sorted(self.authors, key=Author.sort_key)]
        if len(names) > 2:
            return f"{names[0]} et al."
        return " & ".join(names)

    def matches(self, query: str) -> bool:
        needle = query.casefold()
        haystacks = [self.title, self.byline, *self.tags]
        return any(needle in h.casefold() for h in haystacks)

    @staticmethod
    def from_row(row: dict[str, str]) -> "Book":
        authors = [Author(*name.split(",", 1)[::-1]) for name in row.get("authors", "").split(";") if name]
        return Book(
            isbn=row["isbn"],
            title=row["title"],
            authors=authors,
            genre=Genre.parse(row.get("genre", "fiction")),
            published=int(row["published"]) if row.get("published") else None,
        )


@dataclass
class Copy:
    """A physical copy of a book, identified by its barcode."""

    barcode: str
    isbn: str
    shelf: str
    condition: str = "good"
    withdrawn: bool = False

    def withdraw(self, reason: str) -> None:
        if self.withdrawn:
            raise LibraryError(f"copy {self.barcode} already withdrawn")
        self.withdrawn = True
        self.condition = f"withdrawn: {reason}"


@dataclass
class Member:
    member_id: int
    name: str
    email: str
    tier: MembershipTier = MembershipTier.BASIC
    joined: date = field(default_factory=date.today)
    blocked_until: Optional[date] = None

    EMAIL_RE: ClassVar[re.Pattern[str]] = re.compile(r"^[^@\s]+@[^@\s]+\.[a-z]{2,}$", re.I)

    def __post_init__(self) -> None:
        if not Member.EMAIL_RE.match(self.email):
            raise ValidationError(f"invalid e-mail for {self.name}: {self.email}")

    def is_blocked(self, today: Optional[date] = None) -> bool:
        today = today or date.today()
        return self.blocked_until is not None and today <= self.blocked_until

    def block(self, days: int, *, reason: str = "overdue") -> date:
        until = date.today() + timedelta(days=days)
        if self.blocked_until is None or until > self.blocked_until:
            self.blocked_until = until
        return self.blocked_until


@dataclass
class Loan:
    barcode: str
    member_id: int
    start: datetime
    due: datetime
    returned: Optional[datetime] = None
    renewals: int = 0

    @classmethod
    def open(cls, barcode: str, member_id: int, days: int = DEFAULT_LOAN_DAYS) -> "Loan":
        now = datetime.now()
        return cls(barcode=barcode, member_id=member_id, start=now, due=now + timedelta(days=days))

    @property
    def is_open(self) -> bool:
        return self.returned is None

    def days_overdue(self, at: Optional[datetime] = None) -> int:
        at = at or datetime.now()
        end = self.returned or at
        return max(0, (end - self.due).days)

    def renew(self, days: int = DEFAULT_LOAN_DAYS) -> None:
        if not self.is_open:
            raise LibraryError("cannot renew a returned loan")
        if self.renewals >= MAX_RENEWALS:
            raise LibraryError(f"loan of {self.barcode} already renewed {self.renewals} times")
        self.renewals += 1
        self.due += timedelta(days=days)

    def close(self, when: Optional[datetime] = None) -> int:
        self.returned = when or datetime.now()
        return self.days_overdue()


@dataclass(order=True)
class Hold:
    """A reservation queue entry; ordered by priority then time."""

    priority: int
    placed: datetime
    member_id: int = field(compare=False)
    isbn: str = field(compare=False)


class Page(Generic[T]):
    """A page of results with the total count, for paginated listings."""

    def __init__(self, items: list[T], total: int, offset: int = 0, limit: int = 20) -> None:
        self.items = items
        self.total = total
        self.offset = offset
        self.limit = limit

    def __iter__(self) -> Iterator[T]:
        return iter(self.items)

    def __len__(self) -> int:
        return len(self.items)

    @property
    def has_next(self) -> bool:
        return self.offset + len(self.items) < self.total

    def map(self, fn) -> "Page":
        return Page([fn(item) for item in self.items], self.total, self.offset, self.limit)

    @classmethod
    def of(cls, items: Iterable[T], offset: int = 0, limit: int = 20) -> "Page[T]":
        everything = list(items)
        return cls(everything[offset : offset + limit], len(everything), offset, limit)


class Index(Generic[K, T]):
    """Tiny multi-map used to look up entities by an arbitrary key."""

    def __init__(self, key) -> None:
        self._key = key
        self._buckets: dict[K, list[T]] = {}

    def add(self, item: T) -> None:
        self._buckets.setdefault(self._key(item), []).append(item)

    def discard(self, item: T) -> None:
        bucket = self._buckets.get(self._key(item), [])
        if item in bucket:
            bucket.remove(item)

    def get(self, key: K) -> list[T]:
        return list(self._buckets.get(key, ()))

    def __contains__(self, key: object) -> bool:
        return bool(self._buckets.get(key))  # type: ignore[arg-type]


def describe_book(book: Book, *, with_tags: bool = False) -> str:
    """One-line description: ``Title — Author (year)``."""
    year = f" ({book.published})" if book.published else ""
    text = f"{book.title} — {book.byline}{year}"
    if with_tags and book.tags:
        text += " [" + ", ".join(sorted(book.tags)) + "]"
    return text


def classify(book: Book) -> str:
    """Shelf code derived from genre and the first author's surname."""
    match book.genre, book.authors:
        case (Genre.REFERENCE, _):
            return "REF"
        case (Genre.PERIODICAL, _):
            return "PER"
        case (genre, [first, *_]):
            prefix = genre.value[:3].upper()
            return f"{prefix}-{first.family_name[:3].upper()}"
        case (genre, []):
            return genre.value[:3].upper()
    return "???"


def café_hours() -> dict[str, str]:
    """Opening hours of the reading café (unicode identifier on purpose)."""
    horaire = {"lun": "9–18", "mar": "9–18", "mer": "9–20"}
    return {jour: heures for jour, heures in horaire.items()}


def overdue_fee(loan: Loan, *, per_day: float = 0.25, cap: float = 10.0) -> float:
    days = loan.days_overdue()
    return min(cap, round(days * per_day, 2))


def group_by_genre(books: Iterable[Book]) -> dict[Genre, list[Book]]:
    groups: dict[Genre, list[Book]] = {}
    for book in books:
        groups.setdefault(book.genre, []).append(book)
    for genre in groups:
        groups[genre].sort(key=lambda b: (b.title.casefold(), b.isbn))
    return groups
