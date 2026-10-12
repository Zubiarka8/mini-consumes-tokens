//! DOT and Mermaid renderings of a module dependency graph.
//!
//! Paths come from the indexed repository and are therefore untrusted: both
//! renderers escape or replace them so a hostile file name cannot inject
//! syntax into the output.

use std::collections::BTreeSet;
use std::fmt::Write as _;

/// Normalised view shared by both renderers.
struct Graph<'a> {
    nodes: BTreeSet<&'a str>,
    edges: BTreeSet<(&'a str, &'a str)>,
    cycle_nodes: BTreeSet<&'a str>,
    cycle_edges: BTreeSet<(&'a str, &'a str)>,
}

impl<'a> Graph<'a> {
    fn new(root: &'a str, edges: &'a [(String, String)], cycles: &'a [Vec<String>]) -> Self {
        let mut nodes = BTreeSet::new();
        nodes.insert(root);
        let mut edge_set = BTreeSet::new();
        for (from, to) in edges {
            nodes.insert(from.as_str());
            nodes.insert(to.as_str());
            edge_set.insert((from.as_str(), to.as_str()));
        }
        let mut cycle_nodes = BTreeSet::new();
        let mut cycle_edges = BTreeSet::new();
        for cycle in cycles {
            for (i, node) in cycle.iter().enumerate() {
                cycle_nodes.insert(node.as_str());
                nodes.insert(node.as_str());
                let next = cycle.get(i + 1).or_else(|| cycle.first());
                if let Some(next) = next {
                    cycle_edges.insert((node.as_str(), next.as_str()));
                }
            }
        }
        // Only edges that really exist are marked as cycle edges.
        cycle_edges.retain(|e| edge_set.contains(e));
        Self {
            nodes,
            edges: edge_set,
            cycle_nodes,
            cycle_edges,
        }
    }
}

