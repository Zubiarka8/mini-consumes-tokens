package ledger

import (
	"bufio"
	"fmt"
	"io"
	"sort"
	"strings"
	"text/tabwriter"
	"time"
)

// Map applies fn to every element.
func Map[T, U any](in []T, fn func(T) U) []U {
	out := make([]U, 0, len(in))
	for _, v := range in {
		out = append(out, fn(v))
	}
	return out
}

// Filter keeps the elements fn accepts.
func Filter[T any](in []T, fn func(T) bool) []T {
	var out []T
	for _, v := range in {
		if fn(v) {
			out = append(out, v)
		}
	}
	return out
}

// Reduce folds the slice into one value.
func Reduce[T, A any](in []T, init A, fn func(A, T) A) A {
	acc := init
	for _, v := range in {
		acc = fn(acc, v)
	}
	return acc
}

// GroupBy buckets elements by key, preserving order inside each bucket.
func GroupBy[T any, K comparable](in []T, key func(T) K) map[K][]T {
	groups := make(map[K][]T)
	for _, v := range in {
		k := key(v)
		groups[k] = append(groups[k], v)
	}
	return groups
}

// Keys returns the keys of m in sorted order.
func Keys[K interface{ ~string | ~int }, V any](m map[K]V) []K {
	keys := make([]K, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool { return keys[i] < keys[j] })
	return keys
}

// Row is one line of a balance report.
type Row struct {
	Account AccountID
	Name    string
	Kind    Kind
	Depth   int
	Balance Money
}

// Report renders ledger data as text. It only reads from the Store.
type Report struct {
	store Store
	clock Clock
	rates *RateTable
	Title string
	Width int
}

// NewReport creates a report over store; rates may be nil when everything
// is in one currency.
func NewReport(store Store, clock Clock, rates *RateTable) *Report {
	if clock == nil {
		clock = SystemClock
	}
	return &Report{store: store, clock: clock, rates: rates, Width: 72}
}

// rows collects one Row per account, in chart order.
func (r *Report) rows(at time.Time) ([]Row, error) {
	accounts, err := r.store.Accounts(nil)
	if err != nil {
		return nil, err
	}
	rows := make([]Row, 0, len(accounts))
	for _, a := range accounts {
		bal, err := r.store.Balance(a.ID, at)
		if err != nil {
			return nil, Wrap("report rows", err)
		}
		rows = append(rows, Row{
			Account: a.ID,
			Name:    a.Name,
			Kind:    a.Kind,
			Depth:   a.Depth(),
			Balance: bal,
		})
	}
	return rows, nil
}

// TrialBalance lists every account with its debit or credit balance and
// checks that the two columns agree.
func (r *Report) TrialBalance(w io.Writer, at time.Time) error {
	rows, err := r.rows(at)
	if err != nil {
		return err
	}
	tw := tabwriter.NewWriter(w, 0, 4, 2, ' ', tabwriter.AlignRight|tabwriter.Debug)
	defer tw.Flush()
	fmt.Fprintln(tw, "Account\tName\tDebit\tCredit\t")

	var debit, credit Money
	for _, row := range rows {
		var d, c string
		if row.Kind.NormalSide() == Debit {
			d = row.Balance.Format(false)
			debit = debit.Add(row.Balance)
		} else {
			c = row.Balance.Format(false)
			credit = credit.Add(row.Balance)
		}
		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t\n", row.Account, row.Name, d, c)
	}
	fmt.Fprintf(tw, "\tTotal\t%s\t%s\t\n", debit.Format(false), credit.Format(false))
	if debit.Cmp(credit) != 0 {
		return UnbalancedError{Debits: debit, Credits: credit}
	}
	return nil
}

// BalanceSheet prints assets against liabilities plus equity.
func (r *Report) BalanceSheet(w io.Writer, at time.Time) error {
	rows, err := r.rows(at)
	if err != nil {
		return err
	}
	groups := GroupBy(rows, func(row Row) Kind { return row.Kind })
	bw := bufio.NewWriter(w)
	defer bw.Flush()

	section := func(title string, kinds ...Kind) Money {
		var total Money
		fmt.Fprintf(bw, "%s\n%s\n", title, strings.Repeat("-", len(title)))
		for _, kind := range kinds {
			for _, row := range groups[kind] {
				indent := strings.Repeat("  ", row.Depth-1)
				fmt.Fprintf(bw, "%s%-*s %12s\n", indent, r.Width-14-len(indent), row.Name, row.Balance.Format(true))
				total = total.Add(row.Balance)
			}
		}
		fmt.Fprintf(bw, "%-*s %12s\n\n", r.Width-13, "Total "+strings.ToLower(title), total.Format(true))
		return total
	}

	assets := section("Assets", Asset)
	claims := section("Liabilities", Liability).Add(section("Equity", Equity))
	if assets.Cmp(claims) != 0 {
		return UnbalancedError{Debits: assets, Credits: claims}
	}
	return nil
}

