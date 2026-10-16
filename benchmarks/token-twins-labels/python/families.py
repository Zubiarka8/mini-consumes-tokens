"""Groups whose members form more than one family, and one group that does not.

Labeled evaluation corpus for the duplicate report (see ../labels.toon).
Parsed by mct-cli, never imported or run.
"""


# Case py-split-members: two summing rules that share one shape.
# Family A sums paid amounts, family B sums late days of unreturned loans.
def paid_total(orders):
    total = 0
    for order in orders:
        if order.status == "cancelled":
            continue
        total += order.amount
    return max(total, 0)


def settled_total(invoices):
    total = 0
    for invoice in invoices:
        if invoice.status == "cancelled":
            continue
        total += invoice.amount
    return max(total, 0)


def overdue_days(loans):
    days = 0
    for loan in loans:
        if loan.state == "returned":
            continue
        days += loan.late_days
    return max(days, 0)


def outstanding_days(rentals):
    days = 0
    for rental in rentals:
        if rental.state == "returned":
            continue
        days += rental.late_days
    return max(days, 0)


# Case py-split-dataflow: two scale copies and one skew that reuses a coordinate.
def scale_points(points, factor):
    scaled = []
    for point in points:
        x = point.x * factor
        y = point.y * factor
        scaled.append(Point(x, y))
    return scaled


def scale_vertices(vertices, ratio):
    result = []
    for vertex in vertices:
        x = vertex.x * ratio
        y = vertex.y * ratio
        result.append(Point(x, y))
    return result


def skew_points(points, factor):
    skewed = []
    for point in points:
        x = point.x * factor
        y = point.y * x
        skewed.append(Point(x, y))
    return skewed


# Case py-single-chunks: three renamed copies of one batching generator.
def chunked(items, size):
    batch = []
    for item in items:
        batch.append(item)
        if len(batch) == size:
            yield batch
            batch = []
    if batch:
        yield batch


def batched_rows(rows, limit):
    pending = []
    for row in rows:
        pending.append(row)
        if len(pending) == limit:
            yield pending
            pending = []
    if pending:
        yield pending


def paged_ids(ids, page_size):
    page = []
    for identifier in ids:
        page.append(identifier)
        if len(page) == page_size:
            yield page
            page = []
    if page:
        yield page
