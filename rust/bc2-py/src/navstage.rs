//! Bindings for round NK5's `bc2_navbuild::build` / `stage`: NavKit's whole build in Rust (`navbuild.build`, a thin
//! call now) and a stage on a ready walkability grid (`navvehicle.stage_on_grid`) with its file
//! (`navbuild.write_file`). The Python stage, `merge_detailed`, `detail_pass`'s glue and the meshes' assembly were
//! retired.
//!
//! The settings come as dicts the Python makes from its modules at each call (`navbuild._cfg_dict`,
//! `navbuild._switches`, `navwalk._settings`): the tests switch the modules' names. The log is the caller's callable,
//! called with each line as the build makes it.

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

/// `navbuild._cfg_dict(cfg)`: a `BuildSettings`.
fn cfg_of(d: &Bound<'_, PyDict>) -> PyResult<BuildSettings> {
    let (cover_n, cover_d): (Option<f64>, Option<f64>) = get(d, "node_cover")?;
    let (step_n, step_d): (f64, f64) = get(d, "detail_step")?;
    let one_way: String = get(d, "one_way_portals")?;
    let t20: Option<Vec<u32>> = opt(d, "tile_20m_sectors")?;
    Ok(BuildSettings {
        pitch: get(d, "pitch")?,
        sector_samples: get(d, "sector_samples")?,
        window: get(d, "window")?,
        tile: get(d, "tile")?,
        tiles: get(d, "tiles")?,
        resolution: get(d, "resolution")?,
        link_step: get(d, "link_step")?,
        radius: get(d, "radius")?,
        dist_edge_max: get(d, "dist_edge_max")?,
        y_offset: get(d, "y_offset")?,
        area_y_min: get(d, "area_y_min")?,
        area_y_max: get(d, "area_y_max")?,
        border_node_spacing: get(d, "border_node_spacing")?,
        node_cover_normal: cover_n,
        node_cover_detailed: cover_d,
        order_nodes: get(d, "order_nodes")?,
        clip_to_reference: get(d, "clip_to_reference")?,
        one_way_portals: OneWayRule::from_name(&one_way)
            .ok_or_else(|| value_error(format!("one_way_portals {one_way:?}")))?,
        detail_features: get(d, "detail_features")?,
        detail_step_normal: step_n,
        detail_step_detailed: step_d,
        detail_blocks_edges: get(d, "detail_blocks_edges")?,
        tile_20m_sectors: t20.map(|v| v.into_iter().collect()),
    })
}

/// `navbuild._switches()`: the modules' switches.
fn switches_of(d: &Bound<'_, PyDict>) -> PyResult<Switches> {
    let gabriel: Option<String> = opt(d, "gabriel")?;
    let shadow: Option<(usize, f64, f64, f64, f64, f64, bool)> = opt(d, "shadow")?;
    let header: Vec<u32> = get(d, "header")?;
    Ok(Switches {
        area_link: rule(&get::<String>(d, "area_link")?)?,
        link_rule: rule(&get::<String>(d, "link_rule")?)?,
        symmetric_portals: get(d, "symmetric_portals")?,
        edge_rule_grid_min: get(d, "edge_rule_grid_min")?,
        edge_sector_tiles: get(d, "edge_sector_tiles")?,
        max_area: opt(d, "max_area")?,
        gabriel: match gabriel.as_deref() {
            Some("detailed") => Gabriel::Detailed,
            Some("all") => Gabriel::All,
            _ => Gabriel::Off,
        },
        sort_nodes_by_id: get(d, "sort_nodes_by_id")?,
        border_owner: get(d, "border_owner")?,
        owner_edges: opt(d, "owner_edges")?.unwrap_or(true),
        node_bounds: opt(d, "node_bounds")?.unwrap_or(true),
        diagonal_links: opt(d, "diagonal_links")?.unwrap_or(true),
        order_node_min: get(d, "order_node_min")?,
        order_min_island: get(d, "order_min_island")?,
        detail_half_cells: get(d, "detail_half_cells")?,
        detail_in_detailed: get(d, "detail_in_detailed")?,
        detail_f: get(d, "detail_f")?,
        detail_terrain_only: get(d, "detail_terrain_only")?,
        cut_corners: get(d, "cut_corners")?,
        chain_turns: get(d, "chain_turns")?,
        shadow: shadow.map(
            |(min_nodes, xz, dy, anchor_xz, anchor_dy, y_offset, objects_only)| Shadow {
                min_nodes,
                xz,
                dy,
                anchor_xz,
                anchor_dy,
                y_offset,
                objects_only,
            },
        ),
        header: header.try_into().map_err(|_| value_error("36 header words needed"))?,
        roof_prune: get(d, "roof_prune")?,
        objective_floor: get(d, "objective_floor")?,
    })
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
    cfg: &Bound<'py, PyDict>,
    switches: &Bound<'py, PyDict>,
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

/// `navbuild.stage(None, None, xoff, zoff, step=step, cfg=cfg, grid=(s, keep, gx, gz, stair))`: the grid as
/// `(nx, nz, min_x, min_z, pitch, start int64 bytes, y float64 bytes, keep, (gx, f32), (gz, f32), stair, terrain)`.
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the stage's inputs, settings and log")]
#[expect(clippy::type_complexity, reason = "the grid's arrays as a tuple")]
pub fn navkit_stage_grid<'py>(
    py: Python<'py>,
    grid: (
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
    ),
    xoff: i64,
    zoff: i64,
    step: f64,
    cfg: &Bound<'py, PyDict>,
    switches: &Bound<'py, PyDict>,
    log: &Bound<'py, PyAny>,
    t0: f64,
) -> PyResult<NavStageGraphs> {
    let (nx, nz, min_x, min_z, pitch, start, y, keep, (gx, gx32), (gz, gz32), stair, terrain) = grid;
    let (cfg, sw) = (cfg_of(cfg)?, switches_of(switches)?);
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
    let ground = build::grid_ground(
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
        &cfg,
    )
    .map_err(value_error)?;
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

/// `navbuild.write_file(graphs, out, log, t0, cfg)` for a stage kept in the core (no anchors: no shadow islands;
/// `orders`: its order nodes pruned in small islands).
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "write_file's inputs")]
pub fn navkit_write_stage<'py>(
    py: Python<'py>,
    stage_graphs: &NavStageGraphs,
    out: &str,
    cfg: &Bound<'py, PyDict>,
    switches: &Bound<'py, PyDict>,
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
