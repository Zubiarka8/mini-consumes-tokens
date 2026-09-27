"""Persistence for the library: an abstract repository, an in-memory
implementation for tests and a SQLite-backed one for production."""

from __future__ import annotations

import abc
import csv
import io
import json
import sqlite3
import threading
from datetime import datetime
from pathlib import Path
from typing import Generic, Iterable, Iterator, Optional, Protocol, TypeVar

from . import models
from .errors import NotFoundError, StorageError, ConflictError, retry, translate_errors
from .models import Book, Copy, Hold, Loan, Member, Page

E = TypeVar("E")
KeyT = TypeVar("KeyT")


class HasKey(Protocol):
    def key(self) -> object: ...


class Repository(abc.ABC, Generic[KeyT, E]):
    """Keyed store of entities of one type."""

    kind: str = "entity"

    @abc.abstractmethod
    def get(self, key: KeyT) -> Optional[E]:
        raise NotImplementedError

    @abc.abstractmethod
    def put(self, key: KeyT, value: E) -> None:
        raise NotImplementedError

    @abc.abstractmethod
    def delete(self, key: KeyT) -> bool:
        raise NotImplementedError

    @abc.abstractmethod
    def scan(self) -> Iterator[tuple[KeyT, E]]:
        raise NotImplementedError

    def require(self, key: KeyT) -> E:
        value = self.get(key)
        if value is None:
            raise NotFoundError(self.kind, key)
        return value

    def values(self) -> list[E]:
        return [value for _, value in self.scan()]

    def count(self) -> int:
        return sum(1 for _ in self.scan())

    def page(self, offset: int = 0, limit: int = 20) -> Page[E]:
        return Page.of(self.values(), offset, limit)


class MemoryRepository(Repository[KeyT, E]):
    """Dictionary-backed repository with optimistic versioning."""

    def __init__(self, kind: str = "entity") -> None:
        self.kind = kind
        self._data: dict[KeyT, E] = {}
        self._versions: dict[KeyT, int] = {}
        self._lock = threading.RLock()

    def get(self, key: KeyT) -> Optional[E]:
        with self._lock:
            return self._data.get(key)

    def put(self, key: KeyT, value: E) -> None:
        with self._lock:
            self._data[key] = value
            self._versions[key] = self._versions.get(key, 0) + 1

    def put_if_version(self, key: KeyT, value: E, expected: int) -> int:
        with self._lock:
            current = self._versions.get(key, 0)
            if current != expected:
                raise ConflictError(f"{self.kind} {key!r}: expected v{expected}, found v{current}")
            self.put(key, value)
            return current + 1

    def version(self, key: KeyT) -> int:
        return self._versions.get(key, 0)

    def delete(self, key: KeyT) -> bool:
        with self._lock:
            self._versions.pop(key, None)
            return self._data.pop(key, None) is not None

    def scan(self) -> Iterator[tuple[KeyT, E]]:
        with self._lock:
            items = list(self._data.items())
        yield from items


SCHEMA = """
CREATE TABLE IF NOT EXISTS books (
    isbn TEXT PRIMARY KEY,
    body TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS copies (
    barcode TEXT PRIMARY KEY,
    isbn TEXT NOT NULL REFERENCES books(isbn),
    body TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS members (
    member_id INTEGER PRIMARY KEY,
    body TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS loans (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    barcode TEXT NOT NULL,
    member_id INTEGER NOT NULL,
    body TEXT NOT NULL
);
"""


class SqliteRepository(Repository[str, dict]):
    """Stores JSON documents in one SQLite table."""

    def __init__(self, conn: sqlite3.Connection, table: str, key_column: str) -> None:
        if not table.isidentifier() or not key_column.isidentifier():
            raise StorageError(f"unsafe table/column name: {table}.{key_column}")
        self.conn = conn
        self.table = table
        self.key_column = key_column
        self.kind = table.rstrip("s")

    @retry(attempts=3)
    def get(self, key: str) -> Optional[dict]:
        with translate_errors(f"get {self.table}"):
            row = self.conn.execute(
                f"SELECT body FROM {self.table} WHERE {self.key_column} = ?", (key,)
            ).fetchone()
        return json.loads(row[0]) if row else None

    @retry(attempts=3)
    def put(self, key: str, value: dict) -> None:
        body = json.dumps(value, default=_json_default, ensure_ascii=False)
        with translate_errors(f"put {self.table}"), self.conn:
            self.conn.execute(
                f"INSERT OR REPLACE INTO {self.table} ({self.key_column}, body) VALUES (?, ?)",
                (key, body),
            )

    def delete(self, key: str) -> bool:
        with translate_errors(f"delete {self.table}"), self.conn:
            cur = self.conn.execute(f"DELETE FROM {self.table} WHERE {self.key_column} = ?", (key,))
        return cur.rowcount > 0

    def scan(self) -> Iterator[tuple[str, dict]]:
        with translate_errors(f"scan {self.table}"):
            rows = self.conn.execute(f"SELECT {self.key_column}, body FROM {self.table}").fetchall()
        for key, body in rows:
            yield key, json.loads(body)


