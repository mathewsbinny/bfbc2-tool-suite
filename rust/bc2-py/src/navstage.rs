//! Bindings for round NK5's `bc2_navbuild::build` / `stage`: NavKit's whole build in Rust (`navwalk.build`, a thin
//! call) and a stage on a ready walkability grid (`navvehicle.stage_on_grid`) with its file. The Python stage,
//! `merge_detailed`, `detail_pass`'s glue and the meshes' assembly were retired (NK5), navbuild.py itself in NK5-rest;
//! a whole stage's answer and one sector's detail pass are given to Python for the tests (`navkit_stage_answer`,
//! `NavLevel.detail_pass`).
//!
//! The build's settings and switches are the Rust defaults (`BuildSettings::default()`, `Switches::default()`;
//! `navkit_settings` lists them): a caller passes a dict of what it changes, or `None` (round NK5-rest retired
//! navbuild.py, which held them). The walkability settings come from `navwalk._settings` at each call. The log is the
//! caller's callable, called with each line as the build makes it.

#![allow(
    clippy::similar_names,
    clippy::type_complexity,
    reason = "the Python's tuples as they come (gx / gz and their float32 flags)"
)]

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use bc2_navbuild::build::{self, DetailedInputs, Edits, Inputs, Report};
use bc2_navbuild::graphfile::Shadow;
use bc2_navbuild::stage::{self, BuildSettings, Gabriel, GridInfo, Log, OneWayRule, StageArgs, StageGraph, Switches};
use bc2_navbuild::walk::Ladder;
use bc2_navbuild::{Floats, Rule};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

use crate::navwalk::{f64s, flags, get, item, opt, points, regions, settings, tris, usizes, value_error, water};

fn rule(name: &str) -> PyResult<Rule> {
    Rule::from_name(name).ok_or_else(|| value_error(format!("unknown link rule {name:?}")))
}

fn has(d: &Bound<'_, PyDict>, key: &str) -> PyResult<bool> {
    d.contains(key)
}

/// The keys [`cfg_of`] takes.
const CFG_KEYS: [&str; 21] = [
    "pitch",
    "sector_samples",
    "window",
    "tile",
    "tiles",
    "resolution",
    "link_step",
    "radius",
    "dist_edge_max",
    "y_offset",
    "area_y_min",
    "area_y_max",
    "border_node_spacing",
    "node_cover",
    "order_nodes",
    "clip_to_reference",
    "one_way_portals",
    "detail_features",
    "detail_step",
    "detail_blocks_edges",
    "tile_20m_sectors",
];

/// The keys [`switches_of`] takes.
const SWITCH_KEYS: [&str; 29] = [
    "area_link",
    "link_rule",
    "symmetric_portals",
    "edge_rule_grid_min",
    "edge_sector_tiles",
    "max_area",
    "gabriel",
    "spanner",
    "sort_nodes_by_id",
    "border_owner",
    "owner_edges",
    "node_bounds",
    "diagonal_links",
    "order_node_min",
    "order_min_island",
    "detail_half_cells",
    "detail_in_detailed",
    "detail_f",
    "detail_terrain_only",
    "cut_corners",
    "chain_turns",
    "df_slope",
    "placement",
    "dice_seeds",
    "midline_cut",
    "shadow",
    "header",
    "roof_prune",
    "objective_floor",
];

/// Refuses a key the settings do not have (a misspelt override would otherwise be ignored).
fn known(d: &Bound<'_, PyDict>, keys: &[&str], what: &str) -> PyResult<()> {
    for k in d.keys() {
        let k: String = k.extract()?;
        if !keys.contains(&k.as_str()) {
            return Err(value_error(format!("unknown {what} {k:?}")));
        }
    }
    Ok(())
}

