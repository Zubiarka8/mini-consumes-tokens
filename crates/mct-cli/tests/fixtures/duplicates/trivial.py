# Trivial-call corpus: bodies made only of flat calls next to real logic whose
# names suggest logging. Callee names must not decide anything. Parsed, never
# run.


def log_order(order):
    """Log every field."""
    logger.info("id", order.id)
    logger.info("total", order.total)
    logger.info("rate", order.rate)
    logger.info("fee", order.fee)
    logger.info("tax", order.tax)


def audit_order(order):
    """Record every field."""
    audit.record("id", order.id)
    audit.record("total", order.total)
    audit.record("rate", order.rate)
    audit.record("fee", order.fee)
    audit.record("tax", order.tax)


def print_report(rows):
    total = 0
    for row in rows:
        total += row.amount
    print(total)
    return total


def log_summary(rows):
    count = 0
    for row in rows:
        count += row.weight
    log(count)
    return count
