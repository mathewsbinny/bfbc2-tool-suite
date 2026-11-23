//! Bindings for `bc2_navbuild::walk` (round NK4): NavKit's walkability end to end. `navbuild.stage` hands the
//! sample files and the walkability inputs over once (`navkit_walk_stage`); `navwalk.walkable`, `coarse`,
//! `ladder_ends`, `near_points`, `navedits.edits_for` and `navbuild.water_tops` / `deep_water` / `outside_bounds`
//! are thin calls for their other callers. Their Python was retired (`rust/BENCHMARKS.md`, "NK4"), so a
//! `ValueError` here reaches the caller.
//!
//! A run crosses as a dict: `nx`, `nz`, `pitch`, `min_x`, `min_z`, `header` (the other six floats), `start`
//! (int64 bytes), `y` (bytes, `float32` when `y_f32`), `normal` (bytes, three per surface, `float32` when
//! `normal_f32`; optional where unused), `flags` (uint8 bytes; optional where unused), `hit` (uint32 bytes, three per
//! surface, or None).
#![allow(clippy::many_single_char_names, reason = "the reference's names")]
#![expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]

use std::path::PathBuf;

use bc2_navbuild::Floats;
use bc2_navbuild::graph::RoofCfg;
use bc2_navbuild::walk::{
    self, Capsule, Ladder, Normals, P3, Region, Samples, StageInputs, Tris, WalkInputs, WalkSettings, WalkStats,
    Walked, WaterMesh,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyByteArray, PyBytes, PyDict, PyList, PyTuple};

use crate::navbuild::NavLevel;

pub(crate) fn value_error(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

pub(crate) fn item<'py>(d: &Bound<'py, PyDict>, key: &str) -> PyResult<Option<Bound<'py, PyAny>>> {
    Ok(d.get_item(key)?.filter(|v| !v.is_none()))
}

pub(crate) fn need<'py>(d: &Bound<'py, PyDict>, key: &str) -> PyResult<Bound<'py, PyAny>> {
    item(d, key)?.ok_or_else(|| value_error(format!("{key}: missing")))
}

pub(crate) fn get<'py, T: FromPyObjectOwned<'py>>(d: &Bound<'py, PyDict>, key: &str) -> PyResult<T> {
    need(d, key)?.extract::<T>().map_err(Into::into)
}

pub(crate) fn opt<'py, T: FromPyObjectOwned<'py>>(d: &Bound<'py, PyDict>, key: &str) -> PyResult<Option<T>> {
    item(d, key)?.map(|v| v.extract::<T>().map_err(Into::into)).transpose()
}

/// A `bytes` object's bytes.
pub(crate) fn raw(v: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    Ok(v.cast::<PyBytes>()?.as_bytes().to_vec())
}

fn bytes_of(d: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<Vec<u8>>> {
    item(d, key)?
        .map(|v| {
            v.cast_into::<PyBytes>()
                .map(|b| b.as_bytes().to_vec())
                .map_err(PyErr::from)
        })
        .transpose()
}

pub(crate) fn le<const N: usize>(b: &[u8], what: &str) -> PyResult<Vec<[u8; N]>> {
    let (whole, rest) = b.as_chunks::<N>();
    if !rest.is_empty() {
        return Err(value_error(format!("{what}: {} bytes, not a multiple of {N}", b.len())));
    }
    Ok(whole.to_vec())
}

pub(crate) fn f64s(b: &[u8], f32_: bool, what: &str) -> PyResult<Vec<f64>> {
    Ok(if f32_ {
        le::<4>(b, what)?
            .into_iter()
            .map(|c| walk::widen(f32::from_le_bytes(c)))
            .collect()
    } else {
        le::<8>(b, what)?.into_iter().map(f64::from_le_bytes).collect()
    })
}

pub(crate) fn usizes(b: &[u8], what: &str) -> PyResult<Vec<usize>> {
    le::<8>(b, what)?
        .into_iter()
        .map(|c| usize::try_from(i64::from_le_bytes(c)).map_err(|_| value_error(format!("{what}: a negative value"))))
        .collect()
}

pub(crate) fn flags(b: &[u8]) -> Vec<bool> {
    b.iter().map(|&v| v != 0).collect()
}

fn bool_bytes<'py>(py: Python<'py>, v: &[bool]) -> Bound<'py, PyBytes> {
    let b: Vec<u8> = v.iter().map(|&x| u8::from(x)).collect();
    PyBytes::new(py, &b)
}

