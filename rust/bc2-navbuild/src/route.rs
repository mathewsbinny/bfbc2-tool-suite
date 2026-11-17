//! `navbuild.build_graph`'s route edges (round E3c): the walkable pairs of a sector's nodes, and the joining of the
//! node groups they leave apart (the search from one group to another's nearest node, string-pulled into straight
//! walkable legs with waypoint nodes).
//!
//! The reference's arithmetic: `math.dist` ([`pymath::dist3`], [`pymath::dist2`] on cells), float `//`
//! ([`pymath::floordiv`]) for the 15 m buckets. Its union-find (path halving, `parent[find(u)] = find(v)`) is kept
//! as it is: which root a group has decides which groups a pass still joins.

use std::collections::HashSet;

use rayon::prelude::*;

use crate::trace::P3;
use crate::window::WindowSearch;
use crate::{FastMap, FastSet, Level, NavError, Rule, pymath};

/// A route node: its surface, its local cell and its position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Node {
    /// The surface it stands on.
    pub k: usize,
    /// Its local cell (i, j).
    pub cell: (i64, i64),
    /// Its position.
    pub pos: P3,
}

/// The sector-wide values the route edges read.
#[derive(Debug, Clone, Copy)]
pub struct RouteCfg {
    /// The sector's sample origin (global i, j of local 0, 0).
    pub gi0: i64,
    /// See `gi0`.
    pub gj0: i64,
    /// The sector window's local sample range (start, stop).
    pub window: (i64, i64),
    /// `cfg.dist_edge_max`.
    pub dist_edge_max: f64,
    /// `LINK_RULE`.
    pub rule: Rule,
}

/// The 15 m bucket of a position: `(int(x // cell), int(z // cell))`; `None` where Python's `int` would fail.
fn bucket(p: P3, cell: f64) -> Option<(i64, i64)> {
    let as_int = |v: f64| (v.is_finite() && v.abs() < 9.0e18).then_some(v as i64);
    Some((
        as_int(pymath::floordiv(p.0, cell)?)?,
        as_int(pymath::floordiv(p.2, cell)?)?,
    ))
}

/// `build_graph`'s walkable pairs: for each node, in order, the later nodes in the 3 x 3 buckets of `dist_edge_max`
/// around it (buckets in the reference's order, nodes in theirs) within `dist_edge_max` whose straight line is walkable
/// (`line_walkable` over `blocked`): `(node, other node, distance)`. Nodes are tested in parallel; the order is the
/// reference's.
///
/// # Errors
/// [`NavError::Shape`] for a position whose bucket the reference cannot compute (a NaN, infinite or zero cell).
#[allow(clippy::implicit_hasher)] // Level::line_walkable's set
pub fn edge_pairs(
    level: &Level,
    nodes: &[Node],
    cfg: RouteCfg,
    blocked: &HashSet<(usize, usize)>,
) -> Result<Vec<(usize, usize, f64)>, NavError> {
    let cell = cfg.dist_edge_max;
    let keys: Vec<(i64, i64)> = nodes
        .iter()
        .map(|n| bucket(n.pos, cell).ok_or_else(|| NavError::Shape("a node position with no 15 m bucket".into())))
        .collect::<Result<_, _>>()?;
    let mut grid: FastMap<(i64, i64), Vec<usize>> = FastMap::default();
    for (idx, &key) in keys.iter().enumerate() {
        grid.entry(key).or_default().push(idx);
    }
    let per: Vec<Vec<(usize, usize, f64)>> = (0..nodes.len())
        .into_par_iter()
        .map(|idx| {
            let mut out = Vec::new();
            let (Some(p), Some(&(gx, gz))) = (nodes.get(idx), keys.get(idx)) else {
                return out;
            };
            for dx in -1..=1 {
                for dz in -1..=1 {
                    for &q in grid.get(&(gx + dx, gz + dz)).into_iter().flatten() {
                        if q <= idx {
                            continue;
                        }
                        let Some(o) = nodes.get(q) else { continue };
                        let d = pymath::dist3(p.pos, o.pos);
                        if d <= cfg.dist_edge_max
                            && level.line_walkable(p.k, p.cell, o.cell, cfg.gi0, cfg.gj0, o.k, blocked, cfg.rule)
                        {
                            out.push((idx, q, d));
                        }
                    }
                }
            }
            out
        })
        .collect();
    Ok(per.into_iter().flatten().collect())
}

