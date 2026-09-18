from logger import log


class Invoice:
    def __init__(self):
        self.total = 0.0

    def add_item(self, price):
        self.total += price
        log("item added")
        return self.total

    def add_item_with_tax(self, price, tax_rate):
        taxed = price * (1 + tax_rate)
        self.total += taxed
        log("item added with tax")
        return self.total
