//! Wire-format and container helpers that were copied and renamed.
//!
//! Labeled evaluation corpus for the duplicate report (see ../labels.toon).
//! Parsed by mct-cli, never compiled.

// Case rs-real-checksum: one checksum rule for two frame sections.
pub fn header_checksum(bytes: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    for (index, byte) in bytes.iter().enumerate() {
        let weight = (index as u32 % 7) + 1;
        sum = sum.wrapping_add(u32::from(*byte) * weight);
    }
    sum ^ 0xA5A5_A5A5
}

fn payload_checksum(data: &[u8]) -> u32 {
    let mut acc: u32 = 0;
    for (position, value) in data.iter().enumerate() {
        let factor = (position as u32 % 7) + 1;
        acc = acc.wrapping_add(u32::from(*value) * factor);
    }
    acc ^ 0xA5A5_A5A5
}

pub struct Item {
    pub weight: f64,
    pub quantity: u32,
}

pub struct Inventory {
    items: Vec<Item>,
}

pub struct Cart {
    items: Vec<Item>,
}

// Case rs-real-methods: one total-weight rule in two containers of the same items.
impl Inventory {
    pub fn total_weight(&self) -> f64 {
        let mut total = 0.0;
        for item in &self.items {
            total += item.weight * f64::from(item.quantity);
        }
        total
    }
}

impl Cart {
    pub fn total_weight(&self) -> f64 {
        let mut sum = 0.0;
        for entry in &self.items {
            sum += entry.weight * f64::from(entry.quantity);
        }
        sum
    }
}
