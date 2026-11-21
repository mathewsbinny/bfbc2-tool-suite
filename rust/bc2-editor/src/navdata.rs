//! A level's AI navigation data as the AI workspace shows it (round GR2: the Navigation Studio folded into the editor).
//!
//! - [`NavData`]: a Kynapse path-data file (`pathdata.hpd`) - its walkable cells on the 0.8 m grid, the areas with their
//!   boundaries, the route graph and its islands, ladder links and the ground reached only through them - as
//!   `core/bfbc2core/navdata.py` reads it, step for step (the cells by NavKit's `navdiff.area_cells`,
//!   [`bc2_navbuild::parity::area_cells`]; heights from the terrain's height field where a cell lies on it).
//! - [`compare_cells`]: which cells of NavKit's build DICE's file also has, and the other way (`navdiff`'s rule).
//! - [`spawn_islands`], [`island_gaps`]: the Studio's Islands and spawns panel; [`route`]: its path test
//!   (`navpath.route`: A* over the file's own directed edges and stored costs, [`bc2_navbuild::astar`]).
//! - [`AiLayer`], [`NavIndex`], [`anomalies`], [`reference_report`]: `overlay.py`'s AI layer of a level partition, its
//!   nearest-node lookups, the anomaly overlay and the compare-with-a-reference report.
//!
//! The Studio's answers were recorded before it was retired (`tests/record_navstudio.py`) and `tests/nav_parity.rs`
//! checks these against them. Nothing here writes a file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bc2_formats::dbx::{self, Node};
use bc2_formats::terrain::Heightfield;
use bc2_formats::{hpd, kymesh};
use bc2_navbuild::astar::{Graph, Norm};
use bc2_navbuild::trace::P3;

/// The cells' grid (m): `navdiff.GRID`.
pub const GRID: f64 = 0.8;
/// How far above the ground Kynapse stores boundaries and route nodes (m): `navdiff.Y_OFFSET`.
pub const Y_OFFSET: f64 = 0.95;
/// Areas with a smaller pitch are detailed zones (DICE's 0.2 m).
pub const DETAILED: f64 = 0.5;
/// A ladder link climbs more than this (m) ...
pub const LADDER_DY: f64 = 2.5;
/// ... and more than this many metres per metre across.
pub const LADDER_SLOPE: f64 = 1.5;
/// Stacked floors pair by height up to this difference (m): `navdiff.STACK_TOL`.
pub const STACK_TOL: f64 = 4.0;
/// The path test and the spawn check join a point to a route node within this horizontal distance (m) ...
pub const JOIN_XZ: f64 = 15.0;
/// ... and this height (m).
pub const JOIN_Y: f64 = 4.0;
/// The runtime's node gather radius (m): `overlay.R_GATHER`.
pub const R_GATHER: f64 = 15.0;
/// A node and a point are on the same floor within this height (m).
pub const SAME_FLOOR: f64 = 2.5;
/// Islands under this many nodes are small.
pub const SMALL_ISLAND: usize = 25;
/// Areas under this many cells with no portal are slivers.
pub const SLIVER_CELLS: usize = 5;
/// A node with no same-floor node within this distance (m) is lonely.
pub const LONELY_NODE: f64 = 12.0;

/// One walkable area of a sector's mesh.
#[derive(Debug, Clone)]
pub struct NavArea {
    /// Its sector's id.
    pub sector: u32,
    /// Its index within the sector.
    pub index: usize,
    /// Its tile.
    pub tile: (i64, i64),
    /// Its layer.
    pub layer: i64,
    /// Its sampling pitch (m).
    pub pitch: f64,
    /// Its lowest height.
    pub min_y: f64,
    /// Its highest height.
    pub max_y: f64,
    /// Its outline's polylines (two points or more; stored 0.95 m above the ground).
    pub outline: Vec<Vec<[f32; 3]>>,
    /// Its portals: the two ends and the area it leads to within the sector.
    pub portals: Vec<([f64; 3], [f64; 3], usize)>,
    /// Its border portals' ends.
    pub borders: Vec<([f64; 3], [f64; 3])>,
    /// Its island (an index into [`NavData::islands`]; -1 without a route node in its sector).
    pub island: i64,
    /// Its walkable cells.
    pub cells: usize,
    /// Reached only through a ladder link.
    pub ladder: bool,
}

impl NavArea {
    /// A detailed zone (pitch under 0.5 m).
    #[must_use]
    pub fn detailed(&self) -> bool {
        self.pitch < DETAILED
    }
}

/// A connected part of the route graph.
#[derive(Debug, Clone)]
pub struct Island {
    /// Its route nodes.
    pub nodes: usize,
    /// The cells of its areas.
    pub cells: usize,
    /// The mean of its nodes' positions (stored height).
    pub centre: [f64; 3],
}

/// A path-data file as the AI workspace draws it.
#[derive(Debug, Clone, Default)]
pub struct NavData {
    /// The file.
    pub path: PathBuf,
    /// Its areas, sector after sector.
    pub areas: Vec<NavArea>,
    /// Every walkable cell's grid position (i, j): its centre is (i, j) x 0.8 m.
    pub cell_ij: Vec<[i64; 2]>,
    /// Each cell's ground height.
    pub cell_y: Vec<f64>,
    /// Each cell's area.
    pub cell_area: Vec<u32>,
    /// The route nodes' positions (as stored: 0.95 m above the ground).
    pub node_pos: Vec<[f64; 3]>,
    /// Each node's sector.
    pub node_sector: Vec<u32>,
    /// The route edges (source, destination).
    pub edges: Vec<[usize; 2]>,
    /// Each edge's cost (m, as stored).
    pub edge_cost: Vec<f64>,
    /// Each node's island.
    pub node_island: Vec<u32>,
    /// The islands, largest first (island 0 is the main network).
    pub islands: Vec<Island>,
    /// Each edge: a ladder link.
    pub ladder_edge: Vec<bool>,
}

impl NavData {
    /// A cell's centre on the xz plane.
    #[must_use]
    pub fn cell_xz(&self, k: usize) -> [f64; 2] {
        self.cell_ij
            .get(k)
            .map_or([0.0; 2], |c| [c[0] as f64 * GRID, c[1] as f64 * GRID])
    }

    /// Whether each cell is ladder ground.
    #[must_use]
    pub fn ladder_cells(&self) -> Vec<bool> {
        self.cell_area
            .iter()
            .map(|&a| self.areas.get(a as usize).is_some_and(|x| x.ladder))
            .collect()
    }

    /// The sectors of its areas, sorted.
    #[must_use]
    pub fn sectors(&self) -> Vec<u32> {
        let mut s: Vec<u32> = self.areas.iter().map(|a| a.sector).collect();
        s.sort_unstable();
        s.dedup();
        s
    }
}

/// `navdata.summary`: the Navigation panel's figures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    /// Areas.
    pub areas: usize,
    /// Detailed areas.
    pub detailed_areas: usize,
    /// Cells.
    pub cells: usize,
    /// Route nodes.
    pub nodes: usize,
    /// Route edges.
    pub edges: usize,
    /// Islands.
    pub islands: usize,
    /// Islands of 25 nodes or more.
    pub large_islands: usize,
    /// Sectors.
    pub sectors: usize,
    /// Ladder links (each two edges).
    pub ladder_links: usize,
    /// Cells of ladder ground.
    pub ladder_ground_cells: usize,
}

