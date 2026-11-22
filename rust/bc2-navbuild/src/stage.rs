//! NavKit's build after walkability (round NK5): `navbuild.stage` - sector discovery, the areas, the border-node
//! runs, the seam nodes carried from another stage, the one-way portal pieces, the detail pass with its glue (the
//! cells by grid position, `eligible` from the portal sides, the outline sides moved to D / F, the D sides that block
//! route edges), the traced boundaries, the meshes' assembly (area values, tile items, rectangle, neighbour links,
//! bounds) and the route graphs with their ids - and `navbuild.merge_detailed`, the second pass at pitch 0.2 merged
//! with the normal build. [`crate::build`] runs them from a level's inputs to the file.
//!
//! **The same output as the Python it replaces** (`rust/BENCHMARKS.md` "NK5"): every regression level's file and log
//! lines byte for byte. The order the Python's dicts and sets gave is kept where it reaches the output: the sectors
//! sorted, the areas by (tile x, tile z, lowest height) with a stable sort, the border runs in window order, an area's
//! cells by grid position in first-seen order (a dict), the node ids in the sectors' order. Where the Python iterated
//! a set (the detailed sectors of `merge_detailed`, the broken sides), only membership or a sum reached the output.
//! The floats are the Python's: the same `f64` operations in the same order (`int * float` as `float(int) * float`,
//! `math.hypot` as [`crate::pymath::hypot`], `round()` half to even).

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::struct_excessive_bools,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::items_after_statements,
    clippy::manual_midpoint,
    clippy::format_push_string,
    clippy::type_complexity,
    clippy::single_match_else,
    clippy::option_if_let_else,
    clippy::map_unwrap_or,
    reason = "the reference's single-letter names (x, y, i, j, k, g ...) and its long step-by-step functions kept for               side-by-side reading; the settings are the module's switches; indexes into lists made beside them;               == and (a + b) / 2.0 as the Python computes them"
)]

use std::collections::{BTreeMap, BTreeSet, HashSet};

use bc2_formats::kymesh;
use rayon::prelude::*;

use crate::detail::{self, Candidate, DetailSettings, GridCell, Side};
use crate::graphfile::{self, Border, RouteNode, SectorGraph};
use crate::sector::{self, GraphCfg, GraphJob};
use crate::trace::{self, BorderRun, EdgeKey, EdgeRows, Emitted, Kind, OneWay, P3, TraceCfg};
use crate::{Cell, FastMap, Level, NavError, Rule, pymath};

/// `ONE_WAY_PORTALS`: what becomes of a portal piece that cannot have a piece back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OneWayRule {
    /// `'drop'`: always outline.
    Drop,
    /// `'keep'`: always a one-way portal.
    Keep,
    /// `'restore'`: outline unless its two areas are joined through nothing else level-wide.
    Restore,
}

impl OneWayRule {
    /// The setting's name (`'drop'`, `'keep'`, `'restore'`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            OneWayRule::Drop => "drop",
            OneWayRule::Keep => "keep",
            OneWayRule::Restore => "restore",
        }
    }

    /// The rule of a name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<OneWayRule> {
        match name {
            "drop" => Some(OneWayRule::Drop),
            "keep" => Some(OneWayRule::Keep),
            "restore" => Some(OneWayRule::Restore),
            _ => None,
        }
    }
}

/// `navbuild.BuildSettings`: what one build reads (the normal infantry build by default).
#[derive(Debug, Clone, PartialEq)]
pub struct BuildSettings {
    /// The sample grid's pitch (m).
    pub pitch: f64,
    /// Samples per sector side (100 m / pitch).
    pub sector_samples: i64,
    /// A sector window's local sample range (start, stop).
    pub window: (i64, i64),
    /// Samples per tile.
    pub tile: i64,
    /// Tiles per axis.
    pub tiles: i64,
    /// The mesh's resolution.
    pub resolution: f64,
    /// The raw area link step (m).
    pub link_step: f64,
    /// The entity radius (m).
    pub radius: f64,
    /// `DistEdgeMax` (m).
    pub dist_edge_max: f64,
    /// Outline and node height over the ground (m).
    pub y_offset: f64,
    /// An area's height range below its lowest ground.
    pub area_y_min: f64,
    /// ... and above its highest.
    pub area_y_max: f64,
    /// Border node spacing along a run (m).
    pub border_node_spacing: f64,
    /// `node_cover['normal']`.
    pub node_cover_normal: Option<f64>,
    /// `node_cover['detailed']`.
    pub node_cover_detailed: Option<f64>,
    /// Order nodes at AI order positions.
    pub order_nodes: bool,
    /// Ground outside DICE's extent left out (with bounds).
    pub clip_to_reference: bool,
    /// `one_way_portals`.
    pub one_way_portals: OneWayRule,
    /// D lines and F cells.
    pub detail_features: bool,
    /// `detail_step['normal']`.
    pub detail_step_normal: f64,
    /// `detail_step['detailed']`.
    pub detail_step_detailed: f64,
    /// Route edges do not cross a D side.
    pub detail_blocks_edges: bool,
    /// Sector ids tiled at 20 m (an experiment), or `None`.
    pub tile_20m_sectors: Option<BTreeSet<u32>>,
}

/// The files' null offset (`kymesh.NULL`).
const MESH_NULL: u32 = 0xFFFF_FFFF;

/// `DETAILED_PITCH`.
pub const DETAILED_PITCH: f64 = 0.2;
/// `DETAILED_LINK_STEP`.
pub const DETAILED_LINK_STEP: f64 = 0.3;
/// `CARRY_RADIUS`.
pub const CARRY_RADIUS: f64 = 1.0;

impl Default for BuildSettings {
    fn default() -> Self {
        BuildSettings {
            pitch: 0.8,
            sector_samples: 125,
            window: (-1, 127),
            tile: 50,
            tiles: 3,
            resolution: 0.025,
            link_step: 0.6,
            radius: 0.4,
            dist_edge_max: 15.0,
            y_offset: 0.95,
            area_y_min: 0.0,
            area_y_max: 1.52,
            border_node_spacing: 10.0,
            node_cover_normal: None,
            node_cover_detailed: Some(4.0),
            order_nodes: true,
            clip_to_reference: true,
            one_way_portals: OneWayRule::Drop,
            detail_features: true,
            detail_step_normal: 0.6,
            detail_step_detailed: 0.175,
            detail_blocks_edges: true,
            tile_20m_sectors: None,
        }
    }
}

impl BuildSettings {
    /// These settings with the grid of pitch p (`grid_settings`).
    #[must_use]
    pub fn at_pitch(&self, p: f64) -> BuildSettings {
        let sector_samples = (100.0 / p).round_ties_even() as i64;
        let detailed = (p - DETAILED_PITCH).abs() < 1e-6;
        BuildSettings {
            pitch: p,
            sector_samples,
            window: (-1, sector_samples + 2),
            tiles: if detailed { 6 } else { 3 },
            tile: ((if detailed { 20.0 } else { 40.0 }) / p).round_ties_even() as i64,
            resolution: if detailed { 0.05 } else { 0.025 },
            link_step: if detailed { DETAILED_LINK_STEP } else { 0.6 },
            ..self.clone()
        }
    }

    /// The settings of the detailed-zone pass.
    #[must_use]
    pub fn detailed(&self) -> BuildSettings {
        self.at_pitch(DETAILED_PITCH)
    }

    /// At pitch 0.2 (`kind == 'detailed'`).
    #[must_use]
    pub fn is_detailed(&self) -> bool {
        (self.pitch - DETAILED_PITCH).abs() < 1e-6
    }

    fn detail_step(&self) -> f64 {
        if self.is_detailed() {
            self.detail_step_detailed
        } else {
            self.detail_step_normal
        }
    }

    fn node_cover(&self) -> Option<f64> {
        if self.is_detailed() {
            self.node_cover_detailed
        } else {
            self.node_cover_normal
        }
    }

    /// (samples per tile, tiles per axis) of a sector (`tiling`).
    fn tiling(&self, sid: u32) -> (i64, i64) {
        if self.tile_20m_sectors.as_ref().is_some_and(|s| s.contains(&sid)) {
            (25, 6)
        } else {
            (self.tile, self.tiles)
        }
    }
}

/// `GABRIEL_EDGES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gabriel {
    /// In pitch-0.2 sectors.
    Detailed,
    /// Everywhere.
    All,
    /// Nowhere.
    Off,
}

