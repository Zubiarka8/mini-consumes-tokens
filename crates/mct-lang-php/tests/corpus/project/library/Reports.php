<?php

/**
 * Shelf — reports: a small tabular report model, four output formatters
 * (aligned text, CSV, Markdown and JSON) and the builder that produces the
 * inventory, overdue, top-borrower and genre reports from the repositories.
 */

declare(strict_types=1);

namespace Shelf\Reports;

use InvalidArgumentException;
use Shelf\Domain\Book;
use Shelf\Domain\Genre;
use Shelf\Domain\Loan;
use Shelf\Persistence\BookRepository;
use Shelf\Persistence\LoanRepository;
use Shelf\Persistence\MemberRepository;
use Shelf\Services\FineCalculator;
use Shelf\Support\{Arrayable, Clock, Collection};

use function Shelf\Support\money_format_cents;
use function Shelf\Support\str_limit;
use const Shelf\Support\DATE_FORMAT;

const COLUMN_GAP = 2;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

final class Report implements Arrayable
{
    /** @var list<array<string, scalar|null>> */
    private array $rows = [];

    /** @var array<string, string> */
    private array $notes = [];

    /** @param list<string> $columns */
    public function __construct(
        public readonly string $title,
        public readonly array $columns,
        public readonly ?string $generatedAt = null,
    ) {
        if ($columns === []) {
            throw new InvalidArgumentException('A report needs at least one column');
        }
    }

    public function addRow(array $row): self
    {
        $missing = array_diff($this->columns, array_keys($row));
        if ($missing !== []) {
            throw new InvalidArgumentException('Missing columns: ' . implode(', ', $missing));
        }
        $this->rows[] = array_intersect_key($row, array_flip($this->columns));
        return $this;
    }

    public function addNote(string $key, string $note): self
    {
        $this->notes[$key] = $note;
        return $this;
    }

    public function rows(): array
    {
        return $this->rows;
    }

    public function notes(): array
    {
        return $this->notes;
    }

    public function sortBy(string $column, bool $descending = false): self
    {
        usort($this->rows, static fn (array $a, array $b) => $descending
            ? $b[$column] <=> $a[$column]
            : $a[$column] <=> $b[$column]);
        return $this;
    }

    public function isEmpty(): bool
    {
        return $this->rows === [];
    }

    public function toArray(): array
    {
        return [
            'title' => $this->title,
            'generated_at' => $this->generatedAt,
            'columns' => $this->columns,
            'rows' => $this->rows,
            'notes' => $this->notes,
        ];
    }
}

// ---------------------------------------------------------------------------
// Formatters
// ---------------------------------------------------------------------------

interface Formatter
{
    public function format(Report $report): string;

    public function extension(): string;
}

abstract class TextFormatter implements Formatter
{
    protected function cell(mixed $value): string
    {
        return match (true) {
            $value === null => '—',
            is_bool($value) => $value ? 'yes' : 'no',
            is_float($value) => number_format($value, 1),
            default => str_limit((string) $value, 48),
        };
    }

    /** @return array<string, int> column => display width */
    protected function widths(Report $report): array
    {
        $widths = array_combine($report->columns, array_map('mb_strlen', $report->columns));
        foreach ($report->rows() as $row) {
            foreach ($row as $column => $value) {
                $widths[$column] = max($widths[$column], mb_strlen($this->cell($value)));
            }
        }
        return $widths;
    }
}

final class TableFormatter extends TextFormatter
{
    public function format(Report $report): string
    {
        $widths = $this->widths($report);
        $pad = fn (string $text, string $column) => str_pad($text, $widths[$column] + COLUMN_GAP);

        $lines = [strtoupper($report->title), ''];
        $lines[] = rtrim(implode('', array_map($pad, $report->columns, $report->columns)));
        $lines[] = str_repeat('-', array_sum($widths) + COLUMN_GAP * (count($widths) - 1));
        foreach ($report->rows() as $row) {
            $cells = [];
            foreach ($report->columns as $column) {
                $cells[] = $pad($this->cell($row[$column]), $column);
            }
            $lines[] = rtrim(implode('', $cells));
        }
        foreach ($report->notes() as $key => $note) {
            $lines[] = "* {$key}: {$note}";
        }
        return implode(PHP_EOL, $lines) . PHP_EOL;
    }

    public function extension(): string
    {
        return 'txt';
    }
}

final class CsvFormatter implements Formatter
{
    public function __construct(private readonly string $separator = ',')
    {
    }

    public function format(Report $report): string
    {
        $handle = fopen('php://temp', 'r+');
        fputcsv($handle, $report->columns, $this->separator);
        foreach ($report->rows() as $row) {
            fputcsv($handle, array_values($row), $this->separator);
        }
        rewind($handle);
        $csv = stream_get_contents($handle);
        fclose($handle);
        return $csv === false ? '' : $csv;
    }

    public function extension(): string
    {
        return 'csv';
    }
}

