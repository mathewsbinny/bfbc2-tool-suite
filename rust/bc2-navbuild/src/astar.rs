//! `bfbc2core.navpath.route`'s search (round E3o): A* over a path-data file's directed route edges with their stored
//! costs, the straight-line distance to the goal as the estimate, step for step as the reference runs it.
//!
//! The reference keeps a heap of `(estimate, cost so far, node)` tuples (Python compares them in that order, so the
//! pops come in that total order whatever the heap's layout), skips a node popped again, stops at the goal, and
//! relaxes the edges of a node in the adjacency's order (`ng < best[m]`). The estimate is `np.linalg.norm(pos[m] -
//! pos[goal])`, whose last bits depend on the BLAS NumPy calls (`dot` of three values): [`Norm`] names the two
//! formulas known, and the Python side picks the one that gives NumPy's own bits on the machine (or runs the
//! reference when neither does).

use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// How `np.linalg.norm` of a three-value vector is computed by the BLAS in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Norm {
    /// `sqrt(fma(z, z, fma(y, y, x * x)))` (`OpenBLAS`' `ddot` with fused multiply-adds: its `SkylakeX` kernel, the
    /// development PC's). The fma is `libm`'s, correctly rounded in Rust (round CI1): `f64::mul_add` without the
    /// `fma` target feature calls the C runtime's `fma`, so the estimate's bits would be the installed Windows'.
    Fma,
    /// `sqrt((x * x + y * y) + z * z)`.
    Plain,
}

impl Norm {
    /// The norm of `(x, y, z)`.
    #[must_use]
    pub fn of(self, x: f64, y: f64, z: f64) -> f64 {
        match self {
            Self::Fma => libm::fma(z, z, libm::fma(y, y, x * x)).sqrt(),
            Self::Plain => (x * x + y * y + z * z).sqrt(),
        }
    }
}

/// A file's route graph in the adjacency the reference builds (`navpath._adjacency`): the edges sorted by source
/// (stable), `first[n]..first[n + 1]` the edges of node `n`.
#[derive(Debug, Clone)]
pub struct Graph {
    /// Per node its first edge; `nodes + 1` values.
    pub first: Vec<usize>,
    /// Each edge's destination.
    pub to: Vec<usize>,
    /// Each edge's stored cost.
    pub cost: Vec<f64>,
    /// Each node's position.
    pub pos: Vec<[f64; 3]>,
}

/// A search's outcome: the nodes expanded (popped first time), and the route (start to goal) with its cost when
/// the goal was reached.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// `len(done)`.
    pub expanded: usize,
    /// The node chain, start to goal; empty when the goal was not reached.
    pub nodes: Vec<usize>,
    /// The sum of the stored costs along it.
    pub cost: f64,
}

#[derive(Debug, Clone, Copy)]
struct Entry(f64, f64, usize);

impl PartialEq for Entry {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Entry {
    /// Reversed (a min-heap), as Python compares the tuples; the values are never NaN (checked by `Graph::valid`).
    fn cmp(&self, o: &Self) -> Ordering {
        let f = |a: f64, b: f64| b.partial_cmp(&a).unwrap_or(Ordering::Equal);
        f(self.0, o.0).then(f(self.1, o.1)).then(o.2.cmp(&self.2))
    }
}

impl Graph {
    /// Whether the graph is one the search answers as the reference does: consistent sizes, edges in range, every
    /// position and cost finite (no NaN in the comparisons).
    #[must_use]
    pub fn valid(&self) -> bool {
        let n = self.pos.len();
        self.first.len() == n + 1
            && self.to.len() == self.cost.len()
            && self.first.windows(2).all(|w| matches!(w, [a, b] if a <= b))
            && self.first.last().is_some_and(|&e| e == self.to.len())
            && self.to.iter().all(|&t| t < n)
            && self.cost.iter().all(|c| c.is_finite())
            && self.pos.iter().flatten().all(|v| v.is_finite())
    }

