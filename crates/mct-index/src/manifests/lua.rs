//! LuaRocks metadata, read statically: nothing here runs Lua, spawns a
//! process or touches the network. Everything but the narrow items re-exported
//! below stays private to this module.

mod data;
mod rockspec;
mod tree;
mod version;

pub(crate) use rockspec::{
    analyse, Analysis, DECLARATION_SECTIONS, KIND_DEPENDENCY, KIND_FILE, KIND_MODULE, KIND_SECTION,
    STATE_COMPLETE, STATE_INCOMPLETE, STATE_UNREADABLE,
};
pub(crate) use tree::{parse_lockfile, parse_store_manifest, Lockfile, StoreError, StoreManifest};

/// Whether `installed` meets `constraint` under LuaRocks' own rules. `None`
/// when LuaRocks itself would refuse the constraint text. No constraint
/// accepts every version.
pub(crate) fn constraint_accepts(constraint: Option<&str>, installed: &str) -> Option<bool> {
    let Some(text) = constraint else {
        return Some(true);
    };
    version::parse_constraints(text).map(|c| c.accepts(&version::parse_version(installed)))
}

/// A plain `parse_rockspec` for the generic manifest dispatcher.
pub(crate) fn parse_rockspec(contents: &str) -> Vec<super::ManifestDependency> {
    analyse(contents).projection()
}
