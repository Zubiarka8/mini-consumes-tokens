package billing

func Run() {
	inv := &Invoice{}
	inv.AddItem(10.0)
	inv.AddItemWithTax(10.0, 0.5)
}