/// The module's switches (`navbuild`'s, `navislands`', `navdetail`'s and `navwalk`'s names the stage reads; the
/// tests switch them): their defaults are the shipped values.
#[derive(Debug, Clone, PartialEq)]
pub struct Switches {
    /// `AREA_LINK`.
    pub area_link: Rule,
    /// `LINK_RULE`.
    pub link_rule: Rule,
    /// `SYMMETRIC_PORTALS`.
    pub symmetric_portals: bool,
    /// `EDGE_SECTOR_RULE == 'grid_min'` (else `'no_neighbour'`).
    pub edge_rule_grid_min: bool,
    /// `EDGE_SECTOR_TILES`.
    pub edge_sector_tiles: bool,
    /// `MAX_AREA` when `BISECT_AREAS`, else `None`.
    pub max_area: Option<i64>,
    /// `GABRIEL_EDGES`.
    pub gabriel: Gabriel,
    /// `SORT_NODES_BY_ID`.
    pub sort_nodes_by_id: bool,
    /// `BORDER_OWNER`.
    pub border_owner: bool,
    /// Edges only out of the border copies a sector owns (`graphfile::Layout::owner_edges`; HPD-NODE-5, KYG-EDGE-4):
    /// off, until each sector line has owned border nodes on both sides (KYG-NODE-2).
    pub owner_edges: bool,
    /// The sector table's bounds as the box of the sector's nodes (`graphfile::Layout::node_bounds`; KYN-SEC-2).
    pub node_bounds: bool,
    /// A border portal's run cut at the sector square's lines, its pieces beyond them linked to the diagonal
    /// neighbour whose square holds them (KYN-LINK-1).
    pub diagonal_links: bool,
    /// `ORDER_NODE_MIN`.
    pub order_node_min: f64,
    /// `ORDER_MIN_ISLAND`.
    pub order_min_island: usize,
    /// `DETAIL_HALF_CELLS`.
    pub detail_half_cells: bool,
    /// `DETAIL_IN_DETAILED`.
    pub detail_in_detailed: bool,
    /// `DETAIL_F` (normal, detailed).
    pub detail_f: (bool, bool),
    /// `DETAIL_TERRAIN_ONLY` (normal, detailed).
    pub detail_terrain_only: (bool, bool),
    /// `navdetail.CUT_CORNERS`.
    pub cut_corners: bool,
    /// `navdetail.CHAIN_TURNS`.
    pub chain_turns: bool,
    /// `navislands`' rule when `SHADOW_ISLANDS`, else `None`.
    pub shadow: Option<graphfile::Shadow>,
    /// `HEADER`.
    pub header: [u32; bc2_formats::hpd::HEADER_WORDS],
    /// `navwalk.ROOF_PRUNE` (the stage's log line).
    pub roof_prune: bool,
    /// `navwalk.OBJECTIVE_FLOOR` (the stage's log line).
    pub objective_floor: bool,
}

/// `HEADER`: the path data's 36 header words.
pub const HEADER: [u32; bc2_formats::hpd::HEADER_WORDS] = {
    let mut h = [0u32; bc2_formats::hpd::HEADER_WORDS];
    let head = [2, 2, 0, 1, 0, 0, 0, 0, 2, 0, 8, 0];
    let mut i = 0;
    while i < head.len() {
        h[i] = head[i];
        i += 1;
    }
    h[32] = 36;
    h[33] = 12;
    h[34] = 1;
    h
};

impl Default for Switches {
    fn default() -> Self {
        Switches {
            area_link: Rule::Terrain,
            link_rule: Rule::Slope,
            symmetric_portals: true,
            edge_rule_grid_min: true,
            edge_sector_tiles: true,
            max_area: None,
            gabriel: Gabriel::Detailed,
            sort_nodes_by_id: true,
            border_owner: true,
            owner_edges: false,
            node_bounds: true,
            diagonal_links: true,
            order_node_min: 3.0,
            order_min_island: 25,
            detail_half_cells: false,
            detail_in_detailed: false,
            detail_f: (true, false),
            detail_terrain_only: (false, true),
            cut_corners: true,
            chain_turns: true,
            shadow: Some(graphfile::Shadow::default()),
            header: HEADER,
            roof_prune: true,
            objective_floor: true,
        }
    }
}

/// The counts a build reports (`ORDER_IDS`, `ORDER_STATS`, `LADDER_STATS`, `DETAIL_STATS`, `navislands.TERRAIN_IDS`).
#[derive(Debug, Clone, Default)]
pub struct BuildStats {
    /// The order nodes' ids.
    pub order_ids: HashSet<u32>,
    /// Order nodes added.
    pub order_added: usize,
    /// Ladder links made (per stage).
    pub ladders_linked: usize,
    /// Ladders whose ends lie in two sectors.
    pub ladders_split: usize,
    /// D lines.
    pub detail_d: usize,
    /// F cells.
    pub detail_f: usize,
    /// Outline sides moved to D / F.
    pub outline_moved: usize,
    /// Broken sides outline on one cell only.
    pub outline_one_sided: usize,
    /// Route nodes standing on terrain.
    pub terrain_ids: HashSet<u32>,
}

/// The sample grid a stage runs on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridInfo {
    /// Columns along x.
    pub nx: usize,
    /// Columns along z.
    pub nz: usize,
    /// The grid's minimum x (as stored).
    pub min_x: f64,
    /// ... and z.
    pub min_z: f64,
    /// Its pitch.
    pub pitch: f64,
}

/// A stage's ground: its level (kept surfaces, gradients, terrain at the stage's pitch and link step) on its grid,
/// with what walkability reported for the log line.
#[derive(Debug, Clone)]
pub struct Ground {
    /// The level.
    pub level: Level,
    /// Its grid.
    pub grid: GridInfo,
    /// The kept ladders' (foot, top) surfaces.
    pub ladder_pairs: Vec<(usize, usize)>,
    /// `(roof islands dropped, surfaces)` when walkability ran its roof rule (`None`: a ready grid).
    pub roof: Option<(usize, usize)>,
    /// `(objective points, surfaces)` when walkability ran (`None`: a ready grid).
    pub objective: Option<(usize, usize)>,
}

/// What a stage takes besides its ground.
#[derive(Debug, Clone, Default)]
pub struct StageArgs {
    /// The level's sector offsets.
    pub xoff: i64,
    /// See `xoff`.
    pub zoff: i64,
    /// The AI order points.
    pub order_points: Vec<P3>,
    /// Build only these sectors.
    pub sector_filter: Option<BTreeSet<(i64, i64)>>,
    /// The level's minimum sector X / Y when the stage holds only part of it.
    pub grid_min: Option<(i64, i64)>,
    /// The ids start after this.
    pub id_start: u32,
    /// Shared nodes from another stage on this stage's sectors.
    pub carried: BTreeMap<(i64, i64), Vec<(u32, P3)>>,
}

/// One sector of a stage (`info[(X, Y)]`).
#[derive(Debug, Clone)]
pub struct SectorInfo {
    /// (X, Y).
    pub xy: (i64, i64),
    /// Its areas' cells, in the stage's order.
    pub cells: Vec<Vec<Cell>>,
    /// Per area its tile (tx, tz) and layer.
    pub tiles: Vec<[u32; 3]>,
    /// (gi0, gj0, ox, oz).
    pub origin: (i64, i64, f64, f64),
    /// Where the tiles start (local i, j).
    pub start: (i64, i64),
    /// Per area its lowest and highest ground.
    pub ylims: Vec<(f64, f64)>,
    /// Each cell's surface to its area (the last area holding it).
    pub area_of: FastMap<usize, usize>,
    /// Per area its border portals' runs.
    pub borders: Vec<Vec<BorderRun>>,
}

/// One sector's graph and mesh (`(sid, bounds, mesh, pts, pairs, origin)`).
#[derive(Debug, Clone)]
pub struct StageGraph {
    /// The sector id.
    pub sid: u32,
    /// minX, maxX, minY, maxY, minZ, maxZ.
    pub bounds: [f64; 6],
    /// The runtime mesh.
    pub mesh: kymesh::Mesh,
    /// Route nodes: (id, position, surface, local i, local j).
    pub nodes: Vec<(u32, P3, usize, i64, i64)>,
    /// Route pairs (node, node, length).
    pub pairs: Vec<(usize, usize, f64)>,
    /// The sector's origin (ox, oz).
    pub origin: (f64, f64),
}

/// A border node on a sector: (id, position, surface).
pub type BorderNode = (u32, P3, usize);

