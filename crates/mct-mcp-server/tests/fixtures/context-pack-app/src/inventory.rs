//! Stock levels per SKU, with reservations.

use std::collections::HashMap;

/// Available stock per SKU.
#[derive(Debug, Default)]
pub struct Inventory {
    stock: HashMap<String, u32>,
}

impl Inventory {
    /// An empty inventory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `quantity` units of `sku` to stock.
    pub fn restock(&mut self, sku: &str, quantity: u32) {
        *self.stock.entry(sku.to_string()).or_insert(0) += quantity;
    }

    /// Units of `sku` currently available.
    pub fn available(&self, sku: &str) -> u32 {
        self.stock.get(sku).copied().unwrap_or(0)
    }

    /// Takes `quantity` units of `sku` out of stock if enough are available.
    /// Returns whether the reservation succeeded; on failure nothing changes.
    pub fn reserve(&mut self, sku: &str, quantity: u32) -> bool {
        match self.stock.get_mut(sku) {
            Some(level) if *level >= quantity => {
                *level -= quantity;
                true
            }
            _ => false,
        }
    }

    /// Puts `quantity` units of `sku` back into stock.
    pub fn release(&mut self, sku: &str, quantity: u32) {
        self.restock(sku, quantity);
    }

    /// Every SKU at or below `threshold` units, sorted by name.
    pub fn low_stock(&self, threshold: u32) -> Vec<String> {
        let mut low: Vec<String> = self
            .stock
            .iter()
            .filter(|(_, level)| **level <= threshold)
            .map(|(sku, _)| sku.clone())
            .collect();
        low.sort();
        low
    }
}