// IncomeStatement prints income minus expenses for a period.
func (r *Report) IncomeStatement(w io.Writer, from, to time.Time) error {
	accounts, err := r.store.Accounts(func(a *Account) bool {
		return a.Kind == Income || a.Kind == Expense
	})
	if err != nil {
		return err
	}
	var income, expense Money
	for _, a := range accounts {
		entries, err := r.store.History(a.ID, from, to)
		if err != nil {
			return err
		}
		var net Money
		for _, e := range entries {
			if e.Side == a.Kind.NormalSide() {
				net = net.Add(e.Amount)
			} else {
				net = net.Sub(e.Amount)
			}
		}
		fmt.Fprintf(w, "%-30s %12s\n", a.Name, net.Format(true))
		if a.Kind == Income {
			income = income.Add(net)
		} else {
			expense = expense.Add(net)
		}
	}
	fmt.Fprintf(w, "%-30s %12s\n", "Net result", income.Sub(expense).Format(true))
	return nil
}

// Statement lists an account's entries with a running balance.
type Statement struct {
	Account *Account
	Lines   []StatementLine
	Opening Money
	Closing Money
}

// StatementLine is one entry plus the balance after it.
type StatementLine struct {
	Date    time.Time
	Entry   Entry
	Running Money
}

// StatementFor builds the statement of one account for [from, to).
func (r *Report) StatementFor(id AccountID, from, to time.Time) (*Statement, error) {
	acct, err := r.store.Account(id)
	if err != nil {
		return nil, err
	}
	opening, err := r.store.Balance(id, from.Add(-time.Nanosecond))
	if err != nil {
		return nil, err
	}
	entries, err := r.store.History(id, from, to)
	if err != nil {
		return nil, err
	}
	st := &Statement{Account: acct, Opening: opening}
	running := opening
	normal := acct.Kind.NormalSide()
	for _, e := range entries {
		if e.Side == normal {
			running = running.Add(e.Amount)
		} else {
			running = running.Sub(e.Amount)
		}
		st.Lines = append(st.Lines, StatementLine{Entry: e, Running: running})
	}
	st.Closing = running
	return st, nil
}

// WriteTo renders the statement; it implements io.WriterTo.
func (st *Statement) WriteTo(w io.Writer) (int64, error) {
	cw := &countingWriter{w: w}
	fmt.Fprintf(cw, "Statement for %s (%s)\n", st.Account.Name, st.Account.ID)
	fmt.Fprintf(cw, "Opening balance: %s\n", st.Opening)
	for _, line := range st.Lines {
		fmt.Fprintf(cw, "  %-6s %12s %12s  %s\n",
			line.Entry.Side, line.Entry.Amount.Format(false), line.Running.Format(false), line.Entry.Memo)
	}
	fmt.Fprintf(cw, "Closing balance: %s\n", st.Closing)
	return cw.n, cw.err
}

// countingWriter remembers how many bytes passed through and the first
// error.
type countingWriter struct {
	w   io.Writer
	n   int64
	err error
}

func (c *countingWriter) Write(p []byte) (int, error) {
	if c.err != nil {
		return 0, c.err
	}
	n, err := c.w.Write(p)
	c.n += int64(n)
	c.err = err
	return n, err
}

// Convert expresses every row in currency to using the report's rates.
func (r *Report) Convert(rows []Row, to Currency) ([]Row, error) {
	if r.rates == nil {
		return nil, Invalid("report convert", "no rate table")
	}
	var multi MultiError
	out := Map(rows, func(row Row) Row {
		converted, err := r.rates.Convert(row.Balance, to)
		if err != nil {
			multi.Append(err)
			return row
		}
		row.Balance = converted
		return row
	})
	return out, multi.ErrorOrNil()
}

// Largest returns the n rows with the biggest absolute balance.
func Largest(rows []Row, n int) []Row {
	sorted := append([]Row(nil), rows...)
	sort.Slice(sorted, func(i, j int) bool {
		return sorted[i].Balance.Abs().Cmp(sorted[j].Balance.Abs()) > 0
	})
	if n < len(sorted) {
		sorted = sorted[:n]
	}
	return sorted
}

// Summary is a compact view of the books for dashboards.
type Summary struct {
	Accounts     int
	Transactions int
	ByKind       map[Kind]Money
	Oldest       time.Time
	Newest       time.Time
}

// Summarize walks the rows and the store once.
func (r *Report) Summarize(at time.Time) (Summary, error) {
	rows, err := r.rows(at)
	if err != nil {
		return Summary{}, err
	}
	s := Summary{Accounts: len(rows), ByKind: make(map[Kind]Money)}
	for _, row := range rows {
		s.ByKind[row.Kind] = s.ByKind[row.Kind].Add(row.Balance)
	}
	if ms, ok := r.store.(*MemoryStore); ok {
		_, s.Transactions = ms.Len()
	}
	return s, nil
}

// Format prints the summary on one line per kind, in a fixed order.
func (s Summary) Format(w io.Writer) {
	fmt.Fprintf(w, "%d accounts, %d transactions\n", s.Accounts, s.Transactions)
	for _, kind := range []Kind{Asset, Liability, Equity, Income, Expense} {
		if total, ok := s.ByKind[kind]; ok {
			fmt.Fprintf(w, "  %-10s %s\n", kind, total)
		}
	}
}

// Names returns the display names of the rows.
func Names(rows []Row) []string {
	return Map(rows, Row.name)
}

func (row Row) name() string { return strings.TrimSpace(row.Name) }

// Positive keeps rows with a positive balance, using a method expression.
func Positive(rows []Row) []Row {
	return Filter(rows, func(row Row) bool { return !row.Balance.Negative() && !row.Balance.IsZero() })
}

// Net sums the signed balances of the rows.
func Net(rows []Row) Money {
	return Reduce(rows, Money{}, func(acc Money, row Row) Money { return acc.Add(row.Balance) })
}
