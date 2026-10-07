package ledger

import (
	"fmt"
	"sort"
	"strings"
	"time"
)

// AccountID identifies an account; by convention it is a dotted path such as
// "assets.cash.till1".
type AccountID string

// TxID identifies a transaction.
type TxID string

// Tags is a free-form label set attached to accounts and transactions.
type Tags map[string]string

// Amount is another name for Money, kept for callers written against the
// first version of the API.
type Amount = Money

// Kind is the accounting class of an account.
type Kind uint8

const (
	Asset Kind = iota + 1
	Liability
	Equity
	Income
	Expense
)

// kindNames is indexed by Kind.
var kindNames = map[Kind]string{
	Asset:     "asset",
	Liability: "liability",
	Equity:    "equity",
	Income:    "income",
	Expense:   "expense",
}

// String implements fmt.Stringer.
func (k Kind) String() string {
	if name, ok := kindNames[k]; ok {
		return name
	}
	return fmt.Sprintf("kind(%d)", uint8(k))
}

// NormalSide is the side on which the account's balance grows: debit for
// assets and expenses, credit for everything else.
func (k Kind) NormalSide() Side {
	if k == Asset || k == Expense {
		return Debit
	}
	return Credit
}

// ParseKind is the inverse of Kind.String.
func ParseKind(s string) (Kind, error) {
	for kind, name := range kindNames {
		if strings.EqualFold(name, s) {
			return kind, nil
		}
	}
	return 0, Invalid("parse kind", "unknown account kind %q", s)
}

// Side tells whether an entry debits or credits its account.
type Side bool

const (
	Debit  Side = true
	Credit Side = false
)

// Opposite flips the side.
func (s Side) Opposite() Side { return !s }

func (s Side) String() string {
	if s == Debit {
		return "debit"
	}
	return "credit"
}

// Account is one node of the chart of accounts.
type Account struct {
	ID       AccountID
	Name     string
	Kind     Kind
	Currency Currency
	Parent   AccountID
	Limit    Money
	Tags     Tags
	Opened   time.Time
	Closed   time.Time
	version  uint64
}

// IsOpen reports whether the account accepts postings at t.
func (a *Account) IsOpen(t time.Time) bool {
	if t.Before(a.Opened) {
		return false
	}
	return a.Closed.IsZero() || t.Before(a.Closed)
}

// Version is the optimistic-concurrency counter maintained by the store.
func (a *Account) Version() uint64 { return a.version }

// Path splits the ID into its dotted components.
func (a *Account) Path() []string { return strings.Split(string(a.ID), ".") }

// Depth is the number of components of the ID.
func (a *Account) Depth() int { return len(a.Path()) }

// IsChildOf reports whether a lies below ancestor in the chart.
func (a *Account) IsChildOf(ancestor AccountID) bool {
	return strings.HasPrefix(string(a.ID), string(ancestor)+".")
}

// Clone returns a deep copy, so callers cannot mutate the stored account.
func (a *Account) Clone() *Account {
	cp := *a
	if a.Tags != nil {
		cp.Tags = make(Tags, len(a.Tags))
		for k, v := range a.Tags {
			cp.Tags[k] = v
		}
	}
	return &cp
}

// Validate checks the account's own invariants.
func (a *Account) Validate() error {
	return Check("validate account",
		func() error { return Require(a.ID != "", "validate account", "empty id") },
		func() error { return Require(a.Name != "", "validate account", "empty name") },
		func() error { return Require(a.Kind >= Asset && a.Kind <= Expense, "validate account", "bad kind") },
		func() error { return Require(a.Currency.Valid(), "validate account", "bad currency") },
		func() error {
			if a.Limit.Negative() {
				return Invalid("validate account", "negative limit %s", a.Limit)
			}
			return nil
		},
	)
}

// AccountOption configures a new account.
type AccountOption func(*Account)

// WithParent places the account below parent.
func WithParent(parent AccountID) AccountOption {
	return func(a *Account) { a.Parent = parent }
}

