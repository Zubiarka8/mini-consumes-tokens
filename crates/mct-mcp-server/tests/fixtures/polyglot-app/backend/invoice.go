package backend

type Invoice struct {
	Total float64
}

func (inv *Invoice) AddItem(price float64) float64 {
	inv.Total += price
	Log("item added")
	return inv.Total
}
