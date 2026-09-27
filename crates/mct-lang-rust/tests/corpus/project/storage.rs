//! Storage backends for the inventory: a generic repository trait, an
//! in-memory implementation and a line-based file snapshot format.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::errors::{audit, InventoryError, Result, Severity};
use crate::model::{
    make_sku, Identified, LocationId, Product, ProductId, StockLevel, StockRecord, Unit,
    Versioned,
};

/// A keyed collection of entities.
pub trait Repository<T: Identified> {
    fn get(&self, id: T::Id) -> Result<Option<T>>;
    fn put(&mut self, value: T) -> Result<()>;
    fn remove(&mut self, id: T::Id) -> Result<Option<T>>;
    fn all(&self) -> Result<Vec<T>>;

    /// Like [`Repository::get`], but a missing entity is an error.
    fn require(&self, id: T::Id) -> Result<T>
    where
        T::Id: Into<ProductId>,
    {
        self.get(id)?
            .ok_or_else(|| InventoryError::UnknownProduct(id.into()))
    }

    fn count(&self) -> Result<usize> {
        Ok(self.all()?.len())
    }
}

impl From<ProductId> for u64 {
    fn from(id: ProductId) -> u64 {
        id.0
    }
}

/// Repository kept entirely in memory, behind a `HashMap`.
#[derive(Debug)]
pub struct MemoryRepository<T: Identified> {
    items: HashMap<T::Id, T>,
    writes: usize,
}

impl<T: Identified> Default for MemoryRepository<T> {
    fn default() -> Self {
        MemoryRepository {
            items: HashMap::new(),
            writes: 0,
        }
    }
}

impl<T: Identified + Clone> MemoryRepository<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn writes(&self) -> usize {
        self.writes
    }

    /// Inserts many values at once, stopping at the first failure.
    pub fn extend<I>(&mut self, values: I) -> Result<usize>
    where
        I: IntoIterator<Item = T>,
    {
        let mut n = 0;
        for v in values {
            self.put(v)?;
            n += 1;
        }
        Ok(n)
    }
}

impl<T: Identified + Clone> Repository<T> for MemoryRepository<T> {
    fn get(&self, id: T::Id) -> Result<Option<T>> {
        Ok(self.items.get(&id).cloned())
    }

    fn put(&mut self, value: T) -> Result<()> {
        self.writes += 1;
        self.items.insert(value.id(), value);
        Ok(())
    }

    fn remove(&mut self, id: T::Id) -> Result<Option<T>> {
        self.writes += 1;
        Ok(self.items.remove(&id))
    }

    fn all(&self) -> Result<Vec<T>> {
        Ok(self.items.values().cloned().collect())
    }
}

/// Stock levels keyed by `(product, location)`, with optimistic versioning.
#[derive(Debug, Default, Clone)]
pub struct StockTable {
    rows: Arc<RwLock<HashMap<(ProductId, LocationId), StockRecord>>>,
}

impl StockTable {
    pub fn new() -> Self {
        Self::default()
    }

