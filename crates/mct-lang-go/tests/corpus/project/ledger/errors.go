// Package ledger is a small double-entry bookkeeping library: money values,
// accounts, transactions, an in-memory store, a posting service and reports.
//
// This file holds the error vocabulary shared by every other file in the
// package: sentinel errors, a typed error with a code, aggregation of several
// errors, a generic Result type and retry helpers.
package ledger

import (
	"context"
	"errors"
	"fmt"
	"io"
	"sort"
	"strings"
	"time"
)

// ErrorCode classifies a ledger failure so callers can branch without
// parsing messages.
type ErrorCode int

const (
	// CodeUnknown is the zero value and never produced on purpose.
	CodeUnknown ErrorCode = iota
	CodeNotFound
	CodeInvalid
	CodeConflict
	CodeUnbalanced
	CodeClosed
	CodeLimit
	CodeTimeout
	codeSentinel
)

var codeNames = [...]string{
	CodeUnknown:    "unknown",
	CodeNotFound:   "not_found",
	CodeInvalid:    "invalid",
	CodeConflict:   "conflict",
	CodeUnbalanced: "unbalanced",
	CodeClosed:     "closed",
	CodeLimit:      "limit",
	CodeTimeout:    "timeout",
}

// String implements fmt.Stringer.
func (c ErrorCode) String() string {
	if c < 0 || int(c) >= len(codeNames) {
		return fmt.Sprintf("code(%d)", int(c))
	}
	return codeNames[c]
}

// Retryable reports whether an operation failing with this code may succeed
// when attempted again unchanged.
func (c ErrorCode) Retryable() bool {
	switch c {
	case CodeConflict, CodeTimeout:
		return true
	default:
		return false
	}
}

// Sentinel errors, compared with errors.Is.
var (
	ErrNotFound   = errors.New("ledger: not found")
	ErrInvalid    = errors.New("ledger: invalid argument")
	ErrConflict   = errors.New("ledger: conflict")
	ErrUnbalanced = errors.New("ledger: transaction does not balance")
	ErrClosed     = errors.New("ledger: account closed")
	ErrLimit      = errors.New("ledger: limit exceeded")
)

// sentinelFor maps a code to the sentinel it wraps.
func sentinelFor(code ErrorCode) error {
	switch code {
	case CodeNotFound:
		return ErrNotFound
	case CodeInvalid:
		return ErrInvalid
	case CodeConflict:
		return ErrConflict
	case CodeUnbalanced:
		return ErrUnbalanced
	case CodeClosed:
		return ErrClosed
	case CodeLimit:
		return ErrLimit
	case CodeTimeout:
		return context.DeadlineExceeded
	}
	return nil
}

// Error is the typed error every exported operation returns. It carries a
// code, the operation that failed, the account involved (if any) and the
// underlying cause.
type Error struct {
	Code    ErrorCode
	Op      string
	Account AccountID
	Cause   error
	When    time.Time
}

// Error implements the error interface.
func (e *Error) Error() string {
	var b strings.Builder
	b.WriteString("ledger: ")
	if e.Op != "" {
		b.WriteString(e.Op)
		b.WriteString(": ")
	}
	b.WriteString(e.Code.String())
	if e.Account != "" {
		fmt.Fprintf(&b, " (account %s)", e.Account)
	}
	if e.Cause != nil {
		b.WriteString(": ")
		b.WriteString(e.Cause.Error())
	}
	return b.String()
}

// Unwrap exposes the cause to errors.Is and errors.As.
func (e *Error) Unwrap() error { return e.Cause }

// Is makes errors.Is(err, ErrNotFound) work for any *Error with that code.
func (e *Error) Is(target error) bool {
	if sentinel := sentinelFor(e.Code); sentinel != nil && sentinel == target {
		return true
	}
	other, ok := target.(*Error)
	if !ok {
		return false
	}
	return other.Code == e.Code && (other.Op == "" || other.Op == e.Op)
}

// Temporary reports whether retrying may help.
func (e *Error) Temporary() bool { return e.Code.Retryable() }

// newError builds an *Error stamped with the current time.
func newError(code ErrorCode, op string, account AccountID, cause error) *Error {
	return &Error{
		Code:    code,
		Op:      op,
		Account: account,
		Cause:   cause,
		When:    time.Now(),
	}
}

// NotFound is the error for a missing account or transaction.
func NotFound(op string, id AccountID) error {
	return newError(CodeNotFound, op, id, nil)
}

// Invalid is the error for a malformed argument; the reason becomes the cause.
func Invalid(op string, format string, args ...any) error {
	return newError(CodeInvalid, op, "", fmt.Errorf(format, args...))
}

