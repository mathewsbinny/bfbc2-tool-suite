//! DICE's node placement (round NKS-deep B, `spec/transforms/kynapse_generation.md` KYG-NODE-7, KYG-NODE-8,
//! KYG-EDGE-5): the route graph Kynapse's map builder makes (`kymapbuilderrgg.cpp` in `BFBC2Game.exe`), on NavKit's
//! sample grid.
//!
//! 1. **Propagation** (0x0115D520, 0x01153000, 0x01158BA0): from the seeds, breadth first, every node sends
//!    NbSectors probes, one every 2 pi / NbSectors radians from +x towards +z, each DistPropagation along the ground
//!    and stopped 2 pitches short of where the ground ends; the probes' ends, farthest first, become nodes unless a
//!    node lies within DistDensityMin, or within DistDensityMax and in a straight walk of it.
//! 2. **Dense edges** (0x01154B00): every node to every node within ConnectionDistance (1.2 x DistPropagation)
//!    across, DistDensityMin or more away in 3-D, in a straight walk.
//! 3. **Simplification** (0x0115B3C0, 0x01158510, 0x01157CA0): edges collapsed into their middle, shortest first,
//!    while every neighbour of the two ends walks straight to the middle within DistEdgeMax; leaves, locked nodes
//!    and edges that would flatten a triangle onto a two-edge node are kept; passes until nothing collapses, then
//!    edges under 4 m with a spatial lock and rerouting, then every edge, then again from the start.
//! 4. **Over-connection** (0x01155FA0, 0x01155150): edges in order, each removed while the graph holds another route
//!    at most OverConnectionRatio (1.2) times as long.
//! 5. Nodes left without an edge removed (0x011578B0, ComponentSizeMin 1).
//!
//! What NavKit has in place of Kynapse's mesh queries: the probes and the straight walks run over the sector's kept
//! surfaces with [`Level::neighbour`] (the area link rule) and the route edges' D sides, a point's height is its
//! cell's surface. The seeds are the sector's border nodes, then one per stretch of ground they do not reach (DICE's
//! seed points come from its own sampling, KYG-SEED-1).
#![allow(
    clippy::indexing_slicing,
    reason = "every index is a node's or an edge's of the arrays this module builds, below their lengths"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::too_many_lines,
    reason = "Kynapse's parameter names (DistPropagation, NbSectors) in the docs; the geometry's names (a, b, m for the ends and the middle, p for the parameters) and cell counts as f64"
)]

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

use crate::trace::P3;
use crate::{Cell, FastMap, FastSet, Level, Rule};

/// The map builder's parameters for one pitch (KYG-PAR-1, KYG-PAR-2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// DistPropagation: GraphAccuracy + 0.35 m (3.35 coarse, 0.85 detailed).
    pub dist: f64,
    /// NbSectors: the probes per node (32 coarse, 128 detailed).
    pub sectors: u32,
    /// DistDensityMax: 0.65 x DistPropagation.
    pub density_max: f64,
    /// DistDensityMin: Kynapse's default entity radius, 0.35 m.
    pub density_min: f64,
    /// ConnectionDistance: 1.2 x DistPropagation (Kynapse's default when unset).
    pub connection: f64,
    /// How far short of the ground's end a probe stops: 2 x the pitch.
    pub back_off: f64,
    /// OverConnectionRatio (1.2).
    pub over_connection: f64,
    /// DistEdgeMax (15 m).
    pub dist_edge_max: f64,
    /// OverSimplify's short edges: under 2 OneMeter... squared, 4 m (the length compared with 4).
    pub short_edge: f64,
    /// The spatial lock's cell (5 OneMeter) ...
    pub lock_cell: f64,
}

impl Params {
    /// DICE's settings at a pitch: GraphAccuracy 3 at 0.8 m, 0.5 at 0.2 m (the detailed zones), NbSectors 32 / 128.
    #[must_use]
    pub fn at_pitch(pitch: f64, dist_edge_max: f64) -> Params {
        let detailed = (pitch - 0.2).abs() < 1e-6;
        let (accuracy, sectors) = if detailed { (0.5, 128) } else { (3.0, 32) };
        let dist = accuracy + KYNAPSE_RADIUS;
        Params {
            dist,
            sectors,
            density_max: 0.65 * dist,
            density_min: KYNAPSE_RADIUS,
            connection: 1.2 * dist,
            back_off: 2.0 * pitch,
            over_connection: 1.2,
            dist_edge_max,
            short_edge: 4.0,
            lock_cell: 5.0,
        }
    }
}