    fn read_rows(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, HashMap<(ProductId, LocationId), StockRecord>>> {
        self.rows
            .read()
            .map_err(|_| InventoryError::storage("read", "stock table lock poisoned"))
    }

    fn write_rows(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, HashMap<(ProductId, LocationId), StockRecord>>>
    {
        self.rows
            .write()
            .map_err(|_| InventoryError::storage("write", "stock table lock poisoned"))
    }

    pub fn level(&self, product: ProductId, location: LocationId) -> Result<Option<StockRecord>> {
        Ok(self.read_rows()?.get(&(product, location)).cloned())
    }

    /// Applies `change` to the row, creating an empty one first if needed.
    /// Fails with `Conflict` if `expected_version` is given and stale.
    pub fn modify<F>(
        &self,
        product: ProductId,
        location: LocationId,
        expected_version: Option<u64>,
        change: F,
    ) -> Result<u64>
    where
        F: FnOnce(&mut StockLevel) -> Result<()>,
    {
        let mut rows = self.write_rows()?;
        let record = rows.entry((product, location)).or_insert_with(|| {
            Versioned::new(StockLevel {
                product,
                location,
                on_hand: 0,
                reserved: 0,
            })
        });
        if let Some(expected) = expected_version {
            if expected != record.version {
                return Err(InventoryError::Conflict {
                    expected,
                    found: record.version,
                });
            }
        }
        let mut outcome = Ok(());
        record.update(|level| outcome = change(level));
        outcome.map(|()| record.version)
    }

    /// Every stock row of one product, across all locations.
    pub fn for_product(&self, product: ProductId) -> Result<Vec<StockLevel>> {
        let rows = self.read_rows()?;
        let mut levels: Vec<StockLevel> = rows
            .values()
            .filter(|r| r.value.product == product)
            .map(|r| r.value.clone())
            .collect();
        levels.sort_by_key(|l| l.location);
        Ok(levels)
    }

    pub fn total_on_hand(&self, product: ProductId) -> Result<u64> {
        Ok(self
            .for_product(product)?
            .iter()
            .map(|l| u64::from(l.on_hand))
            .sum())
    }

    pub fn snapshot(&self) -> Result<Vec<StockLevel>> {
        let rows = self.read_rows()?;
        let mut all: Vec<_> = rows.values().map(|r| r.value.clone()).collect();
        all.sort_by_key(|l| (l.product, l.location));
        Ok(all)
    }
}

/// Serialises products and stock levels to a simple tab-separated file.
pub struct SnapshotFile {
    path: PathBuf,
}

impl SnapshotFile {
    pub fn new(path: impl AsRef<Path>) -> Self {
        SnapshotFile {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes one `P` line per product and one `S` line per stock row.
    pub fn save(&self, products: &[Product], stock: &[StockLevel]) -> Result<()> {
        let mut file = File::create(&self.path)?;
        for p in products {
            writeln!(
                file,
                "P\t{}\t{}\t{}",
                p.id.0,
                encode_unit(p.unit),
                escape(&p.name)
            )?;
        }
        for s in stock {
            writeln!(
                file,
                "S\t{}\t{}\t{}\t{}",
                s.product.0, s.location.0, s.on_hand, s.reserved
            )?;
        }
        audit(
            Severity::Info,
            &format!("snapshot saved to {}", self.path.display()),
        );
        Ok(())
    }

    /// Reads a snapshot back. Unknown record types are skipped with a
    /// warning so old binaries can read newer files.
    pub fn load(&self) -> Result<(Vec<Product>, Vec<StockLevel>)> {
        let reader = BufReader::new(File::open(&self.path)?);
        let mut products = Vec::new();
        let mut stock = Vec::new();
        for (index, line) in reader.lines().enumerate() {
            let line = line?;
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["P", id, unit, name] => {
                    let id = ProductId(id.parse()?);
                    let mut product = Product::new(id, &unescape(name), decode_unit(unit)?)?;
                    product.sku = make_sku(id);
                    products.push(product);
                }
                ["S", product, location, on_hand, reserved] => stock.push(StockLevel {
                    product: ProductId(product.parse()?),
                    location: LocationId(location.parse()?),
                    on_hand: on_hand.parse()?,
                    reserved: reserved.parse()?,
                }),
                [] | [""] => {}
                other => audit(
                    Severity::Warning,
                    &format!("line {}: skipping {:?}", index + 1, other.first()),
                ),
            }
        }
        Ok((products, stock))
    }
}

fn encode_unit(unit: Unit) -> String {
    match unit {
        Unit::Box(n) => format!("box:{n}"),
        other => other.symbol().to_string(),
    }
}

fn decode_unit(text: &str) -> Result<Unit> {
    Ok(match text {
        "pc" => Unit::Piece,
        "kg" => Unit::Kilogram,
        "l" => Unit::Litre,
        _ => match text.strip_prefix("box:") {
            Some(n) => Unit::Box(n.parse()?),
            None => {
                return Err(InventoryError::Validation(format!("unknown unit {text:?}")));
            }
        },
    })
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(id: u64) -> Product {
        Product::new(ProductId(id), &format!("item {id}"), Unit::Piece).unwrap()
    }

    #[test]
    fn memory_repository_round_trip() {
        let mut repo = MemoryRepository::new();
        repo.extend([product(1), product(2)]).unwrap();
        assert_eq!(repo.count().unwrap(), 2);
        assert!(repo.require(ProductId(1)).is_ok());
        assert!(repo.require(ProductId(9)).is_err());
        assert_eq!(repo.writes(), 2);
    }

    #[test]
    fn stale_version_is_a_conflict() {
        let table = StockTable::new();
        let v = table
            .modify(ProductId(1), LocationId(1), None, |l| {
                l.on_hand = 5;
                Ok(())
            })
            .unwrap();
        let err = table
            .modify(ProductId(1), LocationId(1), Some(v - 1), |_| Ok(()))
            .unwrap_err();
        assert_eq!(err.code(), "E_CONFLICT");
    }

    #[test]
    fn escape_round_trips() {
        let raw = "tab\there\nnewline \\ backslash — ünïcødé";
        assert_eq!(unescape(&escape(raw)), raw);
    }

    #[test]
    fn units_round_trip() {
        for unit in [Unit::Piece, Unit::Kilogram, Unit::Litre, Unit::Box(12)] {
            assert_eq!(decode_unit(&encode_unit(unit)).unwrap(), unit);
        }
    }
}
