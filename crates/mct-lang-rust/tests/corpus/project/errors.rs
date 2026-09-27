//! Error type shared by every module of the inventory service, plus the
//! small audit log helpers the error paths use.

use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use crate::model::{LocationId, ProductId};

/// Result alias used throughout the crate.
pub type Result<T, E = InventoryError> = std::result::Result<T, E>;

/// How many errors have been reported since startup.
static ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Everything that can go wrong while handling inventory.
#[derive(Debug, Clone, PartialEq)]
pub enum InventoryError {
    /// Input failed validation; the message says why.
    Validation(String),
    /// No product with this id is known.
    UnknownProduct(ProductId),
    /// No location with this id is known.
    UnknownLocation(LocationId),
    /// Not enough stock to satisfy a request.
    InsufficientStock {
        product: ProductId,
        requested: u32,
        available: u32,
    },
    /// The storage backend failed.
    Storage { operation: &'static str, detail: String },
    /// A concurrent update won the race.
    Conflict { expected: u64, found: u64 },
}

impl InventoryError {
    /// Stable machine-readable code, used by the CLI's exit status and logs.
    pub fn code(&self) -> &'static str {
        match self {
            InventoryError::Validation(_) => "E_VALIDATION",
            InventoryError::UnknownProduct(_) => "E_UNKNOWN_PRODUCT",
            InventoryError::UnknownLocation(_) => "E_UNKNOWN_LOCATION",
            InventoryError::InsufficientStock { .. } => "E_STOCK",
            InventoryError::Storage { .. } => "E_STORAGE",
            InventoryError::Conflict { .. } => "E_CONFLICT",
        }
    }

    /// Whether retrying the same operation could succeed.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            InventoryError::Storage { .. } | InventoryError::Conflict { .. }
        )
    }

    pub fn storage(operation: &'static str, detail: impl fmt::Display) -> Self {
        InventoryError::Storage {
            operation,
            detail: detail.to_string(),
        }
    }

    /// Records this error in the global counter and the audit log.
    pub fn report(self) -> Self {
        ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
        audit(Severity::Error, &self.to_string());
        self
    }
}

impl fmt::Display for InventoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InventoryError::Validation(msg) => write!(f, "invalid input: {msg}"),
            InventoryError::UnknownProduct(id) => write!(f, "unknown product {}", id.0),
            InventoryError::UnknownLocation(id) => write!(f, "unknown location {}", id.0),
            InventoryError::InsufficientStock {
                product,
                requested,
                available,
            } => write!(
                f,
                "insufficient stock for product {}: requested {requested}, available {available}",
                product.0
            ),
            InventoryError::Storage { operation, detail } => {
                write!(f, "storage failure during {operation}: {detail}")
            }
            InventoryError::Conflict { expected, found } => write!(
                f,
                "version conflict: expected {expected}, found {found}"
            ),
        }
    }
}

impl Error for InventoryError {}

impl From<std::io::Error> for InventoryError {
    fn from(err: std::io::Error) -> Self {
        InventoryError::storage("io", err)
    }
}

impl From<std::num::ParseIntError> for InventoryError {
    fn from(err: std::num::ParseIntError) -> Self {
        InventoryError::Validation(format!("not a number: {err}"))
    }
}

/// Number of errors reported so far.
pub fn error_count() -> usize {
    ERROR_COUNT.load(Ordering::Relaxed)
}

/// Severity of an audit entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Debug,
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn tag(self) -> &'static str {
        match self {
            Severity::Debug => "DBG",
            Severity::Info => "INF",
            Severity::Warning => "WRN",
            Severity::Error => "ERR",
        }
    }
}

/// One line in the audit log.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub severity: Severity,
    pub message: String,
    pub sequence: usize,
}

/// In-memory audit log; a real deployment would ship these somewhere.
#[derive(Debug, Default)]
pub struct AuditLog {
    entries: Mutex<Vec<AuditEntry>>,
    min_severity: Option<Severity>,
}

impl AuditLog {
    pub const fn new() -> Self {
        AuditLog {
            entries: Mutex::new(Vec::new()),
            min_severity: None,
        }
    }

    pub fn with_threshold(min: Severity) -> Self {
        AuditLog {
            entries: Mutex::new(Vec::new()),
            min_severity: Some(min),
        }
    }

    pub fn push(&self, severity: Severity, message: &str) {
        if let Some(min) = self.min_severity {
            if severity < min {
                return;
            }
        }
        let mut entries = match self.entries.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let sequence = entries.len();
        entries.push(AuditEntry {
            severity,
            message: message.to_owned(),
            sequence,
        });
    }