/// Kynapse's default entity radius, which DICE's configuration order leaves in place when GraphAccuracy is read.
pub const KYNAPSE_RADIUS: f64 = 0.35;

/// A node: surface, local cell, position, locked (a border node: never moved or removed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PNode {
    /// The surface it stands on.
    pub k: usize,
    /// Its local cell.
    pub cell: (i64, i64),
    /// Its position.
    pub pos: P3,
    /// A border node (a seed that stays).
    pub locked: bool,
}

/// The sector the placement runs on.
pub struct Ground<'a, S: std::hash::BuildHasher> {
    /// The level.
    pub level: &'a Level,
    /// The sector's sample origin (global i, j of local 0, 0).
    pub gi0: i64,
    /// See `gi0`.
    pub gj0: i64,
    /// The sector's origin in metres.
    pub origin: (f64, f64),
    /// The pitch.
    pub pitch: f64,
    /// A node's height over its surface.
    pub y_offset: f64,
    /// The link rule.
    pub rule: Rule,
    /// The sector's D sides (route edges do not cross them).
    pub blocked: &'a HashSet<(usize, usize), S>,
    /// The sector's surfaces and their cells (its areas').
    pub cell_of: FastMap<usize, (i64, i64)>,
}

impl<S: std::hash::BuildHasher> Ground<'_, S> {
    /// The ground of a sector's areas.
    pub fn of<'a>(
        level: &'a Level,
        areas: &[Vec<Cell>],
        gi0: i64,
        gj0: i64,
        origin: (f64, f64),
        pitch: f64,
        y_offset: f64,
        rule: Rule,
        blocked: &'a HashSet<(usize, usize), S>,
    ) -> Ground<'a, S> {
        let mut cell_of = FastMap::default();
        for &(i, j, k) in areas.iter().flatten() {
            cell_of.insert(k, (i, j));
        }
        Ground {
            level,
            gi0,
            gj0,
            origin,
            pitch,
            y_offset,
            rule,
            blocked,
            cell_of,
        }
    }

    fn cell_at(&self, x: f64, z: f64) -> (i64, i64) {
        (
            ((x - self.origin.0) / self.pitch).round() as i64,
            ((z - self.origin.1) / self.pitch).round() as i64,
        )
    }

    fn height(&self, k: usize) -> f64 {
        self.level.y(k) + self.y_offset
    }

    /// One step from surface k in cell c to the next cell (di, dj): the surface there, if the sector holds it, the
    /// link rule joins them and no D side lies between.
    fn step(&self, k: usize, c: (i64, i64), di: i64, dj: i64) -> Option<usize> {
        let (gi, gj) = (self.gi0 + c.0, self.gj0 + c.1);
        let nk = self.level.neighbour(k, gi, gj, di, dj, None, self.rule)?;
        if !self.cell_of.contains_key(&nk) {
            return None;
        }
        if !self.blocked.is_empty()
            && self
                .level
                .crosses_d(self.blocked, k, nk, gi, gj, di, dj, None, self.rule)
        {
            return None;
        }
        Some(nk)
    }

    /// A probe from (k, cell, x, z) along (cx, cz) for at most `len`: where the ground ends (the middle of the last
    /// step that failed; `len` when it does not) and the surfaces passed, with the distance each is reached at.
    fn probe(
        &self,
        k0: usize,
        c0: (i64, i64),
        x0: f64,
        z0: f64,
        dir: (f64, f64),
        len: f64,
    ) -> (f64, Vec<(f64, usize, (i64, i64))>) {
        let h = self.pitch * 0.25;
        let n = (len / h).ceil().max(1.0) as usize;
        let (mut k, mut c) = (k0, c0);
        let mut passed = vec![(0.0, k0, c0)];
        let mut prev = 0.0;
        for s in 1..=n {
            let t = (s as f64 * h).min(len);
            let nc = self.cell_at(x0 + dir.0 * t, z0 + dir.1 * t);
            if nc != c {
                let (di, dj) = (nc.0 - c.0, nc.1 - c.1);
                let step = if di.abs() <= 1 && dj.abs() <= 1 {
                    self.step(k, c, di, dj)
                } else {
                    None
                };
                match step {
                    Some(nk) => {
                        k = nk;
                        c = nc;
                        passed.push((t, k, c));
                    }
                    None => return (f64::midpoint(prev, t), passed),
                }
            }
            prev = t;
        }
        (len, passed)
    }

    /// A straight walk from node a's surface to b's cell ending on b's surface (or within 0.6 m of it):
    /// [`Level::line_walkable`] over the route edges' D sides.
    fn walk(&self, a: &PNode, b: &PNode) -> bool {
        self.level
            .line_walkable(a.k, a.cell, b.cell, self.gi0, self.gj0, b.k, self.blocked, self.rule)
    }

    /// The surface a straight walk from a reaches at (x, z), with its cell.
    fn reach(&self, a: &PNode, x: f64, z: f64) -> Option<(usize, (i64, i64))> {
        let (dx, dz) = (x - a.pos.0, z - a.pos.2);
        let len = dx.hypot(dz);
        if len < 1e-9 {
            return Some((a.k, a.cell));
        }
        let (end, passed) = self.probe(a.k, a.cell, a.pos.0, a.pos.2, (dx / len, dz / len), len);
        if end < len {
            return None;
        }
        passed.last().map(|&(_, k, c)| (k, c))
    }
}

