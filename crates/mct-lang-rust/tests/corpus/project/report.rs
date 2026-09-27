//! Human-readable reports over the warehouse: stock tables, movement
//! summaries and a tiny text-table renderer.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::errors::{format_entry, InventoryError, Result, Severity, AUDIT};
use crate::model::{dimensions::Size, Category, Movement, Product, ProductId, StockLevel};
use crate::service::Warehouse;

/// Column alignment in a rendered table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
    Center,
}

/// A column definition: header and alignment.
#[derive(Debug, Clone)]
pub struct Column {
    pub header: String,
    pub align: Align,
}

impl Column {
    pub fn left(header: &str) -> Self {
        Column {
            header: header.to_string(),
            align: Align::Left,
        }
    }

    pub fn right(header: &str) -> Self {
        Column {
            header: header.to_string(),
            align: Align::Right,
        }
    }
}

/// A plain-text table with fixed columns.
#[derive(Debug, Clone, Default)]
pub struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(columns: Vec<Column>) -> Self {
        Table {
            columns,
            rows: Vec::new(),
        }
    }

    pub fn row<I, S>(&mut self, cells: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut row: Vec<String> = cells.into_iter().map(Into::into).collect();
        row.resize(self.columns.len(), String::new());
        self.rows.push(row);
        self
    }

    fn widths(&self) -> Vec<usize> {
        self.columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                self.rows
                    .iter()
                    .map(|r| display_width(&r[i]))
                    .chain(std::iter::once(display_width(&c.header)))
                    .max()
                    .unwrap_or(0)
            })
            .collect()
    }

    /// Renders the table with a header rule.
    pub fn render(&self) -> String {
        let widths = self.widths();
        let mut out = String::new();
        let header: Vec<String> = self
            .columns
            .iter()
            .zip(&widths)
            .map(|(c, w)| pad(&c.header, *w, c.align))
            .collect();
        let _ = writeln!(out, "{}", header.join(" | "));
        let rule: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
        let _ = writeln!(out, "{}", rule.join("-+-"));
        for row in &self.rows {
            let cells: Vec<String> = row
                .iter()
                .zip(self.columns.iter().zip(&widths))
                .map(|(cell, (col, w))| pad(cell, *w, col.align))
                .collect();
            let _ = writeln!(out, "{}", cells.join(" | "));
        }
        out
    }
}

/// Width in characters, not bytes, so "Crème" counts as 5.
pub fn display_width(text: &str) -> usize {
    text.chars().count()
}

/// Pads `text` to `width` according to `align`.
pub fn pad(text: &str, width: usize, align: Align) -> String {
    let len = display_width(text);
    if len >= width {
        return text.to_string();
    }
    let gap = width - len;
    match align {
        Align::Left => format!("{text}{}", " ".repeat(gap)),
        Align::Right => format!("{}{text}", " ".repeat(gap)),
        Align::Center => {
            let left = gap / 2;
            format!("{}{text}{}", " ".repeat(left), " ".repeat(gap - left))
        }
    }
}

/// Shortens `text` to at most `max` characters, adding an ellipsis.
pub fn truncate(text: &str, max: usize) -> Cow<'_, str> {
    if display_width(text) <= max {
        Cow::Borrowed(text)
    } else {
        let kept: String = text.chars().take(max.saturating_sub(1)).collect();
        Cow::Owned(format!("{kept}…"))
    }
}

/// Anything that can be rendered as a report section.
pub trait Section {
    fn title(&self) -> Cow<'_, str>;
    fn body(&self) -> Result<String>;

    fn render(&self) -> Result<String> {
        let title = self.title();
        let underline = "=".repeat(display_width(&title));
        Ok(format!("{title}\n{underline}\n{}", self.body()?))
    }
}

/// Current stock per product and location.
pub struct StockSection<'a> {
    pub warehouse: &'a Warehouse,
    pub max_name: usize,
}

impl Section for StockSection<'_> {
    fn title(&self) -> Cow<'_, str> {
        Cow::Borrowed("Stock on hand")
    }

    fn body(&self) -> Result<String> {
        let catalog = self.warehouse.catalog()?;
        let mut table = Table::new(vec![
            Column::left("SKU"),
            Column::left("Product"),
            Column::right("On hand"),
            Column::right("Unit"),
        ]);
        for product in catalog.values() {
            let total = total_for(self.warehouse, product)?;
            table.row([
                product.sku.clone(),
                truncate(&product.name, self.max_name).into_owned(),
                total.to_string(),
                product.unit.to_string(),
            ]);
        }
        Ok(table.render())
    }
}

fn total_for(warehouse: &Warehouse, product: &Product) -> Result<u64> {
    let per_location = stock_by_location(warehouse, product.id)?;
    Ok(per_location.values().map(|l| u64::from(l.on_hand)).sum())
}

