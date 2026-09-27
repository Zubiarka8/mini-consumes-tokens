//! Domain model for the warehouse inventory service: products, stock
//! levels, locations and the movements between them.

use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::hash::{Hash, Hasher};

use crate::errors::{InventoryError, Result};

/// Maximum number of characters a product name may have.
pub const MAX_NAME_LEN: usize = 120;

/// Default reorder threshold for a freshly created product.
pub const DEFAULT_REORDER_POINT: u32 = 10;

/// Global counter prefix used for generated SKUs.
pub static SKU_PREFIX: &str = "WH";

/// Identifier of a product, wrapped so it can't be mixed with other ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProductId(pub u64);

/// Identifier of a storage location (aisle/shelf/bin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocationId(pub u32);

/// A tuple-less unit marker for products that are not physically stocked.
#[derive(Debug, Default, Clone, Copy)]
pub struct Virtual;

/// Unit in which a product is counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Piece,
    Kilogram,
    Litre,
    /// A box containing `n` pieces.
    Box(u16),
}

impl Unit {
    /// Converts a quantity in this unit to pieces, when that makes sense.
    pub fn to_pieces(self, quantity: u32) -> Option<u32> {
        match self {
            Unit::Piece => Some(quantity),
            Unit::Box(n) => quantity.checked_mul(u32::from(n)),
            Unit::Kilogram | Unit::Litre => None,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            Unit::Piece => "pc",
            Unit::Kilogram => "kg",
            Unit::Litre => "l",
            Unit::Box(_) => "box",
        }
    }
}

impl Display for Unit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Unit::Box(n) => write!(f, "box of {n}"),
            other => f.write_str(other.symbol()),
        }
    }
}

/// Category tree node; categories nest arbitrarily deep.
#[derive(Debug, Clone)]
pub struct Category {
    pub name: String,
    pub children: Vec<Category>,
}

impl Category {
    pub fn leaf(name: &str) -> Self {
        Category {
            name: name.to_owned(),
            children: Vec::new(),
        }
    }

    pub fn with_child(mut self, child: Category) -> Self {
        self.children.push(child);
        self
    }

    /// Depth-first search for a category by name.
    pub fn find(&self, name: &str) -> Option<&Category> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(name))
    }

    /// Total number of categories in this subtree, including `self`.
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Category::count).sum::<usize>()
    }
}

/// A product that can be stocked in the warehouse.
#[derive(Debug, Clone)]
pub struct Product {
    pub id: ProductId,
    pub sku: String,
    pub name: String,
    pub unit: Unit,
    pub reorder_point: u32,
    pub tags: Vec<String>,
    pub attributes: BTreeMap<String, String>,
}

