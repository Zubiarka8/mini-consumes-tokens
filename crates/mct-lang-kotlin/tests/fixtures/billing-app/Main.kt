package billing

fun main() {
    val invoice = Invoice()
    invoice.addItem(LineItem("widget", 10.0))
    invoice.addItemWithTax(LineItem("gadget", 10.0), 0.5)
    println(invoice.describe())
}
