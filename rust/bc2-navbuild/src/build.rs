//! NavKit's whole build in Rust (round NK5: `navbuild.build`): walkability ([`crate::walk::stage`]), the stage
//! ([`crate::stage`]), the detailed pass merged in, the file ([`crate::graphfile::write_file`]) - from a level's
//! inputs ([`Inputs`]; `navkit.build_inputs` / `navregress.build_level` gather them, [`crate::inputs`]) to
//! the path data and the build's log lines, the same lines and bytes the Python build gave.

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
    reason = "the reference's single-letter names (x, y, i, j, k, g ...) and its long step-by-step functions kept for               side-by-side reading; the settings are the module's switches; indexes into lists made beside them;               == and (a + b) / 2.0 as the Python computes them"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::graphfile::{self, FileSettings, Layout};
use crate::stage::{
    self, BuildSettings, BuildStats, GridInfo, Ground, Log, StageArgs, StageGraph, Switches, py_g, py_repr,
};
use crate::trace::P3;
use crate::walk::{self, Ladder, Region, StageInputs, Tris, WalkInputs, WalkSettings, WaterMesh};
use crate::{Floats, Level, LevelData, NavError};

/// A navigation side file's walkability operations (`navedits.regions_of`).
#[derive(Debug, Clone, Default)]
pub struct Edits {
    /// The region edits in order: (walkable, region).
    pub regions: Vec<(bool, Region)>,
    /// The hand seeds.
    pub seeds: Vec<P3>,
    /// The hand unseeds.
    pub unseeds: Vec<P3>,
    /// Count per operation name, in the order they first occur.
    pub counts: Vec<(String, usize)>,
}

/// The detailed-zone pass's inputs (`detailed`).
#[derive(Debug, Clone)]
pub struct DetailedInputs {
    /// The pitch-0.2 sampling run.
    pub samples: PathBuf,
    /// Its high-capsule run, or `None`.
    pub high: Option<PathBuf>,
    /// The sectors to build at pitch 0.2.
    pub sectors: BTreeSet<(i64, i64)>,
}

/// `navbuild.build`'s arguments.
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    /// The in-game samples (pitch 0.8).
    pub samples: PathBuf,
    /// The high-capsule run.
    pub high: Option<PathBuf>,
    /// The narrow-capsule run.
    pub narrow: Option<PathBuf>,
    /// `forbid_tris.npy`.
    pub forbid: Option<Tris>,
    /// `dynamic_tris.npy`.
    pub dynamic: Option<Tris>,
    /// `stair_tris.npy`.
    pub stairs: Option<Tris>,
    /// `exclude_tris.npy`.
    pub exclude: Option<Tris>,
    /// `exclude_footprint_tris.npy`.
    pub exclude_foot: Option<Tris>,
    /// The step rule's tolerance (0.28).
    pub step: f64,
    /// The level's sector offsets.
    pub xoff: i64,
    /// See `xoff`.
    pub zoff: i64,
    /// The soldier spawns.
    pub seeds: Vec<P3>,
    /// The side file's walkability edits (`None`: no edits; an empty side file's list is `Some`).
    pub edits: Option<Edits>,
    /// The level's water mesh.
    pub water: Option<WaterMesh>,
    /// Ground deeper under water is not walkable (`None`: no water rule).
    pub max_water_depth: Option<f64>,
    /// DICE's extent (minX, minZ, maxX, maxZ).
    pub bounds: Option<[f64; 4]>,
    /// The AI order positions.
    pub order_points: Option<Vec<P3>>,
    /// The `AreaOrder` positions.
    pub area_orders: Option<Vec<P3>>,
    /// The detailed-zone pass.
    pub detailed: Option<DetailedInputs>,
    /// The ladders.
    pub ladders: Option<Vec<Ladder>>,
}

impl Inputs {
    /// The inputs with the shipped defaults (`step` 0.28, the water rule at 1.5 m).
    #[must_use]
    pub fn new(samples: PathBuf, xoff: i64, zoff: i64) -> Inputs {
        Inputs {
            samples,
            step: 0.28,
            xoff,
            zoff,
            max_water_depth: Some(1.5),
            ..Inputs::default()
        }
    }
}