fn stock_by_location(
    warehouse: &Warehouse,
    product: ProductId,
) -> Result<BTreeMap<u32, StockLevel>> {
    let levels = replay_levels(warehouse.ledger(), product);
    Ok(levels.into_iter().map(|l| (l.location.0, l)).collect())
}

/// Rebuilds the levels of one product from the ledger alone.
fn replay_levels(ledger: &[Movement], product: ProductId) -> Vec<StockLevel> {
    use crate::model::LocationId;

    fn bump(map: &mut BTreeMap<LocationId, i64>, at: LocationId, by: i64) {
        *map.entry(at).or_insert(0) += by;
    }

    let mut totals = BTreeMap::new();
    for m in ledger.iter().filter(|m| m.product() == product) {
        match m {
            Movement::Receive { to, .. } => bump(&mut totals, *to, m.delta_at(*to)),
            Movement::Ship { from, .. } => bump(&mut totals, *from, m.delta_at(*from)),
            Movement::Transfer { from, to, .. } => {
                bump(&mut totals, *from, m.delta_at(*from));
                bump(&mut totals, *to, m.delta_at(*to));
            }
            Movement::Adjust { at, .. } => bump(&mut totals, *at, m.delta_at(*at)),
        }
    }
    totals
        .into_iter()
        .map(|(location, on_hand)| StockLevel {
            product,
            location,
            on_hand: u32::try_from(on_hand.max(0)).unwrap_or(u32::MAX),
            reserved: 0,
        })
        .collect()
}

/// Movement counts by kind.
pub struct MovementSection<'a> {
    pub ledger: &'a [Movement],
}

impl Section for MovementSection<'_> {
    fn title(&self) -> Cow<'_, str> {
        Cow::Owned(format!("Movements ({})", self.ledger.len()))
    }

    fn body(&self) -> Result<String> {
        let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
        for m in self.ledger {
            let kind = match m {
                Movement::Receive { .. } => "receive",
                Movement::Ship { .. } => "ship",
                Movement::Transfer { .. } => "transfer",
                Movement::Adjust { .. } => "adjust",
            };
            *by_kind.entry(kind).or_default() += 1;
        }
        let mut table = Table::new(vec![Column::left("Kind"), Column::right("Count")]);
        for (kind, count) in by_kind {
            table.row([kind.to_string(), count.to_string()]);
        }
        Ok(table.render())
    }
}

/// Category tree, indented by depth.
pub struct CategorySection<'a> {
    pub root: &'a Category,
}

impl Section for CategorySection<'_> {
    fn title(&self) -> Cow<'_, str> {
        Cow::Borrowed("Categories")
    }

    fn body(&self) -> Result<String> {
        fn walk(node: &Category, depth: usize, out: &mut String) {
            let _ = writeln!(out, "{}- {}", "  ".repeat(depth), node.name);
            for child in &node.children {
                walk(child, depth + 1, out);
            }
        }
        let mut out = String::new();
        walk(self.root, 0, &mut out);
        let _ = writeln!(out, "({} categories)", self.root.count());
        Ok(out)
    }
}

/// Recent audit entries at warning level or above.
pub struct AuditSection;

impl Section for AuditSection {
    fn title(&self) -> Cow<'_, str> {
        "Warnings".into()
    }

    fn body(&self) -> Result<String> {
        let entries = AUDIT.filtered(Severity::Warning);
        if entries.is_empty() {
            return Ok("none\n".to_string());
        }
        Ok(entries.iter().map(format_entry).collect::<Vec<_>>().join("\n") + "\n")
    }
}

/// Renders every section, separated by blank lines.
pub fn full_report(sections: &[&dyn Section]) -> Result<String> {
    let mut parts = Vec::with_capacity(sections.len());
    for section in sections {
        parts.push(section.render()?);
    }
    if parts.is_empty() {
        return Err(InventoryError::Validation("empty report".into()));
    }
    Ok(parts.join("\n"))
}

/// One-line summary of a shelf's capacity usage.
pub fn shelf_usage(shelf: &Size, item: &Size, count: u32) -> String {
    let used = crate::model::dimensions::stacked_volume(item, count);
    let capacity = shelf.volume().max(1);
    let percent = used * 100 / capacity;
    let bar_len = usize::try_from(percent.min(100) / 5).unwrap_or(20);
    format!(
        "[{}{}] {percent}% ({} items, fits: {})",
        "#".repeat(bar_len),
        ".".repeat(20 - bar_len),
        count,
        item.fits_in(shelf)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_by_characters() {
        assert_eq!(pad("Crème", 7, Align::Right), "  Crème");
        assert_eq!(pad("ab", 6, Align::Center), "  ab  ");
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate("Schraubenschlüssel", 8), "Schraub…");
        assert_eq!(truncate("short", 8), "short");
    }
}
