mod config;
mod util;

use crate::util::helper;
use serde::Serialize;
use std::fmt::Display;

pub fn run() -> usize {
    helper(&config::Settings::default())
}
