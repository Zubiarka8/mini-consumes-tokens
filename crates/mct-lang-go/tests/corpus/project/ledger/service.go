package ledger

import (
	"context"
	"fmt"
	"log"
	"sort"
	"sync"
	"time"
)

// Hook runs around a posting: before it may veto it, after it observes the
// result.
type Hook interface {
	Before(ctx context.Context, tx *Transaction) error
	After(ctx context.Context, tx *Transaction, err error)
}

// HookFuncs adapts plain functions to Hook; either may be nil.
type HookFuncs struct {
	BeforeFunc func(ctx context.Context, tx *Transaction) error
	AfterFunc  func(ctx context.Context, tx *Transaction, err error)
}

// Before implements Hook.
func (h HookFuncs) Before(ctx context.Context, tx *Transaction) error {
	if h.BeforeFunc == nil {
		return nil
	}
	return h.BeforeFunc(ctx, tx)
}

// After implements Hook.
func (h HookFuncs) After(ctx context.Context, tx *Transaction, err error) {
	if h.AfterFunc != nil {
		h.AfterFunc(ctx, tx, err)
	}
}

// Logger is the subset of *log.Logger the service needs.
type Logger interface {
	Printf(format string, args ...any)
}

type discard struct{}

func (discard) Printf(string, ...any) {}

// Config tunes a Service.
type Config struct {
	Author     string
	Retry      RetryPolicy
	Workers    int
	MaxEntries int
	Rates      *RateTable
}

// Service is the entry point applications use: it validates, converts
// currencies, runs hooks and talks to the Store.
type Service struct {
	Store
	cfg   Config
	ids   IDGenerator
	clock Clock
	log   Logger
	hooks []Hook

	mu      sync.Mutex
	pending map[TxID]struct{}
}

// ServiceOption customises NewService.
type ServiceOption func(*Service)

// WithLogger sets the logger.
func WithLogger(l Logger) ServiceOption {
	return func(s *Service) { s.log = l }
}

// WithHook appends a hook.
func WithHook(h Hook) ServiceOption {
	return func(s *Service) { s.hooks = append(s.hooks, h) }
}

// WithClock overrides the clock.
func WithClock(c Clock) ServiceOption {
	return func(s *Service) { s.clock = c }
}

// WithIDs overrides the ID generator.
func WithIDs(g IDGenerator) ServiceOption {
	return func(s *Service) { s.ids = g }
}

// NewService wires a Service over store.
func NewService(store Store, cfg Config, opts ...ServiceOption) *Service {
	s := &Service{
		Store:   store,
		cfg:     cfg,
		ids:     NewSequentialIDs("tx"),
		clock:   SystemClock,
		log:     discard{},
		pending: make(map[TxID]struct{}),
	}
	if s.cfg.Workers <= 0 {
		s.cfg.Workers = 4
	}
	if s.cfg.MaxEntries <= 0 {
		s.cfg.MaxEntries = 100
	}
	if s.cfg.Retry.Attempts == 0 {
		s.cfg.Retry = DefaultRetry
	}
	for _, opt := range opts {
		opt(s)
	}
	return s
}

// DefaultLogger sends messages to the standard logger.
func DefaultLogger() Logger { return log.Default() }

// OpenAll opens every account, parents first, stopping at the first error.
func (s *Service) OpenAll(accounts ...*Account) error {
	sort.SliceStable(accounts, func(i, j int) bool {
		return accounts[i].Depth() < accounts[j].Depth()
	})
	for _, a := range accounts {
		if err := s.Open(a); err != nil {
			return Wrap("open all", err)
		}
		s.log.Printf("opened %s (%s)", a.ID, a.Kind)
	}
	return nil
}

// Post runs the hooks and stores tx, retrying optimistic conflicts.
func (s *Service) Post(ctx context.Context, tx *Transaction) (err error) {
	defer func() {
		for _, h := range s.hooks {
			h.After(ctx, tx, err)
		}
	}()
	if len(tx.Entries) > s.cfg.MaxEntries {
		return newError(CodeLimit, "post", "", fmt.Errorf("%d entries, at most %d", len(tx.Entries), s.cfg.MaxEntries))
	}
	if tx.ID == "" {
		tx.ID = s.ids.Next()
	}
	if tx.Author == "" {
		tx.Author = s.cfg.Author
	}
	tx.Created = s.clock.Now()
	for _, h := range s.hooks {
		if err := h.Before(ctx, tx); err != nil {
			return Wrap("hook", err)
		}
	}
	if !s.reserve(tx.ID) {
		return Conflict("post", "", fmt.Errorf("transaction %s already in flight", tx.ID))
	}
	defer s.release(tx.ID)

	return Retry(ctx, s.cfg.Retry, func(attempt int) error {
		if attempt > 0 {
			s.log.Printf("retrying %s (attempt %d)", tx.ID, attempt+1)
		}
		return s.Store.Post(tx)
	})
}

