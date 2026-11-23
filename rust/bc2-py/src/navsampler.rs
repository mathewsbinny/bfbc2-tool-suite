//! Bindings for NavKit's offline sampler (round NKS-deep C, `bc2_navbuild::sampler`): `navsample_offline.sample_grid`
//! on geometry and a height field in memory (the Python's drivers, the tests), and the level driver (`navkit.py
//! sample --offline`).

use bc2_navbuild::inputs::{Root, quiet_tool};
use bc2_navbuild::ntpath::Fail;
use bc2_navbuild::offline::Field;
use bc2_navbuild::sampler::level::{self, Detailed, LevelOptions};
use bc2_navbuild::sampler::{self, Body, Geometry, Grid};
use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods};

fn value_error(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// The driver's error as Python's: an `OSError` kind as `OSError`, the others as `ValueError` (its kind in front).
fn raised(e: Fail) -> PyErr {
    match e {
        Fail::Raised(r)
            if r.kind.ends_with("Error")
                && (r.kind.contains("OS") || r.kind.contains("File") || r.kind.contains("Permission")) =>
        {
            PyOSError::new_err(r.msg)
        }
        Fail::Raised(r) => PyValueError::new_err(format!("{}: {}", r.kind, r.msg)),
        other => value_error(other),
    }
}

fn exact<const N: usize>(b: &[u8], what: &str) -> PyResult<Vec<[u8; N]>> {
    let (whole, rest) = b.as_chunks::<N>();
    if !rest.is_empty() {
        return Err(value_error(format!("{what}: {} bytes, not a multiple of {N}", b.len())));
    }
    Ok(whole.to_vec())
}

/// A grid as the Python's `Grid` holds it: `(pitch, min_x, min_z, nx, nz, y_bottom, y_top)`.
type PyGridArgs = (f64, f64, f64, usize, usize, f64, f64);

/// `sample_grid(grid, geom, hf, bottoms)` in the Rust core: `tris` the triangles (float64, n x 3 x 3), `kind` /
/// `vt` (uint8), `solid` (int32); `heights` the height field (float32, size x size; `None`: no terrain) with its
/// `hf_min` (x, z) and `cell`; `mask` the grid's (bool, nz x nx) or `None`; one soldier capsule run per bottom, and
/// with `vehicle` a vehicle-shaped run last. Returns `(counts uint8, y float32, normal int8 n x 3, kind uint8,
/// [flags uint8 per run])`.
#[pyfunction]
#[pyo3(signature = (tris, kind, solid, vt, heights, size, hf_min, cell, grid, mask, bottoms, vehicle=false))]
#[expect(clippy::too_many_arguments, reason = "the sampler's inputs as the Python holds them")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
#[expect(clippy::type_complexity, reason = "the arrays the Python makes its Samples from")]
pub fn navkit_offline_sample<'py>(
    py: Python<'py>,
    tris: &Bound<'py, PyBytes>,
    kind: &Bound<'py, PyBytes>,
    solid: &Bound<'py, PyBytes>,
    vt: &Bound<'py, PyBytes>,
    heights: Option<&Bound<'py, PyBytes>>,
    size: usize,
    hf_min: (f64, f64),
    cell: (f64, f64),
    grid: PyGridArgs,
    mask: Option<&Bound<'py, PyBytes>>,
    bottoms: Vec<f64>,
    vehicle: bool,
) -> PyResult<(
    Bound<'py, PyBytes>,
    Bound<'py, PyBytes>,
    Bound<'py, PyBytes>,
    Bound<'py, PyBytes>,
    Vec<Bound<'py, PyBytes>>,
)> {
    let v: Vec<f64> = exact::<8>(tris.as_bytes(), "tris")?
        .into_iter()
        .map(f64::from_le_bytes)
        .collect();
    let (nine, rest) = v.as_chunks::<9>();
    if !rest.is_empty() {
        return Err(value_error("tris: not n x 3 x 3 values"));
    }
    let geo = Geometry {
        tris: nine
            .iter()
            .map(|&[a, b, c, d, e, f, g, h, i]| [[a, b, c], [d, e, f], [g, h, i]])
            .collect(),
        kind: kind.as_bytes().to_vec(),
        solid: exact::<4>(solid.as_bytes(), "solid")?
            .into_iter()
            .map(i32::from_le_bytes)
            .collect(),
        vt: vt.as_bytes().to_vec(),
    };
    let h: Option<Vec<f32>> = heights
        .map(|b| exact::<4>(b.as_bytes(), "heights").map(|v| v.into_iter().map(f32::from_le_bytes).collect()))
        .transpose()?;
    if h.as_ref().is_some_and(|h| size < 2 || h.len() != size * size) {
        return Err(value_error("heights: not a size x size float32 array"));
    }
    let (pitch, min_x, min_z, nx, nz, y_bottom, y_top) = grid;
    let g = Grid {
        pitch,
        min_x,
        min_z,
        nx,
        nz,
        y_bottom,
        y_top,
        mask: mask.map(|m| m.as_bytes().iter().map(|&b| b != 0).collect()),
    };
    let mut bodies: Vec<Body> = bottoms.iter().map(|&b| Body::soldier(b)).collect();
    if vehicle {
        bodies.push(Body::vehicle());
    }
    let (out, _) = py
        .detach(|| {
            let field = h.as_ref().map(|h| Field {
                heights: h,
                size,
                min: hf_min,
                cell,
            });
            sampler::sample(&g, &geo, field.as_ref(), &bodies)
        })
        .map_err(value_error)?;
    let Some(first) = out.first() else {
        return Err(value_error("no runs asked for"));
    };
    let s = &first.surfaces;
    let ys: Vec<u8> = s.y.iter().flat_map(|v| v.to_le_bytes()).collect();
    let ns: Vec<u8> = s.normal.iter().flat_map(|n| n.map(|c| c as u8)).collect();
    Ok((
        PyBytes::new(py, &s.counts),
        PyBytes::new(py, &ys),
        PyBytes::new(py, &ns),
        PyBytes::new(py, &s.kind),
        out.iter().map(|r| PyBytes::new(py, &r.flags)).collect(),
    ))
}

