//! The harness's route-graph and ground work (`testharness.checks.reach_from` / the pair bits of `reachability`,
//! `testharness.classify.Ground.cells`).

use rayon::prelude::*;
use std::collections::HashMap;

/// The pair bits of `checks.reachability`: per (source, target), source major, `1` when the target's node is reached
/// from the source's along the directed edges, `0` when not, `-` when either point has no node. `start` / `nbr`:
/// the adjacency (`navpath._adjacency`: the first edge of each node, `n + 1` entries, and the destinations). The
/// sources' searches run on all cores.
///
/// # Errors
/// A node index outside the graph (the reference raises `IndexError`).
pub fn pair_bits(
    start: &[i64],
    nbr: &[i64],
    sources: &[Option<i64>],
    targets: &[Option<i64>],
) -> Result<String, String> {
    let n = start.len().saturating_sub(1);
    let check = |k: i64| -> Result<usize, String> {
        usize::try_from(k)
            .ok()
            .filter(|&k| k < n)
            .ok_or_else(|| format!("node {k} outside the graph of {n} nodes"))
    };
    for &k in sources.iter().chain(targets).flatten() {
        check(k)?;
    }
    let mut distinct: Vec<i64> = sources.iter().flatten().copied().collect();
    distinct.sort_unstable();
    distinct.dedup();
    let reached: Vec<Result<Vec<bool>, String>> = distinct.par_iter().map(|&s| reach(start, nbr, s as usize)).collect();
    let mut seen: HashMap<i64, Vec<bool>> = HashMap::with_capacity(distinct.len());
    for (s, r) in distinct.into_iter().zip(reached) {
        seen.insert(s, r?);
    }
    let mut out = String::with_capacity(sources.len() * targets.len());
    for s in sources {
        for t in targets {
            let c = match (s, t) {
                (Some(s), Some(t)) => {
                    let r = seen.get(s).and_then(|r| r.get(*t as usize)).copied().unwrap_or(false);
                    if r { '1' } else { '0' }
                }
                _ => '-',
            };
            out.push(c);
        }
    }
    Ok(out)
}

/// The nodes reached from `node` (itself included) along the edges' directions.
fn reach(start: &[i64], nbr: &[i64], node: usize) -> Result<Vec<bool>, String> {
    let n = start.len().saturating_sub(1);
    let mut seen = vec![false; n];
    let mut queue = std::collections::VecDeque::from([node]);
    if let Some(x) = seen.get_mut(node) {
        *x = true;
    }
    while let Some(k) = queue.pop_front() {
        let (Some(&a), Some(&b)) = (start.get(k), start.get(k + 1)) else {
            continue;
        };
        for e in a..b {
            let m = nbr
                .get(e as usize)
                .copied()
                .ok_or_else(|| format!("edge {e} outside the adjacency"))?;
            let m = usize::try_from(m)
                .ok()
                .filter(|&m| m < n)
                .ok_or_else(|| format!("node {m} outside the graph"))?;
            if let Some(x) = seen.get_mut(m)
                && !*x
            {
                *x = true;
                queue.push_back(m);
            }
        }
    }
    Ok(seen)
}

/// `classify.Ground.cells`: the heights of the walkable cells within `COLUMN_XZ` across of a point, over a bucket
/// index of the cells (the same test, `|cx - x| <= r and |cz - z| <= r`, in float64; in cell order).
#[derive(Debug, Clone)]
pub struct CellIndex {
    xz: Vec<[f64; 2]>,
    y: Vec<f64>,
    size: f64,
    finite: bool,
    buckets: HashMap<(i64, i64), Vec<u32>>,
}

impl CellIndex {
    /// The index of cells (`cell_xz` pairs, `cell_y`), bucketed by `size` metres.
    #[must_use]
    pub fn new(xz: Vec<[f64; 2]>, y: Vec<f64>, size: f64) -> Self {
        let mut buckets: HashMap<(i64, i64), Vec<u32>> = HashMap::new();
        for (k, p) in xz.iter().enumerate() {
            if p[0].is_finite() && p[1].is_finite() {
                buckets
                    .entry(((p[0] / size).floor() as i64, (p[1] / size).floor() as i64))
                    .or_default()
                    .push(k as u32);
            }
        }
        let finite = xz.iter().all(|p| p[0].is_finite() && p[1].is_finite());
        Self {
            xz,
            y,
            size,
            finite,
            buckets,
        }
    }

    /// The heights of the cells with `|cx - x| <= r` and `|cz - z| <= r`, in cell order. `None` where a coordinate is
    /// not finite (the reference compares NaNs; the caller runs it).
    #[must_use]
    pub fn cells(&self, x: f64, z: f64, r: f64) -> Option<Vec<f64>> {
        if !x.is_finite() || !z.is_finite() || !r.is_finite() || !self.finite {
            return None;
        }
        let (i0, i1) = (
            ((x - r) / self.size).floor() as i64 - 1,
            ((x + r) / self.size).floor() as i64 + 1,
        );
        let (j0, j1) = (
            ((z - r) / self.size).floor() as i64 - 1,
            ((z + r) / self.size).floor() as i64 + 1,
        );
        let mut hits: Vec<u32> = Vec::new();
        for i in i0..=i1 {
            for j in j0..=j1 {
                if let Some(b) = self.buckets.get(&(i, j)) {
                    for &k in b {
                        let Some(p) = self.xz.get(k as usize) else { continue };
                        if (p[0] - x).abs() <= r && (p[1] - z).abs() <= r {
                            hits.push(k);
                        }
                    }
                }
            }
        }
        hits.sort_unstable();
        Some(
            hits.into_iter()
                .filter_map(|k| self.y.get(k as usize).copied())
                .collect(),
        )
    }
}