/// A stage's result (`{'level', 'info', 'graphs', 'border_nodes', 'next_id', 'dropped', 'pitch', 'sectors'}`).
#[derive(Debug)]
pub struct Stage {
    /// Its ground.
    pub ground: Ground,
    /// Its sectors, sorted.
    pub sectors: Vec<SectorInfo>,
    /// Per sector (the same order) its graph.
    pub graphs: Vec<StageGraph>,
    /// Per sector its border nodes.
    pub border_nodes: BTreeMap<(i64, i64), Vec<BorderNode>>,
    /// The last id given.
    pub next_id: u32,
    /// Carried nodes with no ground here.
    pub dropped: BTreeSet<u32>,
    /// The stage's pitch.
    pub pitch: f64,
    /// The sector square's samples across when its border runs are cut at the square's lines and their pieces beyond
    /// link to the diagonal neighbours (`Switches::diagonal_links`).
    pub diagonal_square: Option<i64>,
    /// Per sector (the same order) its D sides as surface pairs, the route edges' blocked steps
    /// (`DETAIL_BLOCKS_EDGES`; the tests read them).
    pub blocked: Vec<HashSet<(usize, usize)>>,
}

impl Stage {
    /// The index of sector xy in `sectors`.
    #[must_use]
    pub fn index(&self, xy: (i64, i64)) -> Option<usize> {
        self.sectors.binary_search_by(|s| s.xy.cmp(&xy)).ok()
    }

    /// The sectors as a set.
    #[must_use]
    pub fn sector_set(&self) -> BTreeSet<(i64, i64)> {
        self.sectors.iter().map(|s| s.xy).collect()
    }
}

/// A stage's log: one line at a time, with the seconds since the build's start.
pub trait Log {
    /// One line.
    fn line(&mut self, text: &str);
    /// Seconds since the build started (`time.time() - t0`).
    fn seconds(&self) -> f64;
    /// Whether the build should stop (round NK6: the editor's Cancel), checked between its steps.
    fn cancelled(&self) -> bool {
        false
    }
}

/// Python's `repr(float)`.
#[must_use]
pub fn py_repr(v: f64) -> String {
    if v.is_nan() {
        return "nan".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let a = v.abs();
    if a != 0.0 && !(1e-4..1e16).contains(&a) {
        let e = format!("{v:e}");
        let (m, x) = e.split_once('e').unwrap_or((&e, "0"));
        let x: i32 = x.parse().unwrap_or(0);
        return format!("{m}e{}{:02}", if x < 0 { '-' } else { '+' }, x.abs());
    }
    if v == v.trunc() {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

/// Python's `format(v, 'g')`.
#[must_use]
pub fn py_g(v: f64) -> String {
    if !v.is_finite() {
        return py_repr(v);
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0".into() } else { "0".into() };
    }
    let e = format!("{v:.5e}");
    let (m, x) = e.split_once('e').unwrap_or((&e, "0"));
    let x: i32 = x.parse().unwrap_or(0);
    let strip = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s.to_owned()
        }
    };
    if !(-4..6).contains(&x) {
        return format!("{}e{}{:02}", strip(m), if x < 0 { '-' } else { '+' }, x.abs());
    }
    let dec = usize::try_from(5 - x).unwrap_or(0);
    strip(&format!("{v:.dec$}"))
}

fn shape(msg: impl Into<String>) -> NavError {
    NavError::Shape(msg.into())
}

/// The sector id of (X, Y).
#[must_use]
pub fn sector_id(x: i64, y: i64) -> u32 {
    (u32::try_from(x & 0xFF).unwrap_or(0) << 8) | u32::try_from(y & 0xFF).unwrap_or(0)
}

/// `column_sectors`: the sector (offsets applied) of sample column c, as DICE's path data places it.
fn column_sector(g: &GridInfo, c: usize, xoff: i64, zoff: i64, pitch: f64) -> (i64, i64) {
    let x = g.min_x + (c % g.nx.max(1)) as f64 * pitch;
    let z = g.min_z + (c / g.nx.max(1)) as f64 * pitch;
    ((x / 100.0).floor() as i64 - xoff, (z / 100.0).floor() as i64 - zoff)
}

/// `sector_origin_index`: the global sample index of the sector's local (0, 0) and its origin in metres.
fn sector_origin(g: &GridInfo, x: i64, y: i64, xoff: i64, zoff: i64, pitch: f64) -> (i64, i64, f64, f64) {
    let (ox, oz) = ((x + xoff) as f64 * 100.0, (y + zoff) as f64 * 100.0);
    (
        ((ox - g.min_x) / pitch).round_ties_even() as i64,
        ((oz - g.min_z) / pitch).round_ties_even() as i64,
        ox,
        oz,
    )
}

/// min / max of an area's cells' heights as Python's builtins take them (the first of equals).
fn ylims(level: &Level, cells: &[Cell]) -> (f64, f64) {
    let mut ys = cells.iter().map(|c| level.y(c.2));
    let first = ys.next().unwrap_or(f64::NAN);
    ys.fold((first, first), |(lo, hi), v| {
        (if v < lo { v } else { lo }, if v > hi { v } else { hi })
    })
}

/// `drop_one_way`: each one-way candidate not in `keep` becomes outline on its source area.
///
/// # Errors
/// [`NavError::Shape`] for a candidate of an area that is not there.
pub fn drop_one_way(per_area: &mut [EdgeRows], one_way: &[OneWay], keep: &BTreeSet<usize>) -> Result<(), NavError> {
    for (n, ow) in one_way.iter().enumerate() {
        if keep.contains(&n) {
            continue;
        }
        let se = per_area
            .get_mut(ow.src)
            .ok_or_else(|| shape("a one-way candidate of an area that is not there"))?;
        let rows = se.get(&ow.key).cloned().unwrap_or_default();
        let (ys, rest): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| r.0 == ow.along);
        se.insert(ow.key, rest);
        let outline = EdgeKey {
            kind: Kind::Outline,
            ..ow.key
        };
        se.entry(outline, Vec::new).extend(ys);
    }
    Ok(())
}

/// `restore_one_way`: which one-way candidates to keep (`ONE_WAY_PORTALS` `'restore'`), per sector.
fn restore_one_way(sectors: &[SectorInfo], edges: &[(Vec<EdgeRows>, Vec<OneWay>)]) -> Vec<BTreeSet<usize>> {
    let mut ids: FastMap<(usize, usize), usize> = FastMap::default();
    let mut parent: Vec<usize> = Vec::new();
    let mut id = |x: (usize, usize), parent: &mut Vec<usize>| -> usize {
        *ids.entry(x).or_insert_with(|| {
            parent.push(parent.len());
            parent.len() - 1
        })
    };
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while let Some(&p) = parent.get(x) {
            if p == x {
                break;
            }
            let gp = parent.get(p).copied().unwrap_or(p);
            if let Some(slot) = parent.get_mut(x) {
                *slot = gp;
            }
            x = gp;
        }
        x
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra != rb
            && let Some(slot) = parent.get_mut(ra)
        {
            *slot = rb;
        }
    }
    for (s, (per_area, one_way)) in edges.iter().enumerate() {
        let cand: HashSet<(usize, EdgeKey, i64)> = one_way.iter().map(|o| (o.src, o.key, o.along)).collect();
        for (ai, rows) in per_area.iter().enumerate() {
            let a = id((s, ai), &mut parent);
            for (key, r) in rows.iter() {
                if let Kind::Portal(t) = key.kind
                    && r.iter().any(|x| !cand.contains(&(ai, *key, x.0)))
                {
                    let b = id((s, t), &mut parent);
                    union(&mut parent, a, b);
                }
            }
        }
    }
    // areas joined across sectors: one surface in both (the sectors' overlap); the union order does not matter
    let mut owners: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
    for (s, inf) in sectors.iter().enumerate() {
        for (ai, cells) in inf.cells.iter().enumerate() {
            for c in cells {
                if inf.area_of.get(&c.2) == Some(&ai) {
                    let o = owners.entry(c.2).or_default();
                    if !o.contains(&(s, ai)) {
                        o.push((s, ai));
                    }
                }
            }
        }
    }
    for o in owners.values() {
        if let Some((&first, rest)) = o.split_first() {
            for &x in rest {
                let (a, b) = (id(first, &mut parent), id(x, &mut parent));
                union(&mut parent, a, b);
            }
        }
    }
    let mut keep = vec![BTreeSet::new(); edges.len()];
    let mut by_pair: BTreeMap<(usize, usize, usize), Vec<usize>> = BTreeMap::new();
    for (s, (_, one_way)) in edges.iter().enumerate() {
        for (n, o) in one_way.iter().enumerate() {
            by_pair.entry((s, o.src, o.target)).or_default().push(n);
        }
    }
    // sorted(by_pair.items()): by sector (the stage's sorted order), source, target
    for ((s, src, t), ns) in by_pair {
        let (a, b) = (id((s, src), &mut parent), id((s, t), &mut parent));
        if find(&mut parent, a) != find(&mut parent, b) {
            if let Some(k) = keep.get_mut(s) {
                k.extend(ns);
            }
            union(&mut parent, a, b);
        }
    }
    keep
}

