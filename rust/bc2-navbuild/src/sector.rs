//! `navbuild.build_graph` whole (round E3h): a sector's route graph - the area-centre and portal nodes, the border
//! nodes, the coverage nodes, the walkable pairs (Gabriel-pruned on the detailed pitch), the ladder links, the
//! groups joined, the order nodes - from the sector's areas, so that a stage's sectors can be built in parallel
//! (E3c ran these pieces one call at a time, the Python's glue between them, sector after sector).
//!
//! Node ids are the caller's: the nodes come back in the reference's order with their source (a border node's
//! index in `borders`, or new), and the caller hands the new ones `new_id()` in that order, as the reference does
//! when it appends them.
//!
//! The reference's arithmetic: one node per surface and per position rounded to 3 decimals (`round(v, 3)`:
//! [`pymath::round3`]), `math.dist` / `math.hypot` ([`pymath::dist3`], [`pymath::hypot`]), Python's `round` to an
//! int (half to even), `min` the first of equals.

use std::collections::HashSet;

use crate::route::{self, Node, RouteCfg};
use crate::trace::P3;
use crate::{Cell, FastMap, FastSet, Level, NavError, graph, pick, pymath};

/// The build's settings a sector graph reads (`cfg`, the module's switches).
#[derive(Debug, Clone, Copy)]
pub struct GraphCfg {
    /// `cfg.pitch`.
    pub pitch: f64,
    /// `cfg.y_offset`.
    pub y_offset: f64,
    /// The sector window's local sample range (start, stop) (`cfg.window`, step 1).
    pub window: (i64, i64),
    /// `cfg.dist_edge_max`.
    pub dist_edge_max: f64,
    /// `LINK_RULE`.
    pub rule: crate::Rule,
    /// `cfg.node_cover` for this pitch (`None`: no coverage nodes).
    pub cover: Option<f64>,
    /// `GABRIEL_EDGES` applies at this pitch.
    pub gabriel: bool,
    /// DICE's edge rule (KYG-EDGE-3): the walkable pairs pruned to a greedy spanner of this stretch (over the
    /// Gabriel rule when both are set).
    pub spanner: Option<f64>,
    /// `cfg.order_nodes`.
    pub order_nodes: bool,
    /// `ORDER_NODE_MIN`.
    pub order_node_min: f64,
    /// The sample grid's minimum (`s.min_x`, `s.min_z`): the order points' columns.
    pub grid_min: (f64, f64),
}

/// One sector's inputs besides its areas.
#[derive(Debug, Clone, Default)]
pub struct GraphJob {
    /// The sector's sample origin (global i, j of local 0, 0).
    pub gi0: i64,
    /// See `gi0`.
    pub gj0: i64,
    /// The sector's origin in metres (ox, oz).
    pub origin: (f64, f64),
    /// Per area, the middles (x, z) of its portals that get a node (`not p.target < ai`), in order.
    pub mids: Vec<Vec<(f64, f64)>>,
    /// The border nodes: (position, surface), in order.
    pub borders: Vec<(P3, usize)>,
    /// The AI order points (x, y, z) on this sector.
    pub orders: Vec<P3>,
    /// The ladder links: (foot surface, top surface).
    pub ladders: Vec<(usize, usize)>,
    /// The sector's D sides (`level.blocked`).
    pub blocked: HashSet<(usize, usize)>,
}

/// A route node of the graph: its source (`Some(b)`: the border node `borders[b]`; `None`: a new node, given the
/// next id), the node, and whether it is an order node (`ORDER_IDS`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphNode {
    /// The border node it is, or `None` for a new node.
    pub border: Option<usize>,
    /// Surface, cell, position.
    pub node: Node,
    /// An order node.
    pub order: bool,
}

/// A sector's route graph: the nodes in the reference's order, the pairs `(node, node, distance)`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Graph {
    /// The nodes.
    pub nodes: Vec<GraphNode>,
    /// The pairs.
    pub pairs: Vec<(usize, usize, f64)>,
}

