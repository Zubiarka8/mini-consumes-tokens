"""Catalog loading and previewing; repeats the CSV reading block of imports.py.

Labeled evaluation corpus for the duplicate report (see ../labels.toon).
Parsed by mct-cli, never imported or run.
"""
import csv


def import_products(path, catalog, currency):
    catalog.begin()
    with open(path, newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        rows = []
        for row in reader:
            if not any(value.strip() for value in row.values()):
                continue
            rows.append({key.strip().lower(): value.strip() for key, value in row.items()})
    prices = {row["sku"]: float(row["price"]) for row in rows}
    catalog.replace_prices(prices, currency)
    catalog.end()


def preview_csv(path, limit):
    with open(path, newline="", encoding="utf-8") as handle:
        reader = csv.DictReader(handle)
        rows = []
        for row in reader:
            if not any(value.strip() for value in row.values()):
                continue
            rows.append({key.strip().lower(): value.strip() for key, value in row.items()})
    header = " | ".join(rows[0].keys()) if rows else ""
    lines = [header] + [" | ".join(row.values()) for row in rows[:limit]]
    return "\n".join(lines)
