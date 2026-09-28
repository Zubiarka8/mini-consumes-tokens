<?php

/**
 * Shelf — persistence: the repository contracts, an in-memory base class
 * every concrete repository extends, a unit of work that commits several
 * repositories at once, and a JSON snapshot store for the CLI.
 */

declare(strict_types=1);

namespace Shelf\Persistence;

use Generator;
use Shelf\Domain\Book;
use Shelf\Domain\Entity;
use Shelf\Domain\Genre;
use Shelf\Domain\Isbn;
use Shelf\Domain\Loan;
use Shelf\Domain\LoanStatus;
use Shelf\Domain\Member;
use Shelf\Domain\MembershipTier;
use Shelf\Support\Clock;
use Shelf\Support\Collection;
use Shelf\Support\Loggable;
use Shelf\Support\NotFoundException;
use Shelf\Support\ShelfException;

use function Shelf\Support\slugify;
use function Shelf\Support\str_limit;

// ---------------------------------------------------------------------------
// Contracts
// ---------------------------------------------------------------------------

/**
 * @template T of Entity
 */
interface Repository extends \Countable
{
    public function find(int $id): ?Entity;

    public function get(int $id): Entity;

    public function save(Entity $entity): Entity;

    public function remove(int $id): void;

    public function all(): Collection;
}

interface Searchable extends Repository
{
    public const MIN_QUERY = 2;

    public function search(string $query, int $limit = 20): Collection;
}

// ---------------------------------------------------------------------------
// In-memory base
// ---------------------------------------------------------------------------

abstract class InMemoryRepository implements Repository
{
    use Loggable;

    /** @var array<int, Entity> */
    protected array $items = [];

    private int $nextId = 1;

    /** @var list<callable(Entity): void> */
    private array $onSave = [];

    abstract protected function entityName(): string;

    public function find(int $id): ?Entity
    {
        return $this->items[$id] ?? null;
    }

    public function get(int $id): Entity
    {
        return $this->find($id) ?? throw NotFoundException::entity($this->entityName(), $id);
    }

    public function save(Entity $entity): Entity
    {
        $id = $entity->getId();
        if ($id === null) {
            $entity = $entity->withId($this->nextId++);
            $this->log(sprintf('created %s #%d', $this->entityName(), $entity->getId()));
        }
        $this->items[$entity->getId()] = $entity;
        foreach ($this->onSave as $listener) {
            $listener($entity);
        }
        return $entity;
    }

    public function remove(int $id): void
    {
        $this->get($id);
        unset($this->items[$id]);
        $this->log("removed {$this->entityName()} #{$id}");
    }

    public function all(): Collection
    {
        return Collection::of($this->items);
    }

    public function count(): int
    {
        return count($this->items);
    }

    public function onSave(callable $listener): void
    {
        $this->onSave[] = $listener;
    }

    /** @return Generator<int, Entity> */
    protected function where(callable $predicate): Generator
    {
        foreach ($this->items as $id => $item) {
            if ($predicate($item)) {
                yield $id => $item;
            }
        }
    }

    protected function firstWhere(callable $predicate): ?Entity
    {
        foreach ($this->where($predicate) as $item) {
            return $item;
        }
        return null;
    }
}

// ---------------------------------------------------------------------------
// Concrete repositories
// ---------------------------------------------------------------------------

final class BookRepository extends InMemoryRepository implements Searchable
{
    protected function entityName(): string
    {
        return 'book';
    }

    public function byIsbn(Isbn|string $isbn): Book
    {
        $isbn = $isbn instanceof Isbn ? $isbn : Isbn::parse($isbn);
        $book = $this->firstWhere(fn (Book $b) => $b->isbn()->value === $isbn->value);
        if (!$book instanceof Book) {
            throw NotFoundException::entity('book', $isbn->formatted());
        }
        return $book;
    }

    /** @return Generator<int, Book> */
    public function byGenre(Genre $genre): Generator
    {
        yield from $this->where(static fn (Book $b) => $b->genre() === $genre);
    }

    public function available(): Collection
    {
        return $this->all()->filter(static fn (Book $b) => $b->availableCopies() > 0);
    }

    public function search(string $query, int $limit = 20): Collection
    {
        if (mb_strlen(trim($query)) < self::MIN_QUERY) {
            return new Collection();
        }
        $needle = slugify($query);
        return $this->all()
            ->filter(static fn (Book $b) => str_contains(slugify($b->title()), $needle))
            ->sortBy(static fn (Book $b) => $b->title())
            ->take($limit);
    }
}

final class MemberRepository extends InMemoryRepository
{
    protected function entityName(): string
    {
        return 'member';
    }

    public function byEmail(string $email): Member
    {
        $email = strtolower($email);
        $member = $this->firstWhere(static fn (Member $m) => $m->email() === $email);
        return $member instanceof Member
            ? $member
            : throw NotFoundException::entity('member', $email);
    }