/// The file's figures.
#[must_use]
pub fn summary(nav: &NavData) -> Summary {
    Summary {
        areas: nav.areas.len(),
        detailed_areas: nav.areas.iter().filter(|a| a.detailed()).count(),
        cells: nav.cell_ij.len(),
        nodes: nav.node_pos.len(),
        edges: nav.edges.len(),
        islands: nav.islands.len(),
        large_islands: nav.islands.iter().filter(|i| i.nodes >= SMALL_ISLAND).count(),
        sectors: nav.sectors().len(),
        ladder_links: nav.ladder_edge.iter().filter(|&&l| l).count() / 2,
        ladder_ground_cells: nav.areas.iter().filter(|a| a.ladder).map(|a| a.cells).sum(),
    }
}

fn p64(v: [f32; 3]) -> [f64; 3] {
    v.map(f64::from)
}

/// `navdiff.segments(area)`: the outline's consecutive point pairs, then the portals, then the border portals.
fn segments(a: &kymesh::Area) -> Vec<(P3, P3)> {
    let p = |v: [f32; 3]| (f64::from(v[0]), f64::from(v[1]), f64::from(v[2]));
    let mut out = Vec::new();
    for l in &a.outline {
        out.extend(l.iter().zip(l.iter().skip(1)).map(|(&a, &b)| (p(a), p(b))));
    }
    out.extend(a.portals.iter().map(|q| (p(q.p0), p(q.p1))));
    out.extend(a.border_portals.iter().map(|q| (p(q.p0), p(q.p1))));
    out
}

/// `navdata._union_roots`: each node's root after joining every edge's ends (but those `skip` marks) in order, by a
/// union-find with path halving.
#[must_use]
pub fn union_roots(n: usize, edges: &[[usize; 2]], skip: Option<&[bool]>) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..n).collect();
    let find = |parent: &mut Vec<usize>, mut a: usize| {
        while let Some(&p) = parent.get(a)
            && p != a
        {
            let gp = parent.get(p).copied().unwrap_or(p);
            if let Some(x) = parent.get_mut(a) {
                *x = gp;
            }
            a = gp;
        }
        a
    };
    for (k, &[a, b]) in edges.iter().enumerate() {
        if skip.is_some_and(|s| s.get(k).copied().unwrap_or(false)) || a >= n || b >= n {
            continue;
        }
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb
            && let Some(x) = parent.get_mut(ra)
        {
            *x = rb;
        }
    }
    (0..n).map(|k| find(&mut parent, k)).collect()
}

