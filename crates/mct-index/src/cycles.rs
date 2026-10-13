//! Circular-dependency detection over a directed module graph.

use std::collections::HashMap;

/// Strongly connected components of `edges` (`(from, to)` pairs) that form a
/// cycle: every component with more than one node, plus any node with a
/// self-edge. Each cycle lists its members sorted; cycles are sorted too, so
/// the output is deterministic.
///
/// Iterative Tarjan: O(V + E) time, no recursion, so arbitrarily deep graphs
/// cannot overflow the stack. Duplicate edges are tolerated.
pub fn find_cycles(edges: &[(String, String)]) -> Vec<Vec<String>> {
    // Intern node names into dense ids.
    let mut ids: HashMap<&str, usize> = HashMap::new();
    let mut names: Vec<&str> = Vec::new();
    let mut pairs: Vec<(usize, usize)> = Vec::with_capacity(edges.len());
    for (from, to) in edges {
        let a = id_of(from, &mut ids, &mut names);
        let b = id_of(to, &mut ids, &mut names);
        pairs.push((a, b));
    }
    let n = names.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut self_loop = vec![false; n];
    for &(a, b) in &pairs {
        if a == b {
            self_loop[a] = true;
        }
        adj[a].push(b);
    }

    const UNVISITED: usize = usize::MAX;
    let mut index = vec![UNVISITED; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    // Call stack frames: (node, next neighbour position).
    let mut call: Vec<(usize, usize)> = Vec::new();
    let mut counter = 0usize;
    let mut cycles: Vec<Vec<String>> = Vec::new();

    for root in 0..n {
        if index[root] != UNVISITED {
            continue;
        }
        call.push((root, 0));
        while let Some(&(v, pos)) = call.last() {
            if pos == 0 && index[v] == UNVISITED {
                index[v] = counter;
                low[v] = counter;
                counter += 1;
                stack.push(v);
                on_stack[v] = true;
            }
            if let Some(&w) = adj[v].get(pos) {
                if let Some(top) = call.last_mut() {
                    top.1 += 1;
                }
                if index[w] == UNVISITED {
                    call.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            // All neighbours done: finish v.
            call.pop();
            if let Some(&(parent, _)) = call.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == index[v] {
                let mut comp: Vec<usize> = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    comp.push(w);
                    if w == v {
                        break;
                    }
                }
                if comp.len() > 1 || self_loop[v] {
                    let mut members: Vec<String> =
                        comp.iter().map(|&i| names[i].to_string()).collect();
                    members.sort();
                    cycles.push(members);
                }
            }
        }
    }
    cycles.sort();
    cycles
}

fn id_of<'a>(name: &'a str, ids: &mut HashMap<&'a str, usize>, names: &mut Vec<&'a str>) -> usize {
    if let Some(&id) = ids.get(name) {
        return id;
    }
    let id = names.len();
    ids.insert(name, id);
    names.push(name);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    fn c(groups: &[&[&str]]) -> Vec<Vec<String>> {
        groups
            .iter()
            .map(|g| g.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    #[test]
    fn empty_input() {
        assert!(find_cycles(&[]).is_empty());
    }

    #[test]
    fn no_cycle() {
        assert!(find_cycles(&e(&[("a", "b"), ("b", "c"), ("a", "c")])).is_empty());
    }

    #[test]
    fn two_cycle() {
        assert_eq!(
            find_cycles(&e(&[("b", "a"), ("a", "b")])),
            c(&[&["a", "b"]])
        );
    }

    #[test]
    fn three_cycle() {
        let r = find_cycles(&e(&[("a", "b"), ("b", "c"), ("c", "a")]));
        assert_eq!(r, c(&[&["a", "b", "c"]]));
    }

    #[test]
    fn self_loop() {
        assert_eq!(find_cycles(&e(&[("a", "a"), ("a", "b")])), c(&[&["a"]]));
    }

    #[test]
    fn two_disjoint_cycles_sorted_by_first_member() {
        let r = find_cycles(&e(&[("x", "y"), ("y", "x"), ("a", "b"), ("b", "a")]));
        assert_eq!(r, c(&[&["a", "b"], &["x", "y"]]));
    }

    #[test]
    fn cycle_plus_tail() {
        let r = find_cycles(&e(&[("t", "a"), ("a", "b"), ("b", "a"), ("b", "z")]));
        assert_eq!(r, c(&[&["a", "b"]]));
    }

    #[test]
    fn overlapping_cycles_form_one_scc() {
        let r = find_cycles(&e(&[
            ("a", "b"),
            ("b", "a"),
            ("b", "c"),
            ("c", "b"),
            ("c", "d"),
            ("d", "a"),
        ]));
        assert_eq!(r, c(&[&["a", "b", "c", "d"]]));
    }

    #[test]
    fn duplicate_edges_tolerated() {
        let r = find_cycles(&e(&[("a", "b"), ("a", "b"), ("b", "a"), ("b", "a")]));
        assert_eq!(r, c(&[&["a", "b"]]));
    }

    #[test]
    fn deterministic_under_reordering() {
        let edges = e(&[
            ("a", "b"),
            ("b", "c"),
            ("c", "a"),
            ("x", "y"),
            ("y", "x"),
            ("m", "m"),
            ("c", "x"),
        ]);
        let expected = find_cycles(&edges);
        let mut rev = edges.clone();
        rev.reverse();
        assert_eq!(find_cycles(&rev), expected);
        let mut rot = edges.clone();
        rot.rotate_left(3);
        assert_eq!(find_cycles(&rot), expected);
        assert_eq!(expected, c(&[&["a", "b", "c"], &["m"], &["x", "y"]]));
    }

    #[test]
    fn long_chain_has_no_cycle_and_does_not_overflow() {
        let n = 100_000;
        let edges: Vec<(String, String)> = (0..n - 1)
            .map(|i| (format!("n{i}"), format!("n{}", i + 1)))
            .collect();
        assert!(find_cycles(&edges).is_empty());
    }

    #[test]
    fn long_ring_is_one_cycle() {
        let n = 100_000;
        let edges: Vec<(String, String)> = (0..n)
            .map(|i| (format!("n{i}"), format!("n{}", (i + 1) % n)))
            .collect();
        let r = find_cycles(&edges);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].len(), n);
    }
}