final class MarkdownFormatter extends TextFormatter
{
    public function format(Report $report): string
    {
        $header = '| ' . implode(' | ', $report->columns) . ' |';
        $rule = '|' . str_repeat(' --- |', count($report->columns));
        $body = implode(PHP_EOL, array_map(
            fn (array $row) => '| ' . implode(' | ', array_map($this->cell(...), $row)) . ' |',
            $report->rows(),
        ));
        $generated = $report->generatedAt ?? 'unknown';

        return <<<MD
            ## {$report->title}

            _Generated {$generated}_

            {$header}
            {$rule}
            {$body}

            MD;
    }

    public function extension(): string
    {
        return 'md';
    }
}

final class JsonFormatter implements Formatter
{
    public function format(Report $report): string
    {
        return json_encode($report, JSON_PRETTY_PRINT | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR);
    }

    public function extension(): string
    {
        return 'json';
    }
}

function formatter_for(string $name): Formatter
{
    return match (strtolower($name)) {
        'table', 'txt' => new TableFormatter(),
        'csv' => new CsvFormatter(),
        'tsv' => new CsvFormatter("\t"),
        'md', 'markdown' => new MarkdownFormatter(),
        'json' => new JsonFormatter(),
        default => throw new InvalidArgumentException("Unknown format \"{$name}\""),
    };
}

function percent(int $part, int $whole): string
{
    return $whole === 0 ? '0%' : sprintf('%.0f%%', 100 * $part / $whole);
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

final class ReportBuilder
{
    public function __construct(
        private readonly BookRepository $books,
        private readonly MemberRepository $members,
        private readonly LoanRepository $loans,
        private readonly FineCalculator $fines,
        private readonly Clock $clock,
    ) {
    }

    private function report(string $title, array $columns): Report
    {
        return new Report($title, $columns, $this->clock->now()->format(DATE_FORMAT));
    }

    public function inventory(): Report
    {
        $report = $this->report('Inventory', ['isbn', 'title', 'genre', 'available', 'loans']);
        $this->books->all()->each(function (Book $book) use ($report): void {
            $report->addRow([
                'isbn' => $book->isbn()->formatted(),
                'title' => $book->title(),
                'genre' => $book->genre()->label(),
                'available' => $book->availableCopies(),
                'loans' => $this->loans->forBook($book)->count(),
            ]);
        });
        $report->addNote('books', (string) $this->books->count());
        return $report->sortBy('title');
    }

    public function overdue(): Report
    {
        $report = $this->report('Overdue loans', ['member', 'title', 'due', 'days', 'fine']);
        $total = 0;
        foreach ($this->loans->overdue($this->clock) as $loan) {
            $fine = $this->fines->fineFor($loan);
            $total += $fine->cents;
            $report->addRow($this->overdueRow($loan, $fine->cents));
        }
        return $report->addNote('total', money_format_cents($total));
    }

    private function overdueRow(Loan $loan, int $fineCents): array
    {
        return [
            'member' => $loan->member()->email(),
            'title' => $loan->book()->title(),
            'due' => $loan->dueAt()->format(DATE_FORMAT),
            'days' => $loan->daysOverdue($this->clock),
            'fine' => money_format_cents($fineCents),
        ];
    }

    public function topBorrowers(int $limit = 10): Report
    {
        $report = $this->report('Top borrowers', ['rank', 'member', 'tier', 'loans', 'share']);
        $counts = $this->loans->borrowCounts();
        $all = array_sum($counts);
        $rank = 0;
        foreach (array_slice($counts, 0, $limit, true) as $email => $count) {
            $member = $this->members->byEmail($email);
            $report->addRow([
                'rank' => ++$rank,
                'member' => $member->name(),
                'tier' => $member->tier()->label(),
                'loans' => $count,
                'share' => percent($count, $all),
            ]);
        }
        return $report;
    }

    public function genres(): Report
    {
        $report = $this->report('Genres', ['genre', 'titles', 'copies out', 'share']);
        $groups = $this->books->all()->groupBy(static fn (Book $b) => $b->genre()->value);
        $totalTitles = $this->books->count();
        foreach (Genre::cases() as $genre) {
            $titles = $groups[$genre->value] ?? new Collection();
            $report->addRow([
                'genre' => $genre->label(),
                'titles' => $titles->count(),
                'copies out' => $titles->sum(fn (Book $b) => $this->loans->forBook($b)->count()),
                'share' => percent($titles->count(), $totalTitles),
            ]);
        }
        return $report->sortBy('titles', descending: true);
    }

    public function build(string $name): Report
    {
        $factory = match ($name) {
            'inventory' => $this->inventory(...),
            'overdue' => $this->overdue(...),
            'top' => fn () => $this->topBorrowers(5),
            'genres' => $this->genres(...),
            default => throw new InvalidArgumentException("Unknown report \"{$name}\""),
        };
        return $factory();
    }

    public function render(string $name, string $format = 'table'): string
    {
        return formatter_for($format)->format($this->build($name));
    }

    /** @return array<string, string> file name => contents */
    public function export(string $format, string ...$names): array
    {
        $formatter = formatter_for($format);
        $files = [];
        foreach ($names as $name) {
            $files["{$name}.{$formatter->extension()}"] = $formatter->format($this->build($name));
        }
        return $files;
    }
}
