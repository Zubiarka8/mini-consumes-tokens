use shop::inventory::Inventory;
use shop::orders::{place_order, LineItem, Order, OrderError};
use shop::store::OrderStore;

fn cart(quantity: u32, coupon: Option<&str>) -> Order {
    Order {
        customer_id: 7,
        items: vec![LineItem {
            sku: "mug".to_string(),
            quantity,
            unit_cents: 1_200,
        }],
        coupon: coupon.map(str::to_string),
    }
}

#[test]
fn test_place_order_stores_the_discounted_total() {
    let mut inventory = Inventory::new();
    inventory.restock("mug", 5);
    let mut store = OrderStore::new();
    let id = place_order(&cart(2, Some("WELCOME10")), &mut inventory, &mut store).unwrap();
    assert_eq!(store.get(id).unwrap().total_cents, 2_160);
    assert_eq!(inventory.available("mug"), 3);
}

#[test]
fn test_place_order_refuses_an_out_of_stock_line() {
    let mut inventory = Inventory::new();
    let mut store = OrderStore::new();
    let err = place_order(&cart(1, None), &mut inventory, &mut store).unwrap_err();
    assert_eq!(err, OrderError::OutOfStock("mug".to_string()));
    assert_eq!(store.revenue(), 0);
}
