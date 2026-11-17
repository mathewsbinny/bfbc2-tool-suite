//! Bindings for `bc2-convert`, the converter pipeline's hot spots: `bfbc2-console-convert/texture.py`'s `convert`
//! and `convert_many` are `texture_convert` and `texture_convert_many`, a `ValueError` becoming its `ConvertError`
//! with the same message (its Python converter was retired on 2026-11-17).
//!
//! `bfbc2-console-convert/audio.py`'s EA-XAS encoder and decoder are `xas_encode`, `xas_encode_many` and
//! `xas_decode_frames`; `audio.py` keeps its NumPy code as the fallback and the reference.

use bc2_convert::{audio, texture};
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