fn floats_bytes<'py>(py: Python<'py>, v: &Floats) -> (Bound<'py, PyBytes>, bool) {
    match v {
        Floats::F32(a) => (
            PyBytes::new(py, &a.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()),
            true,
        ),
        Floats::F64(a) => (
            PyBytes::new(py, &a.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>()),
            false,
        ),
    }
}

/// A run from its dict (see the module's notes).
fn samples(d: &Bound<'_, PyDict>) -> PyResult<Samples> {
    let start = usizes(&bytes_of(d, "start")?.unwrap_or_default(), "start")?;
    let y_f32: bool = get(d, "y_f32")?;
    let y = f64s(&bytes_of(d, "y")?.unwrap_or_default(), y_f32, "y")?;
    let n = y.len();
    let normal = match bytes_of(d, "normal")? {
        Some(b) => {
            if get::<bool>(d, "normal_f32")? {
                let v: Vec<f32> = le::<4>(&b, "normal")?.into_iter().map(f32::from_le_bytes).collect();
                Normals::F32(v.as_chunks::<3>().0.to_vec())
            } else {
                let v: Vec<f64> = le::<8>(&b, "normal")?.into_iter().map(f64::from_le_bytes).collect();
                Normals::F64(v.as_chunks::<3>().0.to_vec())
            }
        }
        None => Normals::F32(vec![[0.0, 1.0, 0.0]; n]),
    };
    let hit = bytes_of(d, "hit")?
        .map(|b| -> PyResult<Vec<[u32; 3]>> {
            let w: Vec<u32> = le::<4>(&b, "hit")?.into_iter().map(u32::from_le_bytes).collect();
            Ok(w.as_chunks::<3>().0.to_vec())
        })
        .transpose()?;
    let header: Option<Vec<f64>> = opt(d, "header")?;
    let mut h = [0.0; 6];
    for (dst, v) in h.iter_mut().zip(header.unwrap_or_default()) {
        *dst = v;
    }
    let s = Samples {
        pitch: get(d, "pitch")?,
        min_x: get(d, "min_x")?,
        min_z: get(d, "min_z")?,
        nx: get(d, "nx")?,
        nz: get(d, "nz")?,
        header: h,
        start: start.into(),
        y: y.into(),
        y_f32,
        normal,
        flags: bytes_of(d, "flags")?.unwrap_or_else(|| vec![0; n]),
        hit,
    };
    s.check().map_err(value_error)?;
    Ok(s)
}

/// A run as a dict for `navsample.Samples` (`y` / `normal` in their dtypes, `hit` None when the run has none),
/// the arrays as bytearrays: NumPy's views over them are writable, as `navsample.load`'s arrays always were.
fn samples_dict<'py>(py: Python<'py>, s: &Samples) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("nx", s.nx)?;
    d.set_item("nz", s.nz)?;
    d.set_item("pitch", s.pitch)?;
    d.set_item("min_x", s.min_x)?;
    d.set_item("min_z", s.min_z)?;
    d.set_item("header", s.header.to_vec())?;
    let start: Vec<u8> = s.start.iter().flat_map(|&v| (v as i64).to_le_bytes()).collect();
    d.set_item("start", PyByteArray::new(py, &start))?;
    let y: Vec<u8> = if s.y_f32 {
        s.y.iter().flat_map(|&v| walk::narrow(v).to_le_bytes()).collect()
    } else {
        s.y.iter().flat_map(|v| v.to_le_bytes()).collect()
    };
    d.set_item("y", PyByteArray::new(py, &y))?;
    d.set_item("y_f32", s.y_f32)?;
    let (normal, nf32): (Vec<u8>, bool) = match &s.normal {
        Normals::F32(v) => (v.iter().flatten().flat_map(|x| x.to_le_bytes()).collect(), true),
        Normals::F64(v) => (v.iter().flatten().flat_map(|x| x.to_le_bytes()).collect(), false),
    };
    d.set_item("normal", PyByteArray::new(py, &normal))?;
    d.set_item("normal_f32", nf32)?;
    d.set_item("flags", PyByteArray::new(py, &s.flags))?;
    match &s.hit {
        Some(h) => d.set_item(
            "hit",
            PyByteArray::new(
                py,
                &h.iter().flatten().flat_map(|w| w.to_le_bytes()).collect::<Vec<u8>>(),
            ),
        )?,
        None => d.set_item("hit", py.None())?,
    }
    Ok(d)
}

