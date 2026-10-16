//! Loaders that repeat one line-parsing block but differ around it.
//!
//! Labeled evaluation corpus for the duplicate report (see ../labels.toon).
//! Parsed by mct-cli, never compiled.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

// Case rs-block-parse: the "read_to_string ... for line" block repeats in both loaders.
pub fn load_config(path: &Path) -> Result<Config, LoadError> {
    let text = fs::read_to_string(path).map_err(LoadError::Io)?;
    let mut values = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some(pair) => pair,
            None => return Err(LoadError::Syntax(number + 1)),
        };
        values.insert(key.trim().to_string(), value.trim().to_string());
    }
    let port = values.get("port").and_then(|v| v.parse().ok()).unwrap_or(8080);
    Ok(Config { values, port })
}

pub fn load_secrets(path: &Path, vault: &Vault) -> Result<Secrets, LoadError> {
    if !vault.is_unlocked() {
        return Err(LoadError::Locked);
    }
    let text = fs::read_to_string(path).map_err(LoadError::Io)?;
    let mut values = BTreeMap::new();
    for (number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some(pair) => pair,
            None => return Err(LoadError::Syntax(number + 1)),
        };
        values.insert(key.trim().to_string(), value.trim().to_string());
    }
    let decrypted = vault.decrypt_all(&values)?;
    Ok(Secrets { entries: decrypted })
}

// Case rs-nearmiss-timing: a two-line timing snippet in two different functions.
pub fn compact_log(log: &mut Log) -> usize {
    let started = Instant::now();
    let before = log.len();
    log.retain(|entry| !entry.superseded);
    log.shrink_to_fit();
    eprintln!("compacted in {:?}", started.elapsed());
    before - log.len()
}

pub fn rebuild_cache(cache: &mut Cache, source: &Source) -> bool {
    let started = Instant::now();
    cache.clear();
    for (key, value) in source.entries() {
        cache.put(key, value);
    }
    eprintln!("compacted in {:?}", started.elapsed());
    cache.is_warm()
}