/// A build's settings: the Rust defaults (`BuildSettings::default()`, the normal infantry build at pitch 0.8) with
/// the dict's keys over them. A `pitch` key first moves the grid to that pitch (`BuildSettings::at_pitch`), then the
/// other keys replace single fields (`navvehicle.vehicle_settings`, `navkit.py --no-detail`). `None`: the defaults.
pub(crate) fn cfg_of(d: Option<&Bound<'_, PyDict>>) -> PyResult<BuildSettings> {
    let mut c = BuildSettings::default();
    let Some(d) = d else { return Ok(c) };
    known(d, &CFG_KEYS, "build setting")?;
    if let Some(p) = opt::<f64>(d, "pitch")? {
        c = c.at_pitch(p);
    }
    macro_rules! take {
        ($($f:ident),*) => {$(
            if let Some(v) = opt(d, stringify!($f))? {
                c.$f = v;
            }
        )*};
    }
    take!(
        sector_samples,
        window,
        tile,
        tiles,
        resolution,
        link_step,
        radius,
        dist_edge_max,
        y_offset,
        area_y_min,
        area_y_max,
        border_node_spacing,
        order_nodes,
        clip_to_reference,
        detail_features,
        detail_blocks_edges
    );
    if has(d, "node_cover")? {
        let (n, dd): (Option<f64>, Option<f64>) = get(d, "node_cover")?;
        c.node_cover_normal = n;
        c.node_cover_detailed = dd;
    }
    if let Some((n, dd)) = opt::<(f64, f64)>(d, "detail_step")? {
        c.detail_step_normal = n;
        c.detail_step_detailed = dd;
    }
    if let Some(one_way) = opt::<String>(d, "one_way_portals")? {
        c.one_way_portals =
            OneWayRule::from_name(&one_way).ok_or_else(|| value_error(format!("one_way_portals {one_way:?}")))?;
    }
    if has(d, "tile_20m_sectors")? {
        let t20: Option<Vec<u32>> = opt(d, "tile_20m_sectors")?;
        c.tile_20m_sectors = t20.map(|v| v.into_iter().collect());
    }
    Ok(c)
}

/// The switches: the Rust defaults (`Switches::default()`, the shipped values) with the dict's keys over them;
/// `None`: the defaults.
pub(crate) fn switches_of(d: Option<&Bound<'_, PyDict>>) -> PyResult<Switches> {
    let mut w = Switches::default();
    let Some(d) = d else { return Ok(w) };
    known(d, &SWITCH_KEYS, "switch")?;
    if let Some(r) = opt::<String>(d, "area_link")? {
        w.area_link = rule(&r)?;
    }
    if let Some(r) = opt::<String>(d, "link_rule")? {
        w.link_rule = rule(&r)?;
    }
    macro_rules! take {
        ($($f:ident),*) => {$(
            if let Some(v) = opt(d, stringify!($f))? {
                w.$f = v;
            }
        )*};
    }
    take!(
        symmetric_portals,
        edge_rule_grid_min,
        edge_sector_tiles,
        sort_nodes_by_id,
        border_owner,
        owner_edges,
        node_bounds,
        diagonal_links,
        order_node_min,
        order_min_island,
        detail_half_cells,
        detail_in_detailed,
        detail_f,
        detail_terrain_only,
        cut_corners,
        chain_turns,
        placement,
        dice_seeds,
        midline_cut,
        roof_prune,
        objective_floor
    );
    if has(d, "max_area")? {
        w.max_area = opt(d, "max_area")?;
    }
    if has(d, "spanner")? {
        w.spanner = opt(d, "spanner")?;
    }
    if has(d, "df_slope")? {
        w.df_slope = opt(d, "df_slope")?;
    }
    if has(d, "gabriel")? {
        let gabriel: Option<String> = opt(d, "gabriel")?;
        w.gabriel = match gabriel.as_deref() {
            Some("detailed") => Gabriel::Detailed,
            Some("all") => Gabriel::All,
            None => Gabriel::Off,
            Some(o) => return Err(value_error(format!("gabriel {o:?}"))),
        };
    }
    if has(d, "shadow")? {
        let shadow: Option<(usize, f64, f64, f64, f64, f64, bool)> = opt(d, "shadow")?;
        w.shadow = shadow.map(
            |(min_nodes, xz, dy, anchor_xz, anchor_dy, y_offset, objects_only)| Shadow {
                min_nodes,
                xz,
                dy,
                anchor_xz,
                anchor_dy,
                y_offset,
                objects_only,
            },
        );
    }
    if let Some(header) = opt::<Vec<u32>>(d, "header")? {
        w.header = header.try_into().map_err(|_| value_error("36 header words needed"))?;
    }
    Ok(w)
}

