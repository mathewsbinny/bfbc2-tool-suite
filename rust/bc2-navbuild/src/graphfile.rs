//! A NavKit build's last stage, "graphs to file" (round NK2): the sectors' route graphs and runtime meshes made into
//! the path-data file. The Python (`navislands.prune` / `components` / `shadowing`, `navbuild.prune_order_nodes`,
//! `finalize_graph`, `write_file`'s assembly and `sector_links`) was retired in round NK2 against the build's own
//! answers: every regression level's file byte for byte, the golden builds of `tests/navbuild_golden.json`
//! (`rust/BENCHMARKS.md` "NK2").
//!
//! - [`shadow_islands`]: `navislands.shadowing` - the small route islands (mostly on objects, no anchor) next to a
//!   large one, whose edges [`drop_edges`] removes (`SHADOW_ISLANDS`).
//! - [`prune_order_nodes`]: the order nodes in a route island under `ORDER_MIN_ISLAND` nodes lose their edges.
//! - [`finalize`]: a sector's nodes and edges as the file holds them (`finalize_graph`).
//! - [`write_file`]: all of it, then `hpdtool.write`.
//! - [`sector_links`]: a sector's neighbour links (the runtime mesh's link table) from the areas across its border
//!   portals.
//!
//! **The same bytes.** Nothing here depends on an iteration order the Python left to its sets or dicts: the
//! islands and order nodes dropped are sets (which island a node is in does not depend on the union order), the
//! records of dropped order nodes follow the graphs' order, and every sort is the reference's stable sort on the same
//! keys (Python floats, compared as `f64`). Costs are `0x80000000 | floor(dist * 16)` with CPython's `math.dist`
//! ([`crate::pymath::dist3`]) on the positions rounded to `float32`, as the file stores them; `math.hypot` is
//! [`crate::pymath::hypot`]; `round(v, 2)` (the dropped order nodes' report) is [`crate::pymath::round_to`].
//!
//! **Refused** ([`GraphError::Refused`]): a position or bound that is not finite or does not fit a `float32`, a
//! cost or a node's sorted-index field that does not fit the file's 32 bits - values no build makes (the reference
//! raised `struct.error` writing them, or sorted a NaN where it fell). A pair naming a node past its sector's nodes
//! is the reference's `IndexError` ([`GraphError::Index`]).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::BuildHasher;

use crate::pymath::round_to;
use bc2_formats::hpd::{self, Edge, HpdFile, Node, Sector};
use bc2_formats::kymesh::{LinkObject, NeighbourLink};

use crate::pymath::{dist3, hypot};

/// Why the stage stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    /// A pair names a node past its sector's nodes (the reference's `IndexError`).
    #[error("list index out of range")]
    Index,
    /// A value the file cannot hold (what it is).
    #[error("refused: {0}")]
    Refused(String),
}

fn refused(what: impl Into<String>) -> GraphError {
    GraphError::Refused(what.into())
}

/// A route node: its id (border nodes have bit 31 set) and position (the build's floats).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteNode {
    /// The node's id.
    pub id: u32,
    /// x, y, z.
    pub pos: [f64; 3],
}

/// One sector's route graph as the build hands it over (`(sid, bounds, mesh, pts, pairs, origin)`; the mesh is
/// [`write_file`]'s).
#[derive(Debug, Clone, PartialEq)]
pub struct SectorGraph {
    /// The sector id (0xXXYY).
    pub sid: u32,
    /// minX, maxX, minY, maxY, minZ, maxZ.
    pub bounds: [f64; 6],
    /// The nodes (`pts`: id and position).
    pub nodes: Vec<RouteNode>,
    /// The route edges as node indices into `nodes` (`pairs`; their distances are not read).
    pub pairs: Vec<(usize, usize)>,
    /// The sector's 100 m square's minimum corner (x, z), or `None` (every border copy then foreign).
    pub origin: Option<(f64, f64)>,
}

impl SectorGraph {
    fn check(&self) -> Result<(), GraphError> {
        let n = self.nodes.len();
        if self.pairs.iter().any(|&(a, b)| a >= n || b >= n) {
            return Err(GraphError::Index);
        }
        Ok(())
    }
}

/// The shadow-island rule's settings (`navislands`: `SHADOW_MIN_NODES`, `SHADOW_XZ`, `SHADOW_DY`, `ANCHOR_XZ`,
/// `ANCHOR_DY`, `Y_OFFSET`, `SHADOW_OBJECTS_ONLY`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    /// Islands under this many nodes are candidates.
    pub min_nodes: usize,
    /// ... when a node of a large island is this close across (m).
    pub xz: f64,
    /// ... and this close in height.
    pub dy: f64,
    /// An anchor keeps the island of a node this close across ...
    pub anchor_xz: f64,
    /// ... whose ground (node height - `y_offset`) is this close to the anchor's height.
    pub anchor_dy: f64,
    /// Route nodes stand this far above their ground.
    pub y_offset: f64,
    /// Only islands whose nodes stand mostly on objects.
    pub objects_only: bool,
}

impl Default for Shadow {
    fn default() -> Self {
        Shadow {
            min_nodes: 25,
            xz: 8.0,
            dy: 4.0,
            anchor_xz: 2.0,
            anchor_dy: 1.0,
            y_offset: 0.95,
            objects_only: true,
        }
    }
}

