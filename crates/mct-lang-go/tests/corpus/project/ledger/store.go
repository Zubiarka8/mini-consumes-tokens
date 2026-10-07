package ledger

import (
	"context"
	"sort"
	"sync"
	"sync/atomic"
	"time"
)

// Index is a small generic map with a stable iteration order, used for the
// secondary indexes of MemoryStore.
type Index[K comparable, V any] struct {
	mu    sync.RWMutex
	items map[K]V
	order []K
}

// NewIndex creates an empty index.
func NewIndex[K comparable, V any]() *Index[K, V] {
	return &Index[K, V]{items: make(map[K]V)}
}

// Put stores value under key, remembering first-insertion order.
func (ix *Index[K, V]) Put(key K, value V) {
	ix.mu.Lock()
	defer ix.mu.Unlock()
	if _, exists := ix.items[key]; !exists {
		ix.order = append(ix.order, key)
	}
	ix.items[key] = value
}

// Get returns the value stored under key.
func (ix *Index[K, V]) Get(key K) (V, bool) {
	ix.mu.RLock()
	defer ix.mu.RUnlock()
	v, ok := ix.items[key]
	return v, ok
}

// Delete removes key.
func (ix *Index[K, V]) Delete(key K) {
	ix.mu.Lock()
	defer ix.mu.Unlock()
	if _, ok := ix.items[key]; !ok {
		return
	}
	delete(ix.items, key)
	for i, k := range ix.order {
		if k == key {
			ix.order = append(ix.order[:i], ix.order[i+1:]...)
			break
		}
	}
}

// Len is the number of stored keys.
func (ix *Index[K, V]) Len() int {
	ix.mu.RLock()
	defer ix.mu.RUnlock()
	return len(ix.items)
}

// Each calls fn for every entry in insertion order until it returns false.
func (ix *Index[K, V]) Each(fn func(K, V) bool) {
	ix.mu.RLock()
	keys := append([]K(nil), ix.order...)
	ix.mu.RUnlock()
	for _, k := range keys {
		v, ok := ix.Get(k)
		if !ok {
			continue
		}
		if !fn(k, v) {
			return
		}
	}
}

// Stack is a minimal LIFO used to undo partially applied postings.
type Stack[T any] struct {
	items []T
}

// Push adds an element.
func (s *Stack[T]) Push(v T) { s.items = append(s.items, v) }

// Pop removes and returns the last element.
func (s *Stack[T]) Pop() (T, bool) {
	var zero T
	if len(s.items) == 0 {
		return zero, false
	}
	last := s.items[len(s.items)-1]
	s.items = s.items[:len(s.items)-1]
	return last, true
}

// Len is the number of elements.
func (s *Stack[T]) Len() int { return len(s.items) }

// Drain pops every element, calling fn on each.
func (s *Stack[T]) Drain(fn func(T)) {
	for {
		v, ok := s.Pop()
		if !ok {
			return
		}
		fn(v)
	}
}

// Pair couples two values of possibly different types.
type Pair[A, B any] struct {
	First  A
	Second B
}

// Swap returns the pair reversed.
func (p Pair[A, B]) Swap() Pair[B, A] { return Pair[B, A]{First: p.Second, Second: p.First} }

// posting is a stored entry with the transaction it belongs to.
type posting struct {
	tx    TxID
	date  time.Time
	entry Entry
}

// Event describes a change to the store; subscribers receive them on a
// channel.
type Event struct {
	Kind    string
	Account AccountID
	Tx      TxID
	At      time.Time
}

// MemoryStore is the in-memory Store used by tests, the examples and the
// command line tool.
type MemoryStore struct {
	mu       sync.RWMutex
	clock    Clock
	accounts *Index[AccountID, *Account]
	txs      *Index[TxID, *Transaction]
	postings map[AccountID][]posting
	subs     []chan Event
	version  atomic.Uint64
	closed   bool
}

// NewMemoryStore creates an empty store reading time from clock.
func NewMemoryStore(clock Clock) *MemoryStore {
	if clock == nil {
		clock = SystemClock
	}
	return &MemoryStore{
		clock:    clock,
		accounts: NewIndex[AccountID, *Account](),
		txs:      NewIndex[TxID, *Transaction](),
		postings: make(map[AccountID][]posting),
	}
}