/// Reads a path-data file. `heightfield`: the level's terrain, for cell heights where a cell lies on it.
///
/// # Errors
/// The file does not read, is not path data, or a sector mesh does not read.
pub fn load(path: &Path, heightfield: Option<&Heightfield>) -> Result<NavData, String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let f = hpd::parse(&data).map_err(|e| format!("{}: not Kynapse path data ({e})", path.display()))?;
    let meshes: Vec<kymesh::Mesh> = f
        .sectors
        .iter()
        .map(|s| kymesh::parse(s.mesh))
        .collect::<Result<_, _>>()
        .map_err(|e| format!("{}: a sector mesh does not read ({e:?})", path.display()))?;
    // the areas, numbered across the file
    let mut areas = Vec::new();
    let mut segs = Vec::new();
    for (s, m) in f.sectors.iter().zip(&meshes) {
        for (n, a) in m.areas.iter().enumerate() {
            let [_, tx, tz, layer] = a.ids;
            let [_, pitch, max_y, min_y] = a.values;
            areas.push(NavArea {
                sector: s.id,
                index: n,
                tile: (i64::from(tx), i64::from(tz)),
                layer: i64::from(layer),
                pitch: f64::from(pitch),
                min_y: f64::from(min_y),
                max_y: f64::from(max_y),
                outline: a.outline.iter().filter(|l| l.len() >= 2).cloned().collect(),
                portals: a.portals.iter().map(|p| (p64(p.p0), p64(p.p1), p.target)).collect(),
                borders: a.border_portals.iter().map(|b| (p64(b.p0), p64(b.p1))).collect(),
                island: -1,
                cells: 0,
                ladder: false,
            });
            segs.push(segments(a));
        }
    }
    let finite = |p: &P3| p.0.is_finite() && p.1.is_finite() && p.2.is_finite();
    if !segs.iter().flatten().all(|(a, b)| finite(a) && finite(b)) {
        return Err(format!(
            "{}: a boundary value that is not a finite number",
            path.display()
        ));
    }
    // the cells: every area's boundary sampled on the 0.8 m grid (as navdata._cells, in the Rust core)
    let mut cell_ij = Vec::new();
    let mut est = Vec::new();
    let mut cell_area = Vec::new();
    for (k, s) in segs.iter().enumerate() {
        for ((i, j), y) in bc2_navbuild::parity::area_cells(s, GRID, Y_OFFSET) {
            cell_ij.push([i, j]);
            est.push(y);
            cell_area.push(k as u32);
        }
    }
    // navdata._ground: the terrain's height where the cell lies on it (within 1.5 m of the estimate)
    let cell_y: Vec<f64> = match heightfield {
        Some(hf) => cell_ij
            .iter()
            .zip(&est)
            .map(|(c, &e)| {
                let t = hf.height(c[0] as f64 * GRID, c[1] as f64 * GRID);
                if (t - e).abs() < 1.5 { t } else { e }
            })
            .collect(),
        None => est,
    };
    let mut counts = vec![0usize; areas.len()];
    for &a in &cell_area {
        if let Some(c) = counts.get_mut(a as usize) {
            *c += 1;
        }
    }
    for (a, c) in areas.iter_mut().zip(&counts) {
        a.cells = *c;
    }
    // the route graph: border nodes repeat in both sectors with the same id - one node per id
    let mut index: HashMap<u32, usize> = HashMap::new();
    let mut node_pos: Vec<[f64; 3]> = Vec::new();
    let mut node_sector = Vec::new();
    let mut edges = Vec::new();
    let mut edge_cost = Vec::new();
    for s in &f.sectors {
        let mut local = Vec::with_capacity(s.nodes.len());
        for nd in &s.nodes {
            let k = *index.entry(nd.id).or_insert_with(|| {
                node_pos.push(p64(nd.pos));
                node_sector.push(s.id);
                node_pos.len() - 1
            });
            local.push(k);
        }
        for e in &s.edges {
            let (Some(&a), Some(&b)) = (local.get(e.src as usize), local.get(e.dst as usize)) else {
                return Err(format!("{}: a route edge to no node", path.display()));
            };
            edges.push([a, b]);
            edge_cost.push(f64::from(e.tag & 0x7FFF_FFFF) / 16.0);
        }
    }
    let n = node_pos.len();
    let roots = union_roots(n, &edges, None);
    let pos = |k: usize| node_pos.get(k).copied().unwrap_or_default();
    let ladder_edge: Vec<bool> = edges
        .iter()
        .map(|&[a, b]| {
            let (p, q) = (pos(a), pos(b));
            let (dx, dy, dz) = (q[0] - p[0], (q[1] - p[1]).abs(), q[2] - p[2]);
            let h = dx.hypot(dz);
            dy > LADDER_DY && dy > LADDER_SLOPE * h
        })
        .collect();
    let walk_root = union_roots(n, &edges, Some(&ladder_edge));
    // each island's base: its largest walking part (ground on any other is reached through a ladder)
    let mut base: HashMap<usize, usize> = HashMap::new();
    if ladder_edge.iter().any(|&l| l) {
        let mut wsize: HashMap<usize, usize> = HashMap::new();
        for &w in &walk_root {
            *wsize.entry(w).or_default() += 1;
        }
        let size = |w: usize| wsize.get(&w).copied().unwrap_or(0);
        for (&r, &w) in roots.iter().zip(&walk_root) {
            match base.get(&r) {
                Some(&b) if size(w) <= size(b) => {}
                _ => {
                    base.insert(r, w);
                }
            }
        }
    }
    // islands by size, largest first; equal sizes in the order their roots first appear (Counter.most_common)
    let mut order: Vec<usize> = Vec::new();
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for &r in &roots {
        let c = sizes.entry(r).or_insert(0);
        if *c == 0 {
            order.push(r);
        }
        *c += 1;
    }
    order.sort_by_key(|r| std::cmp::Reverse(sizes.get(r).copied().unwrap_or(0)));
    let rank: HashMap<usize, u32> = order.iter().enumerate().map(|(k, &r)| (r, k as u32)).collect();
    let node_island: Vec<u32> = roots.iter().map(|r| rank.get(r).copied().unwrap_or(0)).collect();
    // each area's island: its sector's route node nearest its cells' centre (height weighted 3 to 1)
    let mut sec_nodes: HashMap<u32, Vec<usize>> = HashMap::new();
    for (k, &s) in node_sector.iter().enumerate() {
        sec_nodes.entry(s).or_default().push(k);
    }
    let mut sums = vec![[0.0f64; 3]; areas.len()];
    for ((c, &y), &a) in cell_ij.iter().zip(&cell_y).zip(&cell_area) {
        if let Some(s) = sums.get_mut(a as usize) {
            s[0] += c[0] as f64 * GRID;
            s[2] += c[1] as f64 * GRID;
            s[1] += y;
        }
    }
    for (k, a) in areas.iter_mut().enumerate() {
        let Some(cand) = sec_nodes.get(&a.sector).filter(|c| !c.is_empty()) else {
            continue;
        };
        let cnt = counts.get(k).copied().unwrap_or(0).max(1) as f64;
        let s = sums.get(k).copied().unwrap_or_default();
        let centre = [s[0] / cnt, s[1] / cnt, s[2] / cnt];
        let mut best = (f64::INFINITY, None);
        for &kn in cand {
            let p = pos(kn);
            let d = (p[0] - centre[0]).hypot(p[2] - centre[2]) + 3.0 * (p[1] - Y_OFFSET - centre[1]).abs();
            // np.argmin: the first of the least (a NaN would win; there are none)
            if best.1.is_none() || d < best.0 {
                best = (d, Some(kn));
            }
        }
        if let Some(kn) = best.1 {
            a.island = i64::from(node_island.get(kn).copied().unwrap_or(0));
            let r = roots.get(kn).copied().unwrap_or(kn);
            a.ladder = !base.is_empty() && walk_root.get(kn) != base.get(&r);
        }
    }
    let mut island_cells: HashMap<i64, usize> = HashMap::new();
    for a in &areas {
        *island_cells.entry(a.island).or_default() += a.cells;
    }
    let mut isl_sum = vec![([0.0f64; 3], 0usize); order.len()];
    for (k, &i) in node_island.iter().enumerate() {
        if let Some(s) = isl_sum.get_mut(i as usize) {
            let p = pos(k);
            s.0[0] += p[0];
            s.0[1] += p[1];
            s.0[2] += p[2];
            s.1 += 1;
        }
    }
    let islands = isl_sum
        .iter()
        .enumerate()
        .map(|(k, (s, c))| {
            let c1 = (*c).max(1) as f64;
            Island {
                nodes: *c,
                cells: island_cells.get(&(k as i64)).copied().unwrap_or(0),
                centre: [s[0] / c1, s[1] / c1, s[2] / c1],
            }
        })
        .collect();
    Ok(NavData {
        path: path.to_owned(),
        areas,
        cell_ij,
        cell_y,
        cell_area,
        node_pos,
        node_sector,
        edges,
        edge_cost,
        node_island,
        islands,
        ladder_edge,
    })
}

/// `navdata.compare_cells`: per cell of each file, whether the other has the same ground (the same 0.8 m cell; one
/// floor on each side is the same ground, stacked floors pair by height within 4 m). (NavKit's, DICE's.)
#[must_use]
pub fn compare_cells(cand: &NavData, reference: &NavData) -> (Vec<bool>, Vec<bool>) {
    let keyed = |nav: &NavData| -> Vec<i64> {
        nav.cell_ij
            .iter()
            .map(|c| (c[0] + 50_000) * 100_000 + (c[1] + 50_000))
            .collect()
    };
    let (kc, kr) = (keyed(cand), keyed(reference));
    (
        same(&kc, &cand.cell_y, &kr, &reference.cell_y),
        same(&kr, &reference.cell_y, &kc, &cand.cell_y),
    )
}