/// `navkit_settings()`: the Rust defaults as dicts `(cfg, switches)` in the keys the builds' overrides take (what
/// navbuild.py's `BuildSettings` and module switches held until NK5-rest).
#[pyfunction]
pub fn navkit_settings(py: Python<'_>) -> PyResult<(Bound<'_, PyDict>, Bound<'_, PyDict>)> {
    let c = BuildSettings::default();
    let d = PyDict::new(py);
    d.set_item("pitch", c.pitch)?;
    d.set_item("sector_samples", c.sector_samples)?;
    d.set_item("window", c.window)?;
    d.set_item("tile", c.tile)?;
    d.set_item("tiles", c.tiles)?;
    d.set_item("resolution", c.resolution)?;
    d.set_item("link_step", c.link_step)?;
    d.set_item("radius", c.radius)?;
    d.set_item("dist_edge_max", c.dist_edge_max)?;
    d.set_item("y_offset", c.y_offset)?;
    d.set_item("area_y_min", c.area_y_min)?;
    d.set_item("area_y_max", c.area_y_max)?;
    d.set_item("border_node_spacing", c.border_node_spacing)?;
    d.set_item("node_cover", (c.node_cover_normal, c.node_cover_detailed))?;
    d.set_item("order_nodes", c.order_nodes)?;
    d.set_item("clip_to_reference", c.clip_to_reference)?;
    d.set_item("one_way_portals", c.one_way_portals.name())?;
    d.set_item("detail_features", c.detail_features)?;
    d.set_item("detail_step", (c.detail_step_normal, c.detail_step_detailed))?;
    d.set_item("detail_blocks_edges", c.detail_blocks_edges)?;
    d.set_item(
        "tile_20m_sectors",
        c.tile_20m_sectors.map(|s| s.into_iter().collect::<Vec<u32>>()),
    )?;
    let w = Switches::default();
    let s = PyDict::new(py);
    s.set_item("area_link", w.area_link.name())?;
    s.set_item("link_rule", w.link_rule.name())?;
    s.set_item("symmetric_portals", w.symmetric_portals)?;
    s.set_item("edge_rule_grid_min", w.edge_rule_grid_min)?;
    s.set_item("edge_sector_tiles", w.edge_sector_tiles)?;
    s.set_item("max_area", w.max_area)?;
    s.set_item(
        "gabriel",
        match w.gabriel {
            Gabriel::Detailed => Some("detailed"),
            Gabriel::All => Some("all"),
            Gabriel::Off => None,
        },
    )?;
    s.set_item("spanner", w.spanner)?;
    s.set_item("sort_nodes_by_id", w.sort_nodes_by_id)?;
    s.set_item("border_owner", w.border_owner)?;
    s.set_item("owner_edges", w.owner_edges)?;
    s.set_item("node_bounds", w.node_bounds)?;
    s.set_item("diagonal_links", w.diagonal_links)?;
    s.set_item("order_node_min", w.order_node_min)?;
    s.set_item("order_min_island", w.order_min_island)?;
    s.set_item("detail_half_cells", w.detail_half_cells)?;
    s.set_item("detail_in_detailed", w.detail_in_detailed)?;
    s.set_item("detail_f", w.detail_f)?;
    s.set_item("detail_terrain_only", w.detail_terrain_only)?;
    s.set_item("cut_corners", w.cut_corners)?;
    s.set_item("chain_turns", w.chain_turns)?;
    s.set_item("df_slope", w.df_slope)?;
    s.set_item("placement", w.placement)?;
    s.set_item("dice_seeds", w.dice_seeds)?;
    s.set_item("midline_cut", w.midline_cut)?;
    s.set_item(
        "shadow",
        w.shadow.map(|h| {
            (
                h.min_nodes,
                h.xz,
                h.dy,
                h.anchor_xz,
                h.anchor_dy,
                h.y_offset,
                h.objects_only,
            )
        }),
    )?;
    s.set_item("header", w.header.to_vec())?;
    s.set_item("roof_prune", w.roof_prune)?;
    s.set_item("objective_floor", w.objective_floor)?;
    Ok((d, s))
}

