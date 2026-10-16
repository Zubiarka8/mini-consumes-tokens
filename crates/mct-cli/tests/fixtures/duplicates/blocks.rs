//! Partial-block regression corpus: never compiled, only indexed.
//!
//! `summarize_orders` and `summarize_refunds` differ before and after one
//! shared run of statements (the bucketing loop), so no whole-function group
//! joins them but a partial block must. `copy_left` and `copy_right` are
//! whole-function copies: their shared lines belong to their group and form
//! no partial block.

fn summarize_orders(orders: &[Order], cutoff: u32) -> Summary {
    let started = Instant::now();
    log_start("orders", started);
    let mut buckets: Vec<u32> = Vec::new();
    for entry in orders {
        if entry.amount == 0 {
            continue;
        }
        let slot = (entry.amount / 100) as usize;
        while buckets.len() <= slot {
            buckets.push(0);
        }
        buckets[slot] += 1;
    }
    Summary::new(buckets, cutoff)
}

fn summarize_refunds(refunds: &[Order]) -> usize {
    let mut seen = 0;
    let mut buckets: Vec<u32> = Vec::new();
    for entry in refunds {
        if entry.amount == 0 {
            continue;
        }
        let slot = (entry.amount / 100) as usize;
        while buckets.len() <= slot {
            buckets.push(0);
        }
        buckets[slot] += 1;
    }
    for bucket in &buckets {
        seen += *bucket as usize;
    }
    seen
}

fn copy_left(items: &[Item]) -> Vec<String> {
    let mut names = Vec::new();
    for item in items {
        if item.hidden {
            continue;
        }
        let label = format!("{}-{}", item.kind, item.id);
        names.push(label);
    }
    names.sort();
    names
}

fn copy_right(items: &[Item]) -> Vec<String> {
    let mut names = Vec::new();
    for item in items {
        if item.hidden {
            continue;
        }
        let label = format!("{}-{}", item.kind, item.id);
        names.push(label);
    }
    names.sort();
    names
}
