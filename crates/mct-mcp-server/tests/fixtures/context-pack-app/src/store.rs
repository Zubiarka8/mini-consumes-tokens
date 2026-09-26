//! In-memory order persistence.

use std::collections::BTreeMap;

/// What the store keeps for a placed order.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredOrder {
    pub customer_id: u64,
    pub total_cents: u64,
    pub lines: usize,
}

/// Placed orders by id, ids assigned sequentially from 1.
#[derive(Debug, Default)]
pub struct OrderStore {
    orders: BTreeMap<u64, StoredOrder>,
    next_id: u64,
}

impl OrderStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores `order` and returns its newly assigned id.
    pub fn insert(&mut self, order: StoredOrder) -> u64 {
        self.next_id += 1;
        self.orders.insert(self.next_id, order);
        self.next_id
    }

    /// Removes and returns the order stored under `id`.
    pub fn remove(&mut self, id: u64) -> Option<StoredOrder> {
        self.orders.remove(&id)
    }

    /// The order stored under `id`, if any.
    pub fn get(&self, id: u64) -> Option<&StoredOrder> {
        self.orders.get(&id)
    }

    /// Every order placed by `customer_id`, oldest first.
    pub fn for_customer(&self, customer_id: u64) -> Vec<&StoredOrder> {
        self.orders
            .values()
            .filter(|order| order.customer_id == customer_id)
            .collect()
    }

    /// Revenue across every stored order, in cents.
    pub fn revenue(&self) -> u64 {
        self.orders.values().map(|order| order.total_cents).sum()
    }
}
