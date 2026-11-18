//! Bindings for `bc2-convert`, the converter pipeline's hot spots: `bfbc2-console-convert/texture.py`'s `convert`
//! and `convert_many` are `texture_convert` and `texture_convert_many`, a `ValueError` becoming its `ConvertError`
//! with the same message (its Python converter was retired on 2026-11-17).
//!
//! `bfbc2-console-convert/audio.py`'s EA-XAS encoder and decoder are `xas_encode`, `xas_encode_many` and
//! `xas_decode_frames`; `audio.py` keeps its NumPy code as the fallback and the reference.
//!
//! Round E3g: `mesh.py`'s vertex loop is `mesh_vertices` and `havok.py`'s `Packfile.convert` is
//! `havok_convert_packfile` (on a `HavokClasses`, the class layouts converted once). Both return `None` where the
//! reference is to run instead (a conversion it refuses or reads short); the Python stays the fallback.

use bc2_convert::{audio, havok, mesh, texture};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyBytesMethods, PyList};

/// `texture.convert`: the PC texture's bytes, or `ValueError` with the reason (the retired reference's
/// `ConvertError` message, or that the layout reads past the data).
#[pyfunction]
pub fn texture_convert<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Bound<'py, PyBytes>> {
    let bytes = data.as_bytes();
    let out = py
        .detach(|| texture::convert(bytes))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PyBytes::new(py, &out))
}

/// `texture_convert` of many textures on all cores: each result the bytes or the `ValueError`, returned, not raised.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn texture_convert_many<'py>(py: Python<'py>, items: Vec<Bound<'py, PyBytes>>) -> PyResult<Bound<'py, PyList>> {
    let slices: Vec<&[u8]> = items.iter().map(PyBytesMethods::as_bytes).collect();
    let results = py.detach(|| texture::convert_all(&slices));
    let out = PyList::empty(py);
    for r in results {
        match r {
            Ok(b) => out.append(PyBytes::new(py, &b))?,
            Err(e) => out.append(PyValueError::new_err(e.to_string()).into_value(py))?,
        }
    }
    Ok(out)
}

/// Little-endian int16 samples of `b` (a trailing odd byte is a `ValueError`).
fn i16_samples(b: &[u8]) -> PyResult<Vec<i16>> {
    let (pairs, rest) = b.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(PyValueError::new_err(format!("{} bytes of int16 samples", b.len())));
    }
    Ok(pairs.iter().map(|p| i16::from_le_bytes(*p)).collect())
}

/// `audio.xas_encode_frames`: the frames (frame-major, 76 bytes per frame and channel) of `pcm`, little-endian
/// int16 samples interleaved over `channels`.
#[pyfunction]
pub fn xas_encode<'py>(
    py: Python<'py>,
    pcm: &Bound<'py, PyBytes>,
    channels: usize,
) -> PyResult<Bound<'py, PyByteArray>> {
    let samples = i16_samples(pcm.as_bytes())?;
    let out = py
        .detach(|| audio::encode(&samples, channels))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PyByteArray::new(py, &out))
}

/// `xas_encode` of many `(pcm, channels)` on all cores: each result the frames or the `ValueError`, returned,
/// not raised.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn xas_encode_many<'py>(py: Python<'py>, items: Vec<(Bound<'py, PyBytes>, usize)>) -> PyResult<Bound<'py, PyList>> {
    let samples: Vec<PyResult<Vec<i16>>> = items.iter().map(|(b, _)| i16_samples(b.as_bytes())).collect();
    let jobs: Vec<(&[i16], usize)> = samples
        .iter()
        .zip(&items)
        .map(|(s, (_, ch))| (s.as_ref().map_or(&[][..], Vec::as_slice), *ch))
        .collect();
    let results = py.detach(|| audio::encode_all(&jobs));
    let out = PyList::empty(py);
    for (r, s) in results.into_iter().zip(samples) {
        match (s, r) {
            (Err(e), _) => out.append(e.into_value(py))?,
            (Ok(_), Ok(b)) => out.append(PyByteArray::new(py, &b))?,
            (Ok(_), Err(e)) => out.append(PyValueError::new_err(e.to_string()).into_value(py))?,
        }
    }
    Ok(out)
}