/// What [`shadow_islands`] found (`navislands.STATS`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShadowStats {
    /// The ids of the dropped islands' nodes.
    pub ids: HashSet<u32>,
    /// Islands dropped.
    pub islands: usize,
    /// Route islands before.
    pub before: usize,
}

/// Union-find over node ids (`find` with path halving, as the reference's).
#[derive(Default)]
struct Islands {
    parent: HashMap<u32, u32>,
}

impl Islands {
    fn find(&mut self, x: u32) -> u32 {
        let mut x = x;
        self.parent.entry(x).or_insert(x);
        loop {
            let p = self.parent.get(&x).copied().unwrap_or(x);
            if p == x {
                return x;
            }
            let gp = self.parent.get(&p).copied().unwrap_or(p);
            self.parent.insert(x, gp);
            x = gp;
        }
    }

    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }

    /// Every sector's nodes entered, their pairs joined.
    fn of(graphs: &[SectorGraph]) -> Islands {
        let mut u = Islands::default();
        for g in graphs {
            for p in &g.nodes {
                u.find(p.id);
            }
            for &(a, b) in &g.pairs {
                if let (Some(pa), Some(pb)) = (g.nodes.get(a), g.nodes.get(b)) {
                    u.union(pa.id, pb.id);
                }
            }
        }
        u
    }
}

/// `navislands.components`: each linked node's island root, each root's linked-node count, each node's position
/// (the first sector's that holds it).
/// Each linked node's island root, each root's linked-node count, each node's position.
type Components = (HashMap<u32, u32>, HashMap<u32, usize>, HashMap<u32, [f64; 3]>);

fn components(graphs: &[SectorGraph]) -> Components {
    let mut u = Islands::of(graphs);
    let mut pos: HashMap<u32, [f64; 3]> = HashMap::new();
    for g in graphs {
        for p in &g.nodes {
            pos.entry(p.id).or_insert(p.pos);
        }
    }
    let linked = linked_ids(graphs);
    let mut root = HashMap::new();
    let mut size: HashMap<u32, usize> = HashMap::new();
    #[allow(
        clippy::iter_over_hash_type,
        reason = "each linked node counted under its root: counts"
    )]
    for &i in pos.keys() {
        if linked.contains(&i) {
            let r = u.find(i);
            root.insert(i, r);
            *size.entry(r).or_default() += 1;
        }
    }
    (root, size, pos)
}

/// The ids of the nodes some pair of some sector names (`finalize_graph`'s `linked_ids`).
#[must_use]
pub fn linked_ids(graphs: &[SectorGraph]) -> HashSet<u32> {
    let mut out = HashSet::new();
    for g in graphs {
        for &(a, b) in &g.pairs {
            for i in [a, b] {
                if let Some(p) = g.nodes.get(i) {
                    out.insert(p.id);
                }
            }
        }
    }
    out
}

/// `math.floor(v / w)` as a grid key (the values are finite: [`write_file`] checks every position).
#[expect(clippy::cast_possible_truncation, reason = "a bucket index of a finite position")]
fn bucket(v: f64, w: f64) -> i64 {
    (v / w).floor() as i64
}

