//! Path catalog (issue #629): every walked, non-excluded file with its
//! category, canonical format and the evidence for them, so an agent finds
//! a logo, a PDF or a font by path, name, category or format without
//! walking the tree or reading the file.

pub mod classify;
pub mod formats;

pub use classify::{classify, Classification, Evidence};
pub use formats::{canonical_format, Category};