/// Points (x, y, z): any sequences of three numbers (JSON's lists, tuples; ints or floats as Python's arithmetic
/// takes them).
pub(crate) fn points(v: Option<Vec<[f64; 3]>>) -> Vec<P3> {
    v.unwrap_or_default().into_iter().map(|[x, y, z]| (x, y, z)).collect()
}

/// Triangles: (float64 bytes, nine per triangle, the array was float32) or None.
pub(crate) fn tris(d: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<Tris>> {
    let Some(v) = item(d, key)? else { return Ok(None) };
    let (b, from_f32) = (raw(&v.get_item(0)?)?, v.get_item(1)?.extract::<bool>()?);
    let vals: Vec<f64> = le::<8>(&b, key)?.into_iter().map(f64::from_le_bytes).collect();
    let (whole, rest) = vals.as_chunks::<9>();
    if !rest.is_empty() {
        return Err(value_error(format!("{key}: not nine values per triangle")));
    }
    Ok(Some(Tris {
        tris: whole
            .iter()
            .map(|&[a, b, c, d, e, f, g, h, i]| [[a, b, c], [d, e, f], [g, h, i]])
            .collect(),
        from_f32,
    }))
}

/// A second run (start int64, y float64, flags uint8 bytes) or None.
fn capsule(d: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<Capsule>> {
    let Some(v) = item(d, key)? else { return Ok(None) };
    let (start, y, fl) = (raw(&v.get_item(0)?)?, raw(&v.get_item(1)?)?, raw(&v.get_item(2)?)?);
    Ok(Some(Capsule {
        start: usizes(&start, "start")?,
        y: f64s(&y, false, "y")?,
        flags: fl,
    }))
}

fn inputs(d: &Bound<'_, PyDict>) -> PyResult<WalkInputs> {
    let ladders: Vec<([f64; 3], [f64; 3])> = opt(d, "ladders")?.unwrap_or_default();
    Ok(WalkInputs {
        forbid: tris(d, "forbid")?,
        dynamic: tris(d, "dynamic")?,
        stair_tris: tris(d, "stair_tris")?,
        exclude: tris(d, "exclude")?,
        exclude_foot: tris(d, "exclude_foot")?,
        high: capsule(d, "high")?,
        narrow: capsule(d, "narrow")?,
        step: get(d, "step")?,
        terrain_seeds_only: get(d, "terrain_seeds_only")?,
        stairs: get(d, "stairs")?,
        raised_bars: get(d, "raised_bars")?,
        forbid_tol: get(d, "forbid_tol")?,
        extra_seeds: points(opt(d, "extra_seeds")?),
        force_walkable: bytes_of(d, "force_walkable")?.map(|b| flags(&b)),
        force_unwalkable: bytes_of(d, "force_unwalkable")?.map(|b| flags(&b)),
        unseeds: points(opt(d, "unseeds")?),
        soldier_points: points(opt(d, "soldier_points")?),
        ladders: ladders.into_iter().map(|(lo, hi)| Ladder { lo, hi }).collect(),
        anchors: points(opt(d, "anchors")?),
        objective_points: points(opt(d, "objective_points")?),
    })
}

pub(crate) fn settings(d: &Bound<'_, PyDict>) -> PyResult<WalkSettings> {
    let roof: (f64, f64, f64, f64) = get(d, "roof")?;
    Ok(WalkSettings {
        characters_absent: get(d, "characters_absent")?,
        headroom_band: opt(d, "headroom_band")?,
        steep_terrain: get(d, "steep_terrain")?,
        steep_max_cos: get(d, "steep_max_cos")?,
        steep_span: get(d, "steep_span")?,
        lip_width: opt(d, "lip_width")?,
        lip_object_rise: opt(d, "lip_object_rise")?,
        lip_object_slope_max: get(d, "lip_object_slope_max")?,
        lip_object_flat_all: opt(d, "lip_object_flat_all")?.unwrap_or(false),
        lip_flat_cos: opt(d, "lip_flat_cos")?,
        roof_prune: get(d, "roof_prune")?,
        roof: RoofCfg {
            raise: roof.0,
            share: roof.1,
            near: roof.2,
            near_dy: roof.3,
        },
        dynamic_absent_headroom: get(d, "dynamic_absent_headroom")?,
        dynamic_floor: opt(d, "dynamic_floor")?,
        objective_floor: get(d, "objective_floor")?,
        objective_reach: get(d, "objective_reach")?,
        sector: get(d, "sector")?,
        ladder_reach: get(d, "ladder_reach")?,
        ladder_foot: get(d, "ladder_foot")?,
        ladder_top: get(d, "ladder_top")?,
        terrain_vtable: get(d, "terrain_vtable")?,
        character_vtable: get(d, "character_vtable")?,
        slope: get(d, "slope")?,
        slope_cos: get(d, "slope_cos")?,
        slope_tan: get(d, "slope_tan")?,
    })
}

/// (steep stats, roof stats, objective stats, the ladders' kept ends).
fn stats_tuple<'py>(
    py: Python<'py>,
    s: &WalkStats,
    ladders: &[(Option<usize>, Option<usize>)],
) -> PyResult<Bound<'py, PyTuple>> {
    (s.steep.clone(), s.roof, s.objective, ladders.to_vec()).into_pyobject(py)
}