fn path(d: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<PathBuf>> {
    Ok(opt::<String>(d, key)?.map(PathBuf::from))
}

/// `navbuild.build`'s arguments from the wrapper's dict.
fn inputs_of(d: &Bound<'_, PyDict>) -> PyResult<Inputs> {
    let edits = match item(d, "edits")? {
        None => None,
        Some(e) => {
            let (regs, seeds, unseeds, counts): (
                Bound<'_, PyList>,
                Vec<[f64; 3]>,
                Vec<[f64; 3]>,
                Vec<(String, usize)>,
            ) = e.extract()?;
            Some(Edits {
                regions: regions(&regs)?,
                seeds: points(Some(seeds)),
                unseeds: points(Some(unseeds)),
                counts,
            })
        }
    };
    let detailed = match item(d, "detailed")? {
        None => None,
        Some(v) => {
            let (samples, high, sectors): (String, Option<String>, Vec<(i64, i64)>) = v.extract()?;
            Some(DetailedInputs {
                samples: PathBuf::from(samples),
                high: high.map(PathBuf::from),
                sectors: sectors.into_iter().collect(),
            })
        }
    };
    let ladders: Option<Vec<([f64; 3], [f64; 3])>> = opt(d, "ladders")?;
    let opt_points = |k: &str| -> PyResult<Option<Vec<(f64, f64, f64)>>> {
        Ok(opt::<Vec<[f64; 3]>>(d, k)?.map(|v| points(Some(v))))
    };
    Ok(Inputs {
        samples: path(d, "samples")?.ok_or_else(|| value_error("samples: missing"))?,
        high: path(d, "high")?,
        narrow: path(d, "narrow")?,
        forbid: tris(d, "forbid")?,
        dynamic: tris(d, "dynamic")?,
        stairs: tris(d, "stairs")?,
        exclude: tris(d, "exclude")?,
        exclude_foot: tris(d, "exclude_foot")?,
        step: get(d, "step")?,
        xoff: get(d, "xoff")?,
        zoff: get(d, "zoff")?,
        seeds: points(opt(d, "seeds")?),
        edits,
        water: water(d)?,
        max_water_depth: opt(d, "max_water_depth")?,
        bounds: opt(d, "bounds")?,
        order_points: opt_points("order_points")?,
        area_orders: opt_points("area_orders")?,
        detailed,
        ladders: ladders.map(|l| l.into_iter().map(|(lo, hi)| Ladder { lo, hi }).collect()),
        terrain: match path(d, "heightfield")? {
            None => None,
            Some(p) => {
                let data = std::fs::read(&p).map_err(|e| value_error(format!("{}: {e}", p.display())))?;
                let hf =
                    bc2_formats::terrain::parse(&data).map_err(|e| value_error(format!("{}: {e}", p.display())))?;
                Some(std::sync::Arc::new(hf))
            }
        },
    })
}

/// The caller's log callable, with `time.time() - t0` for the seconds.
struct PyLog {
    f: Py<PyAny>,
    t0: f64,
    err: Mutex<Option<PyErr>>,
}

impl Log for PyLog {
    fn line(&mut self, text: &str) {
        Python::attach(|py| {
            if let Err(e) = self.f.bind(py).call1((text,))
                && let Ok(mut g) = self.err.lock()
            {
                g.get_or_insert(e);
            }
        });
    }

    fn seconds(&self) -> f64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64());
        now - self.t0
    }
}

impl PyLog {
    fn new(f: &Bound<'_, PyAny>, t0: f64) -> PyLog {
        PyLog {
            f: f.clone().unbind(),
            t0,
            err: Mutex::new(None),
        }
    }

    fn check(self) -> PyResult<()> {
        match self.err.into_inner() {
            Ok(Some(e)) => Err(e),
            _ => Ok(()),
        }
    }
}

/// The build's counts for the modules' report dicts.
fn report_dict<'py>(py: Python<'py>, r: &Report) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    let s = &r.stats;
    let mut ids: Vec<u32> = s.order_ids.iter().copied().collect();
    ids.sort_unstable();
    d.set_item("order_ids", ids)?;
    d.set_item("order_added", s.order_added)?;
    d.set_item("ladders", (s.ladders_linked, s.ladders_split))?;
    d.set_item("detail", (s.detail_d, s.detail_f, s.outline_moved, s.outline_one_sided))?;
    let mut t: Vec<u32> = s.terrain_ids.iter().copied().collect();
    t.sort_unstable();
    d.set_item("terrain_ids", t)?;
    if let Some(sh) = &r.shadow {
        d.set_item("shadow", (sh.islands, sh.ids.len(), sh.before))?;
    }
    let removed: Vec<(u32, (f64, f64, f64), usize)> = r
        .removed
        .iter()
        .map(|x| (x.id, (x.pos[0], x.pos[1], x.pos[2]), x.size))
        .collect();
    d.set_item("removed", removed)?;
    d.set_item("steep", r.steep.clone())?;
    d.set_item("written", (r.sectors, r.nodes, r.edges, r.bytes))?;
    Ok(d)
}

