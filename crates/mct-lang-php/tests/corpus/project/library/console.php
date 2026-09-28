<?php

/**
 * Shelf — command-line front end. Loads the other files, seeds a demo
 * catalogue, registers the commands and dispatches `$argv` to one of them.
 *
 *   php console.php borrow ada@example.org 9780262033848
 *   php console.php report overdue --format=md
 */

declare(strict_types=1);

namespace Shelf\Console;

require_once 'Support.php';
require_once 'Domain.php';
require_once __DIR__ . '/Repository.php';
require_once __DIR__ . '/Services.php';
include_once 'Reports.php';

use Attribute;
use ReflectionClass;
use Shelf\Domain\{Book, Genre, Isbn, Member, MembershipTier};
use Shelf\Persistence\{BookRepository, JsonFileStore, LoanRepository, MemberRepository};
use Shelf\Reports\ReportBuilder;
use Shelf\Services\LendingService;
use Shelf\Services\MembershipService;
use Shelf\Services\NotificationService;
use Shelf\Support\{Clock, FrozenClock, ShelfException, SystemClock, ValidationException};

use function Shelf\Reports\formatter_for;
use function Shelf\Services\wire_services;
use function Shelf\Support\env;
use const Shelf\Support\VERSION;

define('SHELF_STARTED_AT', microtime(true));

// ---------------------------------------------------------------------------
// Infrastructure
// ---------------------------------------------------------------------------

#[Attribute(Attribute::TARGET_CLASS)]
final class AsCommand
{
    /** @param list<string> $aliases */
    public function __construct(
        public readonly string $name,
        public readonly string $summary = '',
        public readonly array $aliases = [],
    ) {
    }
}

final class Output
{
    private const COLORS = ['red' => 31, 'green' => 32, 'yellow' => 33, 'dim' => 2];

    public function __construct(private readonly bool $ansi = true)
    {
    }

    public function line(string $text = ''): void
    {
        fwrite(STDOUT, $text . PHP_EOL);
    }

    public function error(string $text): void
    {
        fwrite(STDERR, $this->color('red', $text) . PHP_EOL);
    }

    public function color(string $name, string $text): string
    {
        if (!$this->ansi || !isset(self::COLORS[$name])) {
            return $text;
        }
        return sprintf("\033[%dm%s\033[0m", self::COLORS[$name], $text);
    }
}

/**
 * Parsed `$argv`: positional arguments plus `--name=value` / `--flag`
 * options.
 */
final class Input
{
    /** @var list<string> */
    public readonly array $arguments;

    /** @var array<string, string|true> */
    public readonly array $options;

    public function __construct(array $argv)
    {
        $arguments = $options = [];
        foreach ($argv as $token) {
            if (str_starts_with($token, '--')) {
                [$name, $value] = array_pad(explode('=', substr($token, 2), 2), 2, true);
                $options[$name] = $value;
            } else {
                $arguments[] = $token;
            }
        }
        $this->arguments = $arguments;
        $this->options = $options;
    }

    public function argument(int $index, string $name): string
    {
        return $this->arguments[$index] ?? throw new ValidationException(errors: [$name => 'is required']);
    }

    public function option(string $name, string $default = ''): string
    {
        $value = $this->options[$name] ?? $default;
        return $value === true ? '1' : $value;
    }
}

final class Container
{
    /** @var array<string, object> */
    private array $services = [];

    /** @var array<string, \Closure(self): object> */
    private array $factories = [];

    public function set(string $id, \Closure $factory): void
    {
        $this->factories[$id] = $factory;
    }