/// `navislands.shadowing(graphs, anchors)`: the nodes of the route islands the rule drops - islands under
/// `min_nodes` linked nodes (with `objects_only`, those with fewer than half their nodes in `terrain`) that hold no
/// anchor (a point within `anchor_xz` across and `anchor_dy` of a node's ground) and come within `xz` across and
/// `dy` in height of a node of an island of `min_nodes` or more.
#[must_use]
pub fn shadow_islands<S: BuildHasher>(
    graphs: &[SectorGraph],
    anchors: &[[f64; 3]],
    terrain: &HashSet<u32, S>,
    s: &Shadow,
) -> ShadowStats {
    let (root, size, pos) = components(graphs);
    let mut small: HashSet<u32> = size
        .iter()
        .filter(|&(_, &n)| n < s.min_nodes)
        .map(|(&r, _)| r)
        .collect();
    if s.objects_only && !small.is_empty() {
        let mut on_terrain: HashMap<u32, usize> = HashMap::new();
        #[allow(clippy::iter_over_hash_type, reason = "counts per island")]
        for (i, r) in &root {
            if small.contains(r) && terrain.contains(i) {
                *on_terrain.entry(*r).or_default() += 1;
            }
        }
        small.retain(|r| 2 * on_terrain.get(r).copied().unwrap_or(0) < size.get(r).copied().unwrap_or(0));
    }
    let before = size.len();
    if small.is_empty() || size.len() < 2 {
        return ShadowStats {
            before,
            ..ShadowStats::default()
        };
    }
    let mut grid: HashMap<(i64, i64), Vec<[f64; 3]>> = HashMap::new();
    #[allow(
        clippy::iter_over_hash_type,
        reason = "positions into buckets, only tested with any()"
    )]
    for (i, r) in &root {
        if size.get(r).copied().unwrap_or(0) >= s.min_nodes
            && let Some(p) = pos.get(i)
        {
            grid.entry((bucket(p[0], s.xz), bucket(p[2], s.xz)))
                .or_default()
                .push(*p);
        }
    }
    let mut abucket: HashMap<(i64, i64), Vec<[f64; 3]>> = HashMap::new();
    for a in anchors {
        abucket
            .entry((bucket(a[0], s.anchor_xz), bucket(a[2], s.anchor_xz)))
            .or_default()
            .push(*a);
    }
    let (mut anchored, mut near) = (HashSet::new(), HashSet::new());
    #[allow(
        clippy::iter_over_hash_type,
        reason = "membership tests and inserts into sets; each node's test reads only its own position"
    )]
    for (i, r) in &root {
        if !small.contains(r) {
            continue;
        }
        let Some(p) = pos.get(i) else { continue };
        if !anchored.contains(r) {
            let (bx, bz) = (bucket(p[0], s.anchor_xz), bucket(p[2], s.anchor_xz));
            let hit = (-1..=1).any(|dx| {
                (-1..=1).any(|dz| {
                    abucket.get(&(bx + dx, bz + dz)).is_some_and(|v| {
                        v.iter().any(|a| {
                            hypot(a[0] - p[0], a[2] - p[2]) <= s.anchor_xz
                                && (p[1] - s.y_offset - a[1]).abs() <= s.anchor_dy
                        })
                    })
                })
            });
            if hit {
                anchored.insert(*r);
            }
        }
        if near.contains(r) {
            continue;
        }
        let (bx, bz) = (bucket(p[0], s.xz), bucket(p[2], s.xz));
        let hit = (-1..=1).any(|dx| {
            (-1..=1).any(|dz| {
                grid.get(&(bx + dx, bz + dz)).is_some_and(|v| {
                    v.iter()
                        .any(|q| (q[1] - p[1]).abs() <= s.dy && hypot(q[0] - p[0], q[2] - p[2]) <= s.xz)
                })
            })
        });
        if hit {
            near.insert(*r);
        }
    }
    let drop: HashSet<u32> = near.difference(&anchored).copied().collect();
    let ids = root.iter().filter(|(_, r)| drop.contains(r)).map(|(&i, _)| i).collect();
    ShadowStats {
        ids,
        islands: drop.len(),
        before,
    }
}

/// Every pair with an end in `ids` removed (the nodes stay; [`finalize`] drops those left without an edge).
pub fn drop_edges<S: BuildHasher>(graphs: &mut [SectorGraph], ids: &HashSet<u32, S>) {
    if ids.is_empty() {
        return;
    }
    for g in graphs {
        let nodes = &g.nodes;
        g.pairs.retain(|&(a, b)| {
            nodes.get(a).is_none_or(|p| !ids.contains(&p.id)) && nodes.get(b).is_none_or(|p| !ids.contains(&p.id))
        });
    }
}

/// An order node [`prune_order_nodes`] dropped: its id, its position rounded to 2 places and its island's size
/// (`ORDER_STATS['removed']`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RemovedOrder {
    /// The node's id.
    pub id: u32,
    /// `round(v, 2)` of each coordinate.
    pub pos: [f64; 3],
    /// Its route island's node count (every node, linked or not).
    pub size: usize,
}

/// `navbuild.prune_order_nodes`: the order nodes (`order_ids`) in a route island of fewer than `min_island` nodes
/// (border ids join sectors; every node counts) lose their edges. The dropped ones in the graphs' order, each once.
pub fn prune_order_nodes<S: BuildHasher>(
    graphs: &mut [SectorGraph],
    order_ids: &HashSet<u32, S>,
    min_island: usize,
) -> Vec<RemovedOrder> {
    let mut u = Islands::of(graphs);
    if u.parent.is_empty() {
        return Vec::new();
    }
    let keys: Vec<u32> = u.parent.keys().copied().collect();
    let mut size: HashMap<u32, usize> = HashMap::new();
    for x in keys {
        *size.entry(u.find(x)).or_default() += 1;
    }
    let mut drop = HashSet::new();
    #[allow(clippy::iter_over_hash_type, reason = "counts per root")]
    for &i in order_ids {
        let r = u.find(i);
        if size.get(&r).copied().unwrap_or(0) < min_island {
            drop.insert(i);
        }
    }
    let mut seen = HashSet::new();
    let mut removed = Vec::new();
    for g in graphs.iter() {
        for p in &g.nodes {
            if drop.contains(&p.id) && seen.insert(p.id) {
                let r = u.find(p.id);
                removed.push(RemovedOrder {
                    id: p.id,
                    pos: p.pos.map(|v| round_to(v, 2)),
                    size: size.get(&r).copied().unwrap_or(0),
                });
            }
        }
    }
    drop_edges(graphs, &drop);
    removed
}

/// How [`finalize`] lays a sector's nodes out (`SORT_NODES_BY_ID`, `BORDER_OWNER`, the build's `dist_edge_max`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// A sector's nodes sorted by unsigned id (ordinary nodes, then border nodes).
    pub sort_by_id: bool,
    /// Bit 16 of field c only on the border copies the sector does not own.
    pub border_owner: bool,
    /// The first node's value.
    pub dist_edge_max: f64,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            sort_by_id: true,
            border_owner: true,
            dist_edge_max: 15.0,
        }
    }
}

