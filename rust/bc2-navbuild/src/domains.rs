//! NavKit's routes across domains (`bfbc2-auto-navkit/tools/navdomains.py`, round E4e): its Dijkstra, which the
//! offline cost tables run once per transition point (463 times on `ONS_MP_004`: 7 of the world's 9 s).
//!
//! **The same answers.** `navdomains.dijkstra` pops `(d, node)` tuples from a binary heap: a total order on distinct
//! entries (equal tuples are the same entry), so any correct priority queue pops them in the same sequence, and the
//! relaxations (`d + c < dist[b]`, the neighbours in the adjacency's order) give the same distances bit for bit. A cost
//! that is negative or not a number (where Python's ordering of floats is not a total order) is refused: the caller
//! runs the reference.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};

use rayon::prelude::*;

/// A domain's two-way graph as `navdomains.Domain.adj` holds it: per node its `(neighbour, seconds)` list, in order.
#[derive(Debug, Clone)]
pub struct Graph {
    start: Vec<usize>,
    edges: Vec<(usize, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Entry(f64, usize);

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    /// Reversed (a min-heap): Python's `(d, node)` order, `d` never NaN (refused when the graph is made).
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .partial_cmp(&self.0)
            .unwrap_or(Ordering::Equal)
            .then(other.1.cmp(&self.1))
    }
}

impl Graph {
    /// The graph from per-node adjacency lists (`start`: `n + 1` offsets into `edges`). `None`: offsets that do not
    /// fit, a neighbour out of range, or a cost that is negative or NaN.
    #[must_use]
    pub fn new(start: Vec<usize>, edges: Vec<(usize, f64)>) -> Option<Self> {
        let n = start.len().checked_sub(1)?;
        if start.first() != Some(&0)
            || start.last() != Some(&edges.len())
            || start.windows(2).any(|w| w.first() > w.get(1))
            || edges.iter().any(|&(b, c)| b >= n || c.is_nan() || c < 0.0)
        {
            return None;
        }
        Some(Graph { start, edges })
    }

    /// Nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.start.len() - 1
    }

    /// No nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `navdomains.dijkstra(adj, src, n, targets)`: the distances from `src` (infinity where unreached); with
    /// targets (a non-empty list), the search stops once every target has been popped. `None`: `src` out of range.
    #[must_use]
    pub fn dijkstra(&self, src: usize, targets: Option<&[usize]>) -> Option<Vec<f64>> {
        let n = self.len();
        if src >= n {
            return None;
        }
        let mut dist = vec![f64::INFINITY; n];
        *dist.get_mut(src)? = 0.0;
        let mut left: Option<HashSet<usize>> = targets.filter(|t| !t.is_empty()).map(|t| t.iter().copied().collect());
        let mut heap = BinaryHeap::new();
        heap.push(Entry(0.0, src));
        while let Some(Entry(d, a)) = heap.pop() {
            if d > *dist.get(a)? {
                continue;
            }
            if let Some(l) = left.as_mut() {
                l.remove(&a);
                if l.is_empty() {
                    break;
                }
            }
            let (s, e) = (*self.start.get(a)?, *self.start.get(a + 1)?);
            for &(b, c) in self.edges.get(s..e)? {
                let nd = d + c;
                let slot = dist.get_mut(b)?;
                if nd < *slot {
                    *slot = nd;
                    heap.push(Entry(nd, b));
                }
            }
        }
        Some(dist)
    }

    /// `World.tables`' rows for one domain: for each source in `nodes`, its distances to every node of `nodes` (in
    /// that order), the sources searched in parallel.
    #[must_use]
    pub fn tables(&self, nodes: &[usize]) -> Option<Vec<Vec<f64>>> {
        nodes
            .par_iter()
            .map(|&src| {
                let dist = self.dijkstra(src, None)?;
                nodes
                    .iter()
                    .map(|&m| dist.get(m).copied())
                    .collect::<Option<Vec<f64>>>()
            })
            .collect()
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::expect_used,
    reason = "tests of small known graphs"
)]
mod tests {
    use super::*;

    #[test]
    fn distances_and_targets() {
        // 0 - 1 (1.5), 1 - 2 (2.0), 0 - 2 (4.0); 3 alone
        let g = Graph::new(
            vec![0, 2, 4, 6, 6],
            vec![(1, 1.5), (2, 4.0), (0, 1.5), (2, 2.0), (1, 2.0), (0, 4.0)],
        );
        let g = g.expect("a valid graph");
        assert_eq!(g.dijkstra(0, None), Some(vec![0.0, 1.5, 3.5, f64::INFINITY]));
        assert_eq!(g.dijkstra(0, Some(&[1])).map(|d| d[1]), Some(1.5));
        assert_eq!(g.tables(&[0, 2]), Some(vec![vec![0.0, 3.5], vec![3.5, 0.0]]));
        assert!(g.dijkstra(9, None).is_none());
        assert!(Graph::new(vec![0, 1], vec![(0, f64::NAN)]).is_none());
        assert!(Graph::new(vec![0, 1], vec![(3, 1.0)]).is_none());
    }
}
