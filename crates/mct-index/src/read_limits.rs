//! Bounded reads of untrusted repository content, shared with source tools.

use std::io::{self, Read};
use std::path::Path;

pub const DEFAULT_MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_FILE_BYTES_ENV: &str = "MCT_MAX_FILE_BYTES";

/// Read a regular file, enforcing the limit even if it grows after stat.
pub fn read_repository_file(path: &Path) -> io::Result<Vec<u8>> {
    let limit = match std::env::var(MAX_FILE_BYTES_ENV) {
        Ok(raw) => raw.parse::<u64>().ok().filter(|n| *n > 0).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "MCT_MAX_FILE_BYTES must be a positive integer",
            )
        })?,
        Err(std::env::VarError::NotPresent) => DEFAULT_MAX_FILE_BYTES,
        Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidInput, error)),
    };
    read_with_limit(path, limit)
}

fn read_with_limit(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "repository input must be a regular file",
        ));
    }
    let too_large = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("repository file exceeds the {limit}-byte read limit"),
        )
    };
    if metadata.len() > limit {
        return Err(too_large());
    }
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1)).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > limit {
        return Err(too_large());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn exact_limit_is_allowed_but_larger_files_are_rejected() {
        let path = std::env::temp_dir().join(format!("mct-read-limit-{}", std::process::id()));
        std::fs::write(&path, b"1234").unwrap();
        assert_eq!(read_with_limit(&path, 4).unwrap(), b"1234");
        assert_eq!(
            read_with_limit(&path, 3).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        std::fs::remove_file(path).unwrap();
    }
}
