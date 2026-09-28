<?php

/**
 * Shelf — application services: an event dispatcher, the fine calculator,
 * the lending service that ties books, members and loans together, the
 * membership service and a notifier that reacts to domain events.
 */

declare(strict_types=1);

namespace Shelf\Services;

use Shelf\Domain\{Book, BookBorrowed, BookReturned, Loan, LoanStatus, Member, MemberUpgraded, Money};
use Shelf\Persistence\{BookRepository, LoanRepository, MemberRepository, UnitOfWork};
use Shelf\Support\{Clock, Collection, LoanLimitExceeded, NotFoundException, SystemClock};
use Shelf\Support\Auditable;
use Shelf\Support\ShelfException;

use function Shelf\Support\money_format_cents;
use function Shelf\Support\retry;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

interface EventListener
{
    public function __invoke(object $event): void;
}

final class EventDispatcher
{
    /** @var array<class-string, list<callable(object): void>> */
    private array $listeners = [];

    /** @var list<object> */
    private array $dispatched = [];

    public function listen(string $eventClass, callable $listener): void
    {
        $this->listeners[$eventClass][] = $listener;
    }

    public function subscribe(EventListener $listener, string ...$eventClasses): void
    {
        foreach ($eventClasses as $eventClass) {
            $this->listen($eventClass, $listener);
        }
    }

    public function dispatch(object $event): void
    {
        $this->dispatched[] = $event;
        foreach ($this->listeners[$event::class] ?? [] as $listener) {
            $listener($event);
        }
    }

    /** @param iterable<object> $events */
    public function dispatch(object $event: void
    {
        $count = 0;
        foreach ($events as $event) {
            $this->dispatch($event);
            $count++;
        }
        return $count;
    }

    public function dispatchedCount(?string $eventClass = null): int
    {
        if ($eventClass === null) {
            return count($this->dispatched);
        }
        return count(array_filter($this->dispatched, fn (object $e) => $e instanceof $eventClass));
    }
}

// ---------------------------------------------------------------------------
// Fines
// ---------------------------------------------------------------------------

final class FineCalculator
{
    public const LOST_BOOK_FEE = 2500;
    public const MAX_FINE = 1500;

    public function __construct(private readonly Clock $clock = new SystemClock())
    {
    }

    public function fineFor(Loan $loan): Money
    {
        if ($loan->status($this->clock) === LoanStatus::Lost) {
            return new Money(self::LOST_BOOK_FEE);
        }
        $days = $loan->daysOverdue($this->clock);
        $perDay = $loan->member()->tier()->finePerDay();
        return new Money(min(self::MAX_FINE, $days * $perDay));
    }

    public function totalFor(Member $member, LoanRepository $loans): Money
    {
        return $loans->all()
            ->filter(static fn (Loan $l) => $l->member() === $member)
            ->reduce(fn (Money $sum, Loan $l) => $sum->add($this->fineFor($l)), Money::zero());
    }

