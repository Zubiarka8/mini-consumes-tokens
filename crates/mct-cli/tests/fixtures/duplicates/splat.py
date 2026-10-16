# Splat-parameter corpus: the members rename their `*` and `**` parameters
# and the names they unpack into, so the evidence must tell the declared
# splat parameters apart from the unpacking targets and later reads.


def forward_a(handler, *args, **kwargs):
    head, *rest = args
    if rest:
        return handler(head, rest, kwargs)
    return handler(head, kwargs)


def forward_b(handler, *items, **options):
    first, *tail = items
    if tail:
        return handler(first, tail, options)
    return handler(first, options)
