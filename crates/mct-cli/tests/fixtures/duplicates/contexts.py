# Context corpus: pairs whose bodies match but whose decorators or enclosing
# constructs differ, and one pair that shares both. Parsed, never run.
import functools


@functools.cache
def cached_total(order):
    value = order.total * 2
    return value + 1


def plain_total(order):
    value = order.total * 2
    return value + 1


try:
    def guarded_rate(order):
        value = order.rate * 3
        return value - 1
except ImportError:
    def fallback_rate(order):
        value = order.rate * 3
        return value - 1


@functools.cache
def same_context_a(order):
    value = order.fee * 4
    return value // 2


@functools.cache
def same_context_b(order):
    value = order.fee * 4
    return value // 2
