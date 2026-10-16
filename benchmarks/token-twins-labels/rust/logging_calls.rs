//! Bodies made only of macro invocations: logging and metrics.
//!
//! Labeled evaluation corpus for the duplicate report (see ../labels.toon).
//! Parsed by mct-cli, never compiled.

// Case rs-calls-identical: the same startup banner copied into two binaries.
fn print_server_banner(config: &ServerConfig) {
    info!("listening on {}", config.address);
    info!("workers: {}", config.workers);
    info!("tls: {}", config.tls_enabled);
    info!("data dir: {}", config.data_dir);
    info!("log level: {}", config.log_level);
}

fn print_worker_banner(settings: &ServerConfig) {
    info!("listening on {}", settings.address);
    info!("workers: {}", settings.workers);
    info!("tls: {}", settings.tls_enabled);
    info!("data dir: {}", settings.data_dir);
    info!("log level: {}", settings.log_level);
}

// Case rs-calls-sinks: logging the queue state versus exporting it as gauges.
fn log_queue_state(queue: &QueueStats) {
    debug!("queue depth", queue.depth, queue.name);
    debug!("queue lag", queue.lag_ms, queue.name);
    debug!("queue consumers", queue.consumers, queue.name);
    debug!("queue retries", queue.retries, queue.name);
    debug!("queue dropped", queue.dropped, queue.name);
    debug!("queue paused", queue.paused, queue.name);
}

fn export_queue_gauges(queue: &QueueStats) {
    gauge!("queue_depth", queue.depth, queue.name);
    gauge!("queue_lag_ms", queue.lag_ms, queue.name);
    gauge!("queue_consumers", queue.consumers, queue.name);
    gauge!("queue_retries", queue.retries, queue.name);
    gauge!("queue_dropped", queue.dropped, queue.name);
    gauge!("queue_paused", queue.paused, queue.name);
}
