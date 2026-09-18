use crate::logger::log;

pub struct Invoice {
    total: f64,
}

impl Invoice {
    pub fn new() -> Self {
        Invoice { total: 0.0 }
    }

    pub fn add_item(&mut self, price: f64) -> f64 {
        self.total += price;
        log("item added");
        self.total
    }

    pub fn add_item_with_tax(&mut self, price: f64, tax_rate: f64) -> f64 {
        let taxed = price * (1.0 + tax_rate);
        self.total += taxed;
        log("item added with tax");
        self.total
    }
}
