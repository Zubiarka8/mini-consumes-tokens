# Aligned-evidence corpus: each pair has a shape of its own, so every pair is
# its own group under the broad policy and the evidence can be read per group.


def renamed_a(count, step):
    total = count + step
    return total


def renamed_b(size, delta):
    total = size + delta
    return total


def pattern_a(a, b):
    c = a * a
    c = c - b
    return c


def pattern_b(x, y):
    c = x * y
    c = c - y
    return c


def members_a(user, mailer):
    address = user.email
    mailer.send(address)
    load(address)


def members_b(config, store):
    address = config.path
    store.write(address)
    save(address)


def typed_a(value: int) -> int:
    if value:
        return value + 1
    return value


def typed_b(value: str) -> str:
    if value:
        return value + "x"
    return value


def same_types_a(flag: bool) -> bool:
    while flag:
        flag = not flag
    return flag


def same_types_b(state: bool) -> bool:
    while state:
        state = not state
    return state


def untyped_a(items):
    with items as handle:
        handle.close()


def untyped_b(things):
    with things as handle:
        handle.close()


def retries_a():
    retries = 3
    del retries


def retries_b():
    retries = 5
    del retries


def fmt_a(order):
    yield f"{order.total * order.rate + 1}"


def fmt_b(order):
    yield f"{order.total}"


def documented_a(rows):
    """Count the rows."""
    assert rows


def documented_b(rows):
    """Check that rows exist."""
    assert rows


def shadow_a(xs):
    for x in xs:
        x = x or 0
    for x in xs:
        x = x or 0


def shadow_b(xs):
    for y in xs:
        y = y or 0
    for z in xs:
        z = z or 0
