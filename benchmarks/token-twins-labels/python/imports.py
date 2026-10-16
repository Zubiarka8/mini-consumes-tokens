"""CSV importers that repeat one reading block in three places.

Labeled evaluation corpus for the duplicate report (see ../labels.toon).
Parsed by mct-cli, never imported or run.
"""
import csv


# Case py-block-csv: the "with open ... rows.append" block is repeated in
# import_contacts here and in import_products and preview_csv in exports.py.
def import_contacts(path, address_book):
    if not path.endswith(".csv"):
        raise ValueError("expected a CSV file")
    with open(path, newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        rows = []
        for row in reader:
            if not any(value.strip() for value in row.values()):
                continue
            rows.append({key.strip().lower(): value.strip() for key, value in row.items()})
    for contact in rows:
        address_book.upsert(contact["email"], contact)
    return address_book.size()