/// A position's dedup key: its coordinates `round(v, 3)` (-0.0 as 0.0, as Python's tuple equality takes it).
fn key(p: P3) -> Result<[u64; 3], NavError> {
    let r = |v: f64| {
        pymath::round3(v)
            .map(|x| (x + 0.0).to_bits())
            .ok_or_else(|| NavError::Shape("a node position round(v, 3) does not take".into()))
    };
    Ok([r(p.0)?, r(p.1)?, r(p.2)?])
}

/// `int(round(v))` (half to even); `None` where Python's would fail or a cell index cannot be.
fn round_int(v: f64) -> Option<i64> {
    let r = v.round_ties_even();
    #[allow(clippy::cast_possible_truncation)] // checked to be in range
    (r.is_finite() && r.abs() < 9.0e15).then_some(r as i64)
}

/// The nodes so far with `build_graph`'s `add` (one node per surface and rounded position).
#[allow(clippy::struct_field_names)] // the reference's names
struct Nodes {
    nodes: Vec<GraphNode>,
    taken: FastSet<usize>,
    taken_pos: FastSet<[u64; 3]>,
}

impl Nodes {
    /// `add(i, j, k)` at `pos`: the position when a node was added.
    fn add(&mut self, cell: (i64, i64), k: usize, pos: P3) -> Result<Option<P3>, NavError> {
        let key = key(pos)?;
        if self.taken.contains(&k) || self.taken_pos.contains(&key) {
            return Ok(None);
        }
        self.taken.insert(k);
        self.taken_pos.insert(key);
        self.nodes.push(GraphNode {
            border: None,
            node: Node { k, cell, pos },
            order: false,
        });
        Ok(Some(pos))
    }

    fn plain(&self) -> Vec<Node> {
        self.nodes.iter().map(|n| n.node).collect()
    }
}