/// Union-find's root of x, halving the path (the reference's `find`).
fn find(parent: &mut [usize], mut x: usize) -> usize {
    while let Some(&p) = parent.get(x) {
        if p == x {
            break;
        }
        let gp = parent.get(p).copied().unwrap_or(p);
        if let Some(v) = parent.get_mut(x) {
            *v = gp;
        }
        x = gp;
    }
    x
}

/// Bit l of a bit set.
fn bit(set: &[u64], l: usize) -> bool {
    set.get(l / 64).is_some_and(|b| b >> (l % 64) & 1 == 1)
}

/// `parent[find(u)] = find(v)`.
fn union(parent: &mut [usize], u: usize, v: usize) {
    let rv = find(parent, v);
    let ru = find(parent, u);
    if let Some(p) = parent.get_mut(ru) {
        *p = rv;
    }
}

/// What [`connect_groups`] adds: the waypoint nodes (in creation order: the caller gives them their ids in this
/// order) and the pairs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Joined {
    /// New nodes: surface, cell, position.
    pub nodes: Vec<Node>,
    /// New pairs `(node, node, distance)`, node indices counting the new nodes after the given ones.
    pub pairs: Vec<(usize, usize, f64)>,
}

/// `build_graph`'s joining of the groups its pairs leave apart: `nodes` (all route nodes so far), `pairs` (the node
/// pairs so far, for the groups), `area_cells` (the areas' cells in order: the surface -> cell map before the nodes'
/// own cells), the sector's origin in metres and pitch, `y_offset`, `blocked` (the D sides `line_walkable` does not
/// cross). Repeats passes over the groups, smallest first, until a pass joins none or one group is left.
///
/// # Errors
/// [`NavError::Cell`] when the search takes a surface with no cell (the reference raises too).
#[expect(clippy::too_many_arguments, reason = "the reference's state")]
#[allow(clippy::too_many_lines, clippy::implicit_hasher)]
pub fn connect_groups(
    level: &Level,
    nodes: &[Node],
    pairs: &[(usize, usize)],
    area_cells: &[(i64, i64, usize)],
    origin: (f64, f64),
    pitch: f64,
    y_offset: f64,
    cfg: RouteCfg,
    blocked: &HashSet<(usize, usize)>,
) -> Result<Joined, NavError> {
    let mut pts: Vec<Node> = nodes.to_vec();
    let mut parent: Vec<usize> = (0..pts.len()).collect();
    for &(a, b) in pairs {
        if a < parent.len() && b < parent.len() {
            union(&mut parent, a, b);
        } else {
            return Err(NavError::Shape("a pair naming no node".into()));
        }
    }
    let mut node_at: FastMap<usize, usize> = FastMap::default();
    for (idx, p) in pts.iter().enumerate() {
        node_at.entry(p.k).or_insert(idx);
    }
    let mut cells: FastMap<usize, (i64, i64)> = FastMap::default();
    cells.reserve(area_cells.len() + pts.len());
    // the window's surfaces numbered once for all the searches (None: too many - the generic search runs)
    let mut dense = WindowSearch::new(level, (cfg.gi0, cfg.gj0), cfg.window, cfg.rule);
    // members of groups whose search found nothing -> the surfaces that search reached (a bit per local surface)
    let mut failed: FastMap<Vec<usize>, Vec<u64>> = FastMap::default();
    for &(i, j, k) in area_cells {
        cells.insert(k, (i, j));
    }
    for p in &pts {
        cells.entry(p.k).or_insert(p.cell);
    }
    let mut out = Joined::default();
    let n0 = pts.len();
    loop {
        // the groups of this pass: root -> members, roots in the order of their first member
        let mut order: Vec<usize> = Vec::new();
        let mut groups: FastMap<usize, Vec<usize>> = FastMap::default();
        for idx in 0..pts.len() {
            let r = find(&mut parent, idx);
            groups
                .entry(r)
                .or_insert_with(|| {
                    order.push(r);
                    Vec::new()
                })
                .push(idx);
        }
        if groups.len() < 2 {
            break;
        }
        // sorted by size, stable
        order.sort_by_key(|r| groups.get(r).map_or(0, Vec::len));
        let mut changed = false;
        for root in order {
            let Some(members) = groups.get(&root) else { continue };
            let Some(&first) = members.first() else { continue };
            if find(&mut parent, first) != root {
                continue;
            }
            // (find halves the paths as the reference's does: unions in a pass make long chains otherwise)
            let mut other: FastSet<usize> = FastSet::default();
            for (&k, &at) in &node_at {
                if find(&mut parent, at) != root {
                    other.insert(k);
                }
            }
            let starts: Vec<usize> = members.iter().filter_map(|&m| pts.get(m).map(|p| p.k)).collect();
            let is_target = |n: usize| other.contains(&n);
            // a group whose search found nothing before, with the same members, finds nothing again (the same walk
            // from the same starts: the same surfaces, the cells it enters already there) unless a node of another
            // group now stands on a surface that walk reached
            if let (Some(w), Some(region)) = (dense.as_ref(), failed.get(members))
                && !other.iter().any(|&k| w.local(k).is_some_and(|l| bit(region, l)))
            {
                continue;
            }
            let mut reached = Vec::new();
            let dense_answer = match dense.as_mut() {
                Some(w) => w.search(&starts, &is_target, &mut cells, Some(&mut reached))?.ok(),
                None => None,
            };
            let walked = dense_answer.is_some(); // the dense search ran (its region is in `reached`)
            let found = if let Some(f) = dense_answer {
                f
            } else {
                level.search(&starts, &is_target, &mut cells, cfg.window, cfg.gi0, cfg.gj0, cfg.rule)?
            };
            let Some(path) = found else {
                if let (true, Some(w)) = (walked, dense.as_ref()) {
                    let mut region = vec![0u64; w.len().div_ceil(64)];
                    for l in reached {
                        if let Some(b) = region.get_mut(l / 64) {
                            *b |= 1 << (l % 64);
                        }
                    }
                    failed.insert(members.clone(), region);
                }
                continue;
            };
            // string pulling
            let cell_of = |k: usize| cells.get(&k).copied().ok_or(NavError::Cell(k as i64));
            let mut way = vec![*path.first().ok_or(NavError::Cell(-1))?];
            let mut a = 0usize;
            while a + 1 < path.len() {
                let mut b = path.len() - 1;
                while b > a + 1 {
                    let (Some(&ka), Some(&kb)) = (path.get(a), path.get(b)) else {
                        break;
                    };
                    let (ca, cb) = (cell_of(ka)?, cell_of(kb)?);
                    let d = pymath::dist2((ca.0 as f64, ca.1 as f64), (cb.0 as f64, cb.1 as f64)) * pitch;
                    if d <= cfg.dist_edge_max
                        && level.line_walkable(ka, ca, cb, cfg.gi0, cfg.gj0, kb, blocked, cfg.rule)
                    {
                        break;
                    }
                    b -= 1;
                }
                if let Some(&kb) = path.get(b) {
                    way.push(kb);
                }
                a = b;
            }
            let mut chain = Vec::with_capacity(way.len());
            for k in way {
                if let Some(&at) = node_at.get(&k) {
                    chain.push(at);
                } else {
                    let (i, j) = cell_of(k)?;
                    let node = Node {
                        k,
                        cell: (i, j),
                        pos: (
                            origin.0 + i as f64 * pitch,
                            level.y(k) + y_offset,
                            origin.1 + j as f64 * pitch,
                        ),
                    };
                    pts.push(node);
                    out.nodes.push(node);
                    parent.push(pts.len() - 1);
                    node_at.insert(k, pts.len() - 1);
                    chain.push(pts.len() - 1);
                }
            }
            for w in chain.windows(2) {
                if let [u, v] = *w {
                    let (Some(pu), Some(pv)) = (pts.get(u), pts.get(v)) else {
                        continue;
                    };
                    out.pairs.push((u, v, pymath::dist3(pu.pos, pv.pos)));
                    union(&mut parent, u, v);
                }
            }
            changed = true;
        }
        if !changed {
            break;
        }
    }
    debug_assert_eq!(pts.len(), n0 + out.nodes.len());
    Ok(out)
}
