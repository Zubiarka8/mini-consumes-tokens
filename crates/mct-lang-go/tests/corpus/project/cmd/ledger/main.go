// Command ledger is a tiny command line front end for the ledger package: it
// builds a demo chart of accounts, posts a few transactions and prints the
// reports.
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"sort"
	"strings"
	"syscall"
	"time"

	"example.com/ledger"
	log "example.com/ledger/internal/logging"
	_ "example.com/ledger/internal/metrics"
)

const (
	exitOK = iota
	exitUsage
	exitFailure
)

var (
	version = "dev"
	started = time.Now()
)

type options struct {
	currency ledger.Currency
	author   string
	verbose  bool
	timeout  time.Duration
	args     []string
}

// command is one sub-command of the tool.
type command struct {
	name  string
	help  string
	run   func(ctx context.Context, app *app, args []string) error
	hooks []func(*app)
}

// app bundles the long-lived objects every command needs.
type app struct {
	opts    options
	store   *ledger.MemoryStore
	service *ledger.Service
	report  *ledger.Report
	out     *os.File
}

var commands = []command{
	{name: "demo", help: "post sample transactions and print every report", run: runDemo},
	{name: "balance", help: "print one account's balance", run: runBalance},
	{name: "transfer", help: "move money between two accounts", run: runTransfer},
	{name: "sheet", help: "print the balance sheet", run: runSheet},
	{name: "rates", help: "show the exchange rate table", run: runRates},
	{name: "version", help: "print the version", run: func(_ context.Context, a *app, _ []string) error {
		fmt.Fprintln(a.out, "ledger", version)
		return nil
	}},
}

func init() {
	sort.Slice(commands, func(i, j int) bool { return commands[i].name < commands[j].name })
}

func main() {
	os.Exit(run(os.Args[1:]))
}

