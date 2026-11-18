//! Round F0: Frostbite Wave resources and EA's stream framing (`bc2_formats::wave`) for
//! `bfbc2-console-convert/audio.py`: `wave_read` / `wave_write_pc`, `sps_read` / `snr_read` / `sps_write`. Each
//! answers `None` where the reference raises, or where a value is not one the port reads exactly as the reference does
//! (a segment that is not a pair of `bytes`, a header word past `u32`); the Python then runs, so a result here is
//! always the reference's and the errors stay its own.

use bc2_formats::wave::{self, Source, Stream};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyTuple};

use crate::pathdata::{items, u32_of};

fn source_of(s: &str) -> Option<Source> {
    match s {
        "bfbc2-xbox" => Some(Source::Bfbc2Xbox),
        "bfbc2-pc" => Some(Source::Bfbc2Pc),
        "bf1943" => Some(Source::Bf1943),
        _ => None,
    }
}

fn stream<'py>(py: Python<'py>, st: &Stream) -> PyResult<Bound<'py, PyTuple>> {
    let blocks = PyList::empty(py);
    for (n, data) in &st.blocks {
        blocks.append((n, PyBytes::new(py, data)))?;
    }
    (st.version, st.codec, st.channels, st.rate, st.type_, st.samples, blocks).into_pyobject(py)
}

/// `audio.read_wave(b, source)`'s values: `(flags_id, total, intro, channels, outro, word14, segments, rate, word04,
/// loop_end)` (the last three `None` for BFBC2), each segment `None` or `(data, info)`; `None` where the reference
/// raises.
#[pyfunction]
pub fn wave_read<'py>(py: Python<'py>, b: &Bound<'py, PyBytes>, source: &str) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let Some(src) = source_of(source) else {
        return Ok(None);
    };
    let bytes = b.as_bytes();
    let Ok(w) = py.detach(|| wave::read_wave(bytes, src)) else {
        return Ok(None);
    };
    let segs = PyList::empty(py);
    for s in &w.segments {
        match s {
            None => segs.append(py.None())?,
            Some((data, info)) => segs.append((PyBytes::new(py, data), PyBytes::new(py, info)))?,
        }
    }
    let (rate, w04, loop_end) = match w.bf1943 {
        Some((r, a, b)) => (Some(f64::from(r)), Some(a), Some(b)),
        None => (None, None, None),
    };
    Ok(Some(
        (
            w.flags_id, w.total, w.intro, w.channels, w.outro, w.word14, segs, rate, w04, loop_end,
        )
            .into_pyobject(py)?,
    ))
}

/// A `(data, info)` pair of `bytes`.
fn pair<'py>(o: &Bound<'py, PyAny>) -> Option<(Bound<'py, PyBytes>, Bound<'py, PyBytes>)> {
    let v = items(o)?;
    let [a, b] = v.as_slice() else { return None };
    Some((a.cast::<PyBytes>().ok()?.clone(), b.cast::<PyBytes>().ok()?.clone()))
}

/// `audio.write_wave_pc(w)`, or `None` where the reference raises or the port leaves it to the reference.
#[pyfunction]
pub fn wave_write_pc<'py>(py: Python<'py>, w: &Bound<'py, PyAny>) -> Option<Bound<'py, PyBytes>> {
    let parts = (|| {
        let words = items(&w.call_method0("words").ok()?)?;
        let words: Vec<u32> = words.iter().map(u32_of).collect::<Option<_>>()?;
        let words: [u32; 6] = words.try_into().ok()?;
        let segs = items(&w.getattr("segments").ok()?)?;
        if segs.len() != 3 {
            return None;
        }
        let segs = segs
            .iter()
            .map(|s| if s.is_none() { Some(None) } else { pair(s).map(Some) })
            .collect::<Option<Vec<_>>>()?;
        Some((words, segs))
    })();
    let (words, segs) = parts?;
    let view =
        |i: usize| -> Option<(&[u8], &[u8])> { segs.get(i)?.as_ref().map(|(a, b)| (a.as_bytes(), b.as_bytes())) };
    let out = wave::write_wave_pc(&words, &[view(0), view(1), view(2)]);
    out.ok().map(|b| PyBytes::new(py, &b))
}

/// `audio.read_sps(s)`'s values `(version, codec, channels, rate, type_, samples, blocks)`, or `None` where the
/// reference raises.
#[pyfunction]
pub fn sps_read<'py>(py: Python<'py>, s: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let bytes = s.as_bytes();
    match py.detach(|| wave::read_sps(bytes)) {
        Ok(st) => Ok(Some(stream(py, &st)?)),
        Err(_) => Ok(None),
    }
}

/// `audio.read_snr(s, sns)`'s values, or `None` where the reference raises.
#[pyfunction]
#[pyo3(signature = (s, sns=None))]
pub fn snr_read<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyBytes>,
    sns: Option<&Bound<'py, PyBytes>>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let (bytes, sns) = (s.as_bytes(), sns.map(PyBytesMethods::as_bytes));
    match py.detach(|| wave::read_snr(bytes, sns)) {
        Ok(st) => Ok(Some(stream(py, &st)?)),
        Err(_) => Ok(None),
    }
}

/// `audio.write_sps(st)`, or `None` where the reference raises or the port leaves it to the reference.
#[pyfunction]
pub fn sps_write<'py>(py: Python<'py>, st: &Bound<'py, PyAny>) -> Option<Bound<'py, PyBytes>> {
    let parts = (|| {
        let header = st.call_method0("header").ok()?.extract::<(u32, u32)>().ok()?;
        let blocks = items(&st.getattr("blocks").ok()?)?
            .iter()
            .map(|b| {
                let v = items(b)?;
                let [n, data] = v.as_slice() else { return None };
                Some((u32_of(n)?, data.cast::<PyBytes>().ok()?.clone()))
            })
            .collect::<Option<Vec<_>>>()?;
        Some((header, blocks))
    })();
    let (header, blocks) = parts?;
    let views: Vec<(u32, &[u8])> = blocks.iter().map(|(n, d)| (*n, d.as_bytes())).collect();
    wave::write_sps(header, &views).ok().map(|b| PyBytes::new(py, &b))
}