/// `audio.xas_decode_frames`: little-endian int16 samples, 128 per 76-byte frame of `data`.
#[pyfunction]
pub fn xas_decode_frames<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Bound<'py, PyByteArray>> {
    let bytes = data.as_bytes();
    let pcm = py
        .detach(|| audio::decode_frames(bytes))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    let out: Vec<u8> = pcm.iter().flat_map(|v| v.to_le_bytes()).collect();
    Ok(PyByteArray::new(py, &out))
}

/// One vertex element pair for `mesh_vertices`: (console format, its offset, PC format, its offset).
type ElementPair = (u8, usize, u8, usize);

/// `mesh.py`'s `convert` vertex loop (`emit`): console vertices `first .. first + count` of the region at `src`
/// (stride `sc`) as PC vertices of `sp` bytes; `None` where the reference must run (`bc2_convert::mesh`).
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the reference's emit arguments")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn mesh_vertices<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    src: usize,
    sc: usize,
    sp: usize,
    first: usize,
    count: usize,
    pairs: Vec<ElementPair>,
    colour: bool,
) -> Option<Bound<'py, PyBytes>> {
    let bytes = data.as_bytes();
    let pairs: Vec<mesh::Pair> = pairs
        .iter()
        .map(|&(fc, oc, fp, op)| mesh::Pair { fc, oc, fp, op })
        .collect();
    let out = py.detach(|| mesh::convert_vertices(bytes, src, sc, sp, first, count, &pairs, colour))?;
    Some(PyBytes::new(py, &out))
}

/// One member of a `HavokClasses` row: (name, class, type, subtype, C array size, offset).
type MemberRow = (Option<String>, Option<String>, u8, u8, i16, u32);

/// `havok.classes(exe)` for `havok_convert_packfile`: the layouts given as (name, parent, size, members) rows,
/// members flattened and resolved once.
#[pyclass(frozen)]
pub struct HavokClasses {
    classes: havok::Classes,
}

#[pymethods]
impl HavokClasses {
    #[new]
    fn new(rows: Vec<(String, Option<String>, u32, Vec<MemberRow>)>) -> Self {
        let klasses: Vec<havok::Klass> = rows
            .into_iter()
            .map(|(name, parent, size, members)| havok::Klass {
                name,
                parent,
                size,
                members: members
                    .into_iter()
                    .map(|(name, cls, ty, subtype, carray, offset)| havok::Member {
                        name,
                        cls,
                        ty,
                        subtype,
                        carray,
                        offset,
                    })
                    .collect(),
            })
            .collect();
        HavokClasses {
            classes: havok::Classes::new(&klasses),
        }
    }

    fn __len__(&self) -> usize {
        self.classes.len()
    }
}

/// `havok.Packfile(data, start, classes).convert()`: the whole buffer, the packfile at `start` in the PC's byte
/// order; `None` where the reference is to run (it refuses, or reads short: `bc2_convert::havok`).
#[pyfunction]
pub fn havok_convert_packfile<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    start: usize,
    classes: &HavokClasses,
) -> Option<Bound<'py, PyBytes>> {
    let bytes = data.as_bytes();
    let out = py
        .detach(|| havok::convert_packfile(bytes, start, &classes.classes))
        .ok()?;
    Some(PyBytes::new(py, &out))
}

/// `havok.verify`'s packfile part (round E3o): the packfile at `start` of `console` converted and compared with
/// `pc` from `start` on - each differing byte's `(offset, reason)`, the reason `havok.ignorable`'s text or `None`;
/// `None` where the reference is to run (`bc2_convert::havok::verify_packfile`).
#[pyfunction]
pub fn havok_verify_packfile(
    py: Python<'_>,
    console: &Bound<'_, PyBytes>,
    start: usize,
    classes: &HavokClasses,
    pc: &Bound<'_, PyBytes>,
) -> Option<Vec<(usize, Option<&'static str>)>> {
    let (c, p) = (console.as_bytes(), pc.as_bytes());
    let got = py
        .detach(|| havok::verify_packfile(c, start, &classes.classes, p))
        .ok()?;
    Some(got.into_iter().map(|(i, w)| (i, w.map(havok::Why::text))).collect())
}