/// For each (key, y): does the other file have the key, and (one floor each) or a floor within [`STACK_TOL`]?
fn same(keys: &[i64], y: &[f64], okeys: &[i64], oy: &[f64]) -> Vec<bool> {
    let mut mine_n: HashMap<i64, usize> = HashMap::new();
    for &k in keys {
        *mine_n.entry(k).or_default() += 1;
    }
    // the other file's cells sorted by key, then height (np.lexsort is stable)
    let mut order: Vec<usize> = (0..okeys.len()).collect();
    order.sort_by(|&a, &b| {
        let (ka, kb) = (okeys.get(a), okeys.get(b));
        ka.cmp(&kb).then_with(|| {
            let (ya, yb) = (oy.get(a).copied().unwrap_or(0.0), oy.get(b).copied().unwrap_or(0.0));
            ya.partial_cmp(&yb).unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    let sk: Vec<i64> = order.iter().map(|&k| okeys.get(k).copied().unwrap_or(0)).collect();
    let sy: Vec<f64> = order.iter().map(|&k| oy.get(k).copied().unwrap_or(0.0)).collect();
    // composite (key, height) values, sorted: the neighbours of (key, y) are the nearest floors below and above
    let comp_o: Vec<f64> = sk.iter().zip(&sy).map(|(&k, &h)| k as f64 * 1e5 + (h + 2e4)).collect();
    keys.iter()
        .zip(y)
        .map(|(&key, &h)| {
            let lo = sk.partition_point(|&k| k < key);
            let hi = sk.partition_point(|&k| k <= key);
            let other_n = hi - lo;
            if other_n == 0 {
                return false;
            }
            let mine = mine_n.get(&key).copied().unwrap_or(0);
            let comp_m = key as f64 * 1e5 + (h + 2e4);
            let p = comp_o.partition_point(|&c| c < comp_m);
            let mut best = f64::INFINITY;
            for q in [p.wrapping_sub(1), p] {
                if let (Some(&k), Some(&oyq)) = (sk.get(q), sy.get(q))
                    && k == key
                {
                    best = best.min((oyq - h).abs());
                }
            }
            (other_n == 1 && mine == 1) || best <= STACK_TOL
        })
        .collect()
}

/// The difference figures: DICE's ground NavKit reproduces, NavKit's extra ground and its ladder ground, in per cent
/// of DICE's cells.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Difference {
    /// DICE's ground NavKit also has.
    pub reproduced: f64,
    /// NavKit's ground DICE lacks (not ladder ground).
    pub extra: f64,
    /// NavKit's ladder ground.
    pub ladder: f64,
}

/// The difference figures of a comparison ([`compare_cells`]' answers).
#[must_use]
pub fn difference(navkit: &NavData, same_c: &[bool], same_r: &[bool]) -> Difference {
    let lad = navkit.ladder_cells();
    let n_ref = same_r.len().max(1) as f64;
    let reproduced = if same_r.is_empty() {
        0.0
    } else {
        // 100 x np.mean: the booleans' count over their number, then the hundredfold
        100.0 * (same_r.iter().filter(|&&s| s).count() as f64 / same_r.len() as f64)
    };
    let extra = same_c.iter().zip(&lad).filter(|(s, l)| !**s && !**l).count() as f64;
    let ladder = lad.iter().filter(|&&l| l).count() as f64;
    Difference {
        reproduced,
        extra: 100.0 * extra / n_ref,
        ladder: 100.0 * ladder / n_ref,
    }
}

/// Python's float floor division `a // b` (`float_floor_div`: fmod, then the quotient's floor, rounded as Python
/// rounds it) - the 10 m buckets' keys.
#[must_use]
pub fn py_floordiv(a: f64, b: f64) -> f64 {
    let m = a % b;
    let mut div = (a - m) / b;
    // a remainder of the other sign than the divisor: one step down (the remainder itself is not needed)
    if m != 0.0 && (b < 0.0) != (m < 0.0) {
        div -= 1.0;
    }
    if div == 0.0 {
        return 0.0f64.copysign(a / b);
    }
    let mut f = div.floor();
    if div - f > 0.5 {
        f += 1.0;
    }
    f
}

fn bucket(x: f64) -> i64 {
    py_floordiv(x, 10.0) as i64
}

/// One soldier spawn's route node: its island and the horizontal distance (within 15 m and 4 m in height), or no
/// node near enough (`None`, the distance) - the Studio's `spawn_islands`.
#[must_use]
pub fn spawn_island(nav: &NavData, p: [f64; 3]) -> (Option<u32>, Option<f64>) {
    if nav.node_pos.is_empty() {
        return (None, None);
    }
    let mut best = (f64::INFINITY, 0usize);
    for (k, g) in nav.node_pos.iter().enumerate() {
        let gy = g[1] - Y_OFFSET;
        let d = if (gy - p[1]).abs() <= JOIN_Y {
            (g[0] - p[0]).hypot(g[2] - p[2])
        } else {
            1e9
        };
        if k == 0 || d < best.0 {
            best = (d, k);
        }
    }
    let isl = (best.0 <= JOIN_XZ).then(|| nav.node_island.get(best.1).copied().unwrap_or(0));
    (isl, Some(best.0))
}

/// Per island (the first `limit`): the horizontal distance from its nodes to the main network's nearest node (0 for
/// the main network) - the Studio's `island_gaps`, on a 10 m grid of the main network's nodes.
#[must_use]
pub fn island_gaps(nav: &NavData, limit: usize) -> Vec<f64> {
    let main: Vec<[f64; 2]> = if nav.islands.is_empty() {
        Vec::new()
    } else {
        nav.node_pos
            .iter()
            .zip(&nav.node_island)
            .filter(|(_, i)| **i == 0)
            .map(|(p, _)| [p[0], p[2]])
            .collect()
    };
    let mut grid: HashMap<(i64, i64), Vec<[f64; 2]>> = HashMap::new();
    for p in &main {
        grid.entry((bucket(p[0]), bucket(p[1]))).or_default().push(*p);
    }
    let mut members: Vec<Vec<[f64; 2]>> = vec![Vec::new(); nav.islands.len().min(limit)];
    for (p, &i) in nav.node_pos.iter().zip(&nav.node_island) {
        if let Some(m) = members.get_mut(i as usize) {
            m.push([p[0], p[2]]);
        }
    }
    let mut out = vec![0.0];
    for pts in members.iter().skip(1) {
        let mut best = f64::INFINITY;
        for r in [1i64, 3, 10, 40, 1000] {
            for p in pts {
                let (cx, cz) = (bucket(p[0]), bucket(p[1]));
                let mut consider = |q: &[f64; 2]| best = best.min((q[0] - p[0]).hypot(q[1] - p[1]));
                if r < 40 {
                    for x in cx - r..=cx + r {
                        for z in cz - r..=cz + r {
                            for q in grid.get(&(x, z)).into_iter().flatten() {
                                consider(q);
                            }
                        }
                    }
                } else {
                    for q in &main {
                        consider(q);
                    }
                }
            }
            if best.is_finite() {
                break;
            }
        }
        out.push(if best.is_finite() { best } else { 0.0 });
    }
    out.truncate(nav.islands.len().clamp(1, limit));
    out
}

/// The path test's answer for one file (`navpath.Route`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Route {
    /// A route was found.
    pub found: bool,
    /// Its nodes, start to end.
    pub nodes: Vec<usize>,
    /// The sum of its edges' stored costs (m).
    pub cost: f64,
    /// The node chain's length (m).
    pub length: f64,
    /// Start to end, straight (m).
    pub straight: f64,
    /// The start's route node.
    pub start_node: Option<usize>,
    /// The end's route node.
    pub end_node: Option<usize>,
    /// Metres from the start to its node.
    pub start_join: f64,
    /// Metres from the end to its node.
    pub end_join: f64,
    /// The start's island.
    pub start_island: Option<u32>,
    /// The end's island.
    pub end_island: Option<u32>,
    /// Where the two islands come closest: a node of each and the distance.
    pub gap: Option<(usize, usize, f64)>,
    /// Why there is no route.
    pub reason: String,
}

/// `navpath.join`: the route node nearest a point within 15 m horizontally and 4 m in height, and the distance.
#[must_use]
pub fn join(nav: &NavData, p: [f64; 3]) -> (Option<usize>, f64) {
    if nav.node_pos.is_empty() {
        return (None, f64::INFINITY);
    }
    let mut best = (f64::INFINITY, 0usize);
    for (k, g) in nav.node_pos.iter().enumerate() {
        let d = if (g[1] - Y_OFFSET - p[1]).abs() <= JOIN_Y {
            (g[0] - p[0]).hypot(g[2] - p[2])
        } else {
            f64::INFINITY
        };
        if k == 0 || d < best.0 {
            best = (d, k);
        }
    }
    if best.0 <= JOIN_XZ {
        (Some(best.1), best.0)
    } else {
        (None, best.0)
    }
}

/// The norm of a vector as `np.linalg.norm` gives it (numpy's float64 dot: x*x, then fused multiply-adds).
fn norm3(v: [f64; 3]) -> f64 {
    Norm::Fma.of(v[0], v[1], v[2])
}

/// `navpath.route`: the route from `start` to `end` over the file's route graph, or why there is none.
#[must_use]
pub fn route(nav: &NavData, graph: &Graph, start: [f64; 3], end: [f64; 3]) -> Route {
    let mut r = Route {
        straight: norm3([end[0] - start[0], end[1] - start[1], end[2] - start[2]]),
        ..Route::default()
    };
    (r.start_node, r.start_join) = join(nav, start);
    (r.end_node, r.end_join) = join(nav, end);
    let (Some(a), Some(b)) = (r.start_node, r.end_node) else {
        let (which, d) = if r.start_node.is_none() {
            ("start", r.start_join)
        } else {
            ("end", r.end_join)
        };
        r.reason = format!(
            "no route node within {JOIN_XZ:.0} m of the {which} point (nearest {:.0} m)",
            d.min(9999.0)
        );
        return r;
    };
    r.start_island = nav.node_island.get(a).copied();
    r.end_island = nav.node_island.get(b).copied();
    let found = graph.search(Norm::Fma, a, b);
    if found.nodes.is_empty() {
        if r.start_island == r.end_island {
            "same island, but no route along the edges' directions (one-way edges)".clone_into(&mut r.reason);
        } else {
            r.gap = closest_between(nav, r.start_island, r.end_island);
            let (i, j) = (r.start_island.unwrap_or(0), r.end_island.unwrap_or(0));
            r.reason = match r.gap {
                Some(g) => format!(
                    "the points are on different islands ({i} and {j}); they come closest {:.1} m apart",
                    g.2
                ),
                None => format!("the points are on different islands ({i}, {j})"),
            };
        }
        return r;
    }
    let pos = |k: usize| nav.node_pos.get(k).copied().unwrap_or_default();
    r.length = found
        .nodes
        .iter()
        .zip(found.nodes.iter().skip(1))
        .map(|(&a, &b)| {
            let (p, q) = (pos(a), pos(b));
            norm3([q[0] - p[0], q[1] - p[1], q[2] - p[2]])
        })
        .sum();
    r.nodes = found.nodes;
    r.cost = found.cost;
    r.found = true;
    r
}

/// `navpath._closest`: the closest pair of nodes of two islands (horizontal metres).
fn closest_between(nav: &NavData, ia: Option<u32>, ib: Option<u32>) -> Option<(usize, usize, f64)> {
    let of = |i: Option<u32>| -> Vec<usize> {
        nav.node_island
            .iter()
            .enumerate()
            .filter(|(_, x)| Some(**x) == i)
            .map(|(k, _)| k)
            .collect()
    };
    let (a, b) = (of(ia), of(ib));
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let pos = |k: usize| nav.node_pos.get(k).copied().unwrap_or_default();
    let mut best: Option<(usize, usize, f64)> = None;
    // in blocks of 512 of a, each block's first least (row-major), kept when strictly less than the best so far
    for chunk in a.chunks(512) {
        let mut blk: Option<(usize, usize, f64)> = None;
        for &i in chunk {
            let p = pos(i);
            for &j in &b {
                let q = pos(j);
                let d = (p[0] - q[0]).hypot(p[2] - q[2]);
                if blk.is_none_or(|x| d < x.2) {
                    blk = Some((i, j, d));
                }
            }
        }
        if let Some(x) = blk
            && best.is_none_or(|b| x.2 < b.2)
        {
            best = Some(x);
        }
    }
    best
}

/// The route graph of a file for [`route`] (`navpath._adjacency`: the edges by source, stably).
#[must_use]
pub fn graph(nav: &NavData) -> Graph {
    let mut order: Vec<usize> = (0..nav.edges.len()).collect();
    order.sort_by_key(|&k| nav.edges.get(k).map_or(0, |e| e[0]));
    let n = nav.node_pos.len();
    let mut first = vec![0usize; n + 1];
    for &k in &order {
        if let Some(e) = nav.edges.get(k)
            && let Some(f) = first.get_mut(e[0] + 1)
        {
            *f += 1;
        }
    }
    for k in 1..first.len() {
        let prev = first.get(k - 1).copied().unwrap_or(0);
        if let Some(f) = first.get_mut(k) {
            *f += prev;
        }
    }
    Graph {
        first,
        to: order.iter().map(|&k| nav.edges.get(k).map_or(0, |e| e[1])).collect(),
        cost: order
            .iter()
            .map(|&k| nav.edge_cost.get(k).copied().unwrap_or(0.0))
            .collect(),
        pos: nav.node_pos.clone(),
    }
}

/// NavKit's route cost against DICE's, in per cent (+: NavKit's is dearer), when both found a route.
#[must_use]
pub fn route_cost_change(navkit: Option<&Route>, dice: Option<&Route>) -> Option<f64> {
    let (a, b) = (navkit?, dice?);
    (a.found && b.found).then(|| 100.0 * (a.cost - b.cost) / b.cost.max(1e-9))
}

// ---- overlay.py: a level's AI layer, nearest nodes, anomalies, the compare report

/// A soldier spawner of an AI layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Spawner {
    /// Its position.
    pub pos: [f64; 3],
    /// Its name.
    pub name: String,
    /// Its `PlayerType` (`SSAiPlayer` for the bots).
    pub player_type: Option<String>,
    /// Enabled at the start.
    pub enabled: bool,
    /// Its `Team`.
    pub team: Option<String>,
    /// Its `SoldierSquadId`.
    pub squad_id: Option<String>,
    /// In a squad entity.
    pub in_squad_entity: bool,
}

/// An `AreaOrder` of an AI layer.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaOrder {
    /// Its position.
    pub pos: [f64; 3],
    /// Its name.
    pub name: String,
    /// Its `AreaType`.
    pub area_type: Option<String>,
}

