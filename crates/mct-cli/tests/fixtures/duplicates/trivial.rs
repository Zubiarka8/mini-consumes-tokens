// Trivial-call corpus: bodies made only of flat calls, and calls whose
// arguments hold another call. Callee names must not decide anything. Parsed,
// never run.

fn trace_order(order: &Order) {
    log::info!("id {}", order.id);
    log::info!("total {}", order.total);
    log::info!("rate {}", order.rate);
    log::info!("fee {}", order.fee);
    log::info!("tax {}", order.tax);
}

fn emit_order(order: &Order) {
    metrics::gauge!("id {}", order.id);
    metrics::gauge!("total {}", order.total);
    metrics::gauge!("rate {}", order.rate);
    metrics::gauge!("fee {}", order.fee);
    metrics::gauge!("tax {}", order.tax);
}

fn store_order(order: &Order) {
    insert(encode(order.id));
    insert(encode(order.total));
    insert(encode(order.rate));
    insert(encode(order.fee));
    insert(encode(order.tax));
}

fn cache_order(order: &Order) {
    put(encode(order.id));
    put(encode(order.total));
    put(encode(order.rate));
    put(encode(order.fee));
    put(encode(order.tax));
}
