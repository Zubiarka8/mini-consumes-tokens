//! Command-line front end of the inventory service.
//!
//! ```text
//! inventory add <id> <name> [unit]
//! inventory receive <product> <location> <qty>
//! inventory ship <product> <location> <qty>
//! inventory move <product> <from> <to> <qty>
//! inventory report
//! ```

mod errors;
mod model;
mod report;
mod service;
mod storage;

use std::env;
use std::process::ExitCode;
use std::sync::Arc;

use errors::{audit, error_count, InventoryError, Result, Severity};
use model::{Category, LocationId, ProductId, Unit};
use report::{full_report, AuditSection, CategorySection, MovementSection, Section, StockSection};
use service::{MovementCounter, Warehouse};
use storage::SnapshotFile;

/// Default path of the snapshot file, relative to the working directory.
const DEFAULT_SNAPSHOT: &str = "inventory.tsv";

/// A parsed command line.
#[derive(Debug, PartialEq)]
enum Command {
    Add { id: u64, name: String, unit: Unit },
    Receive { product: ProductId, at: LocationId, quantity: u32 },
    Ship { product: ProductId, at: LocationId, quantity: u32 },
    Move {
        product: ProductId,
        from: LocationId,
        to: LocationId,
        quantity: u32,
    },
    Report,
    Help,
}

/// Options that apply to every command.
#[derive(Debug)]
struct Options {
    snapshot: String,
    verbose: bool,
    dry_run: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            snapshot: DEFAULT_SNAPSHOT.to_string(),
            verbose: false,
            dry_run: false,
        }
    }
}

/// Splits `--flag` options from positional arguments.
fn split_options(args: Vec<String>) -> Result<(Options, Vec<String>)> {
    let mut options = Options::default();
    let mut positional = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-v" | "--verbose" => options.verbose = true,
            "-n" | "--dry-run" => options.dry_run = true,
            "--snapshot" => {
                options.snapshot = iter.next().ok_or_else(|| {
                    InventoryError::Validation("--snapshot needs a path".into())
                })?;
            }
            flag if flag.starts_with("--") => {
                return Err(InventoryError::Validation(format!("unknown flag {flag}")));
            }
            _ => positional.push(arg),
        }
    }
    Ok((options, positional))
}

fn parse_unit(text: Option<&String>) -> Result<Unit> {
    match text.map(String::as_str) {
        None | Some("pc") => Ok(Unit::Piece),
        Some("kg") => Ok(Unit::Kilogram),
        Some("l") => Ok(Unit::Litre),
        Some(other) => match other.strip_prefix("box") {
            Some(n) => Ok(Unit::Box(n.parse()?)),
            None => Err(InventoryError::Validation(format!("unknown unit {other}"))),
        },
    }
}

/// Turns positional arguments into a [`Command`].
fn parse_command(args: &[String]) -> Result<Command> {
    let arg = |i: usize| -> Result<&String> {
        args.get(i)
            .ok_or_else(|| InventoryError::Validation(format!("missing argument #{i}")))
    };
    let number = |i: usize| -> Result<u64> { Ok(arg(i)?.parse()?) };
    let Some(verb) = args.first() else {
        return Ok(Command::Help);
    };
    Ok(match verb.as_str() {
        "add" => Command::Add {
            id: number(1)?,
            name: arg(2)?.clone(),
            unit: parse_unit(args.get(3))?,
        },
        "receive" | "ship" => {
            let product = ProductId(number(1)?);
            let at = LocationId(u32::try_from(number(2)?).map_err(|_| {
                InventoryError::Validation("location id too large".into())
            })?);
            let quantity = u32::try_from(number(3)?)
                .map_err(|_| InventoryError::Validation("quantity too large".into()))?;
            if verb == "receive" {
                Command::Receive {
                    product,
                    at,
                    quantity,
                }
            } else {
                Command::Ship {
                    product,
                    at,
                    quantity,
                }
            }
        }
        "move" => Command::Move {
            product: ProductId(number(1)?),
            from: LocationId(number(2)? as u32),
            to: LocationId(number(3)? as u32),
            quantity: number(4)? as u32,
        },
        "report" => Command::Report,
        "help" | "-h" | "--help" => Command::Help,
        other => {
            return Err(InventoryError::Validation(format!("unknown command {other}")));
        }
    })
}