/// What a build reports besides its log lines.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// The build's counts.
    pub stats: BuildStats,
    /// The shadow islands' answer.
    pub shadow: Option<graphfile::ShadowStats>,
    /// The order nodes dropped.
    pub removed: Vec<graphfile::RemovedOrder>,
    /// Sectors, nodes and edges written.
    pub sectors: usize,
    /// See `sectors`.
    pub nodes: usize,
    /// See `sectors`.
    pub edges: usize,
    /// The file's size.
    pub bytes: usize,
    /// Per stage, `navwalk.STEEP_STATS`' increments.
    pub steep: Vec<(&'static str, usize)>,
}

/// One stage's walkability (`stage`'s walkability block): the run, its counts' log lines, the ground.
#[allow(clippy::too_many_arguments)]
fn walk_ground(
    inp: &Inputs,
    samples: &Path,
    high: Option<&Path>,
    seed_points: &[P3],
    cfg: &BuildSettings,
    ws: &WalkSettings,
    report: &mut Report,
    log: &mut dyn Log,
) -> Result<Ground, NavError> {
    let soldiers = inp.seeds.clone();
    let mut seeds = inp.seeds.clone();
    let (mut regions, mut unseeds) = (None, Vec::new());
    if let Some(e) = &inp.edits {
        regions = Some(e.regions.clone());
        seeds.extend(e.seeds.iter().copied());
        unseeds.clone_from(&e.unseeds);
    }
    let clip = inp.bounds.is_some() && cfg.clip_to_reference;
    let wet = inp.water.is_some() && inp.max_water_depth.is_some();
    let si = StageInputs {
        samples: samples.to_path_buf(),
        high: high.map(Path::to_path_buf),
        narrow: inp.narrow.clone(),
        pitch: cfg.pitch,
        coarse_pitch: 0.8,
        edits: regions,
        water: match (&inp.water, inp.max_water_depth) {
            (Some(w), Some(l)) => Some((w.clone(), l)),
            _ => None,
        },
        bounds: if clip { inp.bounds } else { None },
    };
    let orders = inp.order_points.clone().unwrap_or_default();
    let mut extra = seeds.clone();
    extra.extend_from_slice(seed_points);
    let mut anchors = seeds;
    anchors.extend_from_slice(&orders);
    anchors.extend_from_slice(seed_points);
    let wi = WalkInputs {
        forbid: inp.forbid.clone(),
        dynamic: inp.dynamic.clone(),
        stair_tris: inp.stairs.clone(),
        exclude: inp.exclude.clone(),
        exclude_foot: inp.exclude_foot.clone(),
        high: None,
        narrow: None,
        step: inp.step,
        terrain_seeds_only: false,
        stairs: true,
        raised_bars: false,
        forbid_tol: 0.08,
        extra_seeds: extra,
        force_walkable: None,
        force_unwalkable: None,
        unseeds,
        soldier_points: soldiers,
        ladders: inp.ladders.clone().unwrap_or_default(),
        anchors,
        objective_points: inp.area_orders.clone().unwrap_or_default(),
    };
    let (s, counts, walked) = walk::stage(&si, wi, ws)?;
    if let (Some(e), Some((fw, fu))) = (&inp.edits, counts.edits) {
        let items: Vec<String> = e.counts.iter().map(|(k, v)| format!("'{k}': {v}")).collect();
        log.line(&format!(
            "hand edits: {{{}}}; surfaces forced walkable {fw}, unwalkable {fu}",
            items.join(", ")
        ));
    }
    if wet {
        log.line(&format!(
            "water: {} surfaces more than {} m under water left out",
            counts.water.unwrap_or(0),
            py_repr(inp.max_water_depth.unwrap_or(0.0))
        ));
    }
    if let (true, Some(b)) = (clip, inp.bounds) {
        log.line(&format!(
            "reference extent x {:.1}..{:.1}, z {:.1}..{:.1}: {} surfaces outside left out",
            b[0],
            b[2],
            b[1],
            b[3],
            counts.outside.unwrap_or(0)
        ));
    }
    report.steep.extend(walked.stats.steep.iter().copied());
    let terrain: Vec<bool> = match &s.hit {
        Some(h) => h.iter().map(|w| w[0] == ws.terrain_vtable).collect(),
        None => vec![true; s.len()],
    };
    let grid = GridInfo {
        nx: s.nx,
        nz: s.nz,
        min_x: s.min_x,
        min_z: s.min_z,
        pitch: s.pitch,
    };
    drop(s);
    let roof = walked.stats.roof.map(|r| r.1.unwrap_or((0, 0))).or(Some((0, 0)));
    let objective = Some(walked.stats.objective.unwrap_or((0, 0)));
    let ladder_pairs = walked
        .ladder_pairs
        .iter()
        .filter_map(|&(f, t)| Some((f?, t?)))
        .collect();
    let level = walked
        .level
        .with_keep_terrain(walked.keep, terrain, cfg.pitch, cfg.link_step)?;
    Ok(Ground {
        level,
        grid,
        ladder_pairs,
        roof,
        objective,
    })
}

