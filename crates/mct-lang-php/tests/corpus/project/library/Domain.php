<?php

/**
 * Shelf — the lending domain: genres, membership tiers and loan states as
 * enums, ISBN and money value objects, and the three entities (books,
 * members, loans) plus the events they record when their state changes.
 */

declare(strict_types=1);

namespace Shelf\Domain;

use DateTimeImmutable;
use JsonSerializable;
use Shelf\Support\{Arrayable, Assert, BookUnavailable, Clock, HasEvents, Identifiable};
use Shelf\Support\ValidationException as Invalid;
use function Shelf\Support\{days_between, money_format_cents, slugify};
use const Shelf\Support\DATE_FORMAT;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

interface HasLabel
{
    public function label(): string;
}

enum Genre: string implements HasLabel
{
    case Fiction = 'fiction';
    case Mystery = 'mystery';
    case Science = 'science';
    case History = 'history';
    case Children = 'children';
    case Poetry = 'poetry';

    public const DEFAULT = self::Fiction;

    public function label(): string
    {
        return match ($this) {
            self::Fiction => 'Fiction',
            self::Mystery => 'Mystery & Crime',
            self::Science => 'Popular Science',
            self::History => 'History',
            self::Children => 'Children',
            self::Poetry => 'Poetry',
        };
    }

    public static function fromLabel(string $label): self
    {
        foreach (self::cases() as $genre) {
            if (slugify($genre->label()) === slugify($label)) {
                return $genre;
            }
        }
        return self::tryFrom(strtolower($label)) ?? self::DEFAULT;
    }

    /** Loan length in days: reference books go out for less time. */
    public function loanDays(): int
    {
        return match ($this) {
            self::Science, self::History => 14,
            self::Children => 28,
            default => 21,
        };
    }
}

enum MembershipTier: int implements HasLabel
{
    case Basic = 1;
    case Plus = 2;
    case Premium = 3;

    public function label(): string
    {
        return ucfirst(strtolower($this->name));
    }

    public function loanLimit(): int
    {
        return match ($this) {
            self::Basic => 3,
            self::Plus => 6,
            self::Premium => 12,
        };
    }

    /** Fine in cents for every day a loan is overdue. */
    public function finePerDay(): int
    {
        return match ($this) {
            self::Basic => 50,
            self::Plus => 25,
            self::Premium => 0,
        };
    }

    public function next(): ?self
    {
        return self::tryFrom($this->value + 1);
    }
}

enum LoanStatus
{
    case Active;
    case Overdue;
    case Returned;
    case Lost;

    public function isOpen(): bool
    {
        return $this === self::Active || $this === self::Overdue;
    }
}

// ---------------------------------------------------------------------------
// Value objects
// ---------------------------------------------------------------------------

final readonly class Isbn implements \Stringable
{
    private function __construct(public string $value)
    {
    }

    public static function parse(string $raw): self
    {
        $digits = preg_replace('/[^0-9X]/', '', strtoupper($raw)) ?? '';
        if (strlen($digits) === 10) {
            $digits = self::upgrade($digits);
        }
        if (strlen($digits) !== 13 || !self::checksumMatches($digits)) {
            throw new Invalid(errors: ['isbn' => "\"{$raw}\" is not a valid ISBN"]);
        }
        return new self($digits);
    }

    private static function upgrade(string $isbn10): string
    {
        $core = '978' . substr($isbn10, 0, 9);
        return $core . self::checkDigit($core);
    }

    private static function checkDigit(string $twelve): int
    {
        $sum = 0;
        foreach (str_split($twelve) as $i => $digit) {
            $sum += (int) $digit * ($i % 2 === 0 ? 1 : 3);
        }
        return (10 - $sum % 10) % 10;
    }

    private static function checksumMatches(string $isbn): bool
    {
        return self::checkDigit(substr($isbn, 0, 12)) === (int) $isbn[12];
    }

    public function formatted(): string
    {
        return sprintf(
            '%s-%s-%s-%s-%s',
            substr($this->value, 0, 3),
            substr($this->value, 3, 1),
            substr($this->value, 4, 4),
            substr($this->value, 8, 4),
            substr($this->value, 12),
        );
    }

    public function __toString(): string
    {
        return $this->formatted();
    }
}

final readonly class Money implements \Stringable
{
    public function __construct(
        public int $cents,
        public string $currency = 'EUR',
    ) {
    }

    public static function zero(string $currency = 'EUR'): self
    {
        return new self(0, $currency);
    }

    public function add(self $other): self
    {
        $this->assertSameCurrency($other);
        return new self($this->cents + $other->cents, $this->currency);
    }

    public function multiply(int $factor): self
    {
        return new self($this->cents * $factor, $this->currency);
    }

    public function isZero(): bool
    {
        return $this->cents === 0;
    }

    private function assertSameCurrency(self $other): void
    {
        if ($other->currency !== $this->currency) {
            throw new Invalid("Cannot mix {$this->currency} and {$other->currency}");
        }
    }

    public function __toString(): string
    {
        return money_format_cents($this->cents, $this->currency);
    }
}

// ---------------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------------

abstract class Entity implements Identifiable, Arrayable, JsonSerializable
{
    use HasEvents;

    protected ?int $id = null;

    public function getId(): ?int
    {
        return $this->id;
    }

    public function withId(int $id): static
    {
        $copy = clone $this;
        $copy->id = Assert::positive($id, 'id');
        return $copy;
    }

