"""Import and billing helpers that were copied and renamed.

Labeled evaluation corpus for the duplicate report (see ../labels.toon).
Parsed by mct-cli, never imported or run.
"""


# Case py-real-rows: the same row-normalization rule under two names.
def normalize_customer_rows(rows):
    cleaned = []
    for row in rows:
        name = row.get("name", "").strip()
        email = row.get("email", "").strip().lower()
        if not name or "@" not in email:
            continue
        cleaned.append({"name": name.title(), "email": email})
    return cleaned


def normalize_vendor_rows(entries):
    result = []
    for entry in entries:
        label = entry.get("name", "").strip()
        contact = entry.get("email", "").strip().lower()
        if not label or "@" not in contact:
            continue
        result.append({"name": label.title(), "email": contact})
    return result


# Case py-real-methods: one total-with-tax rule in two document classes.
class InvoiceTotals:
    def __init__(self, lines):
        self.lines = lines

    def total_with_tax(self, rate):
        subtotal = 0
        for line in self.lines:
            subtotal += line.quantity * line.unit_price
        tax = subtotal * rate
        return round(subtotal + tax, 2)


class QuoteTotals:
    def __init__(self, items):
        self.lines = items

    def total_with_tax(self, rate):
        amount = 0
        for item in self.lines:
            amount += item.quantity * item.unit_price
        levy = amount * rate
        return round(amount + levy, 2)