/// A finite value as the file's `float32` (`struct.pack('<f')`: round to nearest; one past `float32`'s range is
/// refused, as the reference's `OverflowError`).
#[expect(
    clippy::cast_possible_truncation,
    reason = "float32 is the file's type: rounded to nearest, as struct packs"
)]
fn f32_of(v: f64, what: &str) -> Result<f32, GraphError> {
    let f = v as f32;
    if f.is_finite() && v.is_finite() {
        Ok(f)
    } else {
        Err(refused(format!("{what} {v} is not a finite float32")))
    }
}

fn u32_of(v: u64, what: &str) -> Result<u32, GraphError> {
    u32::try_from(v).map_err(|_| refused(format!("{what} {v} does not fit 32 bits")))
}

/// `navbuild.finalize_graph(pts, pairs, linked_ids, origin, cfg)`: the sector's nodes with an edge in some sector
/// (`linked`), with their edges, as the file holds them.
///
/// # Errors
/// [`GraphError::Index`] for a pair past the nodes; [`GraphError::Refused`] for a value the file cannot hold.
pub fn finalize<S: BuildHasher>(
    g: &SectorGraph,
    linked: &HashSet<u32, S>,
    layout: &Layout,
) -> Result<(Vec<Node>, Vec<Edge>), GraphError> {
    g.check()?;
    let pts = &g.nodes;
    let mut keep: Vec<usize> = (0..pts.len())
        .filter(|&i| pts.get(i).is_some_and(|p| linked.contains(&p.id)))
        .collect();
    if layout.sort_by_id {
        keep.sort_by_key(|&i| pts.get(i).map_or(0, |p| p.id));
    }
    let mut new = vec![usize::MAX; pts.len()];
    for (q, &old) in keep.iter().enumerate() {
        if let Some(slot) = new.get_mut(old) {
            *slot = q;
        }
    }
    let at32 = |i: usize| -> Result<(f64, f64, f64), GraphError> {
        let p = pts.get(i).ok_or(GraphError::Index)?.pos;
        let c = |v: f64| f32_of(v, "a node position").map(f64::from);
        Ok((c(p[0])?, c(p[1])?, c(p[2])?))
    };
    let mut out_edges: Vec<Vec<(u32, usize)>> = vec![Vec::new(); keep.len()];
    for &(a, b) in &g.pairs {
        let (na, nb) = (
            new.get(a).copied().unwrap_or(usize::MAX),
            new.get(b).copied().unwrap_or(usize::MAX),
        );
        if na == usize::MAX || nb == usize::MAX {
            continue;
        }
        let d = dist3(at32(a)?, at32(b)?);
        let f = (d * 16.0).floor();
        if !(0.0..4_294_967_296.0).contains(&f) {
            return Err(refused(format!("an edge cost of {d} m")));
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked: 0 <= f < 2**32"
        )]
        let cost = u32_of(0x8000_0000 | f as u64, "an edge cost")?;
        if let Some(v) = out_edges.get_mut(na) {
            v.push((cost, nb));
        }
        if let Some(v) = out_edges.get_mut(nb) {
            v.push((cost, na));
        }
    }
    let mut nodes = Vec::with_capacity(keep.len());
    for (q, &idx) in keep.iter().enumerate() {
        let p = pts.get(idx).ok_or(GraphError::Index)?;
        let value = if q == 0 { layout.dist_edge_max } else { 0.0 };
        nodes.push(Node {
            id: p.id,
            pos: [
                f32_of(p.pos[0], "a node position")?,
                f32_of(p.pos[1], "a node position")?,
                f32_of(p.pos[2], "a node position")?,
            ],
            flags: 0x8000_0000,
            value: f32_of(value, "dist_edge_max")?,
            a: 0,
            b: 0,
            c: 0,
        });
    }
    let mut edges = Vec::new();
    for (idx, out) in out_edges.iter_mut().enumerate() {
        let first = edges.len();
        out.sort_by_key(|e| e.1);
        for &(cost, dst) in out.iter() {
            edges.push(Edge {
                tag: cost,
                src: u32_of(idx as u64, "a node index")?,
                dst: u32_of(dst as u64, "a node index")?,
            });
        }
        if let Some(nd) = nodes.get_mut(idx) {
            nd.a = if out.is_empty() {
                0
            } else {
                u32_of(first as u64 + 1, "an edge index")?
            };
        }
    }
    let full: Vec<[f64; 3]> = keep.iter().filter_map(|&i| pts.get(i).map(|p| p.pos)).collect();
    sorted_fields(&mut nodes, &full, g.origin, layout)?;
    Ok((nodes, edges))
}