impl Product {
    /// Builds a validated product.
    pub fn new(id: ProductId, name: &str, unit: Unit) -> Result<Self> {
        let name = name.trim();
        validate_name(name)?;
        Ok(Product {
            id,
            sku: make_sku(id),
            name: name.to_string(),
            unit,
            reorder_point: DEFAULT_REORDER_POINT,
            tags: Vec::new(),
            attributes: BTreeMap::new(),
        })
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn set_attribute(&mut self, key: &str, value: &str) {
        self.attributes.insert(key.to_owned(), value.to_owned());
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }

    /// Whether `quantity` on hand is at or below the reorder point.
    pub fn needs_reorder(&self, quantity: u32) -> bool {
        quantity <= self.reorder_point
    }
}

impl PartialEq for Product {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Product {}

impl Hash for Product {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Checks a product name against the naming rules.
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(InventoryError::Validation("product name is empty".into()));
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(InventoryError::Validation(format!(
            "product name longer than {MAX_NAME_LEN} characters"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(InventoryError::Validation(
            "product name contains control characters".into(),
        ));
    }
    Ok(())
}

/// Generates the SKU for a product id, e.g. `WH-000042`.
pub fn make_sku(id: ProductId) -> String {
    format!("{SKU_PREFIX}-{:06}", id.0)
}

/// Where and how much of a product is stored.
#[derive(Debug, Clone, PartialEq)]
pub struct StockLevel {
    pub product: ProductId,
    pub location: LocationId,
    pub on_hand: u32,
    pub reserved: u32,
}

impl StockLevel {
    pub fn available(&self) -> u32 {
        self.on_hand.saturating_sub(self.reserved)
    }

    pub fn reserve(&mut self, quantity: u32) -> Result<()> {
        if quantity > self.available() {
            return Err(InventoryError::InsufficientStock {
                product: self.product,
                requested: quantity,
                available: self.available(),
            });
        }
        self.reserved += quantity;
        Ok(())
    }

    pub fn release(&mut self, quantity: u32) {
        self.reserved = self.reserved.saturating_sub(quantity);
    }
}

/// A single stock movement, as recorded in the ledger.
#[derive(Debug, Clone, PartialEq)]
pub enum Movement {
    Receive {
        product: ProductId,
        to: LocationId,
        quantity: u32,
    },
    Ship {
        product: ProductId,
        from: LocationId,
        quantity: u32,
    },
    Transfer {
        product: ProductId,
        from: LocationId,
        to: LocationId,
        quantity: u32,
    },
    Adjust {
        product: ProductId,
        at: LocationId,
        delta: i64,
        reason: String,
    },
}

impl Movement {
    pub fn product(&self) -> ProductId {
        match self {
            Movement::Receive { product, .. }
            | Movement::Ship { product, .. }
            | Movement::Transfer { product, .. }
            | Movement::Adjust { product, .. } => *product,
        }
    }

    /// Signed change of stock at `location` caused by this movement.
    pub fn delta_at(&self, location: LocationId) -> i64 {
        match *self {
            Movement::Receive { to, quantity, .. } if to == location => i64::from(quantity),
            Movement::Ship { from, quantity, .. } if from == location => -i64::from(quantity),
            Movement::Transfer {
                from, to, quantity, ..
            } => {
                let q = i64::from(quantity);
                if from == location {
                    -q
                } else if to == location {
                    q
                } else {
                    0
                }
            }
            Movement::Adjust { at, delta, .. } if at == location => delta,
            _ => 0,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Movement::Receive { quantity, .. } => format!("received {quantity}"),
            Movement::Ship { quantity, .. } => format!("shipped {quantity}"),
            Movement::Transfer { quantity, .. } => format!("moved {quantity}"),
            Movement::Adjust { delta, reason, .. } => format!("adjusted {delta:+} ({reason})"),
        }
    }
}

/// Anything that has a stable identifier.
pub trait Identified {
    type Id: Copy + Eq + Hash;

    fn id(&self) -> Self::Id;

    /// Default: a readable label built from the id.
    fn label(&self) -> String
    where
        Self::Id: fmt::Debug,
    {
        format!("{:?}", self.id())
    }
}

impl Identified for Product {
    type Id = ProductId;

    fn id(&self) -> ProductId {
        self.id
    }

    fn label(&self) -> String {
        format!("{} ({})", self.name, self.sku)
    }
}

impl Identified for StockLevel {
    type Id = (ProductId, LocationId);

    fn id(&self) -> Self::Id {
        (self.product, self.location)
    }
}

/// Generic pair of a value and when it was last changed.
#[derive(Debug, Clone)]
pub struct Versioned<T> {
    pub value: T,
    pub version: u64,
}

impl<T: Clone> Versioned<T> {
    pub fn new(value: T) -> Self {
        Versioned { value, version: 1 }
    }

    pub fn update<F>(&mut self, f: F)
    where
        F: FnOnce(&mut T),
    {
        f(&mut self.value);
        self.version += 1;
    }

    pub fn map<U, F: Fn(&T) -> U>(&self, f: F) -> Versioned<U> {
        Versioned {
            value: f(&self.value),
            version: self.version,
        }
    }
}

/// Shorthand for the versioned stock levels the service keeps.
pub type StockRecord = Versioned<StockLevel>;

/// Products keyed by id, in a stable order.
pub type Catalog = BTreeMap<ProductId, Product>;

/// Nested module grouping the dimension helpers.
pub mod dimensions {
    /// Physical size of a stocked item, in millimetres.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Size {
        pub width: u32,
        pub height: u32,
        pub depth: u32,
    }

    impl Size {
        pub const ZERO: Size = Size {
            width: 0,
            height: 0,
            depth: 0,
        };

        pub fn volume(&self) -> u64 {
            u64::from(self.width) * u64::from(self.height) * u64::from(self.depth)
        }

        pub fn fits_in(&self, other: &Size) -> bool {
            let mut mine = [self.width, self.height, self.depth];
            let mut theirs = [other.width, other.height, other.depth];
            mine.sort_unstable();
            theirs.sort_unstable();
            mine.iter().zip(theirs.iter()).all(|(a, b)| a <= b)
        }
    }

    /// Volume of a stack of `n` identical items.
    pub fn stacked_volume(size: &Size, n: u32) -> u64 {
        size.volume() * u64::from(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sku_is_zero_padded() {
        assert_eq!(make_sku(ProductId(42)), "WH-000042");
    }

    #[test]
    fn unicode_names_are_accepted() {
        let p = Product::new(ProductId(1), "Café crème — 1 kg", Unit::Kilogram);
        assert!(p.is_ok());
    }
}
