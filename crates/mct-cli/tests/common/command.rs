#![allow(clippy::unwrap_used)]

use std::process::{Command, Output};

pub fn mct(args: &[&dyn AsRef<std::ffi::OsStr>]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mct-cli"))
        .args(args.iter().map(|arg| arg.as_ref()))
        .output()
        .unwrap()
}
