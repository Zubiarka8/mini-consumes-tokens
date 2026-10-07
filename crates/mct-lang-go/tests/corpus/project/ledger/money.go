package ledger

import (
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"sort"
	"strconv"
	"strings"
	"unicode"
)

// Currency is an ISO-4217 style three-letter code.
type Currency string

// A few currencies the examples and tests use.
const (
	EUR Currency = "EUR"
	USD Currency = "USD"
	GBP Currency = "GBP"
	JPY Currency = "JPY"
	CHF Currency = "CHF"
)

// Minor-unit exponents; anything missing defaults to two decimals.
var exponents = map[Currency]int{
	JPY: 0,
	EUR: 2,
	USD: 2,
	GBP: 2,
	CHF: 2,
}

// Exponent is the number of decimal places of the currency's minor unit.
func (c Currency) Exponent() int {
	if e, ok := exponents[c]; ok {
		return e
	}
	return 2
}

// Valid reports whether c looks like a currency code.
func (c Currency) Valid() bool {
	if len(c) != 3 {
		return false
	}
	for _, r := range c {
		if !unicode.IsUpper(r) {
			return false
		}
	}
	return true
}

// Symbol is the display prefix used by Money.Format.
func (c Currency) Symbol() string {
	switch c {
	case EUR:
		return "€"
	case USD:
		return "$"
	case GBP:
		return "£"
	case JPY:
		return "¥"
	}
	return string(c) + " "
}

// Money is an amount in the minor unit of a currency (cents for EUR). The
// zero value is zero of the empty currency, which adds to anything.
type Money struct {
	Units    int64
	Currency Currency
}

// Zero returns the zero amount of currency c.
func Zero(c Currency) Money { return Money{Currency: c} }

// Of builds a Money from a major and minor part: Of(EUR, 12, 50) is 12.50.
func Of(c Currency, major, minor int64) Money {
	scale := pow10(c.Exponent())
	if major < 0 {
		return Money{Units: major*scale - minor, Currency: c}
	}
	return Money{Units: major*scale + minor, Currency: c}
}

func pow10(n int) int64 {
	result := int64(1)
	for i := 0; i < n; i++ {
		result *= 10
	}
	return result
}

// IsZero reports whether the amount is zero.
func (m Money) IsZero() bool { return m.Units == 0 }

// Negative reports whether the amount is below zero.
func (m Money) Negative() bool { return m.Units < 0 }

// Neg returns -m.
func (m Money) Neg() Money { return Money{Units: -m.Units, Currency: m.Currency} }

// Abs returns |m|.
func (m Money) Abs() Money {
	if m.Units < 0 {
		return m.Neg()
	}
	return m
}

// unify picks the currency of a binary operation and panics, like integer
// division by zero, when two different currencies meet: mixing them is a
// programming error, not a runtime condition.
func unify(a, b Money) Currency {
	switch {
	case a.Currency == b.Currency:
		return a.Currency
	case a.Currency == "":
		return b.Currency
	case b.Currency == "":
		return a.Currency
	}
	panic(fmt.Sprintf("ledger: mixed currencies %s and %s", a.Currency, b.Currency))
}

// Add returns m + o.
func (m Money) Add(o Money) Money {
	return Money{Units: m.Units + o.Units, Currency: unify(m, o)}
}

// Sub returns m - o.
func (m Money) Sub(o Money) Money {
	return Money{Units: m.Units - o.Units, Currency: unify(m, o)}
}

// Mul scales the amount by an integer factor.
func (m Money) Mul(factor int64) Money {
	return Money{Units: m.Units * factor, Currency: m.Currency}
}

// Cmp compares two amounts: -1, 0 or +1.
func (m Money) Cmp(o Money) int {
	unify(m, o)
	switch {
	case m.Units < o.Units:
		return -1
	case m.Units > o.Units:
		return 1
	}
	return 0
}

// Less reports whether m < o; it makes Money usable with sort.Slice helpers.
func (m Money) Less(o Money) bool { return m.Cmp(o) < 0 }

