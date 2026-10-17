# Subgroup corpus: one structural group that mixes two families. Two members
# reach `user_id` / `display_name` through `load_user`, two reach `order_id` /
# `total` through `load_order`. Locals and parameters are renamed everywhere,
# which must never separate members. Parsed, never executed.


def user_label(record):
    value = record.user_id
    found = load_user(value)
    if found is None:
        return "missing"
    return found.display_name


def order_label(entry):
    key = entry.order_id
    match = load_order(key)
    if match is None:
        return "missing"
    return match.total


def owner_label(row):
    ident = row.user_id
    person = load_user(ident)
    if person is None:
        return "missing"
    return person.display_name


def invoice_label(item):
    number = item.order_id
    bill = load_order(number)
    if bill is None:
        return "missing"
    return bill.total


# Renaming only: a called parameter and renamed locals form one subgroup.
def apply_each(handler, values):
    results = []
    for value in values:
        results.append(handler(value))
    return results


def map_all(func, things):
    output = []
    for thing in things:
        output.append(func(thing))
    return output


# Same broad shape, different data flow: `square_all` multiplies a value by
# itself, not by the factor. Broad groups all three; consistent does not.
def scale_all(values, factor):
    result = []
    for value in values:
        result.append(value * factor)
    return result


def stretch_all(items, ratio):
    output = []
    for item in items:
        output.append(item * ratio)
    return output


def square_all(values, factor):
    result = []
    for value in values:
        result.append(value * value)
    return result