/// (keep, (gx bytes, gx float32), (gz bytes, gz float32), stair, `NavLevel`, stats).
fn walked(py: Python<'_>, w: Walked) -> PyResult<Bound<'_, PyTuple>> {
    let keep = bool_bytes(py, &w.keep);
    let (gx, gz, stair) = w.gradients();
    let (gx, gz, stair) = (floats_bytes(py, gx), floats_bytes(py, gz), bool_bytes(py, stair));
    let stats = stats_tuple(py, &w.stats, &w.ladder_pairs)?;
    let level = Py::new(py, NavLevel::from_level(w.level))?;
    (keep, gx, gz, stair, level, stats).into_pyobject(py)
}

/// `navwalk.walkable(s, ...)`: the run's dict, the inputs' and the settings' dicts (the wrapper's), answered as
/// (keep, (gx, f32), (gz, f32), stair, `NavLevel`, (steep stats, roof stats, objective stats, ladder ends)).
#[pyfunction]
pub fn navkit_walkable<'py>(
    py: Python<'py>,
    run: &Bound<'py, PyDict>,
    args: &Bound<'py, PyDict>,
    cfg: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyTuple>> {
    let (s, inp, set) = (samples(run)?, inputs(args)?, settings(cfg)?);
    let w = py.detach(|| walk::walkable(&s, &inp, &set)).map_err(value_error)?;
    walked(py, w)
}

pub(crate) fn region(v: &Bound<'_, PyAny>) -> PyResult<Region> {
    let kind: String = v.get_item(0)?.extract()?;
    match kind.as_str() {
        "box" => Ok(Region::Box(v.get_item(1)?.extract()?)),
        "polygon" => Ok(Region::Polygon {
            y: v.get_item(1)?.extract()?,
            poly: v.get_item(2)?.extract()?,
        }),
        _ => Err(value_error(format!("a region of kind {kind:?}"))),
    }
}

pub(crate) fn regions(v: &Bound<'_, PyList>) -> PyResult<Vec<(bool, Region)>> {
    v.iter()
        .map(|o| Ok((o.get_item(0)?.extract::<bool>()?, region(&o.get_item(1)?)?)))
        .collect()
}

pub(crate) fn water(d: &Bound<'_, PyDict>) -> PyResult<Option<WaterMesh>> {
    let Some(w) = item(d, "water")? else { return Ok(None) };
    let (v, t) = (raw(&w.get_item(0)?)?, raw(&w.get_item(1)?)?);
    let v: Vec<f64> = le::<8>(&v, "vertices")?.into_iter().map(f64::from_le_bytes).collect();
    let t: Vec<i64> = le::<8>(&t, "triangles")?.into_iter().map(i64::from_le_bytes).collect();
    if !v.len().is_multiple_of(3) || !t.len().is_multiple_of(3) {
        return Err(value_error("water: not three values per vertex / triangle"));
    }
    Ok(Some((v.as_chunks::<3>().0.to_vec(), t.as_chunks::<3>().0.to_vec())))
}

/// `navbuild.stage`'s walkability (`walk::stage`): `stage` holds `samples` (the file), `high` / `narrow` (files or
/// None), `pitch` (the stage's), `coarse_pitch` (`navwalk.PITCH`), `edits` (a list of (walkable, region) or None),
/// `water` ((vertices float64 bytes, triangles int64 bytes) or None) with `max_water_depth`, `bounds` (or None);
/// `args` the walkable inputs but the forced flags and runs; `cfg` the settings. Answers (the run's dict, (edit
/// counts, water count, outside count), then `navkit_walkable`'s answer).
#[pyfunction]
pub fn navkit_walk_stage<'py>(
    py: Python<'py>,
    stage: &Bound<'py, PyDict>,
    args: &Bound<'py, PyDict>,
    cfg: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyTuple>> {
    let path = |k: &str| -> PyResult<Option<PathBuf>> { Ok(opt::<String>(stage, k)?.map(PathBuf::from)) };
    let edits = item(stage, "edits")?
        .map(|v| v.cast_into::<PyList>().map_err(PyErr::from).and_then(|l| regions(&l)))
        .transpose()?;
    let limit: Option<f64> = opt(stage, "max_water_depth")?;
    let si = StageInputs {
        samples: path("samples")?.ok_or_else(|| value_error("samples: missing"))?,
        high: path("high")?,
        narrow: path("narrow")?,
        pitch: get(stage, "pitch")?,
        coarse_pitch: get(stage, "coarse_pitch")?,
        edits,
        water: match (water(stage)?, limit) {
            (Some(m), Some(l)) => Some((m, l)),
            _ => None,
        },
        bounds: opt(stage, "bounds")?,
        terrain_fill: None,
    };
    let (inp, set) = (inputs(args)?, settings(cfg)?);
    let (s, counts, w) = py.detach(|| walk::stage(&si, inp, &set)).map_err(value_error)?;
    let run = samples_dict(py, &s)?;
    drop(s);
    let counts = (counts.edits, counts.water, counts.outside).into_pyobject(py)?;
    let rest = walked(py, w)?;
    let mut out: Vec<Bound<'py, PyAny>> = vec![run.into_any(), counts.into_any()];
    out.extend(rest.iter());
    PyTuple::new(py, out)
}

/// `navsample.load(path)` of a version 3 file read a block of records at a time (round NK4: without the whole file
/// in memory beside its arrays): (the run's dict, the columns read), or None for any other file (`navsample.load`
/// reads it as before and says what is wrong).
#[pyfunction]
pub fn navkit_navsample_stream<'py>(py: Python<'py>, path: &str) -> PyResult<Option<(Bound<'py, PyDict>, usize)>> {
    let p = PathBuf::from(path);
    let got = py.detach(|| Samples::load_v3(&p)).map_err(value_error)?;
    got.map(|(s, read)| Ok((samples_dict(py, &s)?, read))).transpose()
}