// Allocate splits the amount in proportion to the given ratios without
// losing a unit: the remainder is spread one unit at a time from the first
// share on.
func (m Money) Allocate(ratios ...int64) ([]Money, error) {
	if len(ratios) == 0 {
		return nil, Invalid("allocate", "no ratios")
	}
	var total int64
	for _, r := range ratios {
		if r < 0 {
			return nil, Invalid("allocate", "negative ratio %d", r)
		}
		total += r
	}
	if total == 0 {
		return nil, Invalid("allocate", "ratios sum to zero")
	}
	shares := make([]Money, len(ratios))
	var assigned int64
	for i, r := range ratios {
		share := m.Units * r / total
		shares[i] = Money{Units: share, Currency: m.Currency}
		assigned += share
	}
	for i := 0; assigned < m.Units; i = (i + 1) % len(shares) {
		shares[i].Units++
		assigned++
	}
	for i := 0; assigned > m.Units; i = (i + 1) % len(shares) {
		shares[i].Units--
		assigned--
	}
	return shares, nil
}

// Split divides the amount into n equal parts, the first parts taking the
// remainder.
func (m Money) Split(n int) ([]Money, error) {
	if n <= 0 {
		return nil, Invalid("split", "cannot split into %d parts", n)
	}
	ratios := make([]int64, n)
	for i := range ratios {
		ratios[i] = 1
	}
	return m.Allocate(ratios...)
}

// String formats the amount as "12.50 EUR".
func (m Money) String() string {
	return m.Format(false) + " " + string(m.Currency)
}

// Format renders the amount with its decimals; withSymbol replaces the
// trailing code by the currency symbol as a prefix.
func (m Money) Format(withSymbol bool) string {
	exp := m.Currency.Exponent()
	sign := ""
	units := m.Units
	if units < 0 {
		sign = "-"
		units = -units
	}
	digits := strconv.FormatInt(units, 10)
	if exp > 0 {
		for len(digits) <= exp {
			digits = "0" + digits
		}
		digits = digits[:len(digits)-exp] + "." + digits[len(digits)-exp:]
	}
	if withSymbol {
		return sign + m.Currency.Symbol() + digits
	}
	return sign + digits
}

// MarshalJSON writes {"amount":"12.50","currency":"EUR"}.
func (m Money) MarshalJSON() ([]byte, error) {
	return json.Marshal(struct {
		Amount   string   `json:"amount"`
		Currency Currency `json:"currency"`
	}{m.Format(false), m.Currency})
}

// UnmarshalJSON is the inverse of MarshalJSON.
func (m *Money) UnmarshalJSON(data []byte) error {
	var raw struct {
		Amount   string   `json:"amount"`
		Currency Currency `json:"currency"`
	}
	if err := json.Unmarshal(data, &raw); err != nil {
		return Wrap("money.unmarshal", err)
	}
	parsed, err := ParseMoney(raw.Amount, raw.Currency)
	if err != nil {
		return err
	}
	*m = parsed
	return nil
}

// ParseMoney reads "12.5", "-3" or "1,234.00" in the given currency.
func ParseMoney(s string, c Currency) (Money, error) {
	if !c.Valid() {
		return Money{}, Invalid("parse", "bad currency %q", string(c))
	}
	s = strings.ReplaceAll(strings.TrimSpace(s), ",", "")
	if s == "" {
		return Money{}, Invalid("parse", "empty amount")
	}
	negative := strings.HasPrefix(s, "-")
	s = strings.TrimPrefix(s, "-")
	whole, frac, _ := strings.Cut(s, ".")
	exp := c.Exponent()
	if len(frac) > exp {
		return Money{}, Invalid("parse", "%q has more than %d decimals", s, exp)
	}
	frac += strings.Repeat("0", exp-len(frac))
	major, err := strconv.ParseInt(whole, 10, 64)
	if err != nil {
		return Money{}, Wrap("parse", err)
	}
	minor := int64(0)
	if frac != "" {
		minor, err = strconv.ParseInt(frac, 10, 64)
		if err != nil {
			return Money{}, Wrap("parse", err)
		}
	}
	m := Of(c, major, minor)
	if negative {
		m = m.Neg()
	}
	return m, nil
}

// MustParse is ParseMoney for constants in tests and examples.
func MustParse(s string, c Currency) Money {
	m, err := ParseMoney(s, c)
	if err != nil {
		panic(err)
	}
	return m
}

// Rate is an exchange rate from one currency to another, kept as an exact
// fraction so repeated conversions do not drift.
type Rate struct {
	From, To Currency
	Num, Den int64
}

