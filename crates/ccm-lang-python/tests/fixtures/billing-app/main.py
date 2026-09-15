from invoice import Invoice


def main():
    inv = Invoice()
    inv.add_item(10.0)
    inv.add_item_with_tax(10.0, 0.5)


main()