/// One sector's detail pass (`detail_pass`): per area (D lines, F records), its D sides as surface pairs, the
/// counts; the edge rows' broken outline sides taken out.
#[derive(Debug, Clone)]
pub struct Detailed {
    /// Per area its D lines and F records.
    pub features: Vec<(Vec<Vec<detail::Point>>, Vec<detail::FCell>)>,
    /// The D sides as surface pairs (`level.blocked`).
    pub blocked: HashSet<(usize, usize)>,
    /// D lines.
    pub d: usize,
    /// F records.
    pub f: usize,
    /// Outline sides moved to D / F (in pairs).
    pub moved: usize,
    /// Broken sides that were outline on one cell only.
    pub one_sided: usize,
}

/// A side's key in the edge rows: (x axis, doubled line, dir, along).
fn side_key(i: i64, j: i64, di: i64, dj: i64) -> (bool, i64, i64, i64) {
    if di == 0 {
        (false, 2 * j + dj, dj, i)
    } else {
        (true, 2 * i + di, di, j)
    }
}

/// `half_cells`: the 45-degree bridges' side cells of an area (`DETAIL_HALF_CELLS`).
#[allow(clippy::type_complexity)]
fn half_cells(level: &Level, cells: &[Cell], gi0: i64, gj0: i64) -> (Vec<(Side, f64)>, Vec<(GridCell, usize)>) {
    let mut cell_k: trace::Ordered<GridCell, usize> = trace::Ordered::default();
    for &(i, j, k) in cells {
        cell_k.insert((i, j), k);
    }
    let mut half: trace::Ordered<Side, f64> = trace::Ordered::default();
    let mut ks: trace::Ordered<GridCell, usize> = trace::Ordered::default();
    for (&(i, j), &k) in cell_k.iter() {
        for (di, dj) in [(1, 1), (1, -1)] {
            let Some(&kb) = cell_k.get(&(i + di, j + dj)) else {
                continue;
            };
            if cell_k.get(&(i + di, j)).is_some() || cell_k.get(&(i, j + dj)).is_some() {
                continue;
            }
            let y0 = (level.y(k) + level.y(kb)) / 2.0;
            for ((xi, xj), legs) in [((i, j + dj), [(0, -dj), (di, 0)]), ((i + di, j), [(-di, 0), (0, dj)])] {
                let col = level.column_surfaces(gi0 + xi, gj0 + xj);
                let mut best: Option<(f64, usize)> = None;
                for q in col {
                    let d = (level.y(q) - y0).abs();
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, q));
                    }
                }
                let Some((_, kx)) = best else { continue };
                if (level.y(kx) - y0).abs() > 1.0 {
                    continue;
                }
                ks.insert((xi, xj), kx);
                for leg in legs {
                    half.insert(((xi, xj), leg), level.y(kx));
                }
            }
        }
    }
    (
        half.iter().map(|(k, v)| (*k, *v)).collect(),
        ks.iter().map(|(k, v)| (*k, *v)).collect(),
    )
}

/// `detail_pass` for one sector's areas (`rows` its edge rows; the broken sides taken out of them; `origin` the
/// sector's (gi0, gj0, ox, oz)).
#[must_use]
pub fn detail_pass(
    level: &Level,
    cells_of: &[Vec<Cell>],
    rows: &mut [EdgeRows],
    origin: (i64, i64, f64, f64),
    cfg: &BuildSettings,
    sw: &Switches,
) -> Detailed {
    let detailed = cfg.is_detailed();
    let mut out = Detailed {
        features: vec![(Vec::new(), Vec::new()); cells_of.len()],
        blocked: HashSet::new(),
        d: 0,
        f: 0,
        moved: 0,
        one_sided: 0,
    };
    if detailed && !sw.detail_in_detailed {
        return out;
    }
    let step = cfg.detail_step();
    let (gi0, gj0, ox, oz) = origin;
    let s = DetailSettings {
        pitch: cfg.pitch,
        ox,
        oz,
        step,
        with_f: if detailed { sw.detail_f.1 } else { sw.detail_f.0 },
        cut_corners: sw.cut_corners,
        chain_turns: sw.chain_turns,
    };
    let terrain_only = if detailed {
        sw.detail_terrain_only.1
    } else {
        sw.detail_terrain_only.0
    };
    for (ai, (cells, edges)) in cells_of.iter().zip(rows.iter_mut()).enumerate() {
        // the cells' dict {(i, j): y} and {(i, j): k}: first place, last value
        let mut at: trace::Ordered<GridCell, usize> = trace::Ordered::default();
        for &(i, j, k) in cells {
            at.insert((i, j), k);
        }
        let area: Vec<(GridCell, f64)> = at.iter().map(|(c, &k)| (*c, level.y(k))).collect();
        // cheap first: no two side-by-side cells more than the step apart, no broken side (the Python's candidates)
        let (half, ks) = if sw.detail_half_cells {
            let (h, hks) = half_cells(level, cells, gi0, gj0);
            let mut ks = at.clone();
            for (c, k) in hks {
                if ks.get(&c).is_none() {
                    ks.insert(c, k);
                }
            }
            (h, ks)
        } else {
            let any = area.iter().any(|&((i, j), y)| {
                [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(di, dj)| {
                    at.get(&(i + di, j + dj))
                        .is_some_and(|&n| (level.y(n) - y).abs() > step)
                })
            });
            if !any {
                continue;
            }
            (Vec::new(), at.clone())
        };
        let linked: HashSet<(bool, i64, i64, i64)> = edges
            .iter()
            .filter(|(k, _)| matches!(k.kind, Kind::Portal(_) | Kind::Border(..)))
            .flat_map(|(k, r)| r.iter().map(move |x| (k.x_axis, k.line2, k.dir, x.0)))
            .collect();
        let breaks = |a: GridCell, b: GridCell| -> bool {
            !terrain_only
                || match (ks.get(&a), ks.get(&b)) {
                    (Some(&ka), Some(&kb)) => level.terrain(ka) && level.terrain(kb),
                    _ => false,
                }
        };
        let got = detail::detail_features(&area, &half, &s, |c| -> Result<bool, std::convert::Infallible> {
            Ok(match *c {
                Candidate::Side(((i, j), (di, dj))) => {
                    let (x, l, d, al) = side_key(i, j, di, dj);
                    !linked.contains(&(x, l, d, al))
                        && !linked.contains(&(x, l, -d, al))
                        && breaks((i, j), (i + di, j + dj))
                }
                Candidate::Leg(((i, j), (di, dj))) => breaks((i, j), (i + di, j + dj)),
            })
        });
        let Ok(feats) = got;
        if !feats.broken.is_empty() && cfg.detail_blocks_edges {
            for &((i, j), (di, dj)) in &feats.d_sides {
                if let (Some(&a), Some(&b)) = (ks.get(&(i, j)), ks.get(&(i + di, j + dj))) {
                    out.blocked.insert((a, b));
                }
            }
        }
        let broken: Vec<Side> = feats
            .broken
            .iter()
            .copied()
            .filter(|&((i, j), (di, dj))| at.get(&(i, j)).is_some() && at.get(&(i + di, j + dj)).is_some())
            .collect();
        if !broken.is_empty() {
            let e_at: HashSet<(bool, i64, i64, i64)> = edges
                .iter()
                .filter(|(k, _)| k.kind == Kind::Outline)
                .flat_map(|(k, r)| r.iter().map(move |x| (k.x_axis, k.line2, k.dir, x.0)))
                .collect();
            let mut gone: BTreeMap<(bool, i64, i64), HashSet<i64>> = BTreeMap::new();
            for ((i, j), (di, dj)) in broken {
                let (x, l, d, al) = side_key(i, j, di, dj);
                if e_at.contains(&(x, l, d, al)) && e_at.contains(&(x, l, -d, al)) {
                    gone.entry((x, l, d)).or_default().insert(al);
                } else if e_at.contains(&(x, l, d, al)) {
                    out.one_sided += 1;
                }
            }
            #[allow(clippy::iter_over_hash_type, reason = "a count, and rows filtered by membership")]
            for ((x_axis, line2, dir), alongs) in gone {
                let key = EdgeKey {
                    kind: Kind::Outline,
                    x_axis,
                    line2,
                    dir,
                };
                if let Some(r) = edges.get_mut(&key) {
                    let before = r.len();
                    r.retain(|x| !alongs.contains(&x.0));
                    out.moved += before - r.len();
                }
            }
        }
        out.d += feats.lines.len();
        out.f += feats.cells.len();
        if let Some(slot) = out.features.get_mut(ai) {
            *slot = (feats.lines, feats.cells);
        }
    }
    out
}

