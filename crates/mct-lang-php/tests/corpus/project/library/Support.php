<?php

/**
 * Shelf — shared support code for the lending library: error types, the
 * attribute that tags them with a stable code, small string/money helpers,
 * traits every entity and repository mixes in, a lazy collection and the
 * clock abstraction the services depend on.
 */

declare(strict_types=1);

namespace Shelf\Support;

use Attribute;
use Closure;
use Countable;
use DateInterval;
use DateTimeImmutable;
use DateTimeZone;
use IteratorAggregate;
use RuntimeException;
use Traversable;

const VERSION = '2.4.0';
const DATE_FORMAT = 'Y-m-d';
const DEFAULT_CURRENCY = 'EUR', MAX_PAGE_SIZE = 200;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[Attribute(Attribute::TARGET_CLASS)]
final class ErrorCode
{
    public function __construct(
        public readonly string $code,
        public readonly int $httpStatus = 400,
    ) {
    }
}

#[ErrorCode('shelf.error', 500)]
class ShelfException extends RuntimeException
{
    /** @var array<string, mixed> */
    protected array $context = [];

    public function withContext(string $key, mixed $value): static
    {
        $this->context[$key] = $value;
        return $this;
    }

    public function context(): array
    {
        return $this->context;
    }

    public function code(): string
    {
        $attributes = (new \ReflectionClass($this))->getAttributes(ErrorCode::class);
        if ($attributes === []) {
            return 'shelf.unknown';
        }
        return $attributes[0]->newInstance()->code;
    }
}

#[ErrorCode('shelf.not_found', 404)]
class NotFoundException extends ShelfException
{
    public static function entity(string $type, int|string $id): self
    {
        return (new self(sprintf('%s #%s not found', $type, $id)))
            ->withContext('type', $type)
            ->withContext('id', $id);
    }
}

#[ErrorCode('shelf.invalid', 422)]
class ValidationException extends ShelfException
{
    /** @var array<string, list<string>> */
    private array $errors = [];

    public function __construct(string $message = 'The given data was invalid.', array $errors = [])
    {
        parent::__construct($message);
        foreach ($errors as $field => $error) {
            $this->addError($field, $error);
        }
    }

    public function addError(string $field, string $message): void
    {
        $this->errors[$field][] = $message;
    }

    public function errors(): array
    {
        return $this->errors;
    }

    public function hasErrors(): bool
    {
        return count($this->errors) > 0;
    }

    public function first(): ?string
    {
        foreach ($this->errors as $messages) {
            return $messages[0] ?? null;
        }
        return null;
    }
}

#[ErrorCode('shelf.limit', 409)]
final class LoanLimitExceeded extends ValidationException
{
    public static function forMember(string $email, int $limit): self
    {
        return new self("Member {$email} already has {$limit} open loans.", [
            'loans' => sprintf('limit of %d reached', $limit),
        ]);
    }
}

#[ErrorCode('shelf.unavailable', 409)]
final class BookUnavailable extends ShelfException
{
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function money_format_cents(int $cents, string $currency = DEFAULT_CURRENCY): string
{
    $sign = $cents < 0 ? '-' : '';
    $cents = abs($cents);
    return sprintf('%s%d.%02d %s', $sign, intdiv($cents, 100), $cents % 100, $currency);
}

function slugify(string $text): string
{
    $text = strtolower(trim($text));
    $text = preg_replace('/[^a-z0-9]+/', '-', $text) ?? '';
    return trim($text, '-');
}

function str_limit(string $text, int $limit = 40, string $end = '…'): string
{
    if (mb_strlen($text) <= $limit) {
        return $text;
    }
    return rtrim(mb_substr($text, 0, $limit - mb_strlen($end))) . $end;
}

/**
 * @template T
 * @param iterable<T> $items
 * @return array<array-key, T>
 */
function array_index_by(iterable $items, callable $key): array
{
    $indexed = [];
    foreach ($items as $item) {
        $indexed[$key($item)] = $item;
    }
    return $indexed;
}

function env(string $key, mixed $default = null): mixed
{
    $value = getenv($key);
    if ($value === false) {
        return $default;
    }
    return match (strtolower($value)) {
        'true', '(true)' => true,
        'false', '(false)' => false,
        'null', '(null)' => null,
        default => $value,
    };
}

function retry(int $times, callable $operation, int $sleepMs = 0): mixed
{
    $attempt = 0;
    beginning:
    try {
        return $operation(++$attempt);
    } catch (ShelfException $e) {
        if ($attempt >= $times) {
            throw $e;
        }
        if ($sleepMs > 0) {
            usleep($sleepMs * 1000);
        }
        goto beginning;
    }
}

final class Assert
{
    public static function notBlank(string $value, string $field): string
    {
        if (trim($value) === '') {
            throw new ValidationException(errors: [$field => 'must not be blank']);
        }
        return $value;
    }

    public static function positive(int $value, string $field): int
    {
        if ($value <= 0) {
            throw new ValidationException(errors: [$field => 'must be positive']);
        }
        return $value;
    }