// Subscribe returns a channel of events; the channel is closed by Close.
func (s *MemoryStore) Subscribe(buffer int) <-chan Event {
	s.mu.Lock()
	defer s.mu.Unlock()
	ch := make(chan Event, buffer)
	s.subs = append(s.subs, ch)
	return ch
}

// publish delivers an event without ever blocking the store: a slow
// subscriber loses events instead of stalling a posting.
func (s *MemoryStore) publish(ev Event) {
	for _, ch := range s.subs {
		select {
		case ch <- ev:
		default:
		}
	}
}

// Close releases the subscribers; further operations fail with ErrClosed.
func (s *MemoryStore) Close() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return nil
	}
	s.closed = true
	for _, ch := range s.subs {
		close(ch)
	}
	s.subs = nil
	return nil
}

// Open registers a new account.
func (s *MemoryStore) Open(account *Account) error {
	if err := account.Validate(); err != nil {
		return Wrap("open", err)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return newError(CodeClosed, "open", account.ID, ErrClosed)
	}
	if _, exists := s.accounts.Get(account.ID); exists {
		return Conflict("open", account.ID, ErrConflict)
	}
	if account.Parent != "" {
		if _, ok := s.accounts.Get(account.Parent); !ok {
			return NotFound("open parent", account.Parent)
		}
	}
	stored := account.Clone()
	stored.version = s.version.Add(1)
	s.accounts.Put(stored.ID, stored)
	s.publish(Event{Kind: "open", Account: stored.ID, At: s.clock.Now()})
	return nil
}

// Account returns a copy of the stored account.
func (s *MemoryStore) Account(id AccountID) (*Account, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	a, ok := s.accounts.Get(id)
	if !ok {
		return nil, NotFound("account", id)
	}
	return a.Clone(), nil
}

// Accounts returns copies of the accounts accepted by filter, sorted by ID.
func (s *MemoryStore) Accounts(filter func(*Account) bool) ([]*Account, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	var out []*Account
	s.accounts.Each(func(_ AccountID, a *Account) bool {
		if filter == nil || filter(a) {
			out = append(out, a.Clone())
		}
		return true
	})
	sort.Slice(out, func(i, j int) bool { return out[i].ID < out[j].ID })
	return out, nil
}