/// The sectors one stage's `sector_links` looks in on a grid: each one's index in the stage and its areas' (tile x,
/// tile z, layer) (`link_scope`).
pub type Scope = BTreeMap<(i64, i64), usize>;

/// The other grid of a mixed seam (`link_across`).
pub struct Across<'a> {
    /// That stage.
    pub stage: &'a Stage,
    /// Its sectors looked in.
    pub scope: &'a Scope,
    /// This stage's pitch.
    pub p_here: f64,
}

/// `sector_links` for sector `s` of `here`: on this grid for a sector in `same`, else on `across`'s grid.
#[must_use]
pub fn sector_links(
    here: &Stage,
    s: usize,
    portals: &[usize],
    rule: Rule,
    same: &Scope,
    across: Option<&Across<'_>>,
) -> Vec<kymesh::NeighbourLink> {
    let Some(inf) = here.sectors.get(s) else {
        return Vec::new();
    };
    let level = &here.ground.level;
    let borders: Vec<Border> = inf
        .borders
        .iter()
        .enumerate()
        .flat_map(|(ai, runs)| {
            runs.iter().enumerate().map(move |(ci, b)| Border {
                area: ai,
                index: ci,
                di: b.dir.0,
                dj: b.dir.1,
                along: here
                    .diagonal_square
                    .map_or(0, |sq| i64::from(b.first >= sq) - i64::from(b.last < 0)),
                t0: b.first,
                t1: b.last,
                line: b.line,
            })
        })
        .collect();
    let (gi0, gj0, ox, oz) = inf.origin;
    graphfile::sector_links(inf.xy.0, inf.xy.1, &borders, portals, |other, border| {
        let (ci, cj) = border.inner_cell();
        let inner = inf
            .cells
            .get(border.area)?
            .iter()
            .find(|c| c.0 == ci && c.1 == cj)
            .map(|c| c.2);
        if let Some(&n_other) = same.get(&other) {
            let o = here.sectors.get(n_other)?;
            let k = level.neighbour(inner?, gi0 + ci, gj0 + cj, border.di, border.dj, None, rule)?;
            let area = *o.area_of.get(&k)?;
            return o.tiles.get(area).copied();
        }
        let acr = across?;
        let &n_other = acr.scope.get(&other)?;
        let o = acr.stage.sectors.get(n_other)?;
        let inner = inner?;
        let (p_here, p_there) = (acr.p_here, acr.stage.pitch);
        let step = p_here.max(p_there);
        let (wx, wz) = (
            ox + ci as f64 * p_here + border.di as f64 * step,
            oz + cj as f64 * p_here + border.dj as f64 * step,
        );
        let wy = level.y(inner);
        let g = &acr.stage.ground.grid;
        let (gi, gj) = (
            ((wx - g.min_x) / p_there).round_ties_even() as i64,
            ((wz - g.min_z) / p_there).round_ties_even() as i64,
        );
        let olv = &acr.stage.ground.level;
        let mut best: Option<(f64, usize)> = None;
        for m in olv.surfaces(gi, gj) {
            let dy = (olv.y(m) - wy).abs();
            if dy <= 0.8 && best.is_none_or(|(d, _)| dy < d) && o.area_of.contains_key(&m) {
                best = Some((dy, m));
            }
        }
        let (_, nearest) = best?;
        let area = *o.area_of.get(&nearest)?;
        o.tiles.get(area).copied()
    })
}

fn p3f(p: P3) -> kymesh::Vec3 {
    [p.0 as f32, p.1 as f32, p.2 as f32]
}

fn ptf(p: detail::Point) -> kymesh::Vec3 {
    [p[0] as f32, p[1] as f32, p[2] as f32]
}

