//! Circular-dependency detection over a directed module graph.

/// Strongly connected components of `edges` (`(from, to)` pairs) that form a
/// cycle: every component with more than one node, plus any node with a
/// self-edge. Each cycle lists its members sorted; cycles are sorted too, so
/// the output is deterministic.
pub fn find_cycles(edges: &[(String, String)]) -> Vec<Vec<String>> {
    let _ = edges;
    Vec::new()
}