/// `finalize_graph`'s fields b and c: the sector's node indices sorted by x (b's low half), y (its high half) and z
/// (c's low half) of the build's positions (`full`, a stable sort of Python floats), and bit 16 of c on a border
/// node's copy in a sector that does not own it (the sector whose 100 m square holds the node owns it).
fn sorted_fields(
    nodes: &mut [Node],
    full: &[[f64; 3]],
    origin: Option<(f64, f64)>,
    layout: &Layout,
) -> Result<(), GraphError> {
    let by = |axis: usize| {
        let mut v: Vec<usize> = (0..full.len()).collect();
        let key = |q: usize| full.get(q).and_then(|p| p.get(axis)).copied().unwrap_or(0.0);
        v.sort_by(|&q, &r| key(q).partial_cmp(&key(r)).unwrap_or(std::cmp::Ordering::Equal));
        v
    };
    let (by_x, by_y, by_z) = (by(0), by(1), by(2));
    for (q, nd) in nodes.iter_mut().enumerate() {
        let rank = |v: &[usize]| v.get(q).copied().unwrap_or(0) as u64;
        let foreign = nd.id & 0x8000_0000 != 0
            && match (layout.border_owner, origin) {
                (true, Some((ox, oz))) => {
                    let p = full.get(q).copied().unwrap_or([0.0; 3]);
                    !(ox <= p[0] && p[0] < ox + 100.0 && oz <= p[2] && p[2] < oz + 100.0)
                }
                _ => true,
            };
        nd.b = u32_of(rank(&by_x) | (rank(&by_y) << 16), "a node's sorted-index field b")?;
        nd.c = u32_of(
            rank(&by_z) | if foreign { 0x10000 } else { 0 },
            "a node's sorted-index field c",
        )?;
    }
    Ok(())
}

/// What [`write_file`] does besides the layout: the shadow-island rule (`None`: off, as `navislands.prune` is not
/// run), the order nodes' pruning (`None`: off, as with no order nodes or `ORDER_MIN_ISLAND` 0), the file header and
/// each sector's build date and time.
#[derive(Debug, Clone, PartialEq)]
pub struct FileSettings {
    /// The shadow-island rule, its anchors and the nodes on terrain.
    pub shadow: Option<(Shadow, Vec<[f64; 3]>, HashSet<u32>)>,
    /// The order nodes and `ORDER_MIN_ISLAND`.
    pub orders: Option<(HashSet<u32>, usize)>,
    /// The node layout.
    pub layout: Layout,
    /// The file header's words (word 2, the sector count, is the writer's).
    pub header: [u32; hpd::HEADER_WORDS],
    /// Each sector's date (`yymmdd`, the month counted from 0).
    pub date: u32,
    /// Each sector's time (`hhmmss00`).
    pub clock: u32,
}

/// What [`write_file`] made and found.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    /// The file's bytes.
    pub data: Vec<u8>,
    /// The shadow-island rule's answer (`None` when off).
    pub shadow: Option<ShadowStats>,
    /// The order nodes dropped.
    pub removed: Vec<RemovedOrder>,
    /// Nodes and edges written.
    pub nodes: usize,
    /// See `nodes`.
    pub edges: usize,
}

/// The build's `navislands.prune` (when `shadow` is set) and `write_file`: the graphs pruned, each sector finalized,
/// its mesh record (`meshes`, one per graph, already written) beside it, the file's bytes.
///
/// # Errors
/// [`GraphError::Index`] for a pair past its sector's nodes; [`GraphError::Refused`] for a value the file cannot
/// hold, or meshes that are not one per graph.
pub fn write_file(graphs: &[SectorGraph], meshes: &[Vec<u8>], s: &FileSettings) -> Result<Written, GraphError> {
    if meshes.len() != graphs.len() {
        return Err(refused("not one mesh record per sector"));
    }
    for g in graphs {
        g.check()?;
        if g.nodes.iter().any(|p| p.pos.iter().any(|v| !v.is_finite())) {
            return Err(refused(format!(
                "a node position of sector {:#06x} that is not finite",
                g.sid
            )));
        }
    }
    let mut graphs = graphs.to_vec();
    let shadow = s.shadow.as_ref().map(|(sh, anchors, terrain)| {
        let st = shadow_islands(&graphs, anchors, terrain, sh);
        drop_edges(&mut graphs, &st.ids);
        st
    });
    let removed = match &s.orders {
        Some((ids, min)) if !ids.is_empty() && *min > 0 => prune_order_nodes(&mut graphs, ids, *min),
        _ => Vec::new(),
    };
    let linked = linked_ids(&graphs);
    let mut sectors = Vec::with_capacity(graphs.len());
    for (g, mesh) in graphs.iter().zip(meshes) {
        let (nodes, edges) = finalize(g, &linked, &s.layout)?;
        let mut bounds = [0f32; 6];
        for (b, &v) in bounds.iter_mut().zip(&g.bounds) {
            *b = f32_of(v, "a sector bound")?;
        }
        sectors.push(Sector {
            id: g.sid,
            bounds,
            reserved: 0,
            date: s.date,
            time: s.clock,
            nodes,
            edges,
            mesh,
        });
    }
    let (n_nodes, n_edges) = sectors
        .iter()
        .fold((0, 0), |(n, e), x| (n + x.nodes.len(), e + x.edges.len()));
    let data = hpd::write(&HpdFile {
        header: s.header,
        sectors,
    })
    .map_err(|e| refused(e.to_string()))?;
    Ok(Written {
        data,
        shadow,
        removed,
        nodes: n_nodes,
        edges: n_edges,
    })
}