/// One grid's build (`navbuild.stage`): its sectors, areas, border nodes, traced boundaries, meshes and route graphs,
/// from its ground.
///
/// # Errors
/// [`NavError::Shape`] with the reference's words where it raised (no ground kept, a sector off the 256 x 256 grid).
#[allow(clippy::too_many_lines)]
pub fn stage(
    ground: Ground,
    args: &StageArgs,
    cfg: &BuildSettings,
    sw: &Switches,
    stats: &mut BuildStats,
    log: &mut dyn Log,
) -> Result<Stage, NavError> {
    let level = &ground.level;
    let g = ground.grid;
    let (xoff, zoff) = (args.xoff, args.zoff);
    let kept = level.keep().iter().filter(|&&b| b).count();
    let mut line = format!(
        "{} surfaces, {kept} walkable kept ({:.0} s)",
        level.len(),
        log.seconds()
    );
    if sw.roof_prune
        && let Some((d, n)) = ground.roof
    {
        line.push_str(&format!("; roof islands dropped {d} ({n} surfaces)"));
    }
    if sw.objective_floor
        && let Some((p, n)) = ground.objective
    {
        line.push_str(&format!("; objective floors {p} ({n} surfaces)"));
    }
    log.line(&line);

    // the sectors that hold kept surfaces
    let mut found: BTreeSet<(i64, i64)> = BTreeSet::new();
    let mut last_col = None;
    for (k, &kp) in level.keep().iter().enumerate() {
        if !kp {
            continue;
        }
        let c = level.column_of(k);
        if last_col == Some(c) {
            continue;
        }
        last_col = Some(c);
        found.insert(column_sector(&g, c, xoff, zoff, cfg.pitch));
    }
    if let Some(f) = &args.sector_filter {
        found.retain(|s| f.contains(s));
    }
    let sectors: Vec<(i64, i64)> = found.into_iter().collect();
    if sectors.is_empty() && args.sector_filter.is_none() {
        return Err(NavError::Build(
            "no walkable ground was kept (the samples hold none inside the box): check the box and the samples".into(),
        ));
    }
    let bad: Vec<&(i64, i64)> = sectors
        .iter()
        .filter(|(x, y)| !((0..=255).contains(x) && (0..=255).contains(y)))
        .collect();
    if let Some(first) = bad.first() {
        return Err(NavError::Build(format!(
            "{} sector(s) outside the 256 x 256 grid (e.g. ({}, {}), offsets {xoff}, {zoff}): the box reaches below \
             the sector offsets - run prepare again after widening the box",
            bad.len(),
            first.0,
            first.1
        )));
    }
    log.line(&format!("{} sectors (pitch {})", sectors.len(), py_g(cfg.pitch)));

    // pass 1: areas per sector (in parallel)
    let (min_xs, min_zs) = args.grid_min.unwrap_or_else(|| {
        (
            sectors.iter().map(|s| s.0).min().unwrap_or(0),
            sectors.iter().map(|s| s.1).min().unwrap_or(0),
        )
    });
    let have: BTreeSet<(i64, i64)> = sectors.iter().copied().collect();
    let specs: Vec<((i64, i64, f64, f64), (i64, i64), (i64, i64))> = sectors
        .iter()
        .map(|&(x, y)| {
            let o = sector_origin(&g, x, y, xoff, zoff, cfg.pitch);
            let edge = if sw.edge_rule_grid_min {
                (min_xs == x, min_zs == y)
            } else {
                (!have.contains(&(x - 1, y)), !have.contains(&(x, y - 1)))
            };
            let st = |e: bool| if sw.edge_sector_tiles && e { 0 } else { -1 };
            (o, (st(edge.0), st(edge.1)), cfg.tiling(sector_id(x, y)))
        })
        .collect();
    let built = specs
        .par_iter()
        .map(|&((gi0, gj0, _, _), start, (tile, tiles))| {
            level.build_areas(gi0, gj0, cfg.window, start, tile, tiles, sw.max_area, sw.area_link)
        })
        .collect::<Result<Vec<_>, NavError>>()?;
    let mut infos: Vec<SectorInfo> = Vec::with_capacity(sectors.len());
    for ((&xy, &(origin, start, _)), areas) in sectors.iter().zip(&specs).zip(built) {
        let lims: Vec<(f64, f64)> = areas.iter().map(|(_, c)| ylims(level, c)).collect();
        let mut order: Vec<usize> = (0..areas.len()).collect();
        // tiles column-major, then the lowest ground (a stable sort; floats as Python compares them)
        order.sort_by(|&a, &b| {
            let (ta, tb) = (areas[a].0, areas[b].0);
            ta.cmp(&tb)
                .then_with(|| lims[a].0.partial_cmp(&lims[b].0).unwrap_or(std::cmp::Ordering::Equal))
        });
        let mut areas: Vec<Option<((i64, i64), Vec<Cell>)>> = areas.into_iter().map(Some).collect();
        let mut cells = Vec::with_capacity(order.len());
        let mut tiles = Vec::with_capacity(order.len());
        let mut per_tile: BTreeMap<(i64, i64), u32> = BTreeMap::new();
        let mut yl = Vec::with_capacity(order.len());
        for &q in &order {
            let Some((t, c)) = areas.get_mut(q).and_then(Option::take) else {
                continue;
            };
            let layer = per_tile.entry(t).or_insert(0);
            tiles.push([u32::try_from(t.0).unwrap_or(0), u32::try_from(t.1).unwrap_or(0), *layer]);
            *layer += 1;
            cells.push(c);
            yl.push(lims[q]);
        }
        let area_of = trace::area_of(&cells);
        infos.push(SectorInfo {
            xy,
            borders: vec![Vec::new(); cells.len()],
            cells,
            tiles,
            origin,
            start,
            ylims: yl,
            area_of,
        });
    }
    let n_areas: usize = infos.iter().map(|i| i.cells.len()).sum();
    log.line(&format!("{n_areas} areas ({:.0} s)", log.seconds()));

    // border nodes shared between sector pairs (global ids), on the overlap lines
    let mut next_id = args.id_start;
    let mut border_nodes: BTreeMap<(i64, i64), Vec<BorderNode>> = BTreeMap::new();
    let mut border_at: FastMap<usize, (u32, P3)> = FastMap::default();
    let line_at = cfg.sector_samples + 1;
    for inf in &infos {
        let (x, y) = inf.xy;
        let (gi0, gj0, ox, oz) = inf.origin;
        for (dx, dy) in [(1, 0), (0, 1)] {
            let other = (x + dx, y + dy);
            if !have.contains(&other) {
                continue;
            }
            let (di, dj) = if dx != 0 { (0, 1) } else { (1, 0) };
            let mut runs: Vec<Vec<(i64, usize)>> = Vec::new();
            let mut active: Vec<usize> = Vec::new();
            for t in cfg.window.0..cfg.window.1 {
                let (i, j) = if dx != 0 { (line_at, t) } else { (t, line_at) };
                let mut nxt = Vec::new();
                for k in level.surfaces(gi0 + i, gj0 + j) {
                    let mut joined = None;
                    for (pos, &r) in active.iter().enumerate() {
                        let last = runs.get(r).and_then(|v| v.last()).copied();
                        if let Some((lt, lk)) = last
                            && lt == t - 1
                            && level.connected(lk, k, di, dj, sw.link_rule)
                        {
                            joined = Some(pos);
                            break;
                        }
                    }
                    match joined {
                        Some(pos) => {
                            let r = active.remove(pos);
                            if let Some(v) = runs.get_mut(r) {
                                v.push((t, k));
                            }
                            nxt.push(r);
                        }
                        None => {
                            runs.push(vec![(t, k)]);
                            nxt.push(runs.len() - 1);
                        }
                    }
                }
                active = nxt;
            }
            for run in &runs {
                let len = run.len() as f64;
                let n = ((len * cfg.pitch / cfg.border_node_spacing).ceil() as i64).max(1);
                for q in 0..n {
                    let at = ((q as f64 + 0.5) * len / n as f64) as usize;
                    let Some(&(t, k)) = run.get(at) else { continue };
                    let (i, j) = if dx != 0 { (line_at, t) } else { (t, line_at) };
                    let pos = (
                        ox + i as f64 * cfg.pitch,
                        level.y(k) + cfg.y_offset,
                        oz + j as f64 * cfg.pitch,
                    );
                    if let Some(&(nid, p)) = border_at.get(&k) {
                        for sec in [(x, y), other] {
                            let list = border_nodes.entry(sec).or_default();
                            if !list.iter().any(|b| b.0 == nid) {
                                list.push((nid, p, k));
                            }
                        }
                        continue;
                    }
                    next_id += 1;
                    let nid = 0x8000_0000 | next_id;
                    border_at.insert(k, (nid, pos));
                    border_nodes.entry((x, y)).or_default().push((nid, pos, k));
                    border_nodes.entry(other).or_default().push((nid, pos, k));
                }
            }
        }
    }
    // shared nodes from another stage: on this grid's ground at the node (the nearest kept surface of the sector
    // within CARRY_RADIUS, on its floor, ground of a larger area first)
    let mut dropped = BTreeSet::new();
    let index_of = |xy: (i64, i64)| infos.binary_search_by(|s| s.xy.cmp(&xy)).ok();
    for (sec, nodes) in &args.carried {
        let Some(si) = index_of(*sec) else {
            dropped.extend(nodes.iter().map(|n| n.0));
            continue;
        };
        let Some(inf) = infos.get(si) else { continue };
        for &(nid, pos) in nodes {
            let gi = ((pos.0 - g.min_x) / cfg.pitch).round_ties_even() as i64;
            let gj = ((pos.2 - g.min_z) / cfg.pitch).round_ties_even() as i64;
            let rad = (CARRY_RADIUS / cfg.pitch).ceil() as i64;
            let mut best: Option<(bool, f64, usize)> = None;
            for di in -rad..=rad {
                for dj in -rad..=rad {
                    let dxz = pymath::hypot(di as f64, dj as f64) * cfg.pitch;
                    if dxz > CARRY_RADIUS {
                        continue;
                    }
                    for k in level.surfaces(gi + di, gj + dj) {
                        if let Some(&ai) = inf.area_of.get(&k)
                            && (level.y(k) + cfg.y_offset - pos.1).abs() <= 0.6
                        {
                            let size = inf.cells.get(ai).map_or(0, Vec::len);
                            let c = (size < 25, dxz, k);
                            let better = best.is_none_or(|b| {
                                (c.0, c.1)
                                    .partial_cmp(&(b.0, b.1))
                                    .is_some_and(std::cmp::Ordering::is_lt)
                                    || ((c.0, c.1) == (b.0, b.1) && c.2 < b.2)
                            });
                            if better {
                                best = Some(c);
                            }
                        }
                    }
                }
            }
            match best {
                Some((_, _, k)) => border_nodes.entry(*sec).or_default().push((nid, pos, k)),
                None => {
                    dropped.insert(nid);
                }
            }
        }
    }

    // boundary edges of every sector (in parallel), the one-way portal pieces decided level-wide
    let mut edges = infos
        .par_iter()
        .map(|inf| {
            let tc = TraceCfg {
                gi0: inf.origin.0,
                gj0: inf.origin.1,
                window: cfg.window,
                y_offset: cfg.y_offset,
                rule: sw.link_rule,
            };
            trace::trace_edges(level, &inf.cells, &inf.area_of, tc, sw.symmetric_portals)
        })
        .collect::<Result<Vec<_>, NavError>>()?;
    let n_cand: usize = edges.iter().map(|e| e.1.len()).sum();
    let keeps: Vec<BTreeSet<usize>> = match cfg.one_way_portals {
        OneWayRule::Restore => restore_one_way(&infos, &edges),
        OneWayRule::Keep => edges.iter().map(|e| (0..e.1.len()).collect()).collect(),
        OneWayRule::Drop => vec![BTreeSet::new(); edges.len()],
    };
    for ((per_area, one_way), keep) in edges.iter_mut().zip(&keeps) {
        drop_one_way(per_area, one_way, keep)?;
    }
    let n_kept: usize = keeps.iter().map(BTreeSet::len).sum();
    log.line(&format!(
        "one-way portal pieces: {n_cand}, kept one-way {n_kept} ({})",
        cfg.one_way_portals.name()
    ));

    // the detail features of every sector (each sector's own D sides), then the outlines (in parallel)
    let mut rows: Vec<Vec<EdgeRows>> = edges.into_iter().map(|e| e.0).collect();
    let details: Vec<Detailed> = infos
        .par_iter()
        .zip(rows.par_iter_mut())
        .map(|(inf, r)| {
            if cfg.detail_features {
                detail_pass(level, &inf.cells, r, inf.origin, cfg, sw)
            } else {
                Detailed {
                    features: vec![(Vec::new(), Vec::new()); inf.cells.len()],
                    blocked: HashSet::new(),
                    d: 0,
                    f: 0,
                    moved: 0,
                    one_sided: 0,
                }
            }
        })
        .collect();
    for d in &details {
        stats.detail_d += d.d;
        stats.detail_f += d.f;
        stats.outline_moved += d.moved;
        stats.outline_one_sided += d.one_sided;
    }
    let emitted: Vec<Vec<Emitted>> = infos
        .par_iter()
        .zip(rows.into_par_iter())
        .map(|(inf, mut r)| {
            r.truncate(inf.cells.len());
            let tc = TraceCfg {
                gi0: inf.origin.0,
                gj0: inf.origin.1,
                window: (0, 0),
                y_offset: cfg.y_offset,
                rule: sw.link_rule,
            };
            trace::trace_emit_cut(
                level,
                &inf.cells,
                r,
                tc,
                (inf.origin.2, inf.origin.3, cfg.pitch),
                sw.diagonal_links.then_some(cfg.sector_samples),
            )
        })
        .collect();
    for (inf, em) in infos.iter_mut().zip(&emitted) {
        for (b, e) in inf.borders.iter_mut().zip(em) {
            b.clone_from(&e.borders);
        }
    }

    // the meshes (their links: every sector of this stage on this grid)
    let mut st = Stage {
        ground,
        sectors: infos,
        graphs: Vec::new(),
        border_nodes,
        next_id,
        dropped,
        pitch: cfg.pitch,
        diagonal_square: sw.diagonal_links.then_some(cfg.sector_samples),
        blocked: Vec::new(),
    };
    let scope: Scope = st.sectors.iter().enumerate().map(|(n, s)| (s.xy, n)).collect();
    let mut meshes = Vec::with_capacity(st.sectors.len());
    for (n, (inf, (em, det))) in st.sectors.iter().zip(emitted.iter().zip(&details)).enumerate() {
        let sid = sector_id(inf.xy.0, inf.xy.1);
        let ntiles = cfg.tiling(sid).1;
        let nt = usize::try_from(ntiles).unwrap_or(0);
        let mut tiles: Vec<Vec<kymesh::TileItem>> = vec![Vec::new(); nt * nt];
        let mut areas = Vec::with_capacity(inf.cells.len());
        for (ai, ((tile, &(low, high)), (e, (d_lines, f_cells)))) in inf
            .tiles
            .iter()
            .zip(&inf.ylims)
            .zip(em.iter().zip(&det.features))
            .enumerate()
        {
            let (lo, hi) = (low + cfg.area_y_min, high + cfg.area_y_max);
            areas.push(kymesh::Area {
                ids: [sid, tile[0], tile[1], tile[2]],
                values: [cfg.radius as f32, cfg.pitch as f32, hi as f32, lo as f32],
                portals: e
                    .portals
                    .iter()
                    .map(|&(p0, p1, t)| kymesh::Portal {
                        p0: p3f(p0),
                        p1: p3f(p1),
                        target: t,
                    })
                    .collect(),
                border_portals: e
                    .borders
                    .iter()
                    .map(|b| kymesh::BorderPortal {
                        p0: p3f(b.p0),
                        p1: p3f(b.p1),
                        direction: [b.dir.0 as f32, 0.0, b.dir.1 as f32],
                        runtime: (MESH_NULL, MESH_NULL),
                    })
                    .collect(),
                d_lines: d_lines.iter().map(|l| l.iter().copied().map(ptf).collect()).collect(),
                outline: e.outline.iter().map(|&(a, b)| vec![p3f(a), p3f(b)]).collect(),
                cells: f_cells
                    .iter()
                    .map(|c| kymesh::Cell {
                        direction: ptf(c.direction),
                        points: c.points.iter().copied().map(ptf).collect(),
                    })
                    .collect(),
                end_row: [0.0; 3],
                null_offsets: Vec::new(),
            });
            let slot = usize::try_from(i64::from(tile[0]) * ntiles + i64::from(tile[1])).unwrap_or(usize::MAX);
            if let Some(t) = tiles.get_mut(slot) {
                t.push(kymesh::TileItem {
                    area: ai,
                    a: lo as f32,
                    b: hi as f32,
                });
            }
        }
        let portals: Vec<usize> = em.iter().map(|e| e.portals.len()).collect();
        let links = sector_links(&st, n, &portals, sw.link_rule, &scope, None);
        // the runtime's tile grid starts at the rectangle's minimum: an edge sector's at its own edge
        let (ox, oz) = (inf.origin.2, inf.origin.3);
        let rect = [
            ox + inf.start.0 as f64 * cfg.pitch,
            ox + 100.0 + 1.1 * cfg.pitch,
            oz + inf.start.1 as f64 * cfg.pitch,
            oz + 100.0 + 1.1 * cfg.pitch,
        ];
        let lys: Vec<(f64, f64)> = inf
            .ylims
            .iter()
            .map(|&(l, h)| (l + cfg.area_y_min, h + cfg.area_y_max))
            .collect();
        meshes.push((
            kymesh::Mesh {
                version: 2,
                flags: 0,
                loaded: 0,
                sector_id: sid,
                resolution: cfg.resolution as f32,
                pitch: cfg.pitch as f32,
                rect: rect.map(|v| v as f32),
                tiles_x: u32::try_from(ntiles).unwrap_or(0),
                tiles_z: u32::try_from(ntiles).unwrap_or(0),
                radius: cfg.radius as f32,
                tiles,
                areas,
                links,
                end_slot: vec![0; 4],
                links_null_offset: Some(MESH_NULL),
            },
            rect,
            lys,
        ));
    }

    // the route graphs: every sector at once (in parallel); the node ids given in the sectors' order
    let gcfg = GraphCfg {
        pitch: cfg.pitch,
        y_offset: cfg.y_offset,
        window: cfg.window,
        dist_edge_max: cfg.dist_edge_max,
        rule: sw.link_rule,
        cover: cfg.node_cover().filter(|&c| c != 0.0),
        gabriel: sw.gabriel == Gabriel::All || (sw.gabriel == Gabriel::Detailed && cfg.is_detailed()),
        order_nodes: cfg.order_nodes,
        order_node_min: sw.order_node_min,
        grid_min: (g.min_x, g.min_z),
    };
    let level = &st.ground.level;
    let mut jobs = Vec::with_capacity(st.sectors.len());
    for ((inf, em), det) in st.sectors.iter().zip(&emitted).zip(&details) {
        let (gi0, gj0, ox, oz) = inf.origin;
        let orders: Vec<P3> = args
            .order_points
            .iter()
            .copied()
            .filter(|p| ox <= p.0 && p.0 < ox + 100.0 && oz <= p.2 && p.2 < oz + 100.0)
            .collect();
        let mut lads = Vec::new();
        for &(f, t) in &st.ground.ladder_pairs {
            let (fi, ti) = (inf.area_of.contains_key(&f), inf.area_of.contains_key(&t));
            if fi && ti {
                lads.push((f, t));
            }
            if fi != ti {
                stats.ladders_split += 1;
            }
        }
        stats.ladders_linked += lads.len();
        let mids = em
            .iter()
            .enumerate()
            .map(|(ai, e)| {
                e.portals
                    .iter()
                    .filter(|p| p.2 >= ai)
                    .map(|&(p0, p1, _)| ((p0.0 + p1.0) / 2.0, (p0.2 + p1.2) / 2.0))
                    .collect()
            })
            .collect();
        let borders = st
            .border_nodes
            .get(&inf.xy)
            .map(|b| b.iter().map(|&(_, p, k)| (p, k)).collect())
            .unwrap_or_default();
        jobs.push(GraphJob {
            gi0,
            gj0,
            origin: (ox, oz),
            mids,
            borders,
            orders,
            ladders: lads,
            blocked: det.blocked.clone(),
        });
    }
    let got = st
        .sectors
        .par_iter()
        .zip(jobs.par_iter())
        .map(|(inf, job)| sector::build_graph(level, &inf.cells, job, &gcfg))
        .collect::<Result<Vec<_>, NavError>>()?;
    let mut graphs = Vec::with_capacity(got.len());
    let mut next_id = st.next_id;
    for (((inf, job), gr), (mesh, rect, lys)) in st.sectors.iter().zip(&jobs).zip(got).zip(meshes) {
        let bn = st.border_nodes.get(&inf.xy).map_or(&[][..], Vec::as_slice);
        let mut nodes = Vec::with_capacity(gr.nodes.len());
        let mut n_orders = 0;
        for n in &gr.nodes {
            let id = match n.border {
                Some(b) => bn.get(b).map_or(0, |x| x.0),
                None => {
                    next_id += 1;
                    next_id
                }
            };
            if n.order {
                stats.order_ids.insert(id);
                n_orders += 1;
            }
            if level.terrain(n.node.k) {
                stats.terrain_ids.insert(id);
            }
            nodes.push((id, n.node.pos, n.node.k, n.node.cell.0, n.node.cell.1));
        }
        if cfg.order_nodes && !job.orders.is_empty() {
            stats.order_added += n_orders;
        }
        // min(ys) / max(ys) over [min_y ...] + [max_y ...] (the first of equals)
        let mut it = lys.iter().map(|v| v.0).chain(lys.iter().map(|v| v.1));
        let Some(first) = it.next() else {
            return Err(shape("min() arg is an empty sequence"));
        };
        let (lo, hi) = it.fold((first, first), |(lo, hi), v| {
            (if v < lo { v } else { lo }, if v > hi { v } else { hi })
        });
        graphs.push(StageGraph {
            sid: mesh.sector_id,
            bounds: [rect[0], rect[1], lo, hi, rect[2], rect[3]],
            mesh,
            nodes,
            pairs: gr.pairs,
            origin: (inf.origin.2, inf.origin.3),
        });
    }
    st.next_id = next_id;
    st.graphs = graphs;
    st.blocked = jobs.into_iter().map(|j| j.blocked).collect();
    Ok(st)
}