/// A `PatrolOrder` of an AI layer: its waypoint chain.
#[derive(Debug, Clone, PartialEq)]
pub struct Patrol {
    /// Its name.
    pub name: String,
    /// Its waypoints.
    pub points: Vec<[f64; 3]>,
}

/// An objective object of an AI layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    /// Its position.
    pub pos: [f64; 3],
    /// `mpobjective` or `communicationstation`.
    pub kind: String,
}

/// The AI layer of a level partition (`overlay.read_ai_layer`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AiLayer {
    /// `LevelData`'s `HasAI` (`None`: no such field).
    pub has_ai: Option<bool>,
    /// The soldier spawners.
    pub spawners: Vec<Spawner>,
    /// The `AreaOrder`s.
    pub area_orders: Vec<AreaOrder>,
    /// The `PatrolOrder`s.
    pub patrols: Vec<Patrol>,
    /// The objective objects.
    pub objectives: Vec<Objective>,
}

const OBJECTIVES: [&str; 2] = ["mpobjective", "communicationstation"];

/// The first value field named `name` in an instance (document order, any depth): `<field name="..">text</field>`.
fn field(n: &Node<'_, '_>, name: &str) -> Option<String> {
    for c in n.children() {
        if c.tag() == "field"
            && c.attr("name") == Some(name)
            && c.attr("ref").is_none()
            && !c.is_container()
            && c.attributes().len() == 1
        {
            return c.value().ok();
        }
        if c.is_container()
            && let Some(v) = field(&c, name)
        {
            return Some(v);
        }
    }
    None
}