/// A border portal as [`sector_links`] reads it: its area, its index among the area's border portals, the
/// direction out of the sector (`int()` of the portal's x and z direction) and the cell run it covers (`t0`, `t1`,
/// `line` of the trace).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    /// The area's index in the sector.
    pub area: usize,
    /// The portal's index among the area's border portals.
    pub index: usize,
    /// x step out of the sector (-1, 0, 1).
    pub di: i64,
    /// z step out of the sector.
    pub dj: i64,
    /// First cell index along the border.
    pub t0: i64,
    /// One past the last.
    pub t1: i64,
    /// The border line (a half-integer grid line).
    pub line: f64,
}

impl Border {
    /// `navbuild.border_cell`'s cell just inside the portal (the middle of its run): (i, j) local to the sector.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "int() of a grid line: truncated toward zero"
    )]
    pub fn inner_cell(&self) -> (i64, i64) {
        let tmid = (self.t0 + self.t1).div_euclid(2);
        let (i, j) = if self.di == 0 {
            (tmid, (self.line + 0.5 * self.dj as f64) as i64)
        } else {
            ((self.line + 0.5 * self.di as f64) as i64, tmid)
        };
        (i - self.di, j - self.dj)
    }
}

/// A neighbour link's objects: per area across (tile x, tile z, layer), the referencing portals (area, offset).
type LinkObjects = BTreeMap<[u32; 3], Vec<(usize, u64)>>;