/// `navbuild.build(...)`: the level's path data written to `out`, the log lines through `log` as they come
/// (`time.time() - t0` their seconds), `clock` the sectors' (date, clock) words. Returns the report dict.
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the build's inputs, settings and log")]
pub fn navkit_build<'py>(
    py: Python<'py>,
    job: &Bound<'py, PyDict>,
    cfg: Option<&Bound<'py, PyDict>>,
    switches: Option<&Bound<'py, PyDict>>,
    walk_cfg: &Bound<'py, PyDict>,
    clock: (u32, u32),
    out: &str,
    log: &Bound<'py, PyAny>,
    t0: f64,
) -> PyResult<Bound<'py, PyDict>> {
    let (inp, cfg, sw, ws) = (
        inputs_of(job)?,
        cfg_of(cfg)?,
        switches_of(switches)?,
        settings(walk_cfg)?,
    );
    let out = PathBuf::from(out);
    let mut lg = PyLog::new(log, t0);
    let got = py.detach(|| build::build(&inp, &out, &cfg, &sw, &ws, clock, &mut lg));
    lg.check()?;
    let r = got.map_err(value_error)?;
    report_dict(py, &r)
}

/// A stage's graphs kept in the Rust core (`navvehicle.stage_on_grid`'s result), for `navkit_write_stage`.
#[pyclass(frozen)]
pub struct NavStageGraphs {
    graphs: Vec<StageGraph>,
    order_ids: HashSet<u32>,
}

#[pymethods]
impl NavStageGraphs {
    fn __len__(&self) -> usize {
        self.graphs.len()
    }

    /// Per sector `(sid, nodes, pairs)`, for checks.
    fn summary(&self) -> Vec<(u32, usize, usize)> {
        self.graphs
            .iter()
            .map(|g| (g.sid, g.nodes.len(), g.pairs.len()))
            .collect()
    }
}

/// A ready walkability grid as the Python hands it: `(nx, nz, min_x, min_z, pitch, start int64 bytes, y float64
/// bytes, keep, (gx, f32), (gz, f32), stair, terrain)`.
type PyGrid<'py> = (
    usize,
    usize,
    f64,
    f64,
    f64,
    Bound<'py, PyBytes>,
    Bound<'py, PyBytes>,
    Bound<'py, PyBytes>,
    (Bound<'py, PyBytes>, bool),
    (Bound<'py, PyBytes>, bool),
    Bound<'py, PyBytes>,
    Bound<'py, PyBytes>,
);

/// The stage's ground from a [`PyGrid`].
fn ground_of(grid: PyGrid<'_>, step: f64, cfg: &BuildSettings) -> PyResult<stage::Ground> {
    let (nx, nz, min_x, min_z, pitch, start, y, keep, (gx, gx32), (gz, gz32), stair, terrain) = grid;
    let floats = |b: &[u8], f32_: bool| -> PyResult<Floats> {
        Ok(if f32_ {
            Floats::F32(
                crate::navwalk::le::<4>(b, "gradients")?
                    .into_iter()
                    .map(f32::from_le_bytes)
                    .collect(),
            )
        } else {
            Floats::F64(f64s(b, false, "gradients")?)
        })
    };
    build::grid_ground(
        GridInfo {
            nx,
            nz,
            min_x,
            min_z,
            pitch,
        },
        usizes(start.as_bytes(), "start")?,
        f64s(y.as_bytes(), false, "y")?,
        flags(keep.as_bytes()),
        floats(gx.as_bytes(), gx32)?,
        floats(gz.as_bytes(), gz32)?,
        flags(stair.as_bytes()),
        flags(terrain.as_bytes()),
        step,
        cfg,
    )
    .map_err(value_error)
}

/// A stage on a ready walkability grid (`navvehicle.stage_on_grid`): its graphs kept in the core for
/// `navkit_write_stage`. `cfg` / `switches`: overrides of the Rust defaults (`None`: the defaults).
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the stage's inputs, settings and log")]
pub fn navkit_stage_grid<'py>(
    py: Python<'py>,
    grid: PyGrid<'py>,
    xoff: i64,
    zoff: i64,
    step: f64,
    cfg: Option<&Bound<'py, PyDict>>,
    switches: Option<&Bound<'py, PyDict>>,
    log: &Bound<'py, PyAny>,
    t0: f64,
) -> PyResult<NavStageGraphs> {
    let (cfg, sw) = (cfg_of(cfg)?, switches_of(switches)?);
    let ground = ground_of(grid, step, &cfg)?;
    let args = StageArgs {
        xoff,
        zoff,
        id_start: 1,
        ..StageArgs::default()
    };
    let mut lg = PyLog::new(log, t0);
    let mut stats = stage::BuildStats::default();
    let got = py.detach(|| stage::stage(ground, &args, &cfg, &sw, &mut stats, &mut lg));
    lg.check()?;
    let st = got.map_err(value_error)?;
    Ok(NavStageGraphs {
        graphs: st.graphs,
        order_ids: stats.order_ids,
    })
}