/// A ready walkability grid as a stage's ground (`stage(..., grid=(s, keep, gx, gz, stair))`: `navvehicle`'s).
///
/// # Errors
/// [`NavError::Shape`] when the arrays do not describe one grid.
#[allow(clippy::too_many_arguments)]
pub fn grid_ground(
    grid: GridInfo,
    start: Vec<usize>,
    y: Vec<f64>,
    keep: Vec<bool>,
    gx: Floats,
    gz: Floats,
    stair: Vec<bool>,
    terrain: Vec<bool>,
    step: f64,
    cfg: &BuildSettings,
) -> Result<Ground, NavError> {
    let level = Level::new(LevelData {
        nx: grid.nx,
        nz: grid.nz,
        start,
        keep,
        y,
        gx,
        gz,
        stair,
        terrain,
        pitch: cfg.pitch,
        step,
        link_step: cfg.link_step,
    })?;
    Ok(Ground {
        level,
        grid,
        ladder_pairs: Vec::new(),
        roof: None,
        objective: None,
    })
}

/// `navbuild.build` up to the file: walkability, the stage, the detailed pass merged in, and the log lines before
/// `write_file`'s. Returns the graphs.
///
/// # Errors
/// [`NavError::Shape`] in the reference's words where it raised.
pub fn build_graphs(
    inp: &Inputs,
    cfg: &BuildSettings,
    sw: &Switches,
    ws: &WalkSettings,
    report: &mut Report,
    log: &mut dyn Log,
) -> Result<Vec<StageGraph>, NavError> {
    let stop = |log: &dyn Log| {
        if log.cancelled() {
            Err(NavError::Cancelled)
        } else {
            Ok(())
        }
    };
    stop(log)?;
    let ground = walk_ground(inp, &inp.samples, inp.high.as_deref(), &[], cfg, ws, report, log)?;
    stop(log)?;
    let args = StageArgs {
        xoff: inp.xoff,
        zoff: inp.zoff,
        order_points: inp.order_points.clone().unwrap_or_default(),
        id_start: 1,
        ..StageArgs::default()
    };
    let a = stage::stage(ground, &args, cfg, sw, &mut report.stats, log)?;
    stop(log)?;
    let graphs = match &inp.detailed {
        None => a.graphs,
        Some(det) => {
            let have = a.sector_set();
            let dset: BTreeSet<(i64, i64)> = det.sectors.intersection(&have).copied().collect();
            let pts = stage::detailed_seeds(&a, &dset, inp.xoff, inp.zoff);
            let (carried, owners) = stage::carried_nodes(&a, &dset);
            log.line(&format!(
                "detailed pass: {} sectors at pitch {}, {} seed points from the normal build, {} shared nodes on \
                 mixed seams",
                dset.len(),
                py_repr(stage::DETAILED_PITCH),
                pts.len(),
                carried.values().map(Vec::len).sum::<usize>()
            ));
            let grid_min = (
                have.iter().map(|s| s.0).min().unwrap_or(0),
                have.iter().map(|s| s.1).min().unwrap_or(0),
            );
            let dcfg = cfg.detailed();
            let ground = walk_ground(inp, &det.samples, det.high.as_deref(), &pts, &dcfg, ws, report, log)?;
            drop(pts);
            stop(log)?;
            let bargs = StageArgs {
                xoff: inp.xoff,
                zoff: inp.zoff,
                order_points: args.order_points.clone(),
                sector_filter: Some(dset.clone()),
                grid_min: Some(grid_min),
                id_start: a.next_id,
                carried,
            };
            let b = stage::stage(ground, &bargs, &dcfg, sw, &mut report.stats, log)?;
            stop(log)?;
            stage::merge(a, b, &dset, &owners, sw, log)
        }
    };
    if let Some(l) = &inp.ladders
        && !l.is_empty()
    {
        log.line(&format!(
            "ladders: {}; links made {} (counted per stage), ends in two sectors {}",
            l.len(),
            report.stats.ladders_linked,
            report.stats.ladders_split
        ));
    }
    if cfg.detail_features {
        let s = &report.stats;
        log.line(&format!(
            "detail features: {} D lines, {} F cells; {} outline sides moved to them, {} kept (outline on one cell \
             only)",
            s.detail_d, s.detail_f, s.outline_moved, s.outline_one_sided
        ));
    } else {
        log.line("detail features: off (no D lines or F cells)");
    }
    Ok(graphs)
}