def _json_default(value: object) -> object:
    if isinstance(value, datetime):
        return value.isoformat()
    if isinstance(value, set):
        return sorted(value)
    if hasattr(value, "value"):
        return value.value  # enums
    raise TypeError(f"cannot serialise {type(value).__name__}")


def open_database(path: Path | str = ":memory:") -> sqlite3.Connection:
    conn = sqlite3.connect(str(path), check_same_thread=False)
    conn.executescript(SCHEMA)
    conn.row_factory = sqlite3.Row
    return conn


class Catalogue:
    """Typed facade over the raw repositories: books, copies, members, loans."""

    def __init__(
        self,
        books: Optional[Repository[str, Book]] = None,
        copies: Optional[Repository[str, Copy]] = None,
        members: Optional[Repository[int, Member]] = None,
    ) -> None:
        self.books = books or MemoryRepository("book")
        self.copies = copies or MemoryRepository("copy")
        self.members = members or MemoryRepository("member")
        self.loans: list[Loan] = []
        self.holds: list[Hold] = []
        self._by_isbn = models.Index(lambda c: c.isbn)

    def add_book(self, book: Book) -> Book:
        self.books.put(book.isbn, book)
        return book

    def add_copy(self, copy: Copy) -> Copy:
        self.books.require(copy.isbn)
        self.copies.put(copy.barcode, copy)
        self._by_isbn.add(copy)
        return copy

    def copies_of(self, isbn: str) -> list[Copy]:
        return [c for c in self._by_isbn.get(isbn) if not c.withdrawn]

    def open_loans(self, member_id: Optional[int] = None) -> list[Loan]:
        return [
            loan
            for loan in self.loans
            if loan.is_open and (member_id is None or loan.member_id == member_id)
        ]

    def loan_for(self, barcode: str) -> Optional[Loan]:
        return next((l for l in self.loans if l.barcode == barcode and l.is_open), None)

    def search(self, query: str, *, limit: int = 20) -> Page[Book]:
        hits = sorted(
            (b for b in self.books.values() if b.matches(query)),
            key=lambda b: (b.title.casefold(), b.isbn),
        )
        return Page.of(hits, 0, limit)

    def export_csv(self) -> str:
        buffer = io.StringIO()
        writer = csv.writer(buffer)
        writer.writerow(["isbn", "title", "authors", "genre", "copies"])
        for book in sorted(self.books.values(), key=lambda b: b.isbn):
            writer.writerow(
                [
                    book.isbn,
                    book.title,
                    ";".join(a.display_name for a in book.authors),
                    book.genre.value,
                    len(self.copies_of(book.isbn)),
                ]
            )
        return buffer.getvalue()

    def import_csv(self, text: str) -> int:
        reader = csv.DictReader(io.StringIO(text))
        imported = 0
        for row in reader:
            book = Book.from_row(row)
            self.add_book(book)
            imported += 1
        return imported


def load_fixture(path: Path) -> Catalogue:
    """Builds a catalogue from a JSON fixture file with books and copies."""
    with translate_errors(f"load {path}"):
        data = json.loads(path.read_text(encoding="utf-8"))
    catalogue = Catalogue()
    for raw in data.get("books", []):
        catalogue.add_book(Book.from_row(raw))
    for raw in data.get("copies", []):
        catalogue.add_copy(Copy(**raw))
    return catalogue


def iter_batches(items: Iterable[E], size: int) -> Iterator[list[E]]:
    batch: list[E] = []
    for item in items:
        batch.append(item)
        if len(batch) >= size:
            yield batch
            batch = []
    if batch:
        yield batch


class UnitOfWork:
    """Collects writes and applies them together, rolling back on error."""

    def __init__(self, catalogue: Catalogue) -> None:
        self.catalogue = catalogue
        self._pending: list[tuple[str, object]] = []
        self.committed = False

    def __enter__(self) -> "UnitOfWork":
        return self

    def __exit__(self, exc_type, exc, tb) -> bool:
        if exc_type is None:
            self.commit()
        else:
            self.rollback()
        return False

    def add(self, kind: str, entity: object) -> None:
        self._pending.append((kind, entity))

    def commit(self) -> None:
        for kind, entity in self._pending:
            match kind:
                case "book":
                    self.catalogue.add_book(entity)  # type: ignore[arg-type]
                case "copy":
                    self.catalogue.add_copy(entity)  # type: ignore[arg-type]
                case "loan":
                    self.catalogue.loans.append(entity)  # type: ignore[arg-type]
                case other:
                    raise StorageError(f"unknown unit-of-work kind {other!r}")
        self._pending.clear()
        self.committed = True

    def rollback(self) -> None:
        self._pending.clear()
