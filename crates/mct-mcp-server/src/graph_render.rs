//! DOT and Mermaid renderings of a module dependency graph.

/// Graphviz DOT. `edges` are `(from, to)`; nodes in a cycle are highlighted.
pub fn render_dot(root: &str, edges: &[(String, String)], cycles: &[Vec<String>]) -> String {
    let _ = (root, edges, cycles);
    String::new()
}

/// Mermaid `flowchart LR`. Same inputs as [`render_dot`].
pub fn render_mermaid(root: &str, edges: &[(String, String)], cycles: &[Vec<String>]) -> String {
    let _ = (root, edges, cycles);
    String::new()
}
