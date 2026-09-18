package billing

data class LineItem(val name: String, val price: Double)

class Invoice {
    var total: Double = 0.0

    fun addItem(item: LineItem): Double {
        total += item.price
        Logger.log("item added")
        return total
    }

    fun addItemWithTax(item: LineItem, taxRate: Double): Double {
        total += item.price * (1 + taxRate)
        Logger.log("item added with tax")
        return total
    }
}

fun Invoice.describe(): String = "total=$total"
