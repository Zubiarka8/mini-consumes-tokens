"""Partial-block regression corpus: never executed, only indexed.

`load_orders` and `load_invoices` differ before and after one shared run of
statements (the de-duplicating loop), so no whole-function group joins them
but a partial block must. `twin_left` and `twin_right` are whole-function
copies: their shared lines belong to their group and form no partial block.
"""


def load_orders(path, limit):
    rows = read_rows(path)
    if limit < 0:
        raise ValueError("negative limit")
    cleaned = []
    for row in rows:
        if not row:
            continue
        name = row.strip().lower()
        name = name.replace("-", "_")
        if name in cleaned:
            continue
        cleaned.append(name)
    return cleaned[:limit]


def load_invoices(source):
    handle = open_source(source, mode="r")
    total = 0
    cleaned = []
    for row in handle:
        if not row:
            continue
        name = row.strip().lower()
        name = name.replace("-", "_")
        if name in cleaned:
            continue
        cleaned.append(name)
    for item in cleaned:
        total += len(item)
    return total


def twin_left(values):
    result = {}
    for value in values:
        key = value.split(":")[0]
        if key not in result:
            result[key] = []
        result[key].append(value)
        if len(result[key]) > 3:
            result[key].pop(0)
    return result


def twin_right(values):
    result = {}
    for value in values:
        key = value.split(":")[0]
        if key not in result:
            result[key] = []
        result[key].append(value)
        if len(result[key]) > 3:
            result[key].pop(0)
    return result