// Post stores a validated transaction atomically: either every entry lands
// or none does.
func (s *MemoryStore) Post(tx *Transaction) error {
	if err := tx.Validate(); err != nil {
		return Wrap("post", err)
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.closed {
		return newError(CodeClosed, "post", "", ErrClosed)
	}
	if _, dup := s.txs.Get(tx.ID); dup {
		return Conflict("post", "", ErrConflict)
	}

	var applied Stack[posting]
	rollback := func() {
		applied.Drain(func(p posting) {
			list := s.postings[p.entry.Account]
			s.postings[p.entry.Account] = list[:len(list)-1]
		})
	}

	for _, e := range tx.Entries {
		acct, ok := s.accounts.Get(e.Account)
		if !ok {
			rollback()
			return NotFound("post", e.Account)
		}
		if !acct.IsOpen(tx.Date) {
			rollback()
			return Closed("post", e.Account)
		}
		if acct.Currency != e.Amount.Currency {
			rollback()
			return Invalid("post", "account %s holds %s, entry is %s", acct.ID, acct.Currency, e.Amount.Currency)
		}
		p := posting{tx: tx.ID, date: tx.Date, entry: e}
		s.postings[e.Account] = append(s.postings[e.Account], p)
		applied.Push(p)

		if bal := s.balanceLocked(e.Account, time.Time{}); !acct.Limit.IsZero() || bal.Negative() {
			if exceeds(acct, bal) {
				rollback()
				return LimitError{Account: acct.ID, Limit: acct.Limit, Wanted: bal.Abs()}
			}
		}
	}

	stored := *tx
	stored.Entries = append([]Entry(nil), tx.Entries...)
	s.txs.Put(tx.ID, &stored)
	for _, id := range tx.Accounts() {
		if acct, ok := s.accounts.Get(id); ok {
			acct.version = s.version.Add(1)
		}
	}
	s.publish(Event{Kind: "post", Tx: tx.ID, At: s.clock.Now()})
	return nil
}

// exceeds reports whether a balance on the wrong side is larger than the
// account's limit.
func exceeds(a *Account, bal Money) bool {
	if bal.Negative() {
		return bal.Abs().Cmp(a.Limit) > 0
	}
	return false
}

// Transaction returns a copy of a stored transaction.
func (s *MemoryStore) Transaction(id TxID) (*Transaction, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	tx, ok := s.txs.Get(id)
	if !ok {
		return nil, newError(CodeNotFound, "transaction", "", ErrNotFound)
	}
	cp := *tx
	cp.Entries = append([]Entry(nil), tx.Entries...)
	return &cp, nil
}

// History lists the entries posted to an account in [from, to). A zero bound
// is open.
func (s *MemoryStore) History(id AccountID, from, to time.Time) ([]Entry, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if _, ok := s.accounts.Get(id); !ok {
		return nil, NotFound("history", id)
	}
	var out []Entry
	for _, p := range s.postings[id] {
		if !from.IsZero() && p.date.Before(from) {
			continue
		}
		if !to.IsZero() && !p.date.Before(to) {
			continue
		}
		out = append(out, p.entry)
	}
	return out, nil
}

// Balance returns the account balance at a moment, positive on the
// account's normal side. A zero time means "now".
func (s *MemoryStore) Balance(id AccountID, at time.Time) (Money, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	if _, ok := s.accounts.Get(id); !ok {
		return Money{}, NotFound("balance", id)
	}
	return s.balanceLocked(id, at), nil
}

// balanceLocked sums postings; the caller holds s.mu.
func (s *MemoryStore) balanceLocked(id AccountID, at time.Time) Money {
	acct, _ := s.accounts.Get(id)
	total := Zero(acct.Currency)
	normal := acct.Kind.NormalSide()
	for _, p := range s.postings[id] {
		if !at.IsZero() && p.date.After(at) {
			continue
		}
		if p.entry.Side == normal {
			total = total.Add(p.entry.Amount)
		} else {
			total = total.Sub(p.entry.Amount)
		}
	}
	return total
}

// Len returns the number of accounts and transactions.
func (s *MemoryStore) Len() (accounts, transactions int) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.accounts.Len(), s.txs.Len()
}

// Walk calls fn for every transaction in posting order until it returns an
// error or ctx ends.
func (s *MemoryStore) Walk(ctx context.Context, fn func(*Transaction) error) error {
	var txs []*Transaction
	s.mu.RLock()
	s.txs.Each(func(_ TxID, tx *Transaction) bool {
		txs = append(txs, tx)
		return true
	})
	s.mu.RUnlock()
	for _, tx := range txs {
		select {
		case <-ctx.Done():
			return newError(CodeTimeout, "walk", "", ctx.Err())
		default:
		}
		if err := fn(tx); err != nil {
			return err
		}
	}
	return nil
}

// Stream sends every transaction on the returned channel from a goroutine
// and closes it when done; cancel ctx to stop early.
func (s *MemoryStore) Stream(ctx context.Context) <-chan Result[*Transaction] {
	out := make(chan Result[*Transaction])
	go func() {
		defer close(out)
		err := s.Walk(ctx, func(tx *Transaction) error {
			select {
			case out <- Ok(tx):
				return nil
			case <-ctx.Done():
				return ctx.Err()
			}
		})
		if err != nil {
			select {
			case out <- Fail[*Transaction](err):
			case <-ctx.Done():
			}
		}
	}()
	return out
}

// SequentialIDs hands out "tx-000001", "tx-000002", ….
type SequentialIDs struct {
	prefix string
	n      atomic.Uint64
}

// NewSequentialIDs creates a generator with the given prefix.
func NewSequentialIDs(prefix string) *SequentialIDs {
	return &SequentialIDs{prefix: prefix}
}

// Next implements IDGenerator.
func (g *SequentialIDs) Next() TxID {
	n := g.n.Add(1)
	digits := []byte("000000")
	for i := len(digits) - 1; i >= 0 && n > 0; i-- {
		digits[i] = byte('0' + n%10)
		n /= 10
	}
	return TxID(g.prefix + "-" + string(digits))
}

var (
	_ Store       = (*MemoryStore)(nil)
	_ IDGenerator = (*SequentialIDs)(nil)
)