/// The ref of the first field named `name` that has one (`xmlinst.field_ref`).
fn field_ref<'t>(n: &Node<'t, '_>, name: &str) -> Option<&'t str> {
    for c in n.children() {
        if c.tag() == "field"
            && c.attr("name") == Some(name)
            && let Some(r) = c.attr("ref")
        {
            return Some(r);
        }
        if let Some(r) = field_ref(&c, name) {
            return Some(r);
        }
    }
    None
}

/// The first `<complex name="Transform">` with a value: its translation, rounded to millimetres, unless it is the
/// origin (`overlay._pos`).
fn transform_pos(n: &Node<'_, '_>) -> Option<[f64; 3]> {
    fn find(n: &Node<'_, '_>) -> Option<String> {
        for c in n.children() {
            if c.tag() == "complex" && c.attr("name") == Some("Transform") && !c.is_container() {
                return c.value().ok();
            }
            if c.is_container()
                && let Some(v) = find(&c)
            {
                return Some(v);
            }
        }
        None
    }
    let text = find(n)?;
    let v: Vec<f64> = text
        .split('/')
        .map(|s| s.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .ok()?;
    let t = [*v.get(12)?, *v.get(13)?, *v.get(14)?];
    t.iter().any(|c| c.abs() > 1e-3).then(|| t.map(round3))
}

/// Python's `round(v, 3)` (the nearest double to the correctly rounded decimal).
#[must_use]
pub fn round3(v: f64) -> f64 {
    format!("{v:.3}").parse().unwrap_or(v)
}

/// Python's `round(v, 2)`.
#[must_use]
pub fn round2(v: f64) -> f64 {
    format!("{v:.2}").parse().unwrap_or(v)
}

/// Every pure-GUID reference in an instance's fields, in document order (`ref="<36 characters>"`).
fn guid_refs<'t>(n: &Node<'t, '_>, out: &mut Vec<&'t str>) {
    for c in n.children() {
        if let Some(r) = c.attr("ref")
            && r.len() == 36
            && r.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-')
        {
            out.push(r);
        }
        guid_refs(&c, out);
    }
}

fn has_squad(n: &Node<'_, '_>) -> bool {
    n.children().any(|c| {
        (c.tag() == "array"
            && c.attr("name") == Some("Squad")
            && c.children().next().is_some_and(|i| i.tag() == "item"))
            || (c.is_container() && has_squad(&c))
    })
}

