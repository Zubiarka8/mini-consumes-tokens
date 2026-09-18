mod invoice;
mod logger;

use invoice::Invoice;

fn main() {
    let mut inv = Invoice::new();
    inv.add_item(10.0);
    inv.add_item_with_tax(10.0, 0.5);
}