// WithLimit sets the overdraft limit.
func WithLimit(limit Money) AccountOption {
	return func(a *Account) { a.Limit = limit }
}

// WithTags merges tags into the account.
func WithTags(pairs ...string) AccountOption {
	return func(a *Account) {
		if a.Tags == nil {
			a.Tags = make(Tags)
		}
		for i := 0; i+1 < len(pairs); i += 2 {
			a.Tags[pairs[i]] = pairs[i+1]
		}
	}
}

// OpenedAt overrides the opening time, which defaults to now.
func OpenedAt(t time.Time) AccountOption {
	return func(a *Account) { a.Opened = t }
}

// NewAccount builds and validates an account.
func NewAccount(id AccountID, name string, kind Kind, currency Currency, opts ...AccountOption) (*Account, error) {
	a := &Account{
		ID:       id,
		Name:     name,
		Kind:     kind,
		Currency: currency,
		Limit:    Zero(currency),
		Opened:   time.Now(),
	}
	for _, opt := range opts {
		opt(a)
	}
	if err := a.Validate(); err != nil {
		return nil, err
	}
	return a, nil
}

// Entry is one line of a transaction: an amount posted to an account.
type Entry struct {
	Account AccountID
	Side    Side
	Amount  Money
	Memo    string
}

// Signed is the amount with a minus sign for credits, so a balanced
// transaction sums to zero.
func (e Entry) Signed() Money {
	if e.Side == Credit {
		return e.Amount.Neg()
	}
	return e.Amount
}

// Meta is audit information embedded in every transaction.
type Meta struct {
	Created time.Time
	Author  string
	Source  string
	Tags
}

