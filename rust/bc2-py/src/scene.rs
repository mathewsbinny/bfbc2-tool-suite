//! The binding of the Rust scene (`bc2_scene::scene`): `core/bfbc2core/level.py`'s `load` calls `scene_load` when the
//! module has it and builds its `Level` from the result (keeping its own element trees for `Entity.element`).

use std::path::PathBuf;

use bc2_scene::scene::{self, LoadOptions, Source};
use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyList, PyTuple};

fn le<T: Copy, const N: usize>(values: impl IntoIterator<Item = T>, to_le: fn(T) -> [u8; N]) -> Vec<u8> {
    values.into_iter().flat_map(to_le).collect()
}

/// `OSError` for a file that cannot be read, `ValueError` for the rest, as `level.load` raises them.
fn scene_error(e: scene::SceneError) -> PyErr {
    match e {
        scene::SceneError::Io { .. } => PyOSError::new_err(e.to_string()),
        other => PyValueError::new_err(other.to_string()),
    }
}

/// `level.load`'s level through Rust: a dict with `entities` (tuples: index, guid, type, short type, category, id,
/// name, partition, matrix (16 floats) or None, blueprint, group, blueprint type, destructible, team, ai, (file,
/// instance, item: -1 for an instance of the level partition)), `shapes` (entity, kind, points, closed, height,
/// field), `collision` (None, or (triangles by blueprint as `f32` x 9 bytes or None, meshes (entity, blueprint),
/// missing)), `terrain` (None, or (min, max, size, raw `u16` bytes, heights `f32` bytes, vertices `f32` x 3 bytes,
/// triangles `u32` x 3 bytes)), `water` (None, or (vertices, triangles, shader per triangle or None, slots or None))
/// and `notes` ((kind, text): `aliases`, `terrain`, `water`). `alias_cache`: the mesh-alias cache's folder, or `None`.
#[pyfunction]
#[pyo3(signature = (name, partition, groups, bases, geometry, heightfield, terrain_stride, water_mesh, water_visual, alias_cache=None))]
#[expect(clippy::too_many_arguments, reason = "level.load's inputs, one per argument")]
pub fn scene_load<'py>(
    py: Python<'py>,
    name: String,
    partition: PathBuf,
    groups: Vec<PathBuf>,
    bases: Vec<PathBuf>,
    geometry: bool,
    heightfield: Option<PathBuf>,
    terrain_stride: usize,
    water_mesh: Option<PathBuf>,
    water_visual: Option<PathBuf>,
    alias_cache: Option<PathBuf>,
) -> PyResult<Bound<'py, PyDict>> {
    let options = LoadOptions {
        name,
        partition,
        groups,
        bases,
        geometry,
        heightfield,
        terrain_stride,
        water: water_mesh.map(|m| (m, water_visual)),
        alias_cache,
    };
    let s = py.detach(|| scene::load(&options)).map_err(scene_error)?;
    let out = PyDict::new(py);
    let entities = PyList::empty(py);
    for e in &s.entities {
        let source = match e.source {
            Source::Instance { instance } => (0, instance, -1),
            Source::GroupMember { file, instance, item } => (file, instance, item as isize),
        };
        let fields: Vec<Bound<'py, PyAny>> = vec![
            e.index.into_pyobject(py)?.into_any(),
            e.guid.as_str().into_pyobject(py)?.into_any(),
            e.type_name.as_str().into_pyobject(py)?.into_any(),
            e.short_type.as_str().into_pyobject(py)?.into_any(),
            e.category.as_str().into_pyobject(py)?.into_any(),
            e.id.as_deref().into_pyobject(py)?.into_any(),
            e.name.as_deref().into_pyobject(py)?.into_any(),
            e.partition.as_str().into_pyobject(py)?.into_any(),
            e.matrix.map(|m| m.to_vec()).into_pyobject(py)?.into_any(),
            e.blueprint.as_deref().into_pyobject(py)?.into_any(),
            e.group.as_deref().into_pyobject(py)?.into_any(),
            e.blueprint_type.as_deref().into_pyobject(py)?.into_any(),
            e.destructible.into_pyobject(py)?.to_owned().into_any(),
            e.team.into_pyobject(py)?.into_any(),
            e.ai.into_pyobject(py)?.into_any(),
            source.into_pyobject(py)?.into_any(),
        ];
        entities.append(PyTuple::new(py, fields)?)?;
    }
    out.set_item("entities", entities)?;
    let shapes = PyList::empty(py);
    for sh in &s.shapes {
        shapes.append((sh.entity, sh.kind, sh.points.clone(), sh.closed, sh.height, sh.field))?;
    }
    out.set_item("shapes", shapes)?;
    match &s.collision {
        Some(c) => {
            let tris = PyDict::new(py);
            let mut bps: Vec<_> = c.blueprints.iter().collect();
            bps.sort_by(|a, b| a.0.cmp(b.0)); // the dict in name order, the same on every run
            for (bp, t) in bps {
                let v = t
                    .as_ref()
                    .map(|t| PyByteArray::new(py, &le(t.iter().flatten().flatten().copied(), f32::to_le_bytes)));
                tris.set_item(bp, v)?;
            }
            out.set_item("collision", (tris, c.meshes.clone(), c.missing.clone()))?;
        }
        None => out.set_item("collision", py.None())?,
    }
    match (&s.heightfield, &s.terrain) {
        (Some(hf), Some((verts, tris))) => out.set_item(
            "terrain",
            (
                hf.min,
                hf.max,
                hf.size,
                PyBytes::new(py, &le(hf.raw.iter().copied(), u16::to_le_bytes)),
                PyByteArray::new(py, &le(hf.heights(), f32::to_le_bytes)),
                PyByteArray::new(py, &le(verts.iter().flatten().copied(), f32::to_le_bytes)),
                PyByteArray::new(py, &le(tris.iter().flatten().copied(), u32::to_le_bytes)),
            ),
        )?,
        _ => out.set_item("terrain", py.None())?,
    }
    match &s.water {
        Some(w) => {
            let (shader, slots) = w.shaders.clone().map_or((None, None), |(a, b)| (Some(a), Some(b)));
            out.set_item(
                "water",
                (
                    PyByteArray::new(py, &le(w.mesh.vertices.iter().flatten().copied(), f32::to_le_bytes)),
                    PyByteArray::new(py, &le(w.mesh.triangles.iter().flatten().copied(), u32::to_le_bytes)),
                    shader,
                    slots,
                ),
            )?;
        }
        None => out.set_item("water", py.None())?,
    }
    out.set_item("notes", s.notes.clone())?;
    Ok(out)
}
