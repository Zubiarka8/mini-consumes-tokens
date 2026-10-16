//! Structural twins: same token shape, different members, callees, types or values.
//!
//! Labeled evaluation corpus for the duplicate report (see ../labels.toon).
//! Parsed by mct-cli, never compiled.

use std::thread;
use std::time::Duration;

// Case rs-fp-domains: check-and-decrement on unrelated aggregates.
fn reserve_seats(event: &mut Event, requested: u32) -> Result<u32, BookingError> {
    if requested > event.available_seats {
        return Err(BookingError::SoldOut(event.id));
    }
    event.available_seats -= requested;
    event.revision += 1;
    Ok(event.available_seats)
}

fn withdraw_funds(account: &mut Account, amount: u32) -> Result<u32, BankError> {
    if amount > account.balance {
        return Err(BankError::Insufficient(account.number));
    }
    account.balance -= amount;
    account.version += 1;
    Ok(account.balance)
}

// Case rs-fp-dataflow: squared deviation versus deviation times value.
fn sum_squared_deviation(values: &[f64], mean: f64) -> f64 {
    let mut total = 0.0;
    for value in values {
        let shifted = value - mean;
        total += shifted * shifted;
    }
    total
}

fn sum_deviation_product(values: &[f64], mean: f64) -> f64 {
    let mut total = 0.0;
    for value in values {
        let shifted = value - mean;
        total += shifted * value;
    }
    total
}

// Case rs-susp-types: one averaging rule over two integer widths.
fn average_u32(values: &[u32]) -> Option<u32> {
    if values.is_empty() {
        return None;
    }
    let total: u32 = values.iter().sum();
    Some(total / values.len() as u32)
}

fn average_u64(values: &[u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let total: u64 = values.iter().sum();
    Some(total / values.len() as u64)
}

// Case rs-susp-retry: one retry loop with a different target and policy values.
fn connect_primary(config: &Config) -> Result<Connection, Error> {
    let mut attempts = 0;
    loop {
        match Connection::open(&config.primary_url, Duration::from_secs(5)) {
            Ok(connection) => return Ok(connection),
            Err(_) if attempts < 3 => {
                attempts += 1;
                thread::sleep(Duration::from_millis(200));
            }
            Err(error) => return Err(error),
        }
    }
}

fn connect_replica(config: &Config) -> Result<Connection, Error> {
    let mut attempts = 0;
    loop {
        match Connection::open(&config.replica_url, Duration::from_secs(30)) {
            Ok(connection) => return Ok(connection),
            Err(_) if attempts < 10 => {
                attempts += 1;
                thread::sleep(Duration::from_millis(1000));
            }
            Err(error) => return Err(error),
        }
    }
}
