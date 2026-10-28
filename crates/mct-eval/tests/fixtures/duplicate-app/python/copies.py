def duplicate_first(value: int) -> int:
    total = value
    total += 1
    total *= 2
    total -= 4
    if total > 12:
        total -= 3
    total += 8
    total *= 2
    total -= 4
    return total

def duplicate_second(value: int) -> int:
    total = value
    total += 1
    total *= 2
    total -= 4
    if total > 12:
        total -= 3
    total += 8
    total *= 2
    total -= 4
    return total