// Convert applies the rate, rounding half away from zero.
func (r Rate) Convert(m Money) (Money, error) {
	if m.Currency != r.From {
		return Money{}, Invalid("convert", "rate %s/%s cannot convert %s", r.From, r.To, m.Currency)
	}
	if r.Den == 0 {
		return Money{}, Invalid("convert", "rate %s/%s has a zero denominator", r.From, r.To)
	}
	scaled := new(big.Int).Mul(big.NewInt(m.Units), big.NewInt(r.Num))
	q, rem := new(big.Int).QuoRem(scaled, big.NewInt(r.Den), new(big.Int))
	twice := new(big.Int).Mul(new(big.Int).Abs(rem), big.NewInt(2))
	if twice.Cmp(big.NewInt(r.Den)) >= 0 {
		if scaled.Sign() < 0 {
			q.Sub(q, big.NewInt(1))
		} else {
			q.Add(q, big.NewInt(1))
		}
	}
	if !q.IsInt64() {
		return Money{}, newError(CodeLimit, "convert", "", fmt.Errorf("%s overflows int64", q))
	}
	return Money{Units: q.Int64(), Currency: r.To}, nil
}

// Invert returns the opposite rate.
func (r Rate) Invert() Rate {
	return Rate{From: r.To, To: r.From, Num: r.Den, Den: r.Num}
}

// RateTable resolves conversions between currencies, going through a base
// currency when no direct rate is registered.
type RateTable struct {
	base  Currency
	rates map[[2]Currency]Rate
}

// NewRateTable creates an empty table that bridges through base.
func NewRateTable(base Currency) *RateTable {
	return &RateTable{base: base, rates: make(map[[2]Currency]Rate)}
}

// Set registers a rate and its inverse.
func (t *RateTable) Set(r Rate) {
	t.rates[[2]Currency{r.From, r.To}] = r
	if r.Num != 0 {
		inv := r.Invert()
		t.rates[[2]Currency{inv.From, inv.To}] = inv
	}
}

// Lookup finds the direct or bridged rate for from -> to.
func (t *RateTable) Lookup(from, to Currency) (Rate, bool) {
	if from == to {
		return Rate{From: from, To: to, Num: 1, Den: 1}, true
	}
	if r, ok := t.rates[[2]Currency{from, to}]; ok {
		return r, true
	}
	first, ok := t.rates[[2]Currency{from, t.base}]
	if !ok {
		return Rate{}, false
	}
	second, ok := t.rates[[2]Currency{t.base, to}]
	if !ok {
		return Rate{}, false
	}
	return Rate{From: from, To: to, Num: first.Num * second.Num, Den: first.Den * second.Den}, true
}

// Convert converts m into currency to.
func (t *RateTable) Convert(m Money, to Currency) (Money, error) {
	rate, ok := t.Lookup(m.Currency, to)
	if !ok {
		return Money{}, NotFound("convert", AccountID(string(m.Currency)+"/"+string(to)))
	}
	return rate.Convert(m)
}

// Currencies lists the currencies that appear in the table, sorted.
func (t *RateTable) Currencies() []Currency {
	seen := map[Currency]bool{t.base: true}
	for key := range t.rates {
		seen[key[0]] = true
		seen[key[1]] = true
	}
	out := make([]Currency, 0, len(seen))
	for c := range seen {
		out = append(out, c)
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}

// Number is the constraint of the numeric helpers below.
type Number interface {
	~int | ~int32 | ~int64 | ~float32 | ~float64
}

// SumOf adds up a slice of any numeric type.
func SumOf[T Number](values []T) T {
	var total T
	for _, v := range values {
		total += v
	}
	return total
}

// MaxOf returns the largest of its arguments.
func MaxOf[T Number](first T, rest ...T) T {
	best := first
	for _, v := range rest {
		if v > best {
			best = v
		}
	}
	return best
}

// Percent returns pct percent of m, rounded to the nearest unit.
func Percent(m Money, pct float64) Money {
	return Money{Units: int64(math.Round(float64(m.Units) * pct / 100)), Currency: m.Currency}
}

// Total adds up amounts, which must share a currency.
func Total(amounts ...Money) Money {
	var sum Money
	for _, a := range amounts {
		sum = sum.Add(a)
	}
	return sum
}

// SortMoney orders amounts ascending in place.
func SortMoney(amounts []Money) {
	sort.Slice(amounts, func(i, j int) bool { return amounts[i].Less(amounts[j]) })
}