func (s *Service) reserve(id TxID) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, busy := s.pending[id]; busy {
		return false
	}
	s.pending[id] = struct{}{}
	return true
}

func (s *Service) release(id TxID) {
	s.mu.Lock()
	delete(s.pending, id)
	s.mu.Unlock()
}

// Transfer moves amount between two accounts in one balanced transaction,
// converting currencies through the configured rate table when the
// accounts differ.
func (s *Service) Transfer(ctx context.Context, from, to AccountID, amount Money, memo string) (*Transaction, error) {
	src, err := s.Account(from)
	if err != nil {
		return nil, err
	}
	dst, err := s.Account(to)
	if err != nil {
		return nil, err
	}
	credit := amount
	if dst.Currency != amount.Currency {
		if s.cfg.Rates == nil {
			return nil, Invalid("transfer", "no rates to convert %s into %s", amount.Currency, dst.Currency)
		}
		credit, err = s.cfg.Rates.Convert(amount, dst.Currency)
		if err != nil {
			return nil, Wrap("transfer", err)
		}
	}
	if src.Currency != amount.Currency {
		return nil, Invalid("transfer", "source %s holds %s, not %s", src.ID, src.Currency, amount.Currency)
	}
	tx, err := NewTx(s.ids.Next(), "transfer "+memo).
		By(s.cfg.Author).
		On(s.clock.Now()).
		Credit(from, amount, memo).
		Debit(to, credit, memo).
		Build()
	if err != nil {
		return nil, err
	}
	if err := s.Post(ctx, tx); err != nil {
		return nil, err
	}
	return tx, nil
}

// Revert posts the reversal of an earlier transaction.
func (s *Service) Revert(ctx context.Context, id TxID, author string) (*Transaction, error) {
	orig, err := s.Transaction(id)
	if err != nil {
		return nil, Wrap("revert", err)
	}
	if orig.Reverts != "" {
		return nil, Invalid("revert", "%s is itself a reversal of %s", id, orig.Reverts)
	}
	rev := orig.Reversal(s.ids.Next(), s.clock.Now(), author)
	if err := s.Post(ctx, rev); err != nil {
		return nil, Wrap("revert", err)
	}
	return rev, nil
}

// batchResult is what a worker reports for one transaction.
type batchResult struct {
	index int
	tx    *Transaction
	err   error
}

// PostAll posts many transactions concurrently with a bounded number of
// workers. It returns one error slot per input, in input order, and a
// MultiError summary (nil when everything succeeded).
func (s *Service) PostAll(ctx context.Context, txs []*Transaction) ([]error, error) {
	jobs := make(chan int)
	results := make(chan batchResult, len(txs))

	var wg sync.WaitGroup
	for w := 0; w < s.cfg.Workers; w++ {
		wg.Add(1)
		go func(worker int) {
			defer wg.Done()
			for i := range jobs {
				err := Recover("post all", func() error { return s.Post(ctx, txs[i]) })
				results <- batchResult{index: i, tx: txs[i], err: err}
			}
		}(w)
	}

	go func() {
		defer close(jobs)
		for i := range txs {
			select {
			case jobs <- i:
			case <-ctx.Done():
				return
			}
		}
	}()

	go func() {
		wg.Wait()
		close(results)
	}()

	errs := make([]error, len(txs))
	var multi MultiError
	for r := range results {
		errs[r.index] = r.err
		multi.Append(r.err)
	}
	return errs, multi.ErrorOrNil()
}

// Reconciliation compares a stored balance with an externally reported one.
type Reconciliation struct {
	Account  AccountID
	Ledger   Money
	External Money
	Checked  time.Time
}

// Diff is how far the ledger is from the external figure.
func (r Reconciliation) Diff() Money { return r.Ledger.Sub(r.External) }

// Matches reports whether both agree exactly.
func (r Reconciliation) Matches() bool { return r.Diff().IsZero() }

