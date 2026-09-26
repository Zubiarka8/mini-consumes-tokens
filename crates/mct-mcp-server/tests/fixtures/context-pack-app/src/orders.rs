//! Order intake: validates a cart, prices it and hands it to the store.

use crate::inventory::Inventory;
use crate::pricing::{apply_discount, subtotal};
use crate::store::{OrderStore, StoredOrder};

/// One line of a customer's cart.
#[derive(Debug, Clone)]
pub struct LineItem {
    pub sku: String,
    pub quantity: u32,
    pub unit_cents: u64,
}

/// A cart submitted for checkout.
#[derive(Debug, Clone)]
pub struct Order {
    pub customer_id: u64,
    pub items: Vec<LineItem>,
    pub coupon: Option<String>,
}

/// Why an order was refused.
#[derive(Debug, PartialEq)]
pub enum OrderError {
    EmptyCart,
    ZeroQuantity(String),
    OutOfStock(String),
}

/// Places `order`: checks it is well formed, reserves stock for every line,
/// prices it (coupon included) and persists it. Returns the stored order's
/// id, or the first reason the order was refused — stock reserved before a
/// refusal is released again.
pub fn place_order(
    order: &Order,
    inventory: &mut Inventory,
    store: &mut OrderStore,
) -> Result<u64, OrderError> {
    validate_order(order)?;
    reserve_stock(order, inventory)?;
    let total = apply_discount(subtotal(&order.items), order.coupon.as_deref());
    let id = store.insert(StoredOrder {
        customer_id: order.customer_id,
        total_cents: total,
        lines: order.items.len(),
    });
    Ok(id)
}

/// Rejects an empty cart or a line with a zero quantity.
pub fn validate_order(order: &Order) -> Result<(), OrderError> {
    if order.items.is_empty() {
        return Err(OrderError::EmptyCart);
    }
    for item in &order.items {
        if item.quantity == 0 {
            return Err(OrderError::ZeroQuantity(item.sku.clone()));
        }
    }
    Ok(())
}

/// Reserves every line's quantity, releasing what was already reserved if a
/// later line is out of stock.
fn reserve_stock(order: &Order, inventory: &mut Inventory) -> Result<(), OrderError> {
    let mut reserved: Vec<&LineItem> = Vec::new();
    for item in &order.items {
        if inventory.reserve(&item.sku, item.quantity) {
            reserved.push(item);
        } else {
            for done in reserved {
                inventory.release(&done.sku, done.quantity);
            }
            return Err(OrderError::OutOfStock(item.sku.clone()));
        }
    }
    Ok(())
}

/// Places every order in `orders`, collecting each one's outcome in order.
pub fn place_orders(
    orders: &[Order],
    inventory: &mut Inventory,
    store: &mut OrderStore,
) -> Vec<Result<u64, OrderError>> {
    orders
        .iter()
        .map(|order| place_order(order, inventory, store))
        .collect()
}

/// Cancels a stored order and gives its stock back.
pub fn cancel_order(id: u64, order: &Order, inventory: &mut Inventory, store: &mut OrderStore) -> bool {
    if store.remove(id).is_none() {
        return false;
    }
    for item in &order.items {
        inventory.release(&item.sku, item.quantity);
    }
    true
}