/// A level partition's AI layer (`overlay.read_ai_layer`, on the binary partition instead of its XML).
///
/// # Errors
/// The partition does not read.
pub fn read_ai_layer(partition: &[u8]) -> Result<AiLayer, String> {
    let tree = dbx::parse_tree(partition).map_err(|e| format!("the level partition does not read: {e}"))?;
    let mut by_guid: HashMap<String, usize> = HashMap::new();
    let mut inst: Vec<Node<'_, '_>> = Vec::new();
    for n in tree.instances() {
        let g = n.attr("guid").unwrap_or("").to_lowercase();
        // a later instance of a guid replaces the earlier one in its place (a dict)
        if let Some(&k) = by_guid.get(&g) {
            if let Some(x) = inst.get_mut(k) {
                *x = n;
            }
        } else {
            by_guid.insert(g, inst.len());
            inst.push(n);
        }
    }
    let mut layer = AiLayer::default();
    let mut has_ai_seen = false;
    for b in &inst {
        let ty = b.attr("type").unwrap_or("");
        let short = ty.rsplit('.').next().unwrap_or(ty);
        match short {
            "LevelData" if !has_ai_seen => {
                if let Some(v) = field(b, "HasAI") {
                    layer.has_ai = Some(v == "true");
                    has_ai_seen = true;
                }
            }
            "SoldierSpawnEntityData" => {
                if let Some(pos) = transform_pos(b) {
                    layer.spawners.push(Spawner {
                        pos,
                        name: field(b, "Name").unwrap_or_default(),
                        player_type: field(b, "PlayerType"),
                        enabled: field(b, "Enabled").as_deref() == Some("true"),
                        team: field(b, "Team"),
                        squad_id: field(b, "SoldierSquadId"),
                        in_squad_entity: has_squad(b),
                    });
                }
            }
            "AreaOrderEntityData" => {
                if let Some(pos) = transform_pos(b) {
                    layer.area_orders.push(AreaOrder {
                        pos,
                        name: field(b, "Name").unwrap_or_default(),
                        area_type: field(b, "AreaType"),
                    });
                }
            }
            "PatrolOrderEntityData" => {
                let mut refs = Vec::new();
                guid_refs(b, &mut refs);
                let mut chain = Vec::new();
                for r in refs {
                    let mut g = r.to_lowercase();
                    let mut seen = std::collections::HashSet::new();
                    while let Some(n) = by_guid.get(&g).and_then(|&k| inst.get(k))
                        && !seen.contains(&g)
                        && n.attr("type").is_some_and(|t| t.ends_with("AreaGeometryEntityData"))
                    {
                        seen.insert(g.clone());
                        if let Some(q) = transform_pos(n) {
                            chain.push(q);
                        }
                        g = match field_ref(n, "Next") {
                            Some(nx) if is_guid(nx) => nx.to_lowercase(),
                            _ => String::new(),
                        };
                    }
                    if !chain.is_empty() {
                        break;
                    }
                }
                if !chain.is_empty() {
                    layer.patrols.push(Patrol {
                        name: field(b, "Name").unwrap_or_default(),
                        points: chain,
                    });
                }
            }
            "ReferenceObjectData" => {
                let r = field_ref(b, "ReferencedObject").unwrap_or("").to_lowercase();
                if let Some(kind) = OBJECTIVES.iter().find(|k| r.contains(*k))
                    && let Some(pos) = transform_pos(b)
                {
                    layer.objectives.push(Objective {
                        pos,
                        kind: (*kind).to_owned(),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(layer)
}

/// `re.fullmatch(r'[0-9a-f-]{36}', s)`.
fn is_guid(s: &str) -> bool {
    s.len() == 36
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-')
}

/// Nearest-node and ground lookups on a file (`overlay.NavIndex`: 10 m buckets).
#[derive(Debug, Clone)]
pub struct NavIndex<'a> {
    nav: &'a NavData,
    size: HashMap<u32, usize>,
    nodes: HashMap<(i64, i64), Vec<usize>>,
    cells: HashMap<(i64, i64), Vec<usize>>,
}

/// A nearest route node: (distance, node, its island's size, on the main island).
pub type Near = (f64, usize, usize, bool);

impl<'a> NavIndex<'a> {
    /// The index of a file.
    #[must_use]
    pub fn new(nav: &'a NavData) -> Self {
        let mut size = HashMap::new();
        for &i in &nav.node_island {
            *size.entry(i).or_insert(0) += 1;
        }
        let mut nodes: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (k, p) in nav.node_pos.iter().enumerate() {
            nodes.entry((bucket(p[0]), bucket(p[2]))).or_default().push(k);
        }
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for k in 0..nav.cell_ij.len() {
            let c = nav.cell_xz(k);
            cells.entry((bucket(c[0]), bucket(c[1]))).or_default().push(k);
        }
        Self {
            nav,
            size,
            nodes,
            cells,
        }
    }

    fn near(grid: &HashMap<(i64, i64), Vec<usize>>, x: f64, z: f64, r: f64) -> impl Iterator<Item = usize> + '_ {
        let (cx, cz) = (bucket(x), bucket(z));
        let n = (r / 10.0).ceil() as i64;
        (cx - n..=cx + n)
            .flat_map(move |i| (cz - n..=cz + n).map(move |j| (i, j)))
            .flat_map(move |key| grid.get(&key).into_iter().flatten().copied())
    }

    /// The nearest same-floor route node within `r` (beyond 200 m: all nodes at once), or `None`.
    #[must_use]
    pub fn nearest_node(&self, p: [f64; 3], r: f64, exclude: Option<usize>) -> Option<Near> {
        let ground = |k: usize| self.nav.node_pos.get(k).map_or(0.0, |q| q[1] - Y_OFFSET);
        let pos = |k: usize| self.nav.node_pos.get(k).copied().unwrap_or_default();
        let mut best: Option<(f64, usize)> = None;
        if r > 200.0 {
            for k in 0..self.nav.node_pos.len() {
                if Some(k) == exclude || (ground(k) - p[1]).abs() > SAME_FLOOR {
                    continue;
                }
                let q = pos(k);
                let d = (q[0] - p[0]).hypot(q[2] - p[2]);
                if best.is_none_or(|b| d < b.0) {
                    best = Some((d, k));
                }
            }
            best = best.filter(|b| b.0 <= r);
        } else {
            for k in Self::near(&self.nodes, p[0], p[2], r) {
                if Some(k) == exclude || (ground(k) - p[1]).abs() > SAME_FLOOR {
                    continue;
                }
                let q = pos(k);
                let d = (q[0] - p[0]).hypot(q[2] - p[2]);
                if d <= r && best.is_none_or(|b| d < b.0) {
                    best = Some((d, k));
                }
            }
        }
        let (d, k) = best?;
        let isl = self.nav.node_island.get(k).copied().unwrap_or(0);
        Some((round2(d), k, self.size.get(&isl).copied().unwrap_or(0), isl == 0))
    }

    /// Whether the file has walkable ground at a point (a cell within 0.6 m, 1.5 m in height).
    #[must_use]
    pub fn has_ground(&self, p: [f64; 3]) -> bool {
        Self::near(&self.cells, p[0], p[2], 0.6).any(|k| {
            let c = self.nav.cell_xz(k);
            (c[0] - p[0]).hypot(c[1] - p[2]) <= 0.6
                && (self.nav.cell_y.get(k).copied().unwrap_or(0.0) - p[1]).abs() < 1.5
        })
    }
}

/// An anomaly of NavKit's build (a row of `overlay.anomalies`).
#[derive(Debug, Clone, PartialEq)]
pub enum Anomaly {
    /// An island under 25 nodes.
    SmallIsland {
        /// The island.
        island: usize,
        /// Its centre (rounded to cm).
        pos: [f64; 3],
        /// Its nodes.
        nodes: usize,
        /// Its cells.
        cells: usize,
    },
    /// A spawner whose nearest node is on a small island.
    SpawnerOnSmallIsland {
        /// The spawner's position.
        pos: [f64; 3],
        /// Its name.
        name: String,
        /// Its `PlayerType`.
        player_type: Option<String>,
        /// Enabled.
        enabled: bool,
        /// The island's size.
        island_size: usize,
        /// The node's distance.
        node_m: f64,
    },
    /// An `AreaOrder` or patrol waypoint with no node within 15 m on its floor.
    OrderWithoutNode {
        /// `AreaOrder` or `PatrolWaypoint`.
        kind: &'static str,
        /// The order's name.
        name: String,
        /// Its position.
        pos: [f64; 3],
    },
    /// An area under 5 cells with no portal or border portal.
    SliverArea {
        /// The area.
        area: usize,
        /// Its sector.
        sector: u32,
        /// Its cells.
        cells: usize,
        /// Its cells' centre (rounded to cm).
        pos: [f64; 3],
    },
    /// A node with no same-floor node within 12 m.
    LonelyNode {
        /// The node.
        node: usize,
        /// Its position (rounded to cm).
        pos: [f64; 3],
    },
}

impl Anomaly {
    /// Where it is (on the ground).
    #[must_use]
    pub fn pos(&self) -> [f64; 3] {
        match self {
            Self::SmallIsland { pos, .. }
            | Self::SpawnerOnSmallIsland { pos, .. }
            | Self::OrderWithoutNode { pos, .. }
            | Self::SliverArea { pos, .. }
            | Self::LonelyNode { pos, .. } => *pos,
        }
    }
}

/// The anomaly overlay's classes, in the Studio's order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Anomalies {
    /// Islands under 25 nodes.
    pub small_islands: Vec<Anomaly>,
    /// Spawners on them.
    pub spawners_on_small_islands: Vec<Anomaly>,
    /// Orders without a node.
    pub orders_without_node: Vec<Anomaly>,
    /// Sliver areas.
    pub sliver_areas: Vec<Anomaly>,
    /// Lonely nodes.
    pub lonely_nodes: Vec<Anomaly>,
}

/// `overlay.anomalies`: the anomalies of a file against the level's own AI layer.
#[must_use]
pub fn anomalies(nav: &NavData, layer: &AiLayer) -> Anomalies {
    let idx = NavIndex::new(nav);
    let mut out = Anomalies::default();
    for (k, isl) in nav.islands.iter().enumerate() {
        if isl.nodes < SMALL_ISLAND {
            out.small_islands.push(Anomaly::SmallIsland {
                island: k,
                pos: isl.centre.map(round2),
                nodes: isl.nodes,
                cells: isl.cells,
            });
        }
    }
    for s in &layer.spawners {
        if let Some(n) = idx.nearest_node(s.pos, R_GATHER, None)
            && n.2 < SMALL_ISLAND
        {
            out.spawners_on_small_islands.push(Anomaly::SpawnerOnSmallIsland {
                pos: s.pos,
                name: s.name.clone(),
                player_type: s.player_type.clone(),
                enabled: s.enabled,
                island_size: n.2,
                node_m: n.0,
            });
        }
    }
    let points = layer.area_orders.iter().map(|o| ("AreaOrder", &o.name, o.pos)).chain(
        layer
            .patrols
            .iter()
            .flat_map(|p| p.points.iter().map(move |q| ("PatrolWaypoint", &p.name, *q))),
    );
    for (kind, name, p) in points {
        if idx.nearest_node(p, R_GATHER, None).is_none() {
            out.orders_without_node.push(Anomaly::OrderWithoutNode {
                kind,
                name: name.clone(),
                pos: p,
            });
        }
    }
    let mut sums: Vec<([f64; 3], usize)> = vec![([0.0; 3], 0); nav.areas.len()];
    for (k, &a) in nav.cell_area.iter().enumerate() {
        if let Some(s) = sums.get_mut(a as usize) {
            let c = nav.cell_xz(k);
            s.0[0] += c[0];
            s.0[2] += c[1];
            s.0[1] += nav.cell_y.get(k).copied().unwrap_or(0.0);
            s.1 += 1;
        }
    }
    for (k, a) in nav.areas.iter().enumerate() {
        if a.cells < SLIVER_CELLS && a.portals.is_empty() && a.borders.is_empty() {
            let (s, c) = sums.get(k).copied().unwrap_or_default();
            let pos = if c > 0 {
                let n = c as f64;
                [round2(s[0] / n), round2(s[1] / n), round2(s[2] / n)]
            } else {
                [0.0; 3]
            };
            out.sliver_areas.push(Anomaly::SliverArea {
                area: k,
                sector: a.sector,
                cells: a.cells,
                pos,
            });
        }
    }
    for (k, p) in nav.node_pos.iter().enumerate() {
        if idx
            .nearest_node([p[0], p[1] - Y_OFFSET, p[2]], LONELY_NODE, Some(k))
            .is_none()
        {
            out.lonely_nodes.push(Anomaly::LonelyNode {
                node: k,
                pos: p.map(round2),
            });
        }
    }
    out
}

/// A compare row's lookups on the target's NavKit build (`overlay._node_row`): walkable ground there, the nearest
/// same-floor node's distance, its island's size and whether it is the main one (`None` without a build).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NodeRow {
    /// Walkable ground at the point.
    pub navkit_ground: Option<bool>,
    /// The nearest node within 15 m.
    pub nearest_node_m: Option<f64>,
    /// Its island's size.
    pub node_island_size: Option<usize>,
    /// On the main island.
    pub node_on_main_island: Option<bool>,
}

/// The lookups at a point.
#[must_use]
pub fn node_row(idx: Option<&NavIndex<'_>>, p: [f64; 3]) -> NodeRow {
    let Some(idx) = idx else {
        return NodeRow::default();
    };
    let n = idx.nearest_node(p, R_GATHER, None);
    NodeRow {
        navkit_ground: Some(idx.has_ground(p)),
        nearest_node_m: n.map(|n| n.0),
        node_island_size: n.map(|n| n.2),
        node_on_main_island: n.map(|n| n.3),
    }
}

/// `overlay.sector_offsets`: a level's `SectorOffsets` asset (`Precomputed/Infantry/SectorOffsets.xml` beside its
/// partition), `None` when it has none.
#[must_use]
pub fn sector_offsets(level_dir: &Path) -> Option<(i64, i64)> {
    for kind in ["Precomputed", "precomputed"] {
        let p = level_dir.join(kind).join("Infantry").join("SectorOffsets.xml");
        if let Ok(t) = std::fs::read_to_string(&p) {
            let num = |name: &str| -> Option<i64> {
                let at = t.find(&format!("name=\"{name}\">"))? + name.len() + 8;
                let rest = t.get(at..)?;
                let end = rest.find('<')?;
                rest.get(..end)?.parse().ok()
            };
            return num("XOffset").zip(num("ZOffset"));
        }
    }
    None
}

/// Path data's default files for a level (the Studio's `default_files`): NavKit's newest build (the newest of
/// `work\navkit\<lv>\pathdata.hpd`, `work\navkit\regress\<lv>.hpd`, `work\navbuild\<lv>_W.hpd`) and DICE's original
/// (the game copy's installed file, or the earliest OPFOR backup when an OPFOR file is installed).
#[must_use]
pub fn default_files(opfor: &Path, lv: &str) -> (Option<PathBuf>, Option<PathBuf>) {
    let lv = lv.to_lowercase();
    let work = opfor.join("work");
    let builds = [
        work.join("navkit").join(&lv).join("pathdata.hpd"),
        work.join("navkit").join("regress").join(format!("{lv}.hpd")),
        work.join("navbuild").join(format!("{lv}_W.hpd")),
    ];
    let newest = builds
        .into_iter()
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.modified().ok()?, p)))
        .fold(
            None,
            |best: Option<(std::time::SystemTime, PathBuf)>, (t, p)| match best {
                Some((bt, _)) if bt >= t => best,
                _ => Some((t, p)),
            },
        )
        .map(|(_, p)| p);
    (newest, original_file(&installed_pathdata(opfor, &lv)))
}