fn dist3(a: P3, b: P3) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt()
}

fn dist2(a: P3, b: P3) -> f64 {
    (a.0 - b.0).hypot(a.2 - b.2)
}

/// Nodes by a grid of `cell` metres in plan.
struct Grid {
    cell: f64,
    at: FastMap<(i64, i64), Vec<usize>>,
}

impl Grid {
    fn new(cell: f64) -> Grid {
        Grid {
            cell,
            at: FastMap::default(),
        }
    }
    fn key(&self, p: P3) -> (i64, i64) {
        ((p.0 / self.cell).floor() as i64, (p.2 / self.cell).floor() as i64)
    }
    fn add(&mut self, p: P3, n: usize) {
        self.at.entry(self.key(p)).or_default().push(n);
    }
    /// The nodes in the cells within `r` of p (a superset of those within r), in grid then insertion order.
    fn near(&self, p: P3, r: f64) -> Vec<usize> {
        let (cx, cz) = self.key(p);
        let s = (r / self.cell).ceil() as i64;
        let mut out = Vec::new();
        for dx in -s..=s {
            for dz in -s..=s {
                if let Some(v) = self.at.get(&(cx + dx, cz + dz)) {
                    out.extend_from_slice(v);
                }
            }
        }
        out
    }
}

/// The placement's result: the nodes (the given seeds first, in their order) and the edges (index pairs, lower
/// first, sorted) with their 3-D lengths.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Placed {
    /// The nodes; `None` where a node was removed (merged or left without an edge).
    pub nodes: Vec<Option<PNode>>,
    /// The edges.
    pub edges: Vec<(usize, usize, f64)>,
}

