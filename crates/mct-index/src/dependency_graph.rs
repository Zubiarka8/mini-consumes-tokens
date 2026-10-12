//! Module-level dependency graph built from resolved `Imports` relations.

use std::collections::{BTreeMap, BTreeSet};

use mct_core::MAX_QUERY_DEPTH;
use rusqlite::params;

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

/// Where one `imports` relation of a file points.
enum ImportTarget {
    /// Resolved to a definition in this indexed file.
    File(String),
    /// Named something outside the index (proven external or unresolved).
    External(String),
    /// Several candidates remain: no edge, no external entry.
    Ambiguous,
}

impl Index {
    /// Direct and transitive dependencies of the module at `module` (a file
    /// path, or a directory prefix treated as one module), up to `depth` hops.
    ///
    /// A module is an indexed file; a directory prefix is the union of its
    /// files, reported under the prefix itself. Only resolved `imports`
    /// relations produce edges; imports into the module itself are dropped,
    /// back-edges to already-visited modules are kept. An unknown module
    /// yields an empty graph that still names the root.
    pub fn dependency_graph(
        &self,
        module: &str,
        depth: u32,
        limit: usize,
    ) -> Result<DependencyGraph> {
        let root = module.trim().replace('\\', "/");
        let root = root.trim_end_matches('/').to_string();
        let mut graph = DependencyGraph {
            root: root.clone(),
            ..DependencyGraph::default()
        };
        let root_files = self.module_files(&root)?;
        if root_files.is_empty() {
            return Ok(graph);
        }
        let depth = depth.clamp(1, MAX_QUERY_DEPTH);
        let limit = limit.max(1);
        let root_set: BTreeSet<&str> = root_files.iter().map(String::as_str).collect();
        let node_of = |path: &str| -> String {
            if root_set.contains(path) {
                root.clone()
            } else {
                path.to_string()
            }
        };

        let mut visited: BTreeMap<String, u32> = BTreeMap::new();
        visited.insert(root.clone(), 0);
        graph.nodes.push(root.clone());
        let mut external = BTreeSet::new();
        let mut frontier = vec![root.clone()];

        for hop in 1..=depth {
            let mut next = Vec::new();
            for node in &frontier {
                let files = if *node == root {
                    root_files.clone()
                } else {
                    vec![node.clone()]
                };
                let mut targets = BTreeSet::new();
                for file in &files {
                    for target in self.import_targets(file)? {
                        match target {
                            ImportTarget::File(path) => {
                                let to = node_of(&path);
                                if to != *node {
                                    targets.insert(to);
                                }
                            }
                            ImportTarget::External(name) => {
                                external.insert(name);
                            }
                            ImportTarget::Ambiguous => {}
                        }
                    }
                }
                for to in targets {
                    if !visited.contains_key(&to) {
                        if graph.nodes.len() >= limit {
                            graph.truncated = true;
                            continue;
                        }
                        visited.insert(to.clone(), hop);
                        graph.nodes.push(to.clone());
                        next.push(to.clone());
                    }
                    graph.edges.push(DependencyEdge {
                        from: node.clone(),
                        to,
                        depth: hop,
                    });
                }
            }
            if next.is_empty() {
                break;
            }
            next.sort();
            frontier = next;
        }

        // Nodes: root, then depth, then path (discovery order within a level
        // follows the sorted frontier, not the path, so sort explicitly).
        graph.nodes.sort_by(|a, b| {
            let key = |n: &String| (*n != root, visited.get(n).copied().unwrap_or(0), n.clone());
            key(a).cmp(&key(b))
        });
        graph
            .edges
            .sort_by(|a, b| (a.depth, &a.from, &a.to).cmp(&(b.depth, &b.from, &b.to)));
        graph.external = external.into_iter().collect();
        Ok(graph)
    }

    /// Indexed files making up the module `path`: the file itself, or every
    /// file under it when it is a directory prefix. Sorted.
    fn module_files(&self, path: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT relative_path FROM files WHERE relative_path = ?1")?;
        let exact: Vec<String> = stmt
            .query_map(params![path], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        if !exact.is_empty() || path.is_empty() {
            return Ok(exact);
        }
        // Prefix match without LIKE so `%`/`_` in a path stay literal.
        let mut stmt = self.conn.prepare_cached(
            "SELECT relative_path FROM files
             WHERE substr(relative_path, 1, length(?1) + 1) = ?1 || '/'
             ORDER BY relative_path",
        )?;
        let rows = stmt
            .query_map(params![path], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Every `imports` relation made by a symbol of `file`, classified.
    fn import_targets(&self, file: &str) -> Result<Vec<ImportTarget>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT r.to_name, r.external, r.member,
                    (SELECT COUNT(*) FROM relation_candidates c WHERE c.relation_id = r.id),
                    (SELECT sf.relative_path FROM relation_candidates c
                       JOIN symbols s ON s.id = c.symbol_id
                       JOIN files sf ON sf.id = s.file_id
                      WHERE c.relation_id = r.id
                      ORDER BY sf.relative_path LIMIT 1)
             FROM relations r
             JOIN symbols caller ON caller.id = r.from_symbol_id
             JOIN files f ON f.id = caller.file_id
             WHERE f.relative_path = ?1 AND r.kind = 'imports'
             ORDER BY r.id",
        )?;
        let rows = stmt
            .query_map(params![file], |row| {
                let name: String = row.get(0)?;
                let external: bool = row.get(1)?;
                let member: bool = row.get(2)?;
                let count: i64 = row.get(3)?;
                let path: Option<String> = row.get(4)?;
                Ok(match (external, count, path) {
                    (true, _, _) | (false, 0, _) => ImportTarget::External(name),
                    (false, 1, Some(path)) if !member => ImportTarget::File(path),
                    _ => ImportTarget::Ambiguous,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}
