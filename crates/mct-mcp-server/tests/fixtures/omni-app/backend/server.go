package backend

import "fmt"

// HandlePost wires an HTTP-ish request into the ledger.
func HandlePost(account string, amount int) string {
	ledger := NewLedger(account)
	total := ledger.Post(amount)
	return Describe(account, total)
}

// Describe renders a human-readable ledger line.
func Describe(account string, total int) string {
	return fmt.Sprintf("%s=%d", account, total)
}