/// Steps 1 and 2: the propagation from `seeds` (border nodes, locked) and the dense edges.
fn propagate<S: std::hash::BuildHasher>(
    g: &Ground<'_, S>,
    p: &Params,
    seeds: &[PNode],
    areas: &[Vec<Cell>],
) -> Vec<PNode> {
    let mut nodes: Vec<PNode> = Vec::new();
    let mut grid = Grid::new(p.density_max.max(1.0));
    let mut taken: FastSet<usize> = FastSet::default();
    let mut queue: VecDeque<usize> = VecDeque::new();
    for s in seeds {
        grid.add(s.pos, nodes.len());
        taken.insert(s.k);
        queue.push_back(nodes.len());
        nodes.push(*s);
    }
    let dirs: Vec<(f64, f64)> = (0..p.sectors)
        .map(|i| {
            let a = f64::from(i) * std::f64::consts::TAU / f64::from(p.sectors);
            (a.cos(), a.sin())
        })
        .collect();
    let run = |nodes: &mut Vec<PNode>, queue: &mut VecDeque<usize>, grid: &mut Grid, taken: &mut FastSet<usize>| {
        while let Some(at) = queue.pop_front() {
            let src = nodes[at];
            // every probe's end, then farthest first (a stable sort: ties keep the probes' order)
            let mut ends: Vec<(f64, PNode)> = Vec::with_capacity(dirs.len());
            for &dir in &dirs {
                let (hit, passed) = g.probe(src.k, src.cell, src.pos.0, src.pos.2, dir, p.dist + p.back_off);
                let e = if hit >= p.dist + p.back_off {
                    p.dist
                } else {
                    (hit - p.back_off).min(p.dist)
                };
                if e <= 1e-6 {
                    continue;
                }
                let Some(&(_, k, cell)) = passed.iter().rev().find(|q| q.0 <= e + 1e-9) else {
                    continue;
                };
                let pos = (src.pos.0 + dir.0 * e, g.height(k), src.pos.2 + dir.1 * e);
                ends.push((
                    dist3(pos, src.pos),
                    PNode {
                        k,
                        cell,
                        pos,
                        locked: false,
                    },
                ));
            }
            ends.sort_by(|a, b| b.0.total_cmp(&a.0));
            for (_, cand) in ends {
                if taken.contains(&cand.k) {
                    continue;
                }
                let crowded = grid.near(cand.pos, p.density_max).into_iter().any(|m| {
                    let o = &nodes[m];
                    let d = dist3(o.pos, cand.pos);
                    d <= p.density_min || (d <= p.density_max && (g.walk(&cand, o) || g.walk(o, &cand)))
                });
                if crowded {
                    continue;
                }
                grid.add(cand.pos, nodes.len());
                taken.insert(cand.k);
                queue.push_back(nodes.len());
                nodes.push(cand);
            }
        }
    };
    run(&mut nodes, &mut queue, &mut grid, &mut taken);
    // ground no seed reached: one seed at the centre cell of its first area, then propagate again
    let mut reached: FastSet<usize> = FastSet::default();
    loop {
        let mut flood: VecDeque<usize> = nodes.iter().map(|n| n.k).filter(|k| reached.insert(*k)).collect();
        while let Some(k) = flood.pop_front() {
            let Some(&c) = g.cell_of.get(&k) else { continue };
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if let Some(nk) = g.step(k, c, di, dj)
                    && reached.insert(nk)
                {
                    flood.push_back(nk);
                }
            }
        }
        let Some(cells) = areas
            .iter()
            .find(|a| a.len() >= 4 && a.iter().all(|c| !reached.contains(&c.2)))
        else {
            break;
        };
        let ij: Vec<(i64, i64)> = cells.iter().map(|c| (c.0, c.1)).collect();
        let Some(&(i, j, k)) = crate::pick::centre_cell(&ij).and_then(|n| cells.get(n)) else {
            break;
        };
        let pos = (
            g.origin.0 + i as f64 * g.pitch,
            g.height(k),
            g.origin.1 + j as f64 * g.pitch,
        );
        grid.add(pos, nodes.len());
        taken.insert(k);
        queue.push_back(nodes.len());
        nodes.push(PNode {
            k,
            cell: (i, j),
            pos,
            locked: false,
        });
        run(&mut nodes, &mut queue, &mut grid, &mut taken);
    }
    nodes
}

/// Step 2: every pair within ConnectionDistance across, DistDensityMin or more apart, in a straight walk.
fn dense_edges<S: std::hash::BuildHasher>(g: &Ground<'_, S>, p: &Params, nodes: &[PNode]) -> BTreeSet<(usize, usize)> {
    let mut grid = Grid::new(p.connection.max(1.0));
    for (n, a) in nodes.iter().enumerate() {
        grid.add(a.pos, n);
    }
    let mut out = BTreeSet::new();
    for (n, a) in nodes.iter().enumerate() {
        for m in grid.near(a.pos, p.connection) {
            if m == n || out.contains(&(n.min(m), n.max(m))) {
                continue;
            }
            let b = &nodes[m];
            if dist2(a.pos, b.pos) <= p.connection && dist3(a.pos, b.pos) >= p.density_min && g.walk(a, b) {
                out.insert((n.min(m), n.max(m)));
            }
        }
    }
    out
}

/// The simplification graph: nodes (removed ones `None`), adjacency, edges with their dirty flag.
struct Simplify {
    nodes: Vec<Option<PNode>>,
    adj: Vec<BTreeSet<usize>>,
    dirty: BTreeMap<(usize, usize), bool>,
}