    fn h(&self, norm: Norm, m: usize, goal: [f64; 3]) -> f64 {
        let p = self.pos.get(m).copied().unwrap_or([f64::NAN; 3]);
        norm.of(p[0] - goal[0], p[1] - goal[1], p[2] - goal[2])
    }

    #[allow(clippy::many_single_char_names)]
    /// `navpath.route`'s A* from `start` to `goal` (both below the node count; the graph [`valid`](Self::valid)).
    #[must_use]
    pub fn search(&self, norm: Norm, start: usize, goal: usize) -> Found {
        let n = self.pos.len();
        let g_pos = self.pos.get(goal).copied().unwrap_or([f64::NAN; 3]);
        let mut best = vec![f64::INFINITY; n];
        let mut came = vec![usize::MAX; n];
        let mut done = vec![false; n];
        let mut expanded = 0usize;
        if let Some(b) = best.get_mut(start) {
            *b = 0.0;
        }
        let mut heap = BinaryHeap::new();
        heap.push(Entry(self.h(norm, start, g_pos), 0.0, start));
        let mut reached = false;
        while let Some(Entry(_, g, v)) = heap.pop() {
            let Some(d) = done.get_mut(v) else { break };
            if *d {
                continue;
            }
            *d = true;
            expanded += 1;
            if v == goal {
                reached = true;
                break;
            }
            let (a, b) = (
                self.first.get(v).copied().unwrap_or(0),
                self.first.get(v + 1).copied().unwrap_or(0),
            );
            for k in a..b {
                let (Some(&m), Some(&c)) = (self.to.get(k), self.cost.get(k)) else {
                    continue;
                };
                let ng = g + c;
                if let Some(bm) = best.get_mut(m)
                    && ng < *bm
                {
                    *bm = ng;
                    if let Some(cm) = came.get_mut(m) {
                        *cm = v;
                    }
                    heap.push(Entry(ng + self.h(norm, m, g_pos), ng, m));
                }
            }
        }
        if !reached {
            return Found {
                expanded,
                nodes: Vec::new(),
                cost: 0.0,
            };
        }
        let mut chain = vec![goal];
        while let Some(&last) = chain.last() {
            if last == start {
                break;
            }
            match came.get(last) {
                Some(&p) if p != usize::MAX && chain.len() <= n => chain.push(p),
                _ => break,
            }
        }
        chain.reverse();
        Found {
            expanded,
            nodes: chain,
            cost: best.get(goal).copied().unwrap_or(0.0),
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// A square of four nodes, two ways round from 0 to 2 (one cheaper), and a one-way edge 3 -> 0.
    fn graph() -> Graph {
        // edges by source: 0->1 (1), 0->3 (5), 1->2 (1), 3->2 (1), 3->0 (1)
        Graph {
            first: vec![0, 2, 3, 3, 5],
            to: vec![1, 3, 2, 2, 0],
            cost: vec![1.0, 5.0, 1.0, 1.0, 1.0],
            pos: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
        }
    }

    #[test]
    fn finds_the_cheaper_way() {
        let g = graph();
        assert!(g.valid());
        let f = g.search(Norm::Fma, 0, 2);
        assert_eq!(f.nodes, vec![0, 1, 2]);
        assert_eq!(f.cost, 2.0);
        // the goal unreachable: every node reachable from the start is expanded
        let f = g.search(Norm::Plain, 2, 0);
        assert_eq!((f.expanded, f.nodes.len()), (1, 0));
        assert_eq!(g.search(Norm::Fma, 0, 0).nodes, vec![0]);
    }

    #[test]
    fn norms() {
        assert_eq!(Norm::Fma.of(3.0, 4.0, 0.0), 5.0);
        assert_eq!(Norm::Plain.of(0.0, 3.0, 4.0), 5.0);
        let mut g = graph();
        g.cost[0] = f64::NAN;
        assert!(!g.valid());
    }
}
