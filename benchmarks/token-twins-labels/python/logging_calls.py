"""Bodies made only of calls: logging, metrics and request tracing.

Labeled evaluation corpus for the duplicate report (see ../labels.toon).
Parsed by mct-cli, never imported or run.
"""


# Case py-calls-identical: the same request trace copied into two handlers.
def trace_checkout_request(log, request):
    log.info("method=%s", request.method)
    log.info("path=%s", request.path)
    log.info("client=%s", request.remote_addr)
    log.info("agent=%s", request.user_agent)
    log.info("request_id=%s", request.request_id)


def trace_refund_request(logger, req):
    logger.info("method=%s", req.method)
    logger.info("path=%s", req.path)
    logger.info("client=%s", req.remote_addr)
    logger.info("agent=%s", req.user_agent)
    logger.info("request_id=%s", req.request_id)


# Case py-calls-fields: one logger, different events and fields.
def log_order_created(logger, order):
    logger.info("order created: %s", order.id)
    logger.info("customer: %s", order.customer_id)
    logger.info("total: %s", order.total)
    logger.info("currency: %s", order.currency)
    logger.info("channel: %s", order.channel)
    logger.info("coupon: %s", order.coupon)


def log_order_shipped(logger, shipment):
    logger.info("order shipped: %s", shipment.order_id)
    logger.info("carrier: %s", shipment.carrier)
    logger.info("tracking: %s", shipment.tracking_code)
    logger.info("warehouse: %s", shipment.warehouse)
    logger.info("eta: %s", shipment.eta)
    logger.info("weight: %s", shipment.weight)


# Case py-calls-sinks: metrics emission versus audit recording.
def emit_payment_metrics(statsd, payment):
    statsd.increment("payments.count", payment.count, tags=payment.tags)
    statsd.gauge("payments.amount", payment.amount, tags=payment.tags)
    statsd.timing("payments.latency", payment.latency, tags=payment.tags)
    statsd.histogram("payments.retries", payment.retries, tags=payment.tags)
    statsd.set("payments.payers", payment.payer, tags=payment.tags)


def record_payment_audit(audit, payment):
    audit.append("payment.id", payment.id, tags=payment.tags)
    audit.attach("payment.payer", payment.payer, tags=payment.tags)
    audit.sign("payment.amount", payment.amount, tags=payment.tags)
    audit.seal("payment.batch", payment.batch, tags=payment.tags)
    audit.store("payment.ledger", payment.ledger, tags=payment.tags)