    public function get(string $id): object
    {
        return $this->services[$id] ??= ($this->factories[$id] ?? throw new ShelfException("No service {$id}"))($this);
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

abstract class Command
{
    public function __construct(protected readonly Container $container, protected readonly Output $out)
    {
    }

    abstract public function run(Input $input): int;

    public function metadata(): AsCommand
    {
        $attribute = (new ReflectionClass($this))->getAttributes(AsCommand::class)[0] ?? null;
        return $attribute?->newInstance() ?? new AsCommand(static::class);
    }

    protected function lending(): LendingService
    {
        return $this->container->get('lending');
    }
}

#[AsCommand('borrow', 'Lend a book to a member', aliases: ['lend'])]
final class BorrowCommand extends Command
{
    public function run(Input $input): int
    {
        $loan = $this->lending()->borrow($input->argument(0, 'email'), $input->argument(1, 'isbn'));
        $this->out->line(sprintf(
            'Loan #%d: "%s" due %s',
            $loan->getId(),
            $loan->book()->title(),
            $loan->dueAt()->format('Y-m-d'),
        ));
        return 0;
    }
}

#[AsCommand('return', 'Close a loan and print the fine')]
final class ReturnCommand extends Command
{
    public function run(Input $input): int
    {
        $fine = $this->lending()->giveBack((int) $input->argument(0, 'loan'));
        $this->out->line($fine->isZero() ? 'Returned on time.' : "Returned late, fine {$fine}.");
        return 0;
    }
}

#[AsCommand('report', 'Print a report: inventory, overdue, top, genres')]
final class ReportCommand extends Command
{
    public function run(Input $input): int
    {
        $builder = $this->container->get('reports');
        assert($builder instanceof ReportBuilder);
        $format = $input->option('format', 'table');
        $this->out->line($builder->render($input->argument(0, 'report'), $format));
        return 0;
    }
}

#[AsCommand('remind', 'Queue reminders for every overdue loan')]
final class RemindCommand extends Command
{
    public function run(Input $input): int
    {
        $notifier = $this->container->get('notifier');
        assert($notifier instanceof NotificationService);
        $count = $notifier->remindOverdue($this->lending()->overdue());
        foreach ($notifier->flush() as [$to, $body]) {
            $this->out->line($this->out->color('dim', $to) . "  {$body}");
        }
        $this->out->line("{$count} reminders queued.");
        return 0;
    }
}

#[AsCommand('upgrade', 'Upgrade frequent borrowers')]
final class UpgradeCommand extends Command
{
    public function run(Input $input): int
    {
        $membership = $this->container->get('membership');
        assert($membership instanceof MembershipService);
        $threshold = (int) $input->option('min', '10');
        $this->out->line(sprintf('%d members upgraded.', $membership->upgradeFrequentBorrowers($threshold)));
        return 0;
    }
}

#[AsCommand('help', 'List the available commands', aliases: ['-h', '--help'])]
final class HelpCommand extends Command
{
    /** @param array<string, Command> $commands */
    public function __construct(Container $container, Output $out, private readonly array $commands = [])
    {
        parent::__construct($container, $out);
    }

    public function run(Input $input): int
    {
        $this->out->line('shelf ' . VERSION);
        $this->out->line();
        foreach ($this->commands as $name => $command) {
            $this->out->line(sprintf('  %-10s %s', $name, $command->metadata()->summary));
        }
        return 0;
    }
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

final class Application
{
    /** @var array<string, Command> */
    private array $commands = [];

    public function __construct(private readonly Container $container, private readonly Output $out)
    {
    }

    public function register(Command ...$commands): self
    {
        foreach ($commands as $command) {
            $meta = $command->metadata();
            foreach ([$meta->name, ...$meta->aliases] as $name) {
                $this->commands[$name] = $command;
            }
        }
        return $this;
    }

    public function run(array $argv): int
    {
        $input = new Input(array_slice($argv, 1));
        $name = $input->arguments[0] ?? 'help';
        $command = $this->commands[$name] ?? new HelpCommand($this->container, $this->out, $this->commands);
        try {
            return $command->run(new Input(array_slice($argv, 2)));
        } catch (ValidationException $e) {
            $this->out->error($e->first() ?? $e->getMessage());
            return 2;
        } catch (ShelfException $e) {
            $this->out->error(sprintf('[%s] %s', $e->code(), $e->getMessage()));
            return 1;
        }
    }
}

function seed(BookRepository $books, MemberRepository $members): void
{
    $catalogue = [
        ['9780262033848', 'Introduction to Algorithms', ['Cormen', 'Leiserson', 'Rivest', 'Stein'], Genre::Science, 3],
        ['9780141439518', 'Pride and Prejudice', ['Jane Austen'], Genre::Fiction, 2],
        ['9780062073488', 'And Then There Were None', ['Agatha Christie'], Genre::Mystery, 1],
        ['9780679783268', 'The Histories', ['Herodotus'], Genre::History, 1],
    ];
    foreach ($catalogue as [$isbn, $title, $authors, $genre, $copies]) {
        $books->save(new Book(Isbn::parse($isbn), $title, $authors, $genre, $copies));
    }
    $members->save(new Member('Ada Lovelace', 'ada@example.org', MembershipTier::Premium));
    $members->save(new Member('Alan Turing', 'alan@example.org'));
}

function bootstrap(Clock $clock, Output $out): Application
{
    $container = new Container();
    $container->set('books', static fn () => new BookRepository());
    $container->set('members', static fn () => new MemberRepository());
    $container->set('loans', static fn () => new LoanRepository());
    $container->set('services', static fn (Container $c) => (object) wire_services(
        $c->get('books'),
        $c->get('members'),
        $c->get('loans'),
        $clock,
    ));
    foreach (['lending', 'membership', 'notifier'] as $id) {
        $container->set($id, static fn (Container $c) => $c->get('services')->{$id});
    }
    $container->set('reports', static fn (Container $c) => new ReportBuilder(
        $c->get('books'),
        $c->get('members'),
        $c->get('loans'),
        $c->get('services')->fines,
        $clock,
    ));

    $store = new JsonFileStore(env('SHELF_DATA', __DIR__ . '/shelf.json'));
    $store->exists()
        ? $store->hydrate($container->get('books'), $container->get('members'))
        : seed($container->get('books'), $container->get('members'));

    $commands = [
        new BorrowCommand($container, $out),
        new ReturnCommand($container, $out),
        new ReportCommand($container, $out),
        new RemindCommand($container, $out),
        new UpgradeCommand($container, $out),
    ];
    return (new Application($container, $out))->register(...$commands);
}

$clock = env('SHELF_TODAY') !== null ? new FrozenClock(env('SHELF_TODAY')) : new SystemClock();
$out = new Output(ansi: stream_isatty(STDOUT));

$main = function (array $argv) use ($clock, $out): int {
    $app = bootstrap($clock, $out);
    $status = $app->run($argv);
    if (env('SHELF_TIMING', false)) {
        $out->line($out->color('dim', sprintf('%.1f ms', (microtime(true) - SHELF_STARTED_AT) * 1000)));
    }
    return $status;
};

$format = fn (string $name) => formatter_for($name)->extension();

if (PHP_SAPI === 'cli' && realpath($argv[0] ?? '') === __FILE__) {
    exit($main($argv));
}