// Conflict is the error for a concurrent modification.
func Conflict(op string, id AccountID, cause error) error {
	return newError(CodeConflict, op, id, cause)
}

// Closed is the error for posting to a closed account.
func Closed(op string, id AccountID) error {
	return newError(CodeClosed, op, id, nil)
}

// LimitError is returned when an account would exceed its overdraft limit.
type LimitError struct {
	Account AccountID
	Limit   Money
	Wanted  Money
}

func (e LimitError) Error() string {
	return fmt.Sprintf("ledger: account %s limit %s exceeded by %s",
		e.Account, e.Limit, e.Wanted.Sub(e.Limit))
}

// Is lets errors.Is(err, ErrLimit) match a LimitError.
func (e LimitError) Is(target error) bool { return target == ErrLimit }

// UnbalancedError describes how far a transaction is from balancing.
type UnbalancedError struct {
	Transaction TxID
	Debits      Money
	Credits     Money
}

func (e UnbalancedError) Error() string {
	return fmt.Sprintf("ledger: transaction %s debits %s but credits %s",
		e.Transaction, e.Debits, e.Credits)
}

// Is lets errors.Is(err, ErrUnbalanced) match an UnbalancedError.
func (e UnbalancedError) Is(target error) bool { return target == ErrUnbalanced }

// Difference is the signed amount by which debits exceed credits.
func (e UnbalancedError) Difference() Money { return e.Debits.Sub(e.Credits) }

// MultiError collects several errors, typically one per entry of a
// transaction, and reports them together.
type MultiError struct {
	errs []error
}

// Append adds err unless it is nil; a nested MultiError is flattened.
func (m *MultiError) Append(err error) {
	if err == nil {
		return
	}
	var nested *MultiError
	if errors.As(err, &nested) && nested != m {
		m.errs = append(m.errs, nested.errs...)
		return
	}
	m.errs = append(m.errs, err)
}

// Len is the number of collected errors.
func (m *MultiError) Len() int { return len(m.errs) }

// ErrorOrNil returns m itself when it holds errors, nil otherwise, so the
// result can be returned directly from a function that returns error.
func (m *MultiError) ErrorOrNil() error {
	if m == nil || len(m.errs) == 0 {
		return nil
	}
	return m
}

// Error joins the messages one per line with a count header.
func (m *MultiError) Error() string {
	switch len(m.errs) {
	case 0:
		return "ledger: no errors"
	case 1:
		return m.errs[0].Error()
	}
	lines := make([]string, 0, len(m.errs)+1)
	lines = append(lines, fmt.Sprintf("ledger: %d errors occurred:", len(m.errs)))
	for _, err := range m.errs {
		lines = append(lines, "\t* "+err.Error())
	}
	return strings.Join(lines, "\n")
}

// Unwrap returns every collected error so errors.Is and errors.As look at
// all of them (the Go 1.20 multi-error protocol).
func (m *MultiError) Unwrap() []error { return m.errs }

// Codes returns the distinct codes of the collected *Error values, sorted.
func (m *MultiError) Codes() []ErrorCode {
	seen := make(map[ErrorCode]struct{})
	for _, err := range m.errs {
		var le *Error
		if errors.As(err, &le) {
			seen[le.Code] = struct{}{}
		}
	}
	codes := make([]ErrorCode, 0, len(seen))
	for code := range seen {
		codes = append(codes, code)
	}
	sort.Slice(codes, func(i, j int) bool { return codes[i] < codes[j] })
	return codes
}

// CodeOf extracts the ErrorCode of err, CodeUnknown when it has none.
func CodeOf(err error) ErrorCode {
	var le *Error
	if errors.As(err, &le) {
		return le.Code
	}
	switch {
	case errors.Is(err, ErrNotFound):
		return CodeNotFound
	case errors.Is(err, ErrUnbalanced):
		return CodeUnbalanced
	case errors.Is(err, ErrLimit):
		return CodeLimit
	case errors.Is(err, context.DeadlineExceeded):
		return CodeTimeout
	}
	return CodeUnknown
}

// Result carries either a value or an error, for pipelines that cannot
// return a pair.
type Result[T any] struct {
	Value T
	Err   error
}

// Ok wraps a value.
func Ok[T any](value T) Result[T] { return Result[T]{Value: value} }

// Fail wraps an error.
func Fail[T any](err error) Result[T] { return Result[T]{Err: err} }

// Unwrap returns the pair back.
func (r Result[T]) Unwrap() (T, error) { return r.Value, r.Err }