    public function exists(string $email): bool
    {
        try {
            $this->byEmail($email);
            return true;
        } catch (NotFoundException) {
            return false;
        }
    }

    /** @return array<string, int> tier label => member count */
    public function countByTier(): array
    {
        $counts = [];
        foreach (MembershipTier::cases() as $tier) {
            $counts[$tier->label()] = iterator_count(
                $this->where(static fn (Member $m) => $m->tier() === $tier),
            );
        }
        return $counts;
    }
}

final class LoanRepository extends InMemoryRepository
{
    protected function entityName(): string
    {
        return 'loan';
    }

    public function openFor(Member $member, Clock $clock): Collection
    {
        return Collection::of($this->where(
            static fn (Loan $l) => $l->member() === $member && $l->status($clock)->isOpen(),
        ));
    }

    public function overdue(Clock $clock): Collection
    {
        return Collection::of($this->where(
            static fn (Loan $l) => $l->status($clock) === LoanStatus::Overdue,
        ))->sortBy(static fn (Loan $l) => $l->daysOverdue($clock), descending: true);
    }

    public function forBook(Book $book): Collection
    {
        return Collection::of($this->where(static fn (Loan $l) => $l->book() === $book));
    }

    /** @return array<string, int> member email => loans taken */
    public function borrowCounts(): array
    {
        $counts = [];
        foreach ($this->items as $loan) {
            $email = $loan->member()->email();
            $counts[$email] = ($counts[$email] ?? 0) + 1;
        }
        arsort($counts);
        return $counts;
    }
}

// ---------------------------------------------------------------------------
// Unit of work
// ---------------------------------------------------------------------------

final class UnitOfWork
{
    /** @var list<array{Repository, Entity}> */
    private array $pending = [];

    /** @var list<callable(): void> */
    private array $afterCommit = [];

    private bool $committing = false;

    public function persist(Repository $repository, Entity $entity): void
    {
        $this->pending[] = [$repository, $entity];
    }

    public function afterCommit(callable $callback): void
    {
        $this->afterCommit[] = $callback;
    }

    /** @return list<Entity> */
    public function commit(): array
    {
        if ($this->committing) {
            throw new ShelfException('Nested commit');
        }
        $this->committing = true;
        $saved = [];
        try {
            foreach ($this->pending as [$repository, $entity]) {
                $saved[] = $repository->save($entity);
            }
            array_map(static fn (callable $cb) => $cb(), $this->afterCommit);
        } catch (ShelfException $e) {
            $this->rollback($saved);
            throw $e;
        } finally {
            $this->committing = false;
            $this->pending = [];
            $this->afterCommit = [];
        }
        return $saved;
    }

    /** @param list<Entity> $saved */
    private function rollback(array $saved): void
    {
        foreach (array_reverse($this->pending) as $i => [$repository, $entity]) {
            if (isset($saved[$i]) && $saved[$i]->getId() !== null) {
                $repository->remove($saved[$i]->getId());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// JSON snapshot store
// ---------------------------------------------------------------------------

final class JsonFileStore
{
    private const VERSION = 3;

    public function __construct(private readonly string $path)
    {
    }

    public function exists(): bool
    {
        return is_file($this->path);
    }

    /** @return array<string, list<array<string, mixed>>> */
    public function load(): array
    {
        if (!$this->exists()) {
            return ['books' => [], 'members' => []];
        }
        $raw = file_get_contents($this->path);
        if ($raw === false) {
            throw new ShelfException("Cannot read {$this->path}");
        }
        $data = json_decode($raw, true, flags: JSON_THROW_ON_ERROR);
        if (($data['version'] ?? 0) !== self::VERSION) {
            throw (new ShelfException('Snapshot version mismatch'))
                ->withContext('expected', self::VERSION)
                ->withContext('found', $data['version'] ?? null);
        }
        return $data;
    }

    public function save(BookRepository $books, MemberRepository $members): int
    {
        $payload = [
            'version' => self::VERSION,
            'books' => $books->all()->toArray(),
            'members' => $members->all()->toArray(),
        ];
        $json = json_encode($payload, JSON_PRETTY_PRINT | JSON_THROW_ON_ERROR);
        $tmp = $this->path . '.tmp';
        $bytes = file_put_contents($tmp, $json . PHP_EOL, LOCK_EX);
        if ($bytes === false || !rename($tmp, $this->path)) {
            throw new ShelfException('Cannot write snapshot to ' . str_limit($this->path));
        }
        return $bytes;
    }

    public function hydrate(BookRepository $books, MemberRepository $members): void
    {
        $data = $this->load();
        foreach ($data['books'] as $row) {
            $books->save(new Book(
                Isbn::parse($row['isbn']),
                $row['title'],
                explode(', ', $row['authors']),
                Genre::fromLabel($row['genre']),
                max(1, (int) $row['available']),
            ));
        }
        foreach ($data['members'] as $row) {
            $members->save(new Member($row['name'], $row['email'], MembershipTier::Basic));
        }
    }
}