/// `navwalk.coarse(f)` on a run's dict: the coarse run's dict.
#[pyfunction]
pub fn navkit_coarse<'py>(py: Python<'py>, run: &Bound<'py, PyDict>, pitch: f64) -> PyResult<Bound<'py, PyDict>> {
    let s = samples(run)?;
    let c = py.detach(|| s.coarse(pitch)).map_err(value_error)?;
    samples_dict(py, &c)
}

/// `navedits.edits_for`'s masks: (forced walkable, forced unwalkable), one byte per surface.
#[pyfunction]
pub fn navkit_edit_masks<'py>(
    py: Python<'py>,
    run: &Bound<'py, PyDict>,
    ops: &Bound<'py, PyList>,
) -> PyResult<(Bound<'py, PyBytes>, Bound<'py, PyBytes>)> {
    let (s, ops) = (samples(run)?, regions(ops)?);
    let (fw, fu) = py.detach(|| walk::edit_masks(&s, &ops)).map_err(value_error)?;
    Ok((bool_bytes(py, &fw), bool_bytes(py, &fu)))
}

/// `navedits.region_mask(x, z, y, region)` (float64 bytes each): one byte per point.
#[pyfunction]
pub fn navkit_region_mask<'py>(
    py: Python<'py>,
    x: &Bound<'py, PyBytes>,
    z: &Bound<'py, PyBytes>,
    y: &Bound<'py, PyBytes>,
    region_: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    let (xs, zs, ys) = (
        f64s(x.as_bytes(), false, "x")?,
        f64s(z.as_bytes(), false, "z")?,
        f64s(y.as_bytes(), false, "y")?,
    );
    if xs.len() != zs.len() || xs.len() != ys.len() {
        return Err(value_error("region_mask: x, z and y differ in length"));
    }
    let r = region(region_)?;
    let pts = xs.iter().zip(&zs).zip(&ys).map(|((&x, &z), &y)| (x, z, y));
    let m = walk::region_mask_at(pts, &r).map_err(value_error)?;
    Ok(bool_bytes(py, &m))
}