/// `navbuild.sector_links(X, Y, areas, mesh_areas, resolve)`: the neighbour links of sector (x, y) - for every
/// border portal (in area order, then portal order), the area across it as `resolve(other sector, portal)` gives it
/// (its tile x, tile z and layer in the other sector), referenced at the portal's offset in its area object
/// (`0x50 + portals * 0x20 + index * 0x2C`, `portals[area]` the area's same-sector portal count). Links sorted by
/// the other sector, each one's objects by (tile x, tile z, layer).
pub fn sector_links(
    x: i64,
    y: i64,
    borders: &[Border],
    portals: &[usize],
    mut resolve: impl FnMut((i64, i64), &Border) -> Option<[u32; 3]>,
) -> Vec<NeighbourLink> {
    let mut links: BTreeMap<(i64, i64), LinkObjects> = BTreeMap::new();
    for b in borders {
        let other = (x + b.di, y + b.dj);
        let Some(hit) = resolve(other, b) else { continue };
        let rel = 0x50 + portals.get(b.area).copied().unwrap_or(0) as u64 * 0x20 + b.index as u64 * 0x2C;
        links
            .entry(other)
            .or_default()
            .entry(hit)
            .or_default()
            .push((b.area, rel));
    }
    links
        .into_iter()
        .map(|((ox, oy), objs)| NeighbourLink {
            sector_id: (u32::try_from(ox & 0xFF).unwrap_or(0) << 8) | u32::try_from(oy & 0xFF).unwrap_or(0),
            objects: objs
                .into_iter()
                .map(|(header, refs)| LinkObject { header, refs })
                .collect(),
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;

    const B: u32 = 0x8000_0000;

    type Part = (Vec<RouteNode>, Vec<(usize, usize)>);

    /// A straight chain of nodes `ids` at x positions `xs`, ground height `y`, at z.
    fn chain(ids: &[u32], xs: &[f64], y: f64, z: f64) -> Part {
        let nodes = ids
            .iter()
            .zip(xs)
            .map(|(&id, &x)| RouteNode {
                id,
                pos: [x, y + 0.95, z],
            })
            .collect();
        (nodes, (0..ids.len() - 1).map(|a| (a, a + 1)).collect())
    }

    fn sector(parts: &[Part], sid: u32, origin: (f64, f64)) -> SectorGraph {
        let mut g = SectorGraph {
            sid,
            bounds: [origin.0, origin.0 + 100.0, 0.0, 10.0, origin.1, origin.1 + 100.0],
            nodes: Vec::new(),
            pairs: Vec::new(),
            origin: Some(origin),
        };
        for (n, p) in parts {
            let off = g.nodes.len();
            g.nodes.extend(n.iter().copied());
            g.pairs.extend(p.iter().map(|&(a, b)| (a + off, b + off)));
        }
        g
    }

    fn main_island() -> Part {
        let ids: Vec<u32> = (1..=30).collect();
        let xs: Vec<f64> = (0..30).map(|k| 2.0 * f64::from(k)).collect();
        chain(&ids, &xs, 10.0, 5.0)
    }

    #[test]
    fn a_small_pocket_beside_a_large_island_goes() {
        let speck = chain(&[101, 102], &[20.0, 21.0], 10.8, 8.0);
        let far = chain(&[201, 202], &[20.0, 21.0], 10.0, 40.0);
        let roof = chain(&[301, 302], &[30.0, 31.0], 15.5, 6.0);
        let mut g = vec![sector(&[main_island(), speck, far, roof], 0x0101, (0.0, 0.0))];
        let none = HashSet::new();
        let st = shadow_islands(&g, &[], &none, &Shadow::default());
        assert_eq!(st.ids, HashSet::from([101, 102]));
        assert_eq!((st.islands, st.before), (1, 4));
        // an anchor on the pocket's floor keeps it
        assert!(
            shadow_islands(&g, &[[20.5, 10.8, 8.0]], &none, &Shadow::default())
                .ids
                .is_empty()
        );
        // one on the main floor below does not
        assert_eq!(
            shadow_islands(&g, &[[20.5, 10.0, 5.0]], &none, &Shadow::default()).islands,
            1
        );
        drop_edges(&mut g, &st.ids);
        let k = linked_ids(&g);
        assert!(!k.contains(&101) && k.contains(&201) && k.contains(&301) && k.contains(&30));
        // on terrain (objects only): it stays; the rule for every island drops it
        let speck = chain(&[101, 102], &[20.0, 21.0], 10.8, 8.0);
        let g = vec![sector(&[main_island(), speck], 0x0101, (0.0, 0.0))];
        let terrain = HashSet::from([101, 102]);
        assert!(shadow_islands(&g, &[], &terrain, &Shadow::default()).ids.is_empty());
        let all = Shadow {
            objects_only: false,
            ..Shadow::default()
        };
        assert_eq!(shadow_islands(&g, &[], &terrain, &all).islands, 1);
    }

    #[test]
    fn border_ids_join_sectors() {
        let xs16: Vec<f64> = (0..16).map(|q| 2.0 * f64::from(q)).collect();
        let mut ids: Vec<u32> = (1..16).collect();
        ids.push(B | 1);
        let a1 = chain(&ids, &xs16, 10.0, 5.0);
        let mut ids2 = vec![B | 1];
        ids2.extend(16..31);
        let xs2: Vec<f64> = (0..16).map(|q| 30.0 + 2.0 * f64::from(q)).collect();
        let a2 = chain(&ids2, &xs2, 10.0, 5.0);
        let p1 = chain(&[501, B | 2], &[28.0, 30.0], 11.0, 7.0);
        let p2 = chain(&[B | 2, 502], &[30.0, 32.0], 11.0, 7.0);
        let mut two = vec![
            sector(&[a1, p1], 0x0101, (0.0, 0.0)),
            sector(&[a2, p2], 0x0201, (100.0, 0.0)),
        ];
        let st = shadow_islands(&two, &[], &HashSet::new(), &Shadow::default());
        assert_eq!(st.ids, HashSet::from([501, 502, B | 2]));
        assert_eq!(st.before, 2);
        drop_edges(&mut two, &st.ids);
        let (nodes, edges) = finalize(&two[0], &linked_ids(&two), &Layout::default()).unwrap();
        let ids: Vec<u32> = nodes.iter().map(|n| n.id).collect();
        // sorted by unsigned id: ordinary nodes, then the border node; the pocket's nodes are gone
        assert_eq!(ids.last(), Some(&(B | 1)));
        assert!(!ids.contains(&501) && !ids.contains(&(B | 2)));
        assert!(
            edges
                .iter()
                .all(|e| (e.src as usize) < nodes.len() && (e.dst as usize) < nodes.len())
        );
    }

    #[test]
    fn finalize_lays_out_nodes_and_edges_as_the_file_holds_them() {
        // ids out of order, a border node outside the sector's square (foreign) and one inside (owned)
        let node = |id, pos| RouteNode { id, pos };
        let g = SectorGraph {
            sid: 0x0101,
            bounds: [0.0; 6],
            nodes: vec![
                node(7, [10.0, 1.0, 10.0]),
                node(B | 3, [100.4, 1.0, 10.0]),
                node(2, [12.5, 2.0, 10.0]),
                node(B | 9, [50.0, 0.5, 0.0]),
                node(4, [0.0, 0.0, 0.0]),
            ],
            pairs: vec![(0, 2), (2, 1), (0, 3)],
            origin: Some((0.0, 0.0)),
        };
        let linked = linked_ids(std::slice::from_ref(&g));
        let (n, edges) = finalize(&g, &linked, &Layout::default()).unwrap();
        assert_eq!(n.iter().map(|x| x.id).collect::<Vec<_>>(), vec![2, 7, B | 3, B | 9]);
        assert_eq!(n[0].value, 15.0);
        assert!(n[1..].iter().all(|x| x.value == 0.0));
        // edges grouped by source, by destination within; a = first edge + 1
        let pairs: Vec<(u32, u32)> = edges.iter().map(|x| (x.src, x.dst)).collect();
        assert_eq!(pairs, vec![(0, 1), (0, 2), (1, 0), (1, 3), (2, 0), (3, 1)]);
        assert_eq!(n.iter().map(|x| x.a).collect::<Vec<_>>(), vec![1, 3, 5, 6]);
        // the cost from the float32 positions: 2.5 m across, 1 m up
        let d = dist3((12.5, 2.0, 10.0), (10.0, 1.0, 10.0));
        assert_eq!(u64::from(edges[0].tag), 0x8000_0000 | (d * 16.0).floor() as u64);
        // b: the node indices sorted by x (low half) and y (high half); c: by z, bit 16 on the foreign copy
        assert_eq!(n.iter().map(|x| x.b & 0xFFFF).collect::<Vec<_>>(), vec![1, 0, 3, 2]);
        assert_eq!(n.iter().map(|x| x.b >> 16).collect::<Vec<_>>(), vec![3, 1, 2, 0]);
        // z: 10, 10, 10, 0 -> [3, 0, 1, 2] (a stable sort)
        assert_eq!(n.iter().map(|x| x.c & 0xFFFF).collect::<Vec<_>>(), vec![3, 0, 1, 2]);
        assert_eq!(n.iter().map(|x| x.c >> 16).collect::<Vec<_>>(), vec![0, 0, 1, 0]);
        // without BORDER_OWNER every border copy is foreign; unsorted keeps the build's order
        let unsorted = Layout {
            border_owner: false,
            sort_by_id: false,
            ..Layout::default()
        };
        let (n, _) = finalize(&g, &linked, &unsorted).unwrap();
        assert_eq!(n.iter().map(|x| x.id).collect::<Vec<_>>(), vec![7, B | 3, 2, B | 9]);
        assert_eq!(n.iter().map(|x| x.c >> 16).collect::<Vec<_>>(), vec![0, 1, 0, 1]);
    }

    #[test]
    fn order_nodes_in_small_islands_lose_their_edges() {
        let mut g = vec![sector(
            &[main_island(), chain(&[900, 901], &[0.0, 1.234_567], 3.0, 50.0)],
            0x0101,
            (0.0, 0.0),
        )];
        let removed = prune_order_nodes(&mut g, &HashSet::from([900, 15]), 25);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, 900);
        assert_eq!(removed[0].pos, [0.0, 3.95, 50.0]);
        assert_eq!(removed[0].size, 2);
        let k = linked_ids(&g);
        assert!(!k.contains(&900) && !k.contains(&901) && k.contains(&15));
    }

    #[test]
    fn the_file_round_trips_and_refuses_what_it_cannot_hold() {
        let mesh = b"KynogonRuntimeMesh\0\0rest".to_vec();
        let g = vec![sector(&[main_island()], 0x0101, (0.0, 0.0))];
        let s = FileSettings {
            shadow: Some((Shadow::default(), Vec::new(), HashSet::new())),
            orders: None,
            layout: Layout::default(),
            header: [0; hpd::HEADER_WORDS],
            date: 261_017,
            clock: 12_000_000,
        };
        let w = write_file(&g, std::slice::from_ref(&mesh), &s).unwrap();
        assert_eq!((w.nodes, w.edges), (30, 58));
        let f = hpd::parse(&w.data).unwrap();
        assert_eq!(f.sectors[0].mesh, mesh.as_slice());
        assert_eq!((f.sectors[0].date, f.sectors[0].time), (261_017, 12_000_000));
        assert_eq!(f.sectors[0].nodes.len(), 30);
        // a pair past the nodes: the reference's IndexError; a NaN position or one past float32: refused
        let mut bad = g.clone();
        bad[0].pairs.push((0, 99));
        assert_eq!(
            write_file(&bad, std::slice::from_ref(&mesh), &s).unwrap_err(),
            GraphError::Index
        );
        let mut bad = g.clone();
        bad[0].nodes[3].pos[1] = f64::NAN;
        assert!(matches!(
            write_file(&bad, std::slice::from_ref(&mesh), &s),
            Err(GraphError::Refused(_))
        ));
        let mut bad = g;
        bad[0].nodes[3].pos[0] = 1e39;
        assert!(matches!(
            write_file(&bad, std::slice::from_ref(&mesh), &s),
            Err(GraphError::Refused(_))
        ));
        assert!(matches!(write_file(&bad, &[], &s), Err(GraphError::Refused(_))));
    }

    #[test]
    fn sector_links_group_sort_and_place_their_references() {
        let border = |area, index, di, dj| Border {
            area,
            index,
            di,
            dj,
            t0: 10,
            t1: 15,
            line: 125.5,
        };
        // area 0 (2 same-sector portals): two border portals east, one north; area 1 (none): one east, one west
        let borders = [
            border(0, 0, 1, 0),
            border(0, 1, 0, 1),
            border(0, 2, 1, 0),
            border(1, 0, 1, 0),
            border(1, 1, -1, 0),
        ];
        let hits = [Some([1, 0, 0]), Some([0, 0, 2]), Some([0, 2, 0]), Some([1, 0, 0]), None];
        let mut n = 0;
        let links = sector_links(0, 5, &borders, &[2, 0], |_other, _b| {
            n += 1;
            hits[n - 1]
        });
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].sector_id, 0x0006);
        assert_eq!(links[1].sector_id, 0x0105);
        let east = &links[1].objects;
        assert_eq!(
            east.iter().map(|o| o.header).collect::<Vec<_>>(),
            vec![[0, 2, 0], [1, 0, 0]]
        );
        assert_eq!(east[1].refs, vec![(0, 0x50 + 2 * 0x20), (1, 0x50)]);
        assert_eq!(east[0].refs, vec![(0, 0x50 + 2 * 0x20 + 2 * 0x2C)]);
        // a sector below 0: its id masked to 8 bits, as the reference's
        let links = sector_links(0, 0, &[border(0, 0, -1, 0)], &[0], |_o, _b| Some([0, 0, 0]));
        assert_eq!(links[0].sector_id, 0xFF00);
        // the border cell: the middle of the run, one step inside the border line
        assert_eq!(border(0, 0, 1, 0).inner_cell(), (125, 12));
        assert_eq!(border(0, 0, 0, -1).inner_cell(), (12, 126));
    }
}