func run(args []string) int {
	opts, err := parseFlags(args)
	if err != nil {
		if errors.Is(err, flag.ErrHelp) {
			return exitOK
		}
		fmt.Fprintln(os.Stderr, "ledger:", err)
		return exitUsage
	}
	if len(opts.args) == 0 {
		usage(os.Stderr)
		return exitUsage
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()
	ctx, cancel := context.WithTimeout(ctx, opts.timeout)
	defer cancel()

	app, err := newApp(opts)
	if err != nil {
		log.Errorf("setup: %v", err)
		return exitFailure
	}
	defer app.store.Close()

	cmd, ok := lookup(opts.args[0])
	if !ok {
		fmt.Fprintf(os.Stderr, "ledger: unknown command %q\n", opts.args[0])
		usage(os.Stderr)
		return exitUsage
	}
	for _, hook := range cmd.hooks {
		hook(app)
	}
	if err := cmd.run(ctx, app, opts.args[1:]); err != nil {
		report(os.Stderr, err)
		return exitFailure
	}
	return exitOK
}

func parseFlags(args []string) (options, error) {
	var opts options
	var currency string
	fs := flag.NewFlagSet("ledger", flag.ContinueOnError)
	fs.StringVar(&currency, "currency", "EUR", "base currency")
	fs.StringVar(&opts.author, "author", "cli", "author recorded on transactions")
	fs.BoolVar(&opts.verbose, "v", false, "log every posting")
	fs.DurationVar(&opts.timeout, "timeout", 5*time.Second, "overall deadline")
	fs.Usage = func() { usage(fs.Output()) }
	if err := fs.Parse(args); err != nil {
		return opts, err
	}
	opts.currency = ledger.Currency(strings.ToUpper(currency))
	if !opts.currency.Valid() {
		return opts, fmt.Errorf("bad currency %q", currency)
	}
	opts.args = fs.Args()
	return opts, nil
}

func usage(w interface{ Write([]byte) (int, error) }) {
	fmt.Fprintln(w, "usage: ledger [flags] <command> [args]")
	fmt.Fprintln(w, "commands:")
	for _, c := range commands {
		fmt.Fprintf(w, "  %-10s %s\n", c.name, c.help)
	}
}

func lookup(name string) (command, bool) {
	for _, c := range commands {
		if c.name == name {
			return c, true
		}
	}
	return command{}, false
}

// newApp builds the store, the service and the report writer.
func newApp(opts options) (*app, error) {
	store := ledger.NewMemoryStore(ledger.SystemClock)
	rates := ledger.NewRateTable(ledger.EUR)
	rates.Set(ledger.Rate{From: ledger.EUR, To: ledger.USD, Num: 108, Den: 100})
	rates.Set(ledger.Rate{From: ledger.EUR, To: ledger.GBP, Num: 85, Den: 100})

	var logger ledger.Logger = ledger.DefaultLogger()
	if !opts.verbose {
		logger = quiet{}
	}
	svc := ledger.NewService(store, ledger.Config{
		Author:  opts.author,
		Workers: 2,
		Rates:   rates,
	},
		ledger.WithLogger(logger),
		ledger.WithHook(ledger.AuditHook(logger)),
		ledger.WithHook(ledger.LimitHook(ledger.Of(opts.currency, 1_000_000, 0))),
	)
	a := &app{
		opts:    opts,
		store:   store,
		service: svc,
		report:  ledger.NewReport(store, ledger.SystemClock, rates),
		out:     os.Stdout,
	}
	if err := openChart(svc, opts.currency); err != nil {
		return nil, err
	}
	return a, nil
}

type quiet struct{}

func (quiet) Printf(string, ...any) {}

// openChart creates the demo chart of accounts.
func openChart(svc *ledger.Service, c ledger.Currency) error {
	specs := []struct {
		id   ledger.AccountID
		name string
		kind ledger.Kind
		opts []ledger.AccountOption
	}{
		{"assets", "Assets", ledger.Asset, nil},
		{"assets.cash", "Cash", ledger.Asset, []ledger.AccountOption{ledger.WithParent("assets")}},
		{"assets.bank", "Bank", ledger.Asset, []ledger.AccountOption{
			ledger.WithParent("assets"),
			ledger.WithLimit(ledger.Of(c, 500, 0)),
		}},
		{"liabilities", "Liabilities", ledger.Liability, nil},
		{"liabilities.card", "Credit card", ledger.Liability, []ledger.AccountOption{ledger.WithParent("liabilities")}},
		{"equity", "Equity", ledger.Equity, nil},
		{"income.sales", "Sales", ledger.Income, []ledger.AccountOption{ledger.WithTags("kind", "operating")}},
		{"expenses.rent", "Rent", ledger.Expense, nil},
	}
	var accounts []*ledger.Account
	for _, spec := range specs {
		a, err := ledger.NewAccount(spec.id, spec.name, spec.kind, c, spec.opts...)
		if err != nil {
			return err
		}
		accounts = append(accounts, a)
	}
	return svc.OpenAll(accounts...)
}

func runDemo(ctx context.Context, a *app, _ []string) error {
	c := a.opts.currency
	seed, err := ledger.NewTx("", "opening").
		Debit("assets.bank", ledger.Of(c, 10_000, 0), "capital").
		Credit("equity", ledger.Of(c, 10_000, 0), "capital").
		Build()
	if err != nil {
		return err
	}
	if err := a.service.Post(ctx, seed); err != nil {
		return err
	}
	batch := []*ledger.Transaction{
		mustTx("sale 1", "assets.cash", "income.sales", ledger.Of(c, 250, 50)),
		mustTx("sale 2", "assets.cash", "income.sales", ledger.Of(c, 99, 99)),
		mustTx("rent", "expenses.rent", "assets.bank", ledger.Of(c, 1_200, 0)),
	}
	errs, err := a.service.PostAll(ctx, batch)
	if err != nil {
		for i, e := range errs {
			if e != nil {
				fmt.Fprintf(a.out, "batch %d: %v\n", i, e)
			}
		}
		return err
	}
	if _, err := a.service.Transfer(ctx, "assets.bank", "assets.cash", ledger.Of(c, 300, 0), "float"); err != nil {
		return err
	}
	return printAll(a)
}

// mustTx builds a two-entry transaction or panics: it is only used for the
// hard-coded demo data above.
func mustTx(summary string, debit, credit ledger.AccountID, amount ledger.Money) *ledger.Transaction {
	tx, err := ledger.NewTx("", summary).Debit(debit, amount, summary).Credit(credit, amount, summary).Build()
	if err != nil {
		panic(err)
	}
	return tx
}

func printAll(a *app) error {
	now := time.Now()
	fmt.Fprintln(a.out, "== trial balance")
	if err := a.report.TrialBalance(a.out, now); err != nil {
		return err
	}
	fmt.Fprintln(a.out, "\n== balance sheet")
	if err := a.report.BalanceSheet(a.out, now); err != nil {
		return err
	}
	fmt.Fprintln(a.out, "\n== income statement")
	if err := a.report.IncomeStatement(a.out, started.Add(-time.Hour), now.Add(time.Hour)); err != nil {
		return err
	}
	sum, err := a.report.Summarize(now)
	if err != nil {
		return err
	}
	fmt.Fprintln(a.out)
	sum.Format(a.out)
	return nil
}

func runBalance(_ context.Context, a *app, args []string) error {
	if len(args) != 1 {
		return errors.New("usage: balance <account>")
	}
	bal, err := a.store.Balance(ledger.AccountID(args[0]), time.Time{})
	if err != nil {
		return err
	}
	fmt.Fprintln(a.out, bal)
	return nil
}

func runTransfer(ctx context.Context, a *app, args []string) error {
	if len(args) != 3 {
		return errors.New("usage: transfer <from> <to> <amount>")
	}
	amount, err := ledger.ParseMoney(args[2], a.opts.currency)
	if err != nil {
		return err
	}
	tx, err := a.service.Transfer(ctx, ledger.AccountID(args[0]), ledger.AccountID(args[1]), amount, "cli")
	if err != nil {
		return err
	}
	fmt.Fprintln(a.out, ledger.Label(tx))
	return nil
}

func runSheet(_ context.Context, a *app, _ []string) error {
	return a.report.BalanceSheet(a.out, time.Time{})
}

func runRates(_ context.Context, a *app, _ []string) error {
	table := ledger.NewRateTable(ledger.EUR)
	for _, c := range table.Currencies() {
		fmt.Fprintln(a.out, c)
	}
	return nil
}

// report prints err; ledger errors are shown with their full chain.
func report(w *os.File, err error) {
	var le *ledger.Error
	switch {
	case errors.As(err, &le):
		fmt.Fprintf(w, "ledger: %s\n", ledger.Explain(le))
	case errors.Is(err, context.DeadlineExceeded):
		fmt.Fprintln(w, "ledger: timed out")
	default:
		fmt.Fprintln(w, "ledger:", err)
	}
}
