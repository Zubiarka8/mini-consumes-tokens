package backend

func HandleCreateInvoice(price float64) float64 {
	inv := &Invoice{}
	return inv.AddItem(price)
}
