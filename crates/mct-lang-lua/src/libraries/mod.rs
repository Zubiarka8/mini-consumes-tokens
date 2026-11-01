//! Library-specific parser regressions. Test-only: production extraction goes
//! through the shared Lua parser, and no module here adds framework logic.

#[cfg(test)]
mod openresty;