impl Simplify {
    fn key(a: usize, b: usize) -> (usize, usize) {
        (a.min(b), a.max(b))
    }
    fn add_edge(&mut self, a: usize, b: usize) {
        if a == b || self.adj[a].contains(&b) {
            return;
        }
        self.adj[a].insert(b);
        self.adj[b].insert(a);
        self.dirty.insert(Self::key(a, b), true);
    }
    fn remove_edge(&mut self, a: usize, b: usize) {
        self.adj[a].remove(&b);
        self.adj[b].remove(&a);
        self.dirty.remove(&Self::key(a, b));
    }
    fn pos(&self, n: usize) -> P3 {
        self.nodes[n].map_or((0.0, 0.0, 0.0), |v| v.pos)
    }
    /// A two-edge node joined to exactly a and b (collapsing a-b would leave it a doubled edge).
    fn triangle_apex(&self, a: usize, b: usize) -> bool {
        self.adj[a]
            .iter()
            .any(|&c| c != b && self.adj[c].len() == 2 && self.adj[c].contains(&a) && self.adj[c].contains(&b))
    }
}

/// The phases of the simplification (0x0115B3C0's state word).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Dirty edges, no lock.
    Dirty,
    /// Short edges (OverSimplify), the spatial lock, rerouting.
    Short,
    /// Every edge, no triangle test.
    All,
}

/// Step 3: edges collapsed into their middles. Returns the graph.
#[allow(
    clippy::too_many_lines,
    reason = "0x0115B3C0's passes and its collapse test, in its order"
)]
fn simplify<S: std::hash::BuildHasher>(
    g: &Ground<'_, S>,
    p: &Params,
    nodes: Vec<PNode>,
    edges: &BTreeSet<(usize, usize)>,
) -> Simplify {
    let n = nodes.len();
    let mut s = Simplify {
        nodes: nodes.into_iter().map(Some).collect(),
        adj: vec![BTreeSet::new(); n],
        dirty: BTreeMap::new(),
    };
    for &(a, b) in edges {
        s.add_edge(a, b);
    }
    let mut phase = Phase::Dirty;
    let mut rounds = 0usize;
    loop {
        rounds += 1;
        if rounds > 10_000 {
            break;
        }
        // the pass's edges: dirty ones, shortest first (ties by the pair)
        let mut list: Vec<(f64, (usize, usize))> = s
            .dirty
            .iter()
            .filter(|&(_, &d)| d)
            .map(|(&e, _)| (dist3(s.pos(e.0), s.pos(e.1)), e))
            .collect();
        list.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        let mut locked: FastSet<(i64, i64)> = FastSet::default();
        let lock_key = |q: P3| ((q.0 / p.lock_cell).floor() as i64, (q.2 / p.lock_cell).floor() as i64);
        let mut any = false;
        for (_, (a, b)) in list {
            match s.dirty.get_mut(&(a, b)) {
                Some(d) if *d => *d = false,
                _ => continue,
            }
            let (Some(na), Some(nb)) = (s.nodes[a], s.nodes[b]) else {
                continue;
            };
            if na.locked || nb.locked || s.adj[a].len() <= 1 || s.adj[b].len() <= 1 {
                continue;
            }
            if phase == Phase::Short && (locked.contains(&lock_key(na.pos)) || locked.contains(&lock_key(nb.pos))) {
                continue;
            }
            if phase != Phase::All && s.triangle_apex(a, b) {
                continue;
            }
            // the middle, on the ground a straight walk from a reaches there
            let (mx, mz) = (f64::midpoint(na.pos.0, nb.pos.0), f64::midpoint(na.pos.2, nb.pos.2));
            let Some((mk, mc)) = g.reach(&na, mx, mz) else { continue };
            let m = PNode {
                k: mk,
                cell: mc,
                pos: (mx, g.height(mk), mz),
                locked: false,
            };
            // every other neighbour of a and b walks straight to the middle within DistEdgeMax; in the short phase
            // one that does not is rerouted to the others
            let others: Vec<usize> = s.adj[a]
                .iter()
                .chain(s.adj[b].iter())
                .copied()
                .filter(|&x| x != a && x != b)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let mut keep: Vec<(usize, usize)> = Vec::new();
            let mut ok = true;
            for &q in &others {
                let Some(nq) = s.nodes[q] else { continue };
                if dist2(nq.pos, m.pos) <= p.dist_edge_max && g.walk(&nq, &m) {
                    keep.push((q, a));
                    continue;
                }
                if phase != Phase::Short {
                    ok = false;
                    break;
                }
                for &x in &others {
                    if x == q {
                        continue;
                    }
                    let Some(nx) = s.nodes[x] else { continue };
                    if dist2(nq.pos, nx.pos) > p.dist_edge_max || !g.walk(&nq, &nx) {
                        ok = false;
                        break;
                    }
                    keep.push((q, x));
                }
                if !ok {
                    break;
                }
            }
            if !ok {
                continue;
            }
            // collapse: a moves to the middle, b goes, their edges become the kept ones
            for x in s.adj[a].clone() {
                s.remove_edge(a, x);
            }
            for x in s.adj[b].clone() {
                s.remove_edge(b, x);
            }
            s.nodes[a] = Some(m);
            s.nodes[b] = None;
            for (u, v) in keep {
                s.add_edge(u, v);
            }
            any = true;
            if phase == Phase::Short {
                let (cx, cz) = lock_key(m.pos);
                let r = 1 + (2.0 * p.dist / p.lock_cell).round() as i64;
                for dx in -r..=r {
                    for dz in -r..=r {
                        locked.insert((cx + dx, cz + dz));
                    }
                }
            }
        }
        if any {
            phase = Phase::Dirty;
            continue;
        }
        match phase {
            Phase::Dirty => {
                phase = Phase::Short;
                let short: Vec<(usize, usize)> = s
                    .dirty
                    .keys()
                    .copied()
                    .filter(|&(a, b)| dist3(s.pos(a), s.pos(b)) < p.short_edge)
                    .collect();
                for e in short {
                    s.dirty.insert(e, true);
                }
            }
            Phase::Short => {
                phase = Phase::All;
                for d in s.dirty.values_mut() {
                    *d = true;
                }
            }
            Phase::All => break,
        }
    }
    s
}

