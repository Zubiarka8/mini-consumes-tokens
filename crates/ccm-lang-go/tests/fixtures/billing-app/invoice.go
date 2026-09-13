package billing

type Invoice struct {
	Total float64
}

func (inv *Invoice) AddItem(price float64) float64 {
	inv.Total += price
	Log("item added")
	return inv.Total
}

func (inv *Invoice) AddItemWithTax(price float64, taxRate float64) float64 {
	taxed := price * (1 + taxRate)
	inv.Total += taxed
	Log("item added with tax")
	return inv.Total
}