/// `navbuild.write_file` up to the bytes: with `anchors`, the shadow islands' rule (its log line); the order nodes in
/// small islands; the tables and the file, each sector built at `(date, clock)`.
///
/// # Errors
/// [`NavError::Shape`] for a value the file cannot hold.
pub fn file_bytes(
    graphs: &[StageGraph],
    anchors: Option<Vec<[f64; 3]>>,
    cfg: &BuildSettings,
    sw: &Switches,
    clock: (u32, u32),
    report: &mut Report,
    log: &mut dyn Log,
) -> Result<Vec<u8>, NavError> {
    let (sg, meshes) = stage::file_inputs(graphs)?;
    let shadow = match (anchors, &sw.shadow) {
        (Some(a), Some(s)) => Some((*s, a, report.stats.terrain_ids.clone())),
        _ => None,
    };
    let orders = (!report.stats.order_ids.is_empty() && sw.order_min_island > 0)
        .then(|| (report.stats.order_ids.clone(), sw.order_min_island));
    let fs = FileSettings {
        shadow,
        orders,
        layout: Layout {
            sort_by_id: sw.sort_nodes_by_id,
            border_owner: sw.border_owner,
            dist_edge_max: cfg.dist_edge_max,
        },
        header: sw.header,
        date: clock.0,
        clock: clock.1,
    };
    let w = graphfile::write_file(&sg, &meshes, &fs).map_err(|e| NavError::Shape(e.to_string()))?;
    if let (Some(st), Some(s)) = (&w.shadow, &sw.shadow) {
        log.line(&format!(
            "shadow islands: {} of {} route islands ({} nodes) under {} nodes within {} m of a larger one lose their \
             nodes",
            st.islands,
            st.before,
            st.ids.len(),
            s.min_nodes,
            py_g(s.xz)
        ));
    }
    report.shadow = w.shadow;
    report.removed.extend(w.removed);
    report.sectors = graphs.len();
    report.nodes = w.nodes;
    report.edges = w.edges;
    report.bytes = w.data.len();
    Ok(w.data)
}

/// The build's anchors: the soldier spawns, the AI order and the objective positions.
#[must_use]
pub fn anchors(inp: &Inputs) -> Vec<[f64; 3]> {
    inp.seeds
        .iter()
        .chain(inp.order_points.iter().flatten())
        .chain(inp.area_orders.iter().flatten())
        .map(|p| [p.0, p.1, p.2])
        .collect()
}

/// `navbuild.build`: the path data written to `out` (whole or not at all) and its last log line.
///
/// # Errors
/// [`NavError::Shape`] in the reference's words; a write error in words.
pub fn build(
    inp: &Inputs,
    out: &Path,
    cfg: &BuildSettings,
    sw: &Switches,
    ws: &WalkSettings,
    clock: (u32, u32),
    log: &mut dyn Log,
) -> Result<Report, NavError> {
    let mut report = Report::default();
    let graphs = build_graphs(inp, cfg, sw, ws, &mut report, log)?;
    if log.cancelled() {
        return Err(NavError::Cancelled);
    }
    let data = file_bytes(&graphs, Some(anchors(inp)), cfg, sw, clock, &mut report, log)?;
    drop(graphs);
    write_out(out, &data, &report, log)?;
    Ok(report)
}