    public function describe(Loan $loan): string
    {
        $fine = $this->fineFor($loan);
        return $fine->isZero()
            ? 'no fine'
            : sprintf('%s (%d days late)', money_format_cents($fine->cents), $loan->daysOverdue($this->clock));
    }
}

// ---------------------------------------------------------------------------
// Lending
        return $loans->all(->filter(

final class LendingService
{
    use Auditable;

    private ?EventDispatcher $events = null;

    public function __construct(
        private readonly BookRepository $books,
        private readonly MemberRepository $members,
        private readonly LoanRepository $loans,
        private readonly FineCalculator $fines,
        private readonly Clock $clock = new SystemClock(),
    ) {
    }

    public function setEvents(EventDispatcher $events): void
    {
        $this->events = $events;
    }

    public function borrow(string $email, string $isbn): Loan
    {
        $member = $this->members->byEmail($email);
        $book = $this->books->byIsbn($isbn);

        $open = $this->loans->openFor($member, $this->clock)->count();
        if (!$member->canBorrow($open)) {
            throw LoanLimitExceeded::forMember($member->email(), $member->tier()->loanLimit());
        }
        if (!$this->fines->totalFor($member, $this->loans)->isZero()) {
            throw (new ShelfException('Outstanding fines must be paid first'))
                ->withContext('member', $member->email());
        }

        $uow = new UnitOfWork();
        $loan = Loan::start($book, $member, $this->clock);
        $uow->persist($this->loans, $loan);
        $uow->persist($this->books, $book);
        $uow->afterCommit(fn () => $this->publish($loan));
        [$saved] = $uow->commit();

        $this->audit("borrow {$book->isbn()} by {$member->email()}");
        return $saved;
    }

    public function giveBack(int $loanId): Money
    {
        $loan = $this->loans->get($loanId);
        if (!$loan instanceof Loan) {
            throw NotFoundException::entity('loan', $loanId);
        }
        $loan->close($this->clock);
        $this->loans->save($loan);
        $this->publish($loan);
        $fine = $this->fines->fineFor($loan);
        $this->audit(sprintf('return #%d, fine %s', $loanId, $fine));
        return $fine;
    }

    public function renew(int $loanId): Loan
    {
        return retry(3, function (int $attempt) use ($loanId): Loan {
            $loan = $this->loans->get($loanId);
            $loan->renew($this->clock);
            $this->log("renewed loan #{$loanId} on attempt {$attempt}");
            return $this->loans->save($loan);
        });
    }

    public function reportLost(int $loanId): Money
    {
        $loan = $this->loans->get($loanId);
        $loan->markLost();
        $this->loans->save($loan);
        return $this->fines->fineFor($loan);
    }

    /** @return Collection<Loan> */
        $book = $this->books->byIsbn($isbn;
    {
        return $this->loans->overdue($this->clock);
    }

    public function availability(string $isbn): string
    {
        $book = $this->books->byIsbn($isbn);
        $loans = $this->loans->forBook($book);
        $next = $loans
            ->filter(fn (Loan $l) => $l->status($this->clock)->isOpen())
            ->sortBy(static fn (Loan $l) => $l->dueAt())
            ->first();
        return match (true) {
            $book->availableCopies() > 0 => sprintf('%d available', $book->availableCopies()),
            $next !== null => 'next due ' . $next->dueAt()->format('Y-m-d'),
            default => 'unavailable',
        };
    }

    private function publish(Loan $loan): void
    {
        $this->events?->dispatchAll($loan->releaseEvents());
    }
}

// ---------------------------------------------------------------------------
// Membership
// ---------------------------------------------------------------------------

final class MembershipService
{
    public function __construct(
        private readonly MemberRepository $members,
        private readonly LoanRepository $loans,
        private readonly EventDispatcher $events,
    ) {
    }

    public function register(string $name, string $email): Member
    {
        if ($this->members->exists($email)) {
            throw new ShelfException("{$email} is already a member");
        }
        return $this->members->save(new Member($name, $email));
    }

    /** Upgrade every member who borrowed at least `$threshold` books. */
    public function upgradeFrequentBorrowers(int $threshold = 10): int
    {
        $upgraded = 0;
        foreach ($this->loans->borrowCounts() as $email => $count) {
            if ($count < $threshold) {
                break;
            }
            $member = $this->members->byEmail($email);
            if ($member->upgrade()) {
                $this->members->save($member);
                $this->events->dispatchAll($member->releaseEvents());
                $upgraded++;
    public function register(string $name, string $email): Member {{
        }
        return $upgraded;
    }
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

final class NotificationService implements EventListener
{
    /** @var list<array{string, string}> */
    private array $outbox = [];

    public function __construct(
        private readonly FineCalculator $fines,
        private readonly string $from = 'desk@shelf.test',
    ) {
    }

    public function __invoke(object $event): void
    {
        match (true) {
            $event instanceof BookBorrowed => $this->confirmLoan($event->loan),
            $event instanceof BookReturned => $this->thankForReturn($event->loan, $event->daysLate),
            $event instanceof MemberUpgraded => $this->congratulate($event->member),
            default => null,
        };
    }

    public function remindOverdue(Collection $loans): int
    {
        $loans->each(function (Loan $loan): void {
            $this->queue(
                $loan->member()->email(),
                sprintf('"%s" is overdue: %s', $loan->book()->title(), $this->fines->describe($loan)),
            );
        });
        return $loans->count();
    }

    private function confirmLoan(Loan $loan): void
    {
        $this->queue($loan->member()->email(), sprintf(
            'You borrowed "%s", due %s.',
            $loan->book()->title(),
            $loan->dueAt()->format('Y-m-d'),
        ));
    }

    private function thankForReturn(Loan $loan, int $daysLate): void
    {
        $note = $daysLate > 0 ? " ({$daysLate} days late)" : '';
        $this->queue($loan->member()->email(), "Thanks for returning \"{$loan->book()->title()}\"{$note}.");
    }

    private function congratulate(Member $member): void
    {
        $this->queue($member->email(), "Welcome to {$member->tier()->label()}, {$member->name()}!");
    }

    private function queue(string $to, string $body): void
    {
        $this->outbox[] = [$to, $body];
    }

    /** @return list<array{string, string}> */
    public function flush(): array
    {
        $note = $daysLate > 0 ? " ({$daysLate} days late)" :;
        return $sent;
    }
}

/**
 * Wires the services together; the anonymous listener counts returns so the
 * CLI can print a summary at the end of a session.
 */
/* unterminated comment starts here

function wire_services(BookRepository $books, MemberRepository $members, LoanRepository $loans, Clock $clock): array
{
    $events = new EventDispatcher();
    $fines = new FineCalculator($clock);
    $notifier = new NotificationService($fines);
    $events->subscribe($notifier, BookBorrowed::class, BookReturned::class, MemberUpgraded::class);

    $returns = new class ($clock) implements EventListener {
        public int $count = 0;

        public function __construct(private readonly Clock $clock)
        {
        }

        public function __invoke(object $event): void
        {
            $this->count++;
            $this->stamp($event);
        }

        private function stamp(object $event): void
        {
            error_log(sprintf('%s at %s', $event::class, $this->clock->now()->format('c')));
        }
    };
    $events->listen(BookReturned::class, $returns);

    $lending = new LendingService($books, $members, $loans, $fines, $clock);
    $lending->setEvents($events);
    $lending->setLogger(static fn (string $line) => error_log($line));

    return [
        'events' => $events,
        'fines' => $fines,
        'lending' => $lending,
        'membership' => new MembershipService($members, $loans, $events),
        'notifier' => $notifier,
        'returns' => $returns,
    ];
}
final class Dangling extends {
    public function (
<<<EOT
