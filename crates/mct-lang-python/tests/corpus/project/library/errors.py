"""Exception hierarchy, retry helpers and the audit trail of the library.

Kept dependency-free so every other module can import it.
"""

from __future__ import annotations

import functools
import logging
import time
from collections import deque
from contextlib import contextmanager
from typing import Any, Callable, Deque, Iterator, Optional, TypeVar

log = logging.getLogger("library")

F = TypeVar("F", bound=Callable[..., Any])

_AUDIT: Deque["AuditRecord"] = deque(maxlen=1000)


class LibraryError(Exception):
    """Base class for every error raised by the library package."""

    code = "E_LIBRARY"
    retryable = False

    def __init__(self, message: str, *, detail: Optional[dict[str, Any]] = None) -> None:
        super().__init__(message)
        self.message = message
        self.detail = detail or {}

    def to_dict(self) -> dict[str, Any]:
        payload = {"code": self.code, "message": self.message}
        if self.detail:
            payload["detail"] = dict(self.detail)
        return payload

    def __str__(self) -> str:
        return f"[{self.code}] {self.message}"


class ValidationError(LibraryError, ValueError):
    code = "E_VALIDATION"


class NotFoundError(LibraryError, LookupError):
    code = "E_NOT_FOUND"

    def __init__(self, kind: str, key: object) -> None:
        super().__init__(f"{kind} {key!r} not found", detail={"kind": kind, "key": str(key)})
        self.kind = kind
        self.key = key


class ConflictError(LibraryError):
    code = "E_CONFLICT"
    retryable = True


class LimitExceededError(LibraryError):
    code = "E_LIMIT"

    def __init__(self, what: str, limit: int, actual: int) -> None:
        super().__init__(
            f"{what} limit of {limit} exceeded ({actual})",
            detail={"what": what, "limit": limit, "actual": actual},
        )


class StorageError(LibraryError, OSError):
    code = "E_STORAGE"
    retryable = True


class AuditRecord:
    """One entry of the in-memory audit trail."""

    __slots__ = ("when", "actor", "action", "subject", "ok")

    def __init__(self, actor: str, action: str, subject: str, ok: bool = True) -> None:
        self.when = time.time()
        self.actor = actor
        self.action = action
        self.subject = subject
        self.ok = ok

    def __repr__(self) -> str:
        status = "ok" if self.ok else "FAILED"
        return f"AuditRecord({self.actor} {self.action} {self.subject} {status})"

    def as_line(self) -> str:
        stamp = time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(self.when))
        return f"{stamp}\t{self.actor}\t{self.action}\t{self.subject}\t{int(self.ok)}"


def audit(actor: str, action: str, subject: str, ok: bool = True) -> AuditRecord:
    record = AuditRecord(actor, action, subject, ok)
    _AUDIT.append(record)
    log.debug("audit %r", record)
    return record


def audit_trail(actor: Optional[str] = None, *, failed_only: bool = False) -> list[AuditRecord]:
    records = list(_AUDIT)
    if actor is not None:
        records = [r for r in records if r.actor == actor]
    if failed_only:
        records = [r for r in records if not r.ok]
    return records


def clear_audit() -> int:
    n = len(_AUDIT)
    _AUDIT.clear()
    return n


def retry(attempts: int = 3, *, delay: float = 0.0, backoff: float = 2.0) -> Callable[[F], F]:
    """Retries the decorated callable while it raises a retryable error."""

    def decorator(fn: F) -> F:
        @functools.wraps(fn)
        def wrapper(*args: Any, **kwargs: Any) -> Any:
            wait = delay
            last: Optional[LibraryError] = None
            for attempt in range(1, attempts + 1):
                try:
                    return fn(*args, **kwargs)
                except LibraryError as exc:
                    if not exc.retryable:
                        raise
                    last = exc
                    log.warning("attempt %d/%d of %s failed: %s", attempt, attempts, fn.__name__, exc)
                    if wait:
                        time.sleep(wait)
                        wait *= backoff
            assert last is not None
            raise last

        return wrapper  # type: ignore[return-value]

    return decorator


def audited(action: str) -> Callable[[F], F]:
    """Records every call of the decorated method in the audit trail.

    The actor is taken from ``self.actor`` when the method has one.
    """

    def decorator(fn: F) -> F:
        @functools.wraps(fn)
        def wrapper(self: Any, *args: Any, **kwargs: Any) -> Any:
            actor = getattr(self, "actor", "system")
            subject = str(args[0]) if args else "-"
            try:
                result = fn(self, *args, **kwargs)
            except Exception:
                audit(actor, action, subject, ok=False)
                raise
            audit(actor, action, subject)
            return result

        return wrapper  # type: ignore[return-value]

    return decorator


@contextmanager
def translate_errors(operation: str) -> Iterator[None]:
    """Turns low-level exceptions into :class:`StorageError`."""
    try:
        yield
    except LibraryError:
        raise
    except (OSError, ValueError) as exc:
        raise StorageError(f"{operation} failed: {exc}") from exc