    pub fn len(&self) -> usize {
        self.entries.lock().map(|e| e.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Entries at or above `severity`, oldest first.
    pub fn filtered(&self, severity: Severity) -> Vec<AuditEntry> {
        let entries = match self.entries.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        entries
            .iter()
            .filter(|e| e.severity >= severity)
            .cloned()
            .collect()
    }

    pub fn drain(&self) -> Vec<AuditEntry> {
        let mut entries = match self.entries.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        std::mem::take(&mut *entries)
    }
}

/// The process-wide audit log.
pub static AUDIT: AuditLog = AuditLog::new();

/// Appends a message to the process-wide audit log.
pub fn audit(severity: Severity, message: &str) {
    AUDIT.push(severity, message);
}

/// Formats a log line: `[ERR #3] message`.
pub fn format_entry(entry: &AuditEntry) -> String {
    format!("[{} #{}] {}", entry.severity.tag(), entry.sequence, entry.message)
}

/// Declares a validation helper that rejects values outside a range.
macro_rules! bounded {
    ($name:ident, $ty:ty, $min:expr, $max:expr) => {
        pub fn $name(value: $ty) -> Result<$ty> {
            if value < $min || value > $max {
                Err(InventoryError::Validation(format!(
                    "{} must be within {}..={}",
                    stringify!($name),
                    $min,
                    $max
                )))
            } else {
                Ok(value)
            }
        }
    };
}

bounded!(check_quantity, u32, 1, 1_000_000);
bounded!(check_shelf, u32, 1, 9_999);

/// Retries `op` up to `attempts` times while it fails with a retryable
/// error, auditing each failure.
pub fn with_retry<T, F>(attempts: u32, mut op: F) -> Result<T>
where
    F: FnMut(u32) -> Result<T>,
{
    let mut last = None;
    for attempt in 1..=attempts {
        match op(attempt) {
            Ok(v) => return Ok(v),
            Err(e) if e.is_retryable() => {
                audit(Severity::Warning, &format!("attempt {attempt} failed: {e}"));
                last = Some(e);
            }
            Err(e) => return Err(e.report()),
        }
    }
    Err(last
        .unwrap_or_else(|| InventoryError::storage("retry", "no attempts made"))
        .report())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable() {
        assert_eq!(
            InventoryError::Validation("x".into()).code(),
            "E_VALIDATION"
        );
        assert_eq!(
            InventoryError::Conflict {
                expected: 1,
                found: 2
            }
            .code(),
            "E_CONFLICT"
        );
    }

    #[test]
    fn retry_gives_up_on_non_retryable() {
        let mut calls = 0;
        let result: Result<()> = with_retry(5, |_| {
            calls += 1;
            Err(InventoryError::Validation("nope".into()))
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }

    #[test]
    fn retry_succeeds_after_conflict() {
        let result = with_retry(3, |attempt| {
            if attempt < 2 {
                Err(InventoryError::Conflict {
                    expected: 1,
                    found: 2,
                })
            } else {
                Ok(attempt)
            }
        });
        assert_eq!(result, Ok(2));
    }

    #[test]
    fn bounded_helpers_reject_out_of_range() {
        assert!(check_quantity(0).is_err());
        assert!(check_quantity(5).is_ok());
        assert!(check_shelf(10_000).is_err());
    }

    #[test]
    fn audit_log_respects_threshold() {
        let log = AuditLog::with_threshold(Severity::Warning);
        log.push(Severity::Info, "ignored");
        log.push(Severity::Error, "kept");
        assert_eq!(log.len(), 1);
        let entries = log.drain();
        assert_eq!(format_entry(&entries[0]), "[ERR #0] kept");
        assert!(log.is_empty());
    }

    #[test]
    fn display_mentions_numbers() {
        let e = InventoryError::InsufficientStock {
            product: ProductId(7),
            requested: 3,
            available: 1,
        };
        let text = e.to_string();
        assert!(text.contains("product 7"));
        assert!(text.contains("requested 3"));
    }

    #[test]
    fn io_errors_become_storage_errors() {
        let io = std::io::Error::new(std::io::ErrorKind::Other, "disk full");
        let e: InventoryError = io.into();
        assert!(e.is_retryable());
        assert_eq!(e.code(), "E_STORAGE");
    }

    #[test]
    fn parse_errors_become_validation_errors() {
        let e: InventoryError = "x".parse::<u32>().unwrap_err().into();
        assert_eq!(e.code(), "E_VALIDATION");
    }

    #[test]
    fn error_count_increases_on_report() {
        let before = error_count();
        let _ = InventoryError::UnknownProduct(ProductId(1)).report();
        assert!(error_count() > before);
    }

    #[test]
    fn severity_tags() {
        let tags: Vec<_> = [
            Severity::Debug,
            Severity::Info,
            Severity::Warning,
            Severity::Error,
        ]
        .iter()
        .map(|s| s.tag())
        .collect();
        assert_eq!(tags, ["DBG", "INF", "WRN", "ERR"]);
    }

    #[test]
    fn filtered_keeps_order() {
        let log = AuditLog::new();
        log.push(Severity::Error, "a");
        log.push(Severity::Debug, "b");
        log.push(Severity::Warning, "c");
        let kept: Vec<_> = log
            .filtered(Severity::Warning)
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert_eq!(kept, ["a", "c"]);
    }
}