    public function jsonSerialize(): array
    {
        return ['id' => $this->id] + $this->toArray();
    }

    abstract public function toArray(): array;
}

class Book extends Entity
{
    private int $onLoan = 0;

    /** @param list<string> $authors */
    public function __construct(
        private readonly Isbn $isbn,
        private string $title,
        private array $authors,
        private Genre $genre = Genre::DEFAULT,
        private int $copies = 1,
    ) {
        Assert::notBlank($title, 'title');
        Assert::positive($copies, 'copies');
    }

    public function isbn(): Isbn
    {
        return $this->isbn;
    }

    public function title(): string
    {
        return $this->title;
    }

    public function genre(): Genre
    {
        return $this->genre;
    }

    public function availableCopies(): int
    {
        return $this->copies - $this->onLoan;
    }

    public function reserveCopy(): void
    {
        if ($this->availableCopies() <= 0) {
            throw (new BookUnavailable("No copies of \"{$this->title}\" left"))
                ->withContext('isbn', (string) $this->isbn);
        }
        $this->onLoan++;
    }

    public function returnCopy(): void
    {
        $this->onLoan = max(0, $this->onLoan - 1);
    }

    public function addCopies(int $count): void
    {
        $this->copies += Assert::positive($count, 'count');
    }

    public function toArray(): array
    {
        return [
            'isbn' => $this->isbn->formatted(),
            'title' => $this->title,
            'authors' => implode(', ', $this->authors),
            'genre' => $this->genre->label(),
            'available' => $this->availableCopies(),
        ];
    }
}

class Member extends Entity
{
    private string $email;

    public function __construct(
        private string $name,
        string $email,
        private MembershipTier $tier = MembershipTier::Basic,
    ) {
        $this->email = Assert::email($email);
    }

    public function name(): string
    {
        return $this->name;
    }

    public function email(): string
    {
        return $this->email;
    }

    public function tier(): MembershipTier
    {
        return $this->tier;
    }

    public function canBorrow(int $openLoans): bool
    {
        return $openLoans < $this->tier->loanLimit();
    }

    public function upgrade(): bool
    {
        $next = $this->tier->next();
        if ($next === null) {
            return false;
        }
        $this->recordEvent(new MemberUpgraded($this, $this->tier, $next));
        $this->tier = $next;
        return true;
    }

    public function toArray(): array
    {
        return [
            'name' => $this->name,
            'email' => $this->email,
            'tier' => $this->tier->label(),
        ];
    }
}

class Loan extends Entity
{
    private ?DateTimeImmutable $returnedAt = null;
    private bool $lost = false;
    private int $renewals = 0;

    public function __construct(
        private readonly Book $book,
        private readonly Member $member,
        private readonly DateTimeImmutable $borrowedAt,
        private DateTimeImmutable $dueAt,
    ) {
        $this->recordEvent(new BookBorrowed($this));
    }

    public static function start(Book $book, Member $member, Clock $clock): self
    {
        $now = $clock->now();
        $book->reserveCopy();
        return new self($book, $member, $now, $now->modify(sprintf('+%d days', $book->genre()->loanDays())));
    }

    public function book(): Book
    {
        return $this->book;
    }

    public function member(): Member
    {
        return $this->member;
    }

    public function dueAt(): DateTimeImmutable
    {
        return $this->dueAt;
    }

    public function status(Clock $clock): LoanStatus
    {
        return match (true) {
            $this->lost => LoanStatus::Lost,
            $this->returnedAt !== null => LoanStatus::Returned,
            $clock->now() > $this->dueAt => LoanStatus::Overdue,
            default => LoanStatus::Active,
        };
    }

    public function daysOverdue(Clock $clock): int
    {
        $end = $this->returnedAt ?? $clock->now();
        return max(0, days_between($this->dueAt, $end));
    }

    public function renew(Clock $clock): void
    {
        if ($this->status($clock) !== LoanStatus::Active || $this->renewals >= 2) {
            throw new Invalid(errors: ['loan' => 'cannot be renewed']);
        }
        $this->renewals++;
        $this->dueAt = $this->dueAt->modify(sprintf('+%d days', $this->book->genre()->loanDays()));
    }

    public function close(Clock $clock): void
    {
        if (!$this->status($clock)->isOpen()) {
            throw new Invalid(errors: ['loan' => 'already closed']);
        }
        $this->returnedAt = $clock->now();
        $this->book->returnCopy();
        $this->recordEvent(new BookReturned($this, $this->daysOverdue($clock)));
    }

    public function markLost(): void
    {
        $this->lost = true;
    }

    public function toArray(): array
    {
        return [
            'book' => $this->book->title(),
            'member' => $this->member->email(),
            'borrowed' => $this->borrowedAt->format(DATE_FORMAT),
            'due' => $this->dueAt->format(DATE_FORMAT),
            'returned' => $this->returnedAt?->format(DATE_FORMAT),
            'renewals' => $this->renewals,
        ];
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

final class BookBorrowed
{
    public function __construct(public readonly Loan $loan)
    {
    }
}

final class BookReturned
{
    public function __construct(
        public readonly Loan $loan,
        public readonly int $daysLate = 0,
    ) {
    }
}

final class MemberUpgraded
{
    public function __construct(
        public readonly Member $member,
        public readonly MembershipTier $from,
        public readonly MembershipTier $to,
    ) {
    }
}