/// `navbuild.build_graph` (`DEDUP_GRAPH_NODES` on, `DETOUR_REPAIR` off) of one sector: `areas` its areas' cells, in
/// the stage's order.
///
/// # Errors
/// [`NavError`] where the reference would raise (a ladder end with no cell, a surface outside the level) or where
/// a number is one the port does not take (a position `round` cannot take, an area with portals and no cells); the
/// caller then runs the reference.
#[allow(clippy::too_many_lines)] // the reference's steps, in its order
pub fn build_graph(level: &Level, areas: &[Vec<Cell>], job: &GraphJob, cfg: &GraphCfg) -> Result<Graph, NavError> {
    let (ox, oz) = job.origin;
    let pitch = cfg.pitch;
    let at =
        |i: i64, j: i64, k: usize| -> P3 { (ox + i as f64 * pitch, level.y(k) + cfg.y_offset, oz + j as f64 * pitch) };
    let route_cfg = RouteCfg {
        gi0: job.gi0,
        gj0: job.gj0,
        window: cfg.window,
        dist_edge_max: cfg.dist_edge_max,
        rule: cfg.rule,
    };
    for &(_, k) in &job.borders {
        level.surface(i64::try_from(k).unwrap_or(-1))?;
    }
    let mut st = Nodes {
        nodes: Vec::new(),
        taken: job.borders.iter().map(|b| b.1).collect(),
        taken_pos: job.borders.iter().map(|b| key(b.0)).collect::<Result<_, _>>()?,
    };
    // the centre node of every area of 4+ cells
    for cells in areas.iter().filter(|c| c.len() >= 4) {
        let ij: Vec<(i64, i64)> = cells.iter().map(|c| (c.0, c.1)).collect();
        let n = pick::centre_cell(&ij).ok_or_else(|| NavError::Shape("an area's centre cell".into()))?;
        let &(i, j, k) = cells
            .get(n)
            .ok_or_else(|| NavError::Shape("an area's centre cell".into()))?;
        st.add((i, j), k, at(i, j, k))?;
    }
    // surface -> cell, the last area's where two hold it (the reference's dict)
    let mut cell_of: FastMap<usize, (i64, i64)> = FastMap::default();
    cell_of.reserve(areas.iter().map(Vec::len).sum());
    for c in areas.iter().flatten() {
        cell_of.insert(c.2, (c.0, c.1));
    }
    // a node per portal, on the area's cell nearest the portal's middle
    for (ai, mids) in job.mids.iter().enumerate() {
        if mids.is_empty() {
            continue;
        }
        let cells = areas
            .get(ai)
            .ok_or_else(|| NavError::Shape("portal middles of an area that is not there".into()))?;
        let ij: Vec<(i64, i64)> = cells.iter().map(|c| (c.0, c.1)).collect();
        let picks = pick::nearest_cells(&ij, ox, oz, pitch, mids)
            .ok_or_else(|| NavError::Shape("an area with portals and no cells".into()))?;
        for n in picks {
            let &(i, j, k) = cells.get(n).ok_or_else(|| NavError::Shape("a portal cell".into()))?;
            st.add((i, j), k, at(i, j, k))?;
        }
    }
    // the border nodes, on their surface's cell (a carried node's position may be off it)
    for (b, &(pos, k)) in job.borders.iter().enumerate() {
        let cell = match cell_of.get(&k) {
            Some(&c) => c,
            None => (
                round_int((pos.0 - ox) / pitch).ok_or_else(|| NavError::Shape("a border node's cell".into()))?,
                round_int((pos.2 - oz) / pitch).ok_or_else(|| NavError::Shape("a border node's cell".into()))?,
            ),
        };
        st.nodes.push(GraphNode {
            border: Some(b),
            node: Node { k, cell, pos },
            order: false,
        });
    }
    // coverage nodes
    if let Some(cover) = cfg.cover {
        let cover_areas: Vec<graph::CoverArea> = areas
            .iter()
            .map(|cells| cells.iter().map(|&(i, j, k)| at(i, j, k)).collect())
            .collect();
        let mut positions: Vec<P3> = st.nodes.iter().map(|n| n.node.pos).collect();
        let mut failed: Option<NavError> = None;
        let mut add = |ai: usize, w: usize| -> Vec<P3> {
            if failed.is_some() {
                return Vec::new();
            }
            let Some(&(i, j, k)) = areas.get(ai).and_then(|c| c.get(w)) else {
                return Vec::new();
            };
            match st.add((i, j), k, at(i, j, k)) {
                Ok(p) => p.into_iter().collect(),
                Err(e) => {
                    failed = Some(e);
                    Vec::new()
                }
            }
        };
        graph::cover_nodes(&cover_areas, &mut positions, cover, &mut add);
        if let Some(e) = failed {
            return Err(e);
        }
    }
    // the walkable pairs, pruned to the Gabriel graph where the build says so
    let mut pairs = route::edge_pairs(level, &st.plain(), route_cfg, &job.blocked)?;
    if let Some(stretch) = cfg.spanner {
        let keep = graph::spanner_prune(st.nodes.len(), &pairs, stretch)
            .ok_or_else(|| NavError::Shape("spanner_prune".into()))?;
        pairs = keep.into_iter().filter_map(|n| pairs.get(n).copied()).collect();
    } else if cfg.gabriel {
        let pos: Vec<P3> = st.nodes.iter().map(|n| n.node.pos).collect();
        let keep = graph::gabriel_prune(&pos, &pairs).ok_or_else(|| NavError::Shape("gabriel_prune".into()))?;
        pairs = keep.into_iter().filter_map(|n| pairs.get(n).copied()).collect();
    }
    // ladder links: a node at each end (the first on its surface, else a new one), joined directly
    for &(kf, kt) in &job.ladders {
        let mut ends = [0usize; 2];
        for (e, k) in ends.iter_mut().zip([kf, kt]) {
            if let Some(q) = st.nodes.iter().position(|n| n.node.k == k) {
                *e = q;
                continue;
            }
            level.surface(i64::try_from(k).unwrap_or(-1))?;
            let &(i, j) = cell_of
                .get(&k)
                .ok_or_else(|| NavError::Cell(i64::try_from(k).unwrap_or(-1)))?;
            st.nodes.push(GraphNode {
                border: None,
                node: Node {
                    k,
                    cell: (i, j),
                    pos: at(i, j, k),
                },
                order: false,
            });
            *e = st.nodes.len() - 1;
        }
        let (Some(a), Some(b)) = (st.nodes.get(ends[0]), st.nodes.get(ends[1])) else {
            return Err(NavError::Shape("a ladder end".into()));
        };
        pairs.push((ends[0], ends[1], pymath::dist3(a.node.pos, b.node.pos)));
    }
    // the groups the pairs leave apart, joined
    let links: Vec<(usize, usize)> = pairs.iter().map(|p| (p.0, p.1)).collect();
    let all_cells: Vec<Cell> = areas.iter().flatten().copied().collect();
    let joined = route::connect_groups(
        level,
        &st.plain(),
        &links,
        &all_cells,
        (ox, oz),
        pitch,
        cfg.y_offset,
        route_cfg,
        &job.blocked,
    )?;
    st.nodes.extend(joined.nodes.into_iter().map(|node| GraphNode {
        border: None,
        node,
        order: false,
    }));
    pairs.extend(joined.pairs);
    // order nodes
    if cfg.order_nodes && !job.orders.is_empty() {
        let mut node_at: FastMap<usize, usize> = FastMap::default();
        for (idx, n) in st.nodes.iter().enumerate() {
            node_at.entry(n.node.k).or_insert(idx);
        }
        let in_areas: FastSet<usize> = areas.iter().flatten().map(|c| c.2).collect();
        for &(x, y, z) in &job.orders {
            let (Some(gi), Some(gj)) = (
                round_int((x - cfg.grid_min.0) / pitch),
                round_int((z - cfg.grid_min.1) / pitch),
            ) else {
                return Err(NavError::Shape("an order point's column".into()));
            };
            // the kept surface of the column in this sector's areas nearest the point's height (within 1 m)
            let mut best: Option<(usize, f64)> = None;
            for k in level.surfaces(gi, gj) {
                let dy = (level.y(k) - y).abs();
                if in_areas.contains(&k) && dy <= 1.0 && best.is_none_or(|(_, b)| dy < b) {
                    best = Some((k, dy));
                }
            }
            let Some((k, _)) = best else { continue };
            if node_at.contains_key(&k) {
                continue;
            }
            let cell = (gi - job.gi0, gj - job.gj0);
            let pos = at(cell.0, cell.1, k);
            // min(near) <= ORDER_NODE_MIN over the nodes on its floor
            if st.nodes.iter().any(|n| {
                (n.node.pos.1 - pos.1).abs() < 1.5
                    && pymath::hypot(n.node.pos.0 - pos.0, n.node.pos.2 - pos.2) <= cfg.order_node_min
            }) {
                continue;
            }
            let u = st.nodes.len();
            st.nodes.push(GraphNode {
                border: None,
                node: Node { k, cell, pos },
                order: true,
            });
            node_at.insert(k, u);
            for q in 0..u {
                let Some(o) = st.nodes.get(q).map(|n| n.node) else {
                    continue;
                };
                let d = pymath::dist3(pos, o.pos);
                if d <= cfg.dist_edge_max
                    && level.line_walkable(k, cell, o.cell, job.gi0, job.gj0, o.k, &job.blocked, cfg.rule)
                {
                    pairs.push((u, q, d));
                }
            }
        }
    }
    Ok(Graph { nodes: st.nodes, pairs })
}