/// A whole stage on a ready grid with everything it made, for the tests (`test_navbuild_native.py`'s whole stages,
/// round NK5-rest: what navbuild.py's Python `stage` returned): a dict of `meshes` (per sector the mesh's bytes),
/// `info` (per sector its areas `[(tile, layer, cells)]` and origin), `graphs` (`[(sid, bounds, pts, pairs,
/// origin)]`), `border_nodes`, `next_id`, `dropped`, `order_ids`, `order_added`, `detail` (`(d, f, moved,
/// one_sided)`) and `blocked` (per sector its D sides, sorted). The sectors in their sorted order.
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the stage's inputs and settings")]
pub fn navkit_stage_answer<'py>(
    py: Python<'py>,
    grid: PyGrid<'py>,
    xoff: i64,
    zoff: i64,
    step: f64,
    cfg: Option<&Bound<'py, PyDict>>,
    switches: Option<&Bound<'py, PyDict>>,
    order_points: Vec<(f64, f64, f64)>,
) -> PyResult<Bound<'py, PyDict>> {
    struct Quiet;
    impl Log for Quiet {
        fn line(&mut self, _text: &str) {}
        fn seconds(&self) -> f64 {
            0.0
        }
    }
    let (cfg, sw) = (cfg_of(cfg)?, switches_of(switches)?);
    let ground = ground_of(grid, step, &cfg)?;
    let args = StageArgs {
        xoff,
        zoff,
        id_start: 1,
        order_points,
        ..StageArgs::default()
    };
    let mut stats = stage::BuildStats::default();
    let st = py
        .detach(|| stage::stage(ground, &args, &cfg, &sw, &mut stats, &mut Quiet))
        .map_err(value_error)?;
    let d = PyDict::new(py);
    let meshes = PyList::empty(py);
    let info = PyList::empty(py);
    for (inf, g) in st.sectors.iter().zip(&st.graphs) {
        let bytes = bc2_formats::kymesh::write(&g.mesh).map_err(|e| value_error(format!("{e:?}")))?;
        meshes.append((inf.xy, PyBytes::new(py, &bytes)))?;
        let areas = PyList::empty(py);
        for (t, cells) in inf.tiles.iter().zip(&inf.cells) {
            areas.append(((t[0], t[1]), t[2], cells.clone()))?;
        }
        info.append((inf.xy, areas, inf.origin))?;
    }
    d.set_item("meshes", meshes)?;
    d.set_item("info", info)?;
    let graphs = PyList::empty(py);
    for g in &st.graphs {
        graphs.append((g.sid, g.bounds.to_vec(), g.nodes.clone(), g.pairs.clone(), g.origin))?;
    }
    d.set_item("graphs", graphs)?;
    let border: Vec<((i64, i64), Vec<BorderNodeTuple>)> = st
        .border_nodes
        .iter()
        .map(|(sec, v)| (*sec, v.iter().map(|&(id, p, k)| (id, p, k)).collect()))
        .collect();
    d.set_item("border_nodes", border)?;
    d.set_item("next_id", st.next_id)?;
    d.set_item("dropped", st.dropped.iter().copied().collect::<Vec<u32>>())?;
    let mut ids: Vec<u32> = stats.order_ids.iter().copied().collect();
    ids.sort_unstable();
    d.set_item("order_ids", ids)?;
    d.set_item("order_added", stats.order_added)?;
    d.set_item(
        "detail",
        (
            stats.detail_d,
            stats.detail_f,
            stats.outline_moved,
            stats.outline_one_sided,
        ),
    )?;
    let blocked: Vec<Vec<(usize, usize)>> = st
        .blocked
        .iter()
        .map(|b| {
            let mut v: Vec<(usize, usize)> = b.iter().copied().collect();
            v.sort_unstable();
            v
        })
        .collect();
    d.set_item("blocked", blocked)?;
    Ok(d)
}