// OrElse returns the value, or fallback when the result failed.
func (r Result[T]) OrElse(fallback T) T {
	if r.Err != nil {
		return fallback
	}
	return r.Value
}

// Must returns the value or panics; it is meant for tests and examples.
func (r Result[T]) Must() T {
	if r.Err != nil {
		panic(r.Err)
	}
	return r.Value
}

// Try runs fn and converts its pair into a Result.
func Try[T any](fn func() (T, error)) Result[T] {
	value, err := fn()
	if err != nil {
		return Fail[T](err)
	}
	return Ok(value)
}

// Recover turns a panic inside fn into an error, so a bug in a callback
// cannot take the posting goroutine down.
func Recover(op string, fn func() error) (err error) {
	defer func() {
		if r := recover(); r != nil {
			switch v := r.(type) {
			case error:
				err = newError(CodeUnknown, op, "", v)
			case string:
				err = newError(CodeUnknown, op, "", errors.New(v))
			default:
				err = newError(CodeUnknown, op, "", fmt.Errorf("panic: %v", v))
			}
		}
	}()
	return fn()
}

// RetryPolicy bounds how often and how fast Retry repeats an operation.
type RetryPolicy struct {
	Attempts int
	Initial  time.Duration
	Max      time.Duration
	Jitter   func(time.Duration) time.Duration
}

// DefaultRetry is a conservative policy for optimistic-concurrency conflicts.
var DefaultRetry = RetryPolicy{
	Attempts: 5,
	Initial:  5 * time.Millisecond,
	Max:      200 * time.Millisecond,
}

// delay is the wait before attempt n (0-based), doubling up to Max.
func (p RetryPolicy) delay(n int) time.Duration {
	d := p.Initial << uint(n)
	if d <= 0 || d > p.Max {
		d = p.Max
	}
	if p.Jitter != nil {
		d = p.Jitter(d)
	}
	return d
}

// Retry calls fn until it succeeds, fails with a non-retryable error, the
// attempts run out or ctx ends. The last error is returned.
func Retry(ctx context.Context, policy RetryPolicy, fn func(attempt int) error) error {
	var last error
	for attempt := 0; attempt < policy.Attempts; attempt++ {
		if err := ctx.Err(); err != nil {
			return newError(CodeTimeout, "retry", "", err)
		}
		last = fn(attempt)
		if last == nil {
			return nil
		}
		var le *Error
		if errors.As(last, &le) && !le.Temporary() {
			return last
		}
		timer := time.NewTimer(policy.delay(attempt))
		select {
		case <-ctx.Done():
			timer.Stop()
			return newError(CodeTimeout, "retry", "", ctx.Err())
		case <-timer.C:
		}
	}
	return last
}

// Describe writes a multi-line explanation of err, following the chain of
// wrapped errors, to w.
func Describe(w io.Writer, err error) {
	for depth := 0; err != nil; depth++ {
		fmt.Fprintf(w, "%s%v\n", strings.Repeat("  ", depth), err)
		err = errors.Unwrap(err)
	}
}

// Explain renders Describe to a string.
func Explain(err error) string {
	var b strings.Builder
	Describe(&b, err)
	return strings.TrimRight(b.String(), "\n")
}

// Wrap annotates err with op while keeping its code, so a caller further up
// still sees the original classification.
func Wrap(op string, err error) error {
	if err == nil {
		return nil
	}
	var le *Error
	if errors.As(err, &le) {
		return newError(le.Code, op, le.Account, err)
	}
	return newError(CodeUnknown, op, "", err)
}

// IsNotFound is shorthand for errors.Is(err, ErrNotFound).
func IsNotFound(err error) bool { return errors.Is(err, ErrNotFound) }

// IsRetryable reports whether err or anything it wraps is temporary.
func IsRetryable(err error) bool {
	var temp interface{ Temporary() bool }
	return errors.As(err, &temp) && temp.Temporary()
}

// Check validates a list of conditions, collecting every failure rather than
// stopping at the first one.
func Check(op string, conds ...func() error) error {
	var multi MultiError
	for _, cond := range conds {
		multi.Append(cond())
	}
	if multi.Len() == 0 {
		return nil
	}
	return Wrap(op, multi.ErrorOrNil())
}

// Require returns Invalid(op, msg) unless ok.
func Require(ok bool, op, msg string) error {
	if ok {
		return nil
	}
	return Invalid(op, "%s", msg)
}

// Guard returns a deferred-friendly function that records a panic in *dst.
func Guard(op string, dst *error) func() {
	return func() {
		if r := recover(); r != nil {
			*dst = newError(CodeUnknown, op, "", fmt.Errorf("%v", r))
		}
	}
}