/// Where the game reads a level's infantry path data.
#[must_use]
pub fn installed_pathdata(opfor: &Path, lv: &str) -> PathBuf {
    opfor
        .join("game")
        .join("Output")
        .join("win32")
        .join("ai")
        .join("levels")
        .join(lv.to_lowercase())
        .join("infantry")
        .join("pathdata.hpd")
}

/// `navkit.original_file`: DICE's original of a game file. From the game copy's install ledger when an owner holds
/// the file there (round DF2: the bottom layer's prior, kept once by its sha256; None where an install put the file
/// where there was none); else by the markers of before the ledger - the earliest backup an OPFOR stage kept
/// (`<file>.opfor-orig` or `<file>.opfor-<stage>-orig`, the oldest first), else the file itself unless an OPFOR
/// stage wrote it where there was no original (`.opfor-new`, or a `-applied` marker without its backup).
#[must_use]
pub fn original_file(p: &Path) -> Option<PathBuf> {
    if let Some(game) = bc2_formats::gameledger::find_game(p) {
        let led = bc2_formats::gameledger::Ledger::new(&game);
        let rel = p.strip_prefix(&game).ok()?.to_string_lossy().into_owned();
        if !led.holders(&rel).ok()?.is_empty() {
            return led.original(&rel).ok()?;
        }
    }
    let d = p.parent()?;
    let base = p.file_name()?.to_string_lossy().to_lowercase();
    let names: HashMap<String, String> = std::fs::read_dir(d)
        .ok()?
        .flatten()
        .map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            (n.to_lowercase(), n)
        })
        .collect();
    let pre = format!("{base}.opfor-");
    let ours: Vec<&String> = names.keys().filter(|f| f.starts_with(&pre)).collect();
    let has = |s: &str| ours.iter().any(|f| f.as_str() == s);
    if has(&format!("{pre}new")) {
        let own = format!("{pre}orig");
        return has(&own).then(|| d.join(names.get(&own).map_or(own.as_str(), String::as_str)));
    }
    if ours
        .iter()
        .any(|f| f.ends_with("-applied") && !has(&format!("{}-orig", &f[..f.len() - "-applied".len()])))
    {
        return None;
    }
    let mut backups: Vec<(std::time::SystemTime, bool, PathBuf)> = ours
        .iter()
        .filter(|f| f.ends_with("-orig"))
        .filter_map(|f| {
            let path = d.join(names.get(*f)?);
            let t = std::fs::metadata(&path)
                .ok()
                .filter(std::fs::Metadata::is_file)?
                .modified()
                .ok()?;
            Some((t, !f.ends_with(".opfor-orig"), path))
        })
        .collect();
    backups.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    if let Some((_, _, b)) = backups.into_iter().next() {
        return Some(b);
    }
    p.is_file().then(|| p.to_owned())
}
