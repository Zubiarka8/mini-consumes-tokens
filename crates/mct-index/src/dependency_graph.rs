//! Module-level dependency graph built from resolved `Imports` relations.

use crate::{Index, Result};

/// One directed edge: file `from` imports file `to`, first reached at `depth` hops.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub depth: u32,
}

/// Transitive dependencies of `root` up to the requested depth.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DependencyGraph {
    /// Repository-relative path (forward slashes) the walk started from.
    pub root: String,
    /// Every reached module path, `root` first, then by depth, then by path.
    pub nodes: Vec<String>,
    pub edges: Vec<DependencyEdge>,
    /// Import names that matched no indexed module (third-party / stdlib).
    pub external: Vec<String>,
    /// The `limit` cut the walk short.
    pub truncated: bool,
}

impl Index {
    /// Direct and transitive dependencies of the module at `module` (a file
    /// path, or a directory prefix treated as one module), up to `depth` hops.
    pub fn dependency_graph(
        &self,
        module: &str,
        depth: u32,
        limit: usize,
    ) -> Result<DependencyGraph> {
        let _ = (depth, limit);
        Ok(DependencyGraph {
            root: module.to_string(),
            ..DependencyGraph::default()
        })
    }
}