/// A border node as the Python's tuple `(id, pos, surface)`.
type BorderNodeTuple = (u32, (f64, f64, f64), usize);

/// One sector's detail pass on the Python's edge rows (`navbuild.detail_pass`, round NK5-rest: the stage's own,
/// `bc2_navbuild::stage::detail_pass`): `(rows after it, per area (D lines, F records as (direction, points)), its D
/// sides sorted, (d, f, moved, one_sided))`. `cfg` / `switches`: overrides of the Rust defaults.
pub fn detail_pass_py<'py>(
    py: Python<'py>,
    level: &bc2_navbuild::Level,
    cells: &[Vec<bc2_navbuild::Cell>],
    per_area: &Bound<'py, PyList>,
    origin: (i64, i64, f64, f64),
    cfg: Option<&Bound<'py, PyDict>>,
    switches: Option<&Bound<'py, PyDict>>,
) -> PyResult<DetailAnswer<'py>> {
    let (cfg, sw) = (cfg_of(cfg)?, switches_of(switches)?);
    let mut rows = crate::navtrace::edges_from_py(per_area)?;
    if rows.len() != cells.len() {
        return Err(value_error("one row set per area needed"));
    }
    let det = if cfg.detail_features {
        stage::detail_pass(level, cells, &mut rows, origin, &cfg, &sw, None)
    } else {
        stage::Detailed {
            features: vec![(Vec::new(), Vec::new()); cells.len()],
            blocked: HashSet::new(),
            d: 0,
            f: 0,
            moved: 0,
            one_sided: 0,
        }
    };
    let pt = |p: [f64; 3]| (p[0], p[1], p[2]);
    let features: Vec<DetailFeatures> = det
        .features
        .iter()
        .map(|(lines, fcells)| {
            (
                lines.iter().map(|l| l.iter().copied().map(pt).collect()).collect(),
                fcells
                    .iter()
                    .map(|c| (pt(c.direction), c.points.iter().copied().map(pt).collect()))
                    .collect(),
            )
        })
        .collect();
    let mut blocked: Vec<(usize, usize)> = det.blocked.iter().copied().collect();
    blocked.sort_unstable();
    let rows = crate::navtrace::edges_to_py(py, &rows, &[])?.0;
    Ok((rows, features, blocked, (det.d, det.f, det.moved, det.one_sided)))
}

/// One area's D lines and F records as the Python takes them.
type DetailFeatures = (Vec<Vec<(f64, f64, f64)>>, Vec<((f64, f64, f64), Vec<(f64, f64, f64)>)>);

/// [`detail_pass_py`]'s answer.
pub type DetailAnswer<'py> = (
    Bound<'py, PyList>,
    Vec<DetailFeatures>,
    Vec<(usize, usize)>,
    (usize, usize, usize, usize),
);

/// `navbuild.write_file(graphs, out, log, t0, cfg)` for a stage kept in the core (no anchors: no shadow islands;
/// `orders`: its order nodes pruned in small islands).
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "write_file's inputs")]
pub fn navkit_write_stage<'py>(
    py: Python<'py>,
    stage_graphs: &NavStageGraphs,
    out: &str,
    cfg: Option<&Bound<'py, PyDict>>,
    switches: Option<&Bound<'py, PyDict>>,
    clock: (u32, u32),
    orders: bool,
    log: &Bound<'py, PyAny>,
    t0: f64,
) -> PyResult<Bound<'py, PyDict>> {
    let (cfg, sw) = (cfg_of(cfg)?, switches_of(switches)?);
    let out = PathBuf::from(out);
    let mut lg = PyLog::new(log, t0);
    let mut report = Report::default();
    if orders {
        report.stats.order_ids.clone_from(&stage_graphs.order_ids);
    }
    let got = py.detach(|| {
        let data = build::file_bytes(&stage_graphs.graphs, None, &cfg, &sw, clock, &mut report, &mut lg)?;
        build::write_out(&out, &data, &report, &mut lg)
    });
    lg.check()?;
    got.map_err(value_error)?;
    report_dict(py, &report)
}