// Reconcile checks each account against the external statement figure.
func (s *Service) Reconcile(ctx context.Context, statement map[AccountID]Money) ([]Reconciliation, error) {
	ids := make([]AccountID, 0, len(statement))
	for id := range statement {
		ids = append(ids, id)
	}
	sort.Slice(ids, func(i, j int) bool { return ids[i] < ids[j] })

	var out []Reconciliation
	var multi MultiError
	now := s.clock.Now()
Accounts:
	for _, id := range ids {
		select {
		case <-ctx.Done():
			multi.Append(newError(CodeTimeout, "reconcile", id, ctx.Err()))
			break Accounts
		default:
		}
		bal, err := s.Balance(id, time.Time{})
		if err != nil {
			multi.Append(err)
			continue Accounts
		}
		want := statement[id]
		if want.Currency != bal.Currency {
			converted, err := s.convert(want, bal.Currency)
			if err != nil {
				multi.Append(err)
				continue
			}
			want = converted
		}
		out = append(out, Reconciliation{Account: id, Ledger: bal, External: want, Checked: now})
	}
	return out, multi.ErrorOrNil()
}

// convert uses the configured rates, or fails when none are set.
func (s *Service) convert(m Money, to Currency) (Money, error) {
	if s.cfg.Rates == nil {
		return Money{}, Invalid("convert", "no rate table")
	}
	return s.cfg.Rates.Convert(m, to)
}

// Describe returns a one-line label for a value the service handed out.
func Label(v any) string {
	switch x := v.(type) {
	case nil:
		return "<nil>"
	case *Account:
		return fmt.Sprintf("account %s (%s)", x.ID, x.Kind)
	case *Transaction:
		return fmt.Sprintf("transaction %s: %s", x.ID, x.Summary)
	case Entry:
		return fmt.Sprintf("%s %s %s", x.Side, x.Account, x.Amount)
	case Money:
		return x.String()
	case error:
		return "error: " + x.Error()
	case fmt.Stringer:
		return x.String()
	default:
		return fmt.Sprintf("%v", x)
	}
}

// Close shuts the store down when it supports it.
func (s *Service) Close() error {
	if c, ok := s.Store.(interface{ Close() error }); ok {
		return c.Close()
	}
	return nil
}

// AuditHook logs every posted transaction together with its outcome.
func AuditHook(l Logger) Hook {
	return HookFuncs{
		BeforeFunc: func(_ context.Context, tx *Transaction) error {
			return Require(len(tx.Entries) > 0, "audit", "empty transaction")
		},
		AfterFunc: func(_ context.Context, tx *Transaction, err error) {
			if err != nil {
				l.Printf("tx %s failed: %v", tx.ID, Explain(err))
				return
			}
			l.Printf("tx %s posted: %s", tx.ID, Label(tx))
		},
	}
}

// LimitHook rejects transactions whose total exceeds max.
func LimitHook(max Money) Hook {
	return HookFuncs{BeforeFunc: func(_ context.Context, tx *Transaction) error {
		if total := tx.Debits(); total.Cmp(max) > 0 {
			return LimitError{Limit: max, Wanted: total}
		}
		return nil
	}}
}

// Each applies fn to every account matching filter; it demonstrates a method
// value (s.Accounts) being passed around.
func (s *Service) Each(filter func(*Account) bool, fn func(*Account) error) error {
	list := s.Accounts
	accounts, err := list(filter)
	if err != nil {
		return err
	}
	for _, a := range accounts {
		if err := fn(a); err != nil {
			return err
		}
	}
	return nil
}

// Totals returns one balance per account kind, converting into target.
func (s *Service) Totals(target Currency) (map[Kind]Money, error) {
	totals := make(map[Kind]Money)
	err := s.Each(nil, func(a *Account) error {
		bal, err := s.Balance(a.ID, time.Time{})
		if err != nil {
			return err
		}
		if bal.Currency != target {
			if bal, err = s.convert(bal, target); err != nil {
				return err
			}
		}
		totals[a.Kind] = totals[a.Kind].Add(bal)
		return nil
	})
	return totals, err
}

// Snapshot captures every balance at one instant using the Map helper with
// explicit type arguments.
func (s *Service) Snapshot(at time.Time) (map[AccountID]Money, error) {
	accounts, err := s.Accounts(nil)
	if err != nil {
		return nil, err
	}
	ids := Map[*Account, AccountID](accounts, func(a *Account) AccountID { return a.ID })
	snap := make(map[AccountID]Money, len(ids))
	for _, id := range ids {
		bal, err := s.Balance(id, at)
		if err != nil {
			return nil, err
		}
		snap[id] = bal
	}
	return snap, nil
}