/// Step 4: each edge in order removed while another route at most `ratio` times its length joins its ends (a
/// search bounded by that length).
fn over_connection(s: &mut Simplify, ratio: f64) {
    let edges: Vec<(usize, usize)> = s.dirty.keys().copied().collect();
    for (a, b) in edges {
        if !s.adj[a].contains(&b) {
            continue;
        }
        let len = dist3(s.pos(a), s.pos(b));
        let bound = ratio * len;
        // Dijkstra from a without the edge a-b, within the bound
        let mut best: FastMap<usize, f64> = FastMap::default();
        let mut heap: BTreeSet<(u64, usize)> = BTreeSet::new();
        best.insert(a, 0.0);
        heap.insert((0f64.to_bits(), a));
        let mut found = false;
        while let Some((dbits, u)) = heap.pop_first() {
            let d = f64::from_bits(dbits);
            if best.get(&u).is_some_and(|&x| x < d) {
                continue;
            }
            if u == b {
                found = true;
                break;
            }
            for &v in &s.adj[u] {
                if (u == a && v == b) || (u == b && v == a) {
                    continue;
                }
                let nd = d + dist3(s.pos(u), s.pos(v));
                if nd > bound + 1e-9 {
                    continue;
                }
                if best.get(&v).is_none_or(|&x| nd < x) {
                    best.insert(v, nd);
                    heap.insert((nd.to_bits(), v));
                }
            }
        }
        if found {
            s.remove_edge(a, b);
        }
    }
}

/// DICE's placement of a sector's route nodes and edges from its border nodes (KYG-NODE-7, KYG-NODE-8, KYG-EDGE-5).
#[must_use]
pub fn place<S: std::hash::BuildHasher>(g: &Ground<'_, S>, p: &Params, seeds: &[PNode], areas: &[Vec<Cell>]) -> Placed {
    let nodes = propagate(g, p, seeds, areas);
    let dense = dense_edges(g, p, &nodes);
    let mut s = simplify(g, p, nodes, &dense);
    over_connection(&mut s, p.over_connection);
    // nodes left without an edge go (border nodes stay: other sectors hold them)
    for n in 0..s.nodes.len() {
        if s.adj[n].is_empty() && s.nodes[n].is_some_and(|v| !v.locked) {
            s.nodes[n] = None;
        }
    }
    let edges = s
        .dirty
        .keys()
        .map(|&(a, b)| (a, b, dist3(s.pos(a), s.pos(b))))
        .collect();
    Placed { nodes: s.nodes, edges }
}

#[cfg(test)]
#[path = "propagate_tests.rs"]
mod tests;