fn usage() -> &'static str {
    "usage: inventory [--snapshot PATH] [-v] [-n] <add|receive|ship|move|report> ..."
}

/// Default category tree shown in reports.
fn default_categories() -> Category {
    Category::leaf("All")
        .with_child(
            Category::leaf("Food")
                .with_child(Category::leaf("Café"))
                .with_child(Category::leaf("Tea")),
        )
        .with_child(Category::leaf("Tools").with_child(Category::leaf("Wrenches")))
}

/// Loads the warehouse, applies `command`, and saves it back unless this
/// is a dry run.
fn execute(command: Command, options: &Options) -> Result<String> {
    let file = SnapshotFile::new(&options.snapshot);
    let mut warehouse = if file.path().exists() {
        Warehouse::load(&file)?
    } else {
        Warehouse::new()
    };
    let counter = Arc::new(MovementCounter::default());
    warehouse.subscribe(counter.clone());

    let output = match command {
        Command::Help => return Ok(usage().to_string()),
        Command::Add { id, name, unit } => {
            let pid = warehouse.add_product(id, &name, unit)?;
            format!("added {}", warehouse.product(pid)?.sku)
        }
        Command::Receive {
            product,
            at,
            quantity,
        } => {
            warehouse.receive(product, at, quantity)?;
            format!("received {quantity} of {}", product.0)
        }
        Command::Ship {
            product,
            at,
            quantity,
        } => {
            warehouse.ship(product, at, quantity)?;
            format!("shipped {quantity} of {}", product.0)
        }
        Command::Move {
            product,
            from,
            to,
            quantity,
        } => {
            warehouse.transfer(product, from, to, quantity)?;
            format!(
                "moved {quantity} of {} ({} movements)",
                product.0,
                counter.count(product)
            )
        }
        Command::Report => {
            let categories = default_categories();
            let stock = StockSection {
                warehouse: &warehouse,
                max_name: 24,
            };
            let movements = MovementSection {
                ledger: warehouse.ledger(),
            };
            let tree = CategorySection { root: &categories };
            let sections: [&dyn Section; 4] = [&stock, &movements, &tree, &AuditSection];
            return full_report(&sections);
        }
    };

    if options.dry_run {
        audit(Severity::Info, "dry run: snapshot not written");
    } else {
        warehouse.save(&file)?;
    }
    for (product, qty) in warehouse.reorder_suggestions()? {
        audit(
            Severity::Warning,
            &format!("reorder {qty} of product {}", product.0),
        );
    }
    Ok(output)
}

fn run(args: Vec<String>) -> Result<String> {
    let (options, positional) = split_options(args)?;
    if options.verbose {
        audit(Severity::Debug, &format!("options: {options:?}"));
    }
    let command = parse_command(&positional)?;
    execute(command, &options)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(args) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error [{}]: {err}", err.code());
            eprintln!("{}", usage());
            if error_count() > 1 {
                eprintln!("({} errors reported)", error_count());
            }
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_add_with_box_unit() {
        let cmd = parse_command(&args(&["add", "7", "Nails", "box50"])).unwrap();
        assert_eq!(
            cmd,
            Command::Add {
                id: 7,
                name: "Nails".into(),
                unit: Unit::Box(50)
            }
        );
    }

    #[test]
    fn empty_command_line_is_help() {
        assert_eq!(parse_command(&[]).unwrap(), Command::Help);
    }

    #[test]
    fn unknown_flag_is_rejected() {
        assert!(split_options(args(&["--nope"])).is_err());
    }

    #[test]
    fn snapshot_flag_takes_a_value() {
        let (opts, rest) = split_options(args(&["--snapshot", "x.tsv", "report"])).unwrap();
        assert_eq!(opts.snapshot, "x.tsv");
        assert_eq!(rest, ["report"]);
    }

    #[test]
    fn category_tree_has_unicode_leaf() {
        assert!(default_categories().find("Café").is_some());
    }
}
