package backend

// Ledger accumulates posted amounts for one account.
type Ledger struct {
	Account string
	Total   int
}

// Post adds an amount to the ledger and returns the new total.
func (l *Ledger) Post(amount int) int {
	l.Total = l.Total + amount
	return l.Total
}

// NewLedger builds an empty ledger for an account.
func NewLedger(account string) *Ledger {
	return &Ledger{Account: account, Total: 0}
}