// Describe summarises the metadata for logs.
func (m Meta) Describe() string {
	parts := []string{m.Author}
	if m.Source != "" {
		parts = append(parts, "via "+m.Source)
	}
	if len(m.Tags) > 0 {
		keys := make([]string, 0, len(m.Tags))
		for k := range m.Tags {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		parts = append(parts, "tags "+strings.Join(keys, ","))
	}
	return strings.Join(parts, " ")
}

// Transaction is a balanced set of entries posted at one moment.
type Transaction struct {
	Meta
	ID      TxID
	Date    time.Time
	Summary string
	Entries []Entry
	Reverts TxID
}

// Debits adds up the debit entries.
func (t *Transaction) Debits() Money {
	var sum Money
	for _, e := range t.Entries {
		if e.Side == Debit {
			sum = sum.Add(e.Amount)
		}
	}
	return sum
}

// Credits adds up the credit entries.
func (t *Transaction) Credits() Money {
	var sum Money
	for _, e := range t.Entries {
		if e.Side == Credit {
			sum = sum.Add(e.Amount)
		}
	}
	return sum
}

// Balanced reports whether debits equal credits.
func (t *Transaction) Balanced() bool {
	return t.Debits().Cmp(t.Credits()) == 0
}

// Accounts lists the distinct accounts the transaction touches, in order of
// first appearance.
func (t *Transaction) Accounts() []AccountID {
	var out []AccountID
	seen := make(map[AccountID]bool, len(t.Entries))
	for _, e := range t.Entries {
		if !seen[e.Account] {
			seen[e.Account] = true
			out = append(out, e.Account)
		}
	}
	return out
}

// Validate checks that the transaction is well formed and balanced.
func (t *Transaction) Validate() error {
	var multi MultiError
	if len(t.Entries) < 2 {
		multi.Append(Invalid("validate transaction", "need at least two entries, got %d", len(t.Entries)))
	}
	for i, e := range t.Entries {
		if e.Account == "" {
			multi.Append(Invalid("validate transaction", "entry %d has no account", i))
		}
		if e.Amount.Negative() {
			multi.Append(Invalid("validate transaction", "entry %d is negative: %s", i, e.Amount))
		}
	}
	if len(t.Entries) >= 2 && !t.Balanced() {
		multi.Append(UnbalancedError{Transaction: t.ID, Debits: t.Debits(), Credits: t.Credits()})
	}
	return multi.ErrorOrNil()
}

// Reversal builds the transaction that undoes t.
func (t *Transaction) Reversal(id TxID, when time.Time, author string) *Transaction {
	rev := &Transaction{
		Meta:    Meta{Created: when, Author: author, Source: "reversal"},
		ID:      id,
		Date:    when,
		Summary: "reversal of " + string(t.ID),
		Reverts: t.ID,
	}
	for _, e := range t.Entries {
		rev.Entries = append(rev.Entries, Entry{
			Account: e.Account,
			Side:    e.Side.Opposite(),
			Amount:  e.Amount,
			Memo:    "reverses: " + e.Memo,
		})
	}
	return rev
}

// Builder assembles a transaction fluently.
type Builder struct {
	tx  Transaction
	err error
}

// NewTx starts a transaction with the given ID and summary.
func NewTx(id TxID, summary string) *Builder {
	return &Builder{tx: Transaction{ID: id, Summary: summary, Date: time.Now()}}
}

// On sets the transaction date.
func (b *Builder) On(date time.Time) *Builder {
	b.tx.Date = date
	return b
}

// By records the author.
func (b *Builder) By(author string) *Builder {
	b.tx.Author = author
	b.tx.Created = time.Now()
	return b
}

// Debit adds a debit entry.
func (b *Builder) Debit(account AccountID, amount Money, memo string) *Builder {
	return b.add(Entry{Account: account, Side: Debit, Amount: amount, Memo: memo})
}

// Credit adds a credit entry.
func (b *Builder) Credit(account AccountID, amount Money, memo string) *Builder {
	return b.add(Entry{Account: account, Side: Credit, Amount: amount, Memo: memo})
}

func (b *Builder) add(e Entry) *Builder {
	if b.err != nil {
		return b
	}
	if e.Amount.Negative() {
		b.err = Invalid("build transaction", "negative amount %s", e.Amount)
		return b
	}
	b.tx.Entries = append(b.tx.Entries, e)
	return b
}

// Build validates and returns the transaction.
func (b *Builder) Build() (*Transaction, error) {
	if b.err != nil {
		return nil, b.err
	}
	tx := b.tx
	if err := tx.Validate(); err != nil {
		return nil, Wrap("build transaction", err)
	}
	return &tx, nil
}

// Store persists accounts and transactions. Implementations must be safe for
// concurrent use.
type Store interface {
	Open(account *Account) error
	Account(id AccountID) (*Account, error)
	Accounts(filter func(*Account) bool) ([]*Account, error)
	Post(tx *Transaction) error
	Transaction(id TxID) (*Transaction, error)
	History(id AccountID, from, to time.Time) ([]Entry, error)
	Balance(id AccountID, at time.Time) (Money, error)
}

// Clock abstracts time.Now so tests can control it.
type Clock interface {
	Now() time.Time
}

// Validator is anything that can check itself; both Account and Transaction
// satisfy it without declaring so.
type Validator interface {
	Validate() error
}

// IDGenerator hands out unique transaction IDs.
type IDGenerator interface {
	Next() TxID
}

// ClockFunc adapts a function to Clock.
type ClockFunc func() time.Time

// Now implements Clock.
func (f ClockFunc) Now() time.Time { return f() }

// systemClock reads the wall clock.
type systemClock struct{}

func (systemClock) Now() time.Time { return time.Now() }

// SystemClock is the Clock used when none is configured.
var SystemClock Clock = systemClock{}

// FixedClock always reports the same instant.
type FixedClock struct{ At time.Time }

// Now implements Clock.
func (c FixedClock) Now() time.Time { return c.At }

// Advance returns a clock d later.
func (c FixedClock) Advance(d time.Duration) FixedClock { return FixedClock{At: c.At.Add(d)} }

// compile-time interface checks.
var (
	_ Validator = (*Account)(nil)
	_ Validator = (*Transaction)(nil)
	_ Clock     = FixedClock{}
	_ Clock     = ClockFunc(nil)
)