/// `navbuild.water_tops` on a grid (nx, nz, pitch, min x, min z): float64 bytes per column.
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the grid and the mesh")]
pub fn navkit_water_tops<'py>(
    py: Python<'py>,
    nx: usize,
    nz: usize,
    pitch: f64,
    min_x: f64,
    min_z: f64,
    vertices: &Bound<'py, PyBytes>,
    triangles: &Bound<'py, PyBytes>,
) -> PyResult<Bound<'py, PyBytes>> {
    let d = PyDict::new(py);
    d.set_item("water", (vertices, triangles))?;
    let (v, t) = water(&d)?.unwrap_or_default();
    let top = py
        .detach(|| walk::water_tops(nx, nz, pitch, min_x, min_z, &v, &t))
        .map_err(value_error)?;
    Ok(PyBytes::new(
        py,
        &top.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>(),
    ))
}

/// `navbuild.deep_water` on a run's dict with the columns' water tops (float64 bytes): one byte per surface.
#[pyfunction]
pub fn navkit_deep_water<'py>(
    py: Python<'py>,
    run: &Bound<'py, PyDict>,
    top: &Bound<'py, PyBytes>,
    limit: f64,
) -> PyResult<Bound<'py, PyBytes>> {
    let s = samples(run)?;
    let t = f64s(top.as_bytes(), false, "top")?;
    Ok(bool_bytes(py, &walk::deep_water(&s, &t, limit)))
}

/// `navbuild.outside_bounds` on a run's dict: one byte per surface.
#[pyfunction]
pub fn navkit_outside_bounds<'py>(
    py: Python<'py>,
    run: &Bound<'py, PyDict>,
    bounds: [f64; 4],
) -> PyResult<Bound<'py, PyBytes>> {
    let s = samples(run)?;
    Ok(bool_bytes(py, &walk::outside_bounds(&s, bounds)))
}

/// `navwalk.ladder_ends(s, ok, keep, lad)` for each ladder ((lo, hi)): [(foot, top)].
#[pyfunction]
pub fn navkit_ladder_ends<'py>(
    run: &Bound<'py, PyDict>,
    ok: &Bound<'py, PyBytes>,
    keep: &Bound<'py, PyBytes>,
    ladders: Vec<([f64; 3], [f64; 3])>,
    cfg: &Bound<'py, PyDict>,
) -> PyResult<Vec<(Option<usize>, Option<usize>)>> {
    let (s, set) = (samples(run)?, settings(cfg)?);
    let (ok, keep) = (flags(ok.as_bytes()), flags(keep.as_bytes()));
    if ok.len() != s.len() || keep.len() != s.len() {
        return Err(value_error("ladder_ends: not a flag per surface"));
    }
    ladders
        .iter()
        .map(|&(lo, hi)| walk::ladder_ends(&s, &ok, &keep, &Ladder { lo, hi }, &set).map_err(value_error))
        .collect()
}

/// `navwalk.near_points(s, points, grow, below, above)`: one byte per surface.
#[pyfunction]
pub fn navkit_near_points<'py>(
    py: Python<'py>,
    run: &Bound<'py, PyDict>,
    pts: Vec<[f64; 3]>,
    grow: f64,
    below: f64,
    above: f64,
) -> PyResult<Bound<'py, PyBytes>> {
    let s = samples(run)?;
    let m = walk::near_points(&s, &points(Some(pts)), grow, below, above).map_err(value_error)?;
    Ok(bool_bytes(py, &m))
}