/// The detailed-zone pass's inputs (`detailed`): its ground at pitch 0.2 (made by the caller with the seed points
/// [`detailed_seeds`] gives) and the sectors to build there.
pub struct DetailedPass {
    /// The sectors chosen (`detailed['sectors']`).
    pub sectors: BTreeSet<(i64, i64)>,
}

/// `merge_detailed`'s seed points: stage a's kept ground in the detailed sectors `dset` (x, ground y, z), in surface
/// order.
#[must_use]
pub fn detailed_seeds(a: &Stage, dset: &BTreeSet<(i64, i64)>, xoff: i64, zoff: i64) -> Vec<P3> {
    let lv = &a.ground.level;
    let g = &a.ground.grid;
    lv.keep()
        .iter()
        .enumerate()
        .filter(|&(_, &k)| k)
        .filter_map(|(k, _)| {
            let c = lv.column_of(k);
            dset.contains(&column_sector(g, c, xoff, zoff, g.pitch)).then(|| {
                (
                    g.min_x + (c % g.nx.max(1)) as f64 * g.pitch,
                    lv.y(k),
                    g.min_z + (c / g.nx.max(1)) as f64 * g.pitch,
                )
            })
        })
        .collect()
}

/// `merge_detailed`'s shared nodes on the seams between a detailed and a normal sector: per detailed sector (of
/// `dset` that `a` holds), a's border nodes there that a sector outside `dset` holds too; and each node's sectors.
#[must_use]
pub fn carried_nodes(
    a: &Stage,
    dset: &BTreeSet<(i64, i64)>,
) -> (
    BTreeMap<(i64, i64), Vec<(u32, P3)>>,
    BTreeMap<u32, BTreeSet<(i64, i64)>>,
) {
    let mut owners: BTreeMap<u32, BTreeSet<(i64, i64)>> = BTreeMap::new();
    for (sec, nodes) in &a.border_nodes {
        for n in nodes {
            owners.entry(n.0).or_default().insert(*sec);
        }
    }
    let mut carried = BTreeMap::new();
    for sec in dset {
        for &(nid, pos, _) in a.border_nodes.get(sec).map_or(&[][..], Vec::as_slice) {
            if owners.get(&nid).is_some_and(|o| o.iter().any(|s| !dset.contains(s))) {
                carried.entry(*sec).or_insert_with(Vec::new).push((nid, pos));
            }
        }
    }
    (carried, owners)
}