    public static function email(string $value, string $field = 'email'): string
    {
        self::notBlank($value, $field);
        if (filter_var($value, FILTER_VALIDATE_EMAIL) === false) {
            throw new ValidationException(errors: [$field => 'must be a valid address']);
        }
        return strtolower($value);
    }

    public static function inRange(int $value, int $min, int $max, string $field): int
    {
        if ($value < $min || $value > $max) {
            throw new ValidationException(errors: [
                $field => sprintf('must be between %d and %d', $min, $max),
            ]);
        }
        return $value;
    }
}

// ---------------------------------------------------------------------------
// Contracts and traits
// ---------------------------------------------------------------------------

interface Arrayable
{
    public function toArray(): array;
}

interface Identifiable
{
    public function getId(): ?int;
}

trait HasEvents
{
    /** @var list<object> */
    private array $recordedEvents = [];

    protected function recordEvent(object $event): void
    {
        $this->recordedEvents[] = $event;
    }

    /** @return list<object> */
    public function releaseEvents(): array
    {
        [$events, $this->recordedEvents] = [$this->recordedEvents, []];
        return $events;
    }

    public function log(string $message): void
    {
        $this->recordEvent(new LogEntry($message));
    }
}

trait Loggable
{
    private ?Closure $logger = null;

    public function setLogger(callable $logger): void
    {
        $this->logger = Closure::fromCallable($logger);
    }

    public function log(string $message): void
    {
        $this->logger?->__invoke(sprintf('[%s] %s', static::class, $message));
    }
}

trait Auditable
{
    use HasEvents, Loggable {
        Loggable::log insteadof HasEvents;
        HasEvents::log as eventLog;
    }

    public function audit(string $action): void
    {
        $this->log("audit: {$action}");
        $this->eventLog($action);
    }
}

final class LogEntry
{
    public function __construct(public readonly string $message)
    {
    }
}

// ---------------------------------------------------------------------------
// Collection
// ---------------------------------------------------------------------------

/**
 * @template T
 * @implements IteratorAggregate<int, T>
 */
class Collection implements IteratorAggregate, Countable, Arrayable
{
    /** @param array<int, T> $items */
    final public function __construct(private array $items = [])
    {
    }

    public static function of(iterable $items): static
    {
        return new static($items instanceof Traversable ? iterator_to_array($items, false) : array_values($items));
    }

    public function map(callable $fn): static
    {
        return new static(array_map($fn, $this->items));
    }

    public function filter(?callable $fn = null): static
    {
        return new static(array_values(array_filter($this->items, $fn ?? static fn ($x) => (bool) $x)));
    }

    public function reduce(callable $fn, mixed $initial = null): mixed
    {
        return array_reduce($this->items, $fn, $initial);
    }

    public function first(?callable $fn = null): mixed
    {
        foreach ($this->items as $item) {
            if ($fn === null || $fn($item)) {
                return $item;
            }
        }
        return null;
    }

    public function sortBy(callable $key, bool $descending = false): static
    {
        $items = $this->items;
        usort($items, static function ($a, $b) use ($key, $descending): int {
            $order = $key($a) <=> $key($b);
            return $descending ? -$order : $order;
        });
        return new static($items);
    }

    /** @return array<array-key, static> */
    public function groupBy(callable $key): array
    {
        $groups = [];
        foreach ($this->items as $item) {
            $groups[$key($item)][] = $item;
        }
        return array_map(static fn (array $group) => new static($group), $groups);
    }

    public function sum(callable $fn): int
    {
        return $this->reduce(static fn (int $carry, $item) => $carry + $fn($item), 0);
    }

    public function take(int $limit): static
    {
        return new static(array_slice($this->items, 0, min($limit, MAX_PAGE_SIZE)));
    }

    public function each(callable $fn): void
    {
        array_walk($this->items, $fn);
    }

    public function isEmpty(): bool
    {
        return $this->items === [];
    }

    public function count(): int
    {
        return count($this->items);
    }

    public function getIterator(): \Generator
    {
        yield from $this->items;
    }

    public function toArray(): array
    {
        return array_map(
            static fn ($item) => $item instanceof Arrayable ? $item->toArray() : $item,
            $this->items,
        );
    }
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

interface Clock
{
    public function now(): DateTimeImmutable;
}

final class SystemClock implements Clock
{
    public function __construct(private readonly DateTimeZone $zone = new DateTimeZone('UTC'))
    {
    }

    public function now(): DateTimeImmutable
    {
        return new DateTimeImmutable('now', $this->zone);
    }
}

final class FrozenClock implements Clock
{
    private DateTimeImmutable $now;

    public function __construct(string $at = '2024-01-01 09:00:00')
    {
        $this->now = new DateTimeImmutable($at);
    }

    public function now(): DateTimeImmutable
    {
        return $this->now;
    }

    public function advance(int $days): void
    {
        $this->now = $this->now->add(new DateInterval("P{$days}D"));
    }
}

function days_between(DateTimeImmutable $from, DateTimeImmutable $to): int
{
    $diff = $from->diff($to);
    return $diff->invert === 1 ? -(int) $diff->days : (int) $diff->days;
}
