package main

import (
	"fmt"

	"example.com/app/billing"
	"example.com/app/store"
	"github.com/other/ledger"
)

func main() {
	fmt.Println(store.Open(), billing.Total(), ledger.Sum())
}