/// `merge_detailed` after stage b: the links across mixed seams, the seam nodes b had no ground for dropped from a's
/// normal sectors, the merged graphs (normal sectors from a, detailed from b, in sector order).
#[must_use]
pub fn merge(
    mut a: Stage,
    mut b: Stage,
    dset: &BTreeSet<(i64, i64)>,
    owners: &BTreeMap<u32, BTreeSet<(i64, i64)>>,
    sw: &Switches,
    log: &mut dyn Log,
) -> Vec<StageGraph> {
    let built = b.sector_set();
    let missing: Vec<&(i64, i64)> = dset.iter().filter(|s| !built.contains(s)).collect();
    if !missing.is_empty() {
        let ids: Vec<String> = missing
            .iter()
            .map(|&&(x, y)| format!("{:04x}", sector_id(x, y)))
            .collect();
        log.line(&format!(
            "detailed pass: {} sectors without ground at pitch {} kept at the normal pitch: {}",
            missing.len(),
            py_repr(DETAILED_PITCH),
            ids.join(", ")
        ));
    }
    let dset = built;
    let drop: BTreeSet<u32> = b
        .dropped
        .iter()
        .copied()
        .filter(|nid| owners.get(nid).is_some_and(|o| o.iter().any(|s| dset.contains(s))))
        .collect();
    log.line(&format!(
        "detailed pass: {} mixed-seam shared nodes without ground at pitch {} dropped",
        drop.len(),
        py_repr(DETAILED_PITCH)
    ));
    // links across mixed seams: b's sectors to a's normal neighbours, a's normal sectors to b's detailed ones
    let a_scope: Scope = a
        .sectors
        .iter()
        .enumerate()
        .filter(|(_, s)| !dset.contains(&s.xy))
        .map(|(n, s)| (s.xy, n))
        .collect();
    let b_scope: Scope = b.sectors.iter().enumerate().map(|(n, s)| (s.xy, n)).collect();
    let b_links: Vec<Vec<kymesh::NeighbourLink>> = (0..b.sectors.len())
        .into_par_iter()
        .map(|n| {
            let portals: Vec<usize> = b
                .graphs
                .get(n)
                .map_or_else(Vec::new, |g| g.mesh.areas.iter().map(|ar| ar.portals.len()).collect());
            let across = (!a_scope.is_empty()).then_some(Across {
                stage: &a,
                scope: &a_scope,
                p_here: b.pitch,
            });
            sector_links(&b, n, &portals, sw.link_rule, &b_scope, across.as_ref())
        })
        .collect();
    let a_links: Vec<(usize, Vec<kymesh::NeighbourLink>)> = a_scope
        .values()
        .copied()
        .collect::<Vec<_>>()
        .into_par_iter()
        .map(|n| {
            let portals: Vec<usize> = a
                .graphs
                .get(n)
                .map_or_else(Vec::new, |g| g.mesh.areas.iter().map(|ar| ar.portals.len()).collect());
            let across = (!b_scope.is_empty()).then_some(Across {
                stage: &b,
                scope: &b_scope,
                p_here: a.pitch,
            });
            (
                n,
                sector_links(&a, n, &portals, sw.link_rule, &a_scope, across.as_ref()),
            )
        })
        .collect();
    for (g, l) in b.graphs.iter_mut().zip(b_links) {
        g.mesh.links = l;
    }
    for (n, l) in a_links {
        if let Some(g) = a.graphs.get_mut(n) {
            g.mesh.links = l;
        }
    }
    let n_norm = a_scope.len();
    let n_det = b.graphs.len();
    let mut merged: BTreeMap<(i64, i64), StageGraph> = BTreeMap::new();
    for (inf, mut g) in a.sectors.iter().zip(std::mem::take(&mut a.graphs)) {
        if dset.contains(&inf.xy) {
            continue;
        }
        if !drop.is_empty() {
            let gone: BTreeSet<usize> = g
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, p)| drop.contains(&p.0))
                .map(|(q, _)| q)
                .collect();
            g.pairs.retain(|p| !gone.contains(&p.0) && !gone.contains(&p.1));
        }
        merged.insert(inf.xy, g);
    }
    for (inf, g) in b.sectors.iter().zip(std::mem::take(&mut b.graphs)) {
        merged.insert(inf.xy, g);
    }
    log.line(&format!("merged: {n_norm} normal + {n_det} detailed sectors"));
    merged.into_values().collect()
}

/// The graphs as `graphfile::write_file` takes them, with their meshes' records (in parallel).
///
/// # Errors
/// The mesh writer's refusal, in words.
pub fn file_inputs(graphs: &[StageGraph]) -> Result<(Vec<SectorGraph>, Vec<Vec<u8>>), NavError> {
    let meshes = graphs
        .par_iter()
        .map(|g| kymesh::write(&g.mesh).map_err(|e| shape(format!("sector {:04x}'s mesh: {e:?}", g.sid))))
        .collect::<Result<Vec<_>, NavError>>()?;
    let sg = graphs
        .iter()
        .map(|g| SectorGraph {
            sid: g.sid,
            bounds: g.bounds,
            nodes: g
                .nodes
                .iter()
                .map(|n| RouteNode {
                    id: n.0,
                    pos: [n.1.0, n.1.1, n.1.2],
                })
                .collect(),
            pairs: g.pairs.iter().map(|p| (p.0, p.1)).collect(),
            origin: Some(g.origin),
        })
        .collect();
    Ok((sg, meshes))
}

#[cfg(test)]
#[path = "stage_tests.rs"]
mod tests;