/// The file written (`atomicio.write_bytes`) and `write_file`'s last line.
///
/// # Errors
/// A write error in words.
pub fn write_out(out: &Path, data: &[u8], report: &Report, log: &mut dyn Log) -> Result<(), NavError> {
    bc2_formats::atomic::write(out, &[data]).map_err(|e| NavError::Build(format!("{}: {e}", out.display())))?;
    log.line(&format!(
        "wrote {}: {} sectors, {} nodes, {} edges, {} bytes ({:.0} s)",
        out.display(),
        report.sectors,
        report.nodes,
        report.edges,
        data.len(),
        log.seconds()
    ));
    Ok(())
}

/// `navbuild.time.localtime()`'s date and clock words of a sector (`(yy * 10000 + (month - 1) * 100 + day,
/// hour * 1000000 + minute * 10000 + second * 100)`).
#[must_use]
pub fn clock_words(year: u32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> (u32, u32) {
    (
        (year % 100) * 10000 + month.saturating_sub(1) * 100 + day,
        hour * 1_000_000 + minute * 10000 + second * 100,
    )
}

/// `np.load(path)` of an (n, 3, 3) triangle array (`levelobjects`' `.npy` files) as walkability takes it
/// (`navwalk._tris`): `None` for no triangles; float32 values widened, `from_f32` set.
///
/// # Errors
/// [`NavError::Build`] for a file that is not such an array, in words; an unreadable file's error.
pub fn read_tris_npy(path: &Path) -> Result<Option<Tris>, NavError> {
    let bad = |what: &str| NavError::Build(format!("{}: {what}", path.display()));
    let data = std::fs::read(path).map_err(|e| NavError::Build(format!("{}: {e}", path.display())))?;
    if data.get(..6) != Some(b"\x93NUMPY".as_slice()) {
        return Err(bad("not a .npy file"));
    }
    let major = data.get(6).copied().unwrap_or(0);
    let (hlen, start) = match major {
        1 => (
            usize::from(u16::from_le_bytes([
                data.get(8).copied().unwrap_or(0),
                data.get(9).copied().unwrap_or(0),
            ])),
            10,
        ),
        2 | 3 => {
            let b: [u8; 4] = data
                .get(8..12)
                .and_then(|s| s.try_into().ok())
                .ok_or_else(|| bad("cut short"))?;
            (u32::from_le_bytes(b) as usize, 12)
        }
        _ => return Err(bad("an unknown .npy version")),
    };
    let header = data.get(start..start + hlen).ok_or_else(|| bad("cut short"))?;
    let header = String::from_utf8_lossy(header);
    let f32_ = header.contains("'descr': '<f4'");
    if !f32_ && !header.contains("'descr': '<f8'") {
        return Err(bad("not float32 or float64 values"));
    }
    if header.contains("'fortran_order': True") {
        return Err(bad("Fortran order"));
    }
    let shape = header
        .split("'shape': (")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .ok_or_else(|| bad("no shape"))?;
    let dims: Vec<usize> = shape
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<usize>().map_err(|_| bad("a shape that is not numbers")))
        .collect::<Result<_, _>>()?;
    let [n, 3, 3] = dims.as_slice() else {
        return Err(bad("triangles are n x 3 x 3 (corners x, y, z)"));
    };
    let body = data.get(start + hlen..).unwrap_or(&[]);
    let size = if f32_ { 4 } else { 8 };
    let want = n.checked_mul(9 * size).ok_or_else(|| bad("too large"))?;
    let body = body.get(..want).ok_or_else(|| bad("cut short"))?;
    if *n == 0 {
        return Ok(None);
    }
    let vals: Vec<f64> = if f32_ {
        body.as_chunks::<4>()
            .0
            .iter()
            .map(|c| walk::widen(f32::from_le_bytes(*c)))
            .collect()
    } else {
        body.as_chunks::<8>().0.iter().map(|c| f64::from_le_bytes(*c)).collect()
    };
    Ok(Some(Tris {
        tris: vals
            .as_chunks::<9>()
            .0
            .iter()
            .map(|&[a, b, c, d, e, f, g, h, i]| [[a, b, c], [d, e, f], [g, h, i]])
            .collect(),
        from_f32: f32_,
    }))
}