/// `navkit.py sample <level> --offline`: the level's runs sampled offline (`bc2_navbuild::sampler::level::
/// sample_level`) into `out` (default `work\navsample_offline`), each line to `log`. Returns `(the detailed sectors
/// sampled at 0.2, of them DICE's only)`.
#[pyfunction]
#[pyo3(signature = (root, level, coarse=true, fine=true, dice=false, vehicle=false, out=None, log=None))]
#[expect(
    clippy::too_many_arguments,
    clippy::fn_params_excessive_bools,
    reason = "the command line's switches"
)]
#[expect(clippy::type_complexity, reason = "two lists of sector ids")]
pub fn navkit_offline_sample_level(
    root: &str,
    level: &str,
    coarse: bool,
    fine: bool,
    dice: bool,
    vehicle: bool,
    out: Option<String>,
    log: Option<&Bound<'_, PyAny>>,
) -> PyResult<(Vec<(i64, i64)>, Vec<(i64, i64)>)> {
    let r = Root::new(root).map_err(raised)?;
    let opt = LevelOptions {
        coarse,
        detailed: fine.then_some(if dice { Detailed::WithDice } else { Detailed::Own }),
        vehicle,
        out,
        ..LevelOptions::default()
    };
    let mut err: Option<PyErr> = None;
    let mut sink = |s: &str| -> bc2_navbuild::ntpath::Res<()> {
        if let Some(f) = log
            && let Err(e) = f.call1((s,))
        {
            err.get_or_insert(e);
            return Err(Fail::Sink);
        }
        Ok(())
    };
    let got = level::sample_level(&r, &level.to_lowercase(), &opt, &mut sink, &quiet_tool);
    if let Some(e) = err {
        return Err(e);
    }
    let rep = got.map_err(raised)?;
    Ok((rep.detailed, rep.dice_only))
}