fn dot_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Graphviz DOT. `edges` are `(from, to)`; nodes in a cycle are highlighted.
pub fn render_dot(root: &str, edges: &[(String, String)], cycles: &[Vec<String>]) -> String {
    let g = Graph::new(root, edges, cycles);
    let mut out = String::from("digraph dependencies {\n  rankdir=LR;\n");
    for node in &g.nodes {
        let q = dot_quote(node);
        let mut attrs = vec![format!("label={q}")];
        if *node == root {
            attrs.push("shape=doublecircle".into());
            attrs.push("style=bold".into());
        }
        if g.cycle_nodes.contains(node) {
            attrs.push("color=red".into());
        }
        let _ = writeln!(out, "  {q} [{}];", attrs.join(", "));
    }
    for (from, to) in &g.edges {
        let mut line = format!("  {} -> {}", dot_quote(from), dot_quote(to));
        if g.cycle_edges.contains(&(*from, *to)) {
            line.push_str(" [color=red, style=dashed]");
        }
        line.push(';');
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("}\n");
    out
}

/// Escape a label for a double-quoted Mermaid string using entity codes.
fn mermaid_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '#' => out.push_str("#35;"),
            '&' => out.push_str("#38;"),
            '"' => out.push_str("#quot;"),
            '<' => out.push_str("#lt;"),
            '>' => out.push_str("#gt;"),
            '|' => out.push_str("#124;"),
            '[' => out.push_str("#91;"),
            ']' => out.push_str("#93;"),
            '`' => out.push_str("#96;"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Mermaid `flowchart LR`. Same inputs as [`render_dot`].
pub fn render_mermaid(root: &str, edges: &[(String, String)], cycles: &[Vec<String>]) -> String {
    let g = Graph::new(root, edges, cycles);
    let ids: Vec<&str> = g.nodes.iter().copied().collect();
    // `ids` is sorted, so binary search yields the n<index> id.
    let id_of = |p: &str| -> String {
        let idx = ids.binary_search(&p).unwrap_or(0);
        format!("n{idx}")
    };
    let mut out = String::from("flowchart LR\n");
    for (i, node) in ids.iter().enumerate() {
        let _ = writeln!(out, "  n{i}[\"{}\"]", mermaid_label(node));
    }
    let mut cycle_links = Vec::new();
    for (i, (from, to)) in g.edges.iter().enumerate() {
        let (a, b) = (id_of(from), id_of(to));
        if g.cycle_edges.contains(&(*from, *to)) {
            let _ = writeln!(out, "  {a} -.-> {b}");
            cycle_links.push(i.to_string());
        } else {
            let _ = writeln!(out, "  {a} --> {b}");
        }
    }
    out.push_str("  classDef root stroke-width:3px,font-weight:bold\n");
    out.push_str("  classDef cycle stroke:red,color:red\n");
    let _ = writeln!(out, "  class {} root", id_of(root));
    if !g.cycle_nodes.is_empty() {
        let list: Vec<String> = g.cycle_nodes.iter().map(|n| id_of(n)).collect();
        let _ = writeln!(out, "  class {} cycle", list.join(","));
    }
    if !cycle_links.is_empty() {
        let _ = writeln!(
            out,
            "  linkStyle {} stroke:red,stroke-dasharray:5",
            cycle_links.join(",")
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(a: &str, b: &str) -> (String, String) {
        (a.to_string(), b.to_string())
    }

    fn chain() -> Vec<(String, String)> {
        vec![e("b.rs", "c.rs"), e("a.rs", "b.rs")]
    }

    #[test]
    fn dot_simple_chain() {
        let out = render_dot("a.rs", &chain(), &[]);
        assert!(out.starts_with("digraph dependencies {\n  rankdir=LR;\n"));
        assert!(out.contains("\"a.rs\" -> \"b.rs\";"));
        assert!(out.contains("\"b.rs\" -> \"c.rs\";"));
        assert!(out.contains("\"a.rs\" [label=\"a.rs\", shape=doublecircle, style=bold];"));
        assert!(!out.contains("red"));
    }

    #[test]
    fn mermaid_simple_chain() {
        let out = render_mermaid("a.rs", &chain(), &[]);
        assert!(out.starts_with("flowchart LR\n"));
        assert!(out.contains("n0[\"a.rs\"]"));
        assert!(out.contains("n0 --> n1"));
        assert!(out.contains("n1 --> n2"));
        assert!(out.contains("class n0 root"));
        assert!(!out.contains("class n0,"));
        assert!(!out.contains("linkStyle"));
    }

    #[test]
    fn cycles_are_highlighted() {
        let edges = vec![e("a", "b"), e("b", "a"), e("a", "c")];
        let cycles = vec![vec!["a".to_string(), "b".to_string()]];
        let dot = render_dot("a", &edges, &cycles);
        assert!(dot.contains("\"a\" -> \"b\" [color=red, style=dashed];"));
        assert!(dot.contains("\"b\" -> \"a\" [color=red, style=dashed];"));
        assert!(dot.contains("\"a\" -> \"c\";"));
        assert!(dot.contains("\"b\" [label=\"b\", color=red];"));
        assert!(dot.contains("\"c\" [label=\"c\"];"));
        let mm = render_mermaid("a", &edges, &cycles);
        assert!(mm.contains("n0 -.-> n1"));
        assert!(mm.contains("n1 -.-> n0"));
        assert!(mm.contains("n0 --> n2"));
        assert!(mm.contains("classDef cycle"));
        assert!(mm.contains("class n0,n1 cycle"));
        assert!(mm.contains("linkStyle 0,2 stroke:red"));
    }

    #[test]
    fn hostile_names_cannot_inject() {
        let evil = "x\"];\n}\nn1[\"y]-->|z|<b>`";
        let edges = vec![e("root", evil)];
        let dot = render_dot("root", &edges, &[]);
        assert!(dot.contains("x\\\"];\\n}\\nn1[\\\"y]-->|z|<b>`"));
        assert_eq!(dot.lines().count(), 6);
        let mm = render_mermaid("root", &edges, &[]);
        assert_eq!(mm.lines().count(), 7, "{mm}");
        let label = mm.lines().find(|l| l.starts_with("  n1[")).unwrap_or("");
        let inner = label.trim_start_matches("  n1[\"").trim_end_matches("\"]");
        for bad in ['"', '<', '>', '|', '[', ']', '`', '\n'] {
            assert!(!inner.contains(bad), "{bad:?} in {inner}");
        }
        assert!(!inner.contains("-->") || !inner.contains('>'));
        assert!(inner.contains("#quot;"));
    }

    #[test]
    fn deterministic_regardless_of_input_order() {
        let a = vec![e("a", "b"), e("b", "c"), e("a", "c")];
        let b = vec![e("a", "c"), e("b", "c"), e("a", "b"), e("a", "b")];
        assert_eq!(render_dot("a", &a, &[]), render_dot("a", &b, &[]));
        assert_eq!(render_mermaid("a", &a, &[]), render_mermaid("a", &b, &[]));
    }

    #[test]
    fn lone_root_is_rendered() {
        let dot = render_dot("only.rs", &[], &[]);
        assert!(dot.contains("\"only.rs\" [label=\"only.rs\""));
        assert!(!dot.contains("->"));
        let mm = render_mermaid("only.rs", &[], &[]);
        assert!(mm.contains("n0[\"only.rs\"]"));
        assert!(mm.contains("class n0 root"));
    }
}