def error_response(exc: BaseException) -> tuple[int, dict[str, Any]]:
    """HTTP status and JSON body for an exception, used by the API layer."""
    if isinstance(exc, NotFoundError):
        return 404, exc.to_dict()
    if isinstance(exc, ValidationError):
        return 422, exc.to_dict()
    if isinstance(exc, (ConflictError, LimitExceededError)):
        return 409, exc.to_dict()
    if isinstance(exc, LibraryError):
        return 500, exc.to_dict()
    log.exception("unexpected error", exc_info=exc)
    return 500, {"code": "E_INTERNAL", "message": "internal error"}


class Timer:
    """Context manager measuring wall-clock time of a block."""

    def __init__(self, label: str, *, threshold: float = 0.5) -> None:
        self.label = label
        self.threshold = threshold
        self.elapsed = 0.0
        self._start = 0.0

    def __enter__(self) -> "Timer":
        self._start = time.perf_counter()
        return self

    def __exit__(self, *exc_info: object) -> bool:
        self.elapsed = time.perf_counter() - self._start
        if self.elapsed > self.threshold:
            log.warning("%s took %.3fs", self.label, self.elapsed)
        return False


class RateLimiter:
    """Token bucket limiting how often a member may place holds."""

    def __init__(self, rate: float, capacity: int) -> None:
        self.rate = rate
        self.capacity = capacity
        self._tokens: dict[int, float] = {}
        self._stamp: dict[int, float] = {}

    def _refill(self, key: int, now: float) -> float:
        last = self._stamp.get(key, now)
        tokens = self._tokens.get(key, float(self.capacity))
        tokens = min(self.capacity, tokens + (now - last) * self.rate)
        self._stamp[key] = now
        self._tokens[key] = tokens
        return tokens

    def allow(self, key: int, *, now: Optional[float] = None) -> bool:
        now = time.monotonic() if now is None else now
        tokens = self._refill(key, now)
        if tokens < 1:
            return False
        self._tokens[key] = tokens - 1
        return True

    def check(self, key: int) -> None:
        if not self.allow(key):
            raise LimitExceededError("hold rate", self.capacity, self.capacity + 1)


def summarize_errors(errors: list[LibraryError]) -> dict[str, int]:
    counts: dict[str, int] = {}
    for err in errors:
        counts[err.code] = counts.get(err.code, 0) + 1
    return dict(sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])))


def format_trail(records: list[AuditRecord], *, limit: int = 50) -> str:
    lines = [r.as_line() for r in records[-limit:]]
    header = f"# {len(lines)} of {len(records)} audit records"
    return "\n".join([header, *lines])


def safe_call(fn: Callable[..., Any], *args: Any, default: Any = None, **kwargs: Any) -> Any:
    """Calls ``fn`` and returns ``default`` instead of raising a LibraryError."""
    try:
        return fn(*args, **kwargs)
    except LibraryError as exc:
        audit("system", "safe_call", getattr(fn, "__name__", "?"), ok=False)
        log.info("safe_call swallowed %s", exc)
        return default


def chain_errors(*errors: LibraryError) -> LibraryError:
    """Links errors so that each one's ``__cause__`` is the previous one."""
    if not errors:
        raise ValueError("chain_errors needs at least one error")
    previous: Optional[LibraryError] = None
    for err in errors:
        if previous is not None:
            err.__cause__ = previous
        previous = err
    return errors[-1]


def is_retryable(exc: BaseException) -> bool:
    return isinstance(exc, LibraryError) and exc.retryable


def walk_causes(exc: BaseException) -> Iterator[BaseException]:
    seen: set[int] = set()
    current: Optional[BaseException] = exc
    while current is not None and id(current) not in seen:
        seen.add(id(current))
        yield current
        current = current.__cause__ or current.__context__


def collect_group(fn: Callable[[], Any]) -> list[LibraryError]:
    """Runs ``fn`` and returns the library errors of an ExceptionGroup it raises."""
    caught: list[LibraryError] = []
    try:
        fn()
    except* LibraryError as group:
        caught.extend(group.exceptions)  # type: ignore[arg-type]
    except* OSError as group:
        caught.extend(StorageError(str(e)) for e in group.exceptions)
    return caught


class ErrorCatalog:
    """Registry of error codes, with a nested entry type."""

    class Entry:
        def __init__(self, code: str, status: int) -> None:
            self.code = code
            self.status = status

        def describe(self) -> str:
            return f"{self.code} -> {self.status}"

    def __init__(self) -> None:
        self.entries = [ErrorCatalog.Entry(cls.code, error_response(cls("x"))[0]) for cls in (ValidationError, ConflictError)]

    def lookup(self, code: str) -> Optional["ErrorCatalog.Entry"]:
        return next((e for e in self.entries if e.code == code), None)
