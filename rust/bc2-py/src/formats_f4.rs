//! Round F4: F3's leftovers, the bindings of the Python modules' last format paths (`rust/BENCHMARKS.md` "F4").
//!
//! | Python | binding | Rust |
//! |---|---|---|
//! | `culltree.Doc`'s string table | `culltree_strings` | here |
//! | `audio.convert` / `decode` (`segment_stream`, `decode_xma` / `decode_xas`, the ffmpeg glue) | `audio_convert`, `audio_decode` | `bc2_convert::wavport` |
//! | `hkpack.Packfile` (a handle: `deref`, `cstr`, `objects_of`, `read_object`, `read_value`) | `hkpack_open` / [`HkPackfile`] | `bc2_formats::havok::reflect` |
//! | `navsample_offline.asset_bodies` (`_shape_parts`, `_hull_faces`, `_box_faces`) | `navkit_asset_bodies` | `bc2_navbuild::hulls` |
//! | `meshview.load_mesh`'s part reader (`_values`) | `meshview_part` | here |
//! | `texqa.read_pc` / `read_xbox` | `texqa_read` | `bc2_convert::texqa` |
//!
//! (`xenos.disasm`'s header is `xenos_disasm` in `xenos.rs`.) Each answers `None` where the Python must (a value the
//! port does not read exactly as the reference does, or an error the Python raises): the Python then runs.

#![allow(clippy::indexing_slicing, reason = "chunks of the sizes taken (2, 4 or 8 bytes)")]
#![allow(
    clippy::many_single_char_names,
    clippy::needless_pass_by_value,
    clippy::type_complexity,
    clippy::chunks_exact_to_as_chunks,
    reason = "the references' names; PyO3 extracts the arguments by value; a part's read as one tuple"
)]

use bc2_convert::wavport::Ffmpeg;
use bc2_formats::havok::{self, reflect};
use pyo3::IntoPyObjectExt;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyList, PyString, PyTuple};

// ------------------------------------------------------------------------------------------------- culltree

/// `culltree.Doc`'s string table: `count` strings, each at its offset (a big-endian word at `24 + 4 i`) from `24 + 4
/// count`, up to its zero, decoded as UTF-8 with replacement. `None` where the Python raises (`struct.error`, no
/// zero after the offset).
#[pyfunction]
pub fn culltree_strings<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    count: u64,
) -> PyResult<Option<Bound<'py, PyList>>> {
    let d = data.as_bytes();
    let Some(base) = usize::try_from(count)
        .ok()
        .and_then(|c| c.checked_mul(4))
        .and_then(|c| c.checked_add(24))
    else {
        return Ok(None);
    };
    let got: Option<Vec<String>> = py.detach(|| {
        (0..base.saturating_sub(24) / 4)
            .map(|i| {
                let w: [u8; 4] = d.get(24 + 4 * i..28 + 4 * i)?.try_into().ok()?;
                let start = base.checked_add(u32::from_be_bytes(w) as usize)?;
                let rest = d.get(start..)?;
                let end = rest.iter().position(|&b| b == 0)?;
                Some(String::from_utf8_lossy(rest.get(..end)?).into_owned())
            })
            .collect()
    });
    got.map(|v| PyList::new(py, v)).transpose()
}

// ------------------------------------------------------------------------------------------------- audio

fn streams_of(streams: &Bound<'_, PyAny>) -> Option<Vec<Option<Vec<u8>>>> {
    let l = streams.cast_exact::<PyList>().ok()?;
    if l.len() != 3 {
        return None;
    }
    l.iter()
        .map(|o| {
            if o.is_none() {
                Some(None)
            } else {
                o.cast_exact::<PyBytes>().ok().map(|b| Some(b.as_bytes().to_vec()))
            }
        })
        .collect()
}

fn views(s: &[Option<Vec<u8>>]) -> [Option<&[u8]>; 3] {
    let v = |i: usize| s.get(i).and_then(Option::as_deref);
    [v(0), v(1), v(2)]
}

/// `audio.convert(b, source, streams, word14)`: the PC Wave's bytes. `streams`: `[streams.get(i) for i in
/// range(3)]` (bytes or `None`); `ffmpeg`: `audio.ffmpeg_path()`, or `None` (not looked for yet, or it raised): a
/// sound that needs it then answers `'ffmpeg'`. Runs without the
/// interpreter's lock (`convert_many`'s threads run side by side).
#[pyfunction]
pub fn audio_convert<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    source: &str,
    streams: &Bound<'py, PyAny>,
    word14: &Bound<'py, PyAny>,
    ffmpeg: Option<String>,
) -> Option<Bound<'py, PyAny>> {
    let s = streams_of(streams)?;
    let word14 = if word14.get_type().is(py.get_type::<pyo3::types::PyInt>()) {
        word14.extract::<u32>().ok()?
    } else {
        return None;
    };
    let data = b.as_bytes();
    let ff = Ffmpeg {
        path: ffmpeg.as_deref(),
        ..Ffmpeg::default()
    };
    match py.detach(|| bc2_convert::wavport::convert(data, source, &views(&s), word14, &ff)) {
        Some(out) => Some(PyBytes::new(py, &out).into_any()),
        None if ff.wanted.load(std::sync::atomic::Ordering::Relaxed) => Some(PyString::new(py, "ffmpeg").into_any()),
        None => None,
    }
}

/// `audio.decode(b, source, streams)`: `(int16 samples interleaved, channels, rate)`, `'ffmpeg'` as
/// `audio_convert`, or `None`.
#[pyfunction]
pub fn audio_decode<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    source: &str,
    streams: &Bound<'py, PyAny>,
    ffmpeg: Option<String>,
) -> Option<Bound<'py, PyAny>> {
    let s = streams_of(streams)?;
    let data = b.as_bytes();
    let ff = Ffmpeg {
        path: ffmpeg.as_deref(),
        ..Ffmpeg::default()
    };
    match py.detach(|| bc2_convert::wavport::decode(data, source, &views(&s), &ff)) {
        Some((pcm, ch, rate)) => {
            let mut bytes: Vec<u8> = Vec::with_capacity(2 * pcm.len());
            for v in &pcm {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            (PyByteArray::new(py, &bytes), ch, rate).into_bound_py_any(py).ok()
        }
        None if ff.wanted.load(std::sync::atomic::Ordering::Relaxed) => Some(PyString::new(py, "ffmpeg").into_any()),
        None => None,
    }
}

// ------------------------------------------------------------------------------------------------- hkpack

/// A packfile read in Rust, behind `hkpack.Packfile` (round F4): the file's bytes (the `bytes` object the Python
/// holds as `d`) and the walk's lookups. The Python builds its dicts (`ptr`, `objects`, `classes` ...) only when a
/// caller reads them (`fields`: `hkpack_parse`'s tuple), and asks the handle until then.
#[pyclass(frozen, module = "bc2_native")]
pub struct HkPackfile {
    data: Py<PyBytes>,
    ix: reflect::Index,
}

fn value_py<'py>(py: Python<'py>, v: &reflect::Value) -> PyResult<Bound<'py, PyAny>> {
    use reflect::Value as V;
    Ok(match v {
        V::Int(i) => i.into_bound_py_any(py)?,
        V::Bool(b) => b.into_bound_py_any(py)?,
        V::Float(f) => f.into_bound_py_any(py)?,
        V::Floats(fs) => PyTuple::new(py, fs)?.into_any(),
        V::Ptr(t, name) => ("ptr", *t, name.as_deref()).into_bound_py_any(py)?,
        V::None => py.None().into_bound(py),
        V::Str(s) => PyString::new(py, s).into_any(),
        V::List(items) => {
            let l = PyList::empty(py);
            for x in items {
                l.append(value_py(py, x)?)?;
            }
            l.into_any()
        }
        V::Dict(items) => {
            let d = PyDict::new(py);
            for (k, x) in items {
                d.set_item(k, value_py(py, x)?)?;
            }
            d.into_any()
        }
        V::Unknown(n) => {
            let l = PyList::empty(py);
            l.append(("?", *n))?;
            l.into_any()
        }
    })
}

#[pymethods]
impl HkPackfile {
    /// The `bytes` object the packfile was read from.
    #[getter]
    fn data(&self, py: Python<'_>) -> Py<PyBytes> {
        self.data.clone_ref(py)
    }

    /// `hkpack_parse(data)`'s tuple: what the Python's dicts are built from.
    fn fields<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyTuple>>> {
        crate::formats_f3::hkpack_parse(py, self.data.bind(py))
    }

    /// `deref(slot)`.
    fn deref(&self, slot: u64) -> Option<u64> {
        self.ix.deref(slot)
    }

    /// `cstr(off)`; `None` where the Python raises.
    fn cstr(&self, py: Python<'_>, off: u64) -> Option<String> {
        reflect::cstr(self.data.bind(py).as_bytes(), off)
    }

    /// `objects_of(name)`.
    fn objects_of(&self, name: &str) -> Vec<u64> {
        self.ix.objects_of(name)
    }

    /// `(read_object(at, cname),)`; `None` where the Python must answer.
    fn read_object<'py>(&self, py: Python<'py>, at: i64, cname: Option<&str>) -> PyResult<Option<Bound<'py, PyTuple>>> {
        let d = self.data.bind(py).as_bytes();
        let Some(v) = reflect::Reader::new(d, &self.ix).read_object(i128::from(at), cname) else {
            return Ok(None);
        };
        Ok(Some(PyTuple::new(py, [value_py(py, &v)?])?))
    }

    /// `(read_value(at, mtype, sub, mclass_name, carray),)`; `None` where the Python must answer.
    fn read_value<'py>(
        &self,
        py: Python<'py>,
        at: i64,
        mtype: u8,
        sub: u8,
        mclass_name: Option<&str>,
        carray: i16,
    ) -> PyResult<Option<Bound<'py, PyTuple>>> {
        let d = self.data.bind(py).as_bytes();
        let Some(v) = reflect::Reader::new(d, &self.ix).read_value(i128::from(at), mtype, sub, mclass_name, carray)
        else {
            return Ok(None);
        };
        Ok(Some(PyTuple::new(py, [value_py(py, &v)?])?))
    }
}

/// `hkpack.Packfile(data)` as a handle: `(handle, base, header words (8), contents_version)`, or `None` where
/// `hkpack` raises.
#[pyfunction]
pub fn hkpack_open<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let bytes = data.as_bytes();
    let Ok(pf) = havok::Packfile::parse(bytes) else {
        return Ok(None);
    };
    let base = pf.base as usize;
    let mut header = [0u32; 8];
    for (k, w) in header.iter_mut().enumerate() {
        let Some(b) = bytes.get(base + 4 * k..base + 4 * k + 4) else {
            return Ok(None);
        };
        *w = u32::from_le_bytes(b.try_into().unwrap_or_default());
    }
    let h = HkPackfile {
        data: data.clone().unbind(),
        ix: reflect::Index::new(&pf),
    };
    Ok(Some(PyTuple::new(
        py,
        [
            Bound::new(py, h)?.into_any(),
            pf.base.into_bound_py_any(py)?,
            PyTuple::new(py, header)?.into_any(),
            PyString::new(py, &pf.contents_version).into_any(),
        ],
    )?))
}

// ------------------------------------------------------------------------------------------------- navsample_offline

/// `navsample_offline.asset_bodies` of a file's bytes: `[(system, name, kind, triangles (bytes of float64, 9 a
/// triangle), part, root kind)]`, or `None` where the Python must answer.
#[pyfunction]
pub fn navkit_asset_bodies<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyList>>> {
    let d = data.as_bytes();
    let Some(pieces) = py.detach(|| bc2_navbuild::hulls::asset_bodies(d)) else {
        return Ok(None);
    };
    let out = PyList::empty(py);
    for p in pieces {
        let bytes: Vec<u8> = p
            .tris
            .iter()
            .flatten()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        out.append((
            p.system,
            p.name,
            p.kind,
            PyByteArray::new(py, &bytes),
            p.part,
            p.root_kind,
        ))?;
    }
    Ok(Some(out))
}

// ------------------------------------------------------------------------------------------------- meshview

/// `float32` of an IEEE half; `None` for a NaN (its payload is NumPy's conversion's to keep or quiet).
fn half(h: u16) -> Option<f32> {
    let sign = u32::from(h >> 15) << 31;
    let exp = u32::from((h >> 10) & 0x1f);
    let mant = u32::from(h & 0x3ff);
    let bits = match exp {
        0x1f if mant != 0 => return None,
        0x1f => sign | 0xff << 23,
        0 => {
            if mant == 0 {
                sign
            } else {
                let v = f32::from(u16::try_from(mant).unwrap_or(0)) * f32::powi(2.0, -24);
                sign | v.to_bits()
            }
        }
        e => sign | (e + 112) << 23 | mant << 13,
    };
    Some(f32::from_bits(bits))
}

/// The bytes per value of `_values`' formats: (float32 width or half count, columns).
fn values_shape(fmt: u32) -> Option<(bool, usize)> {
    Some(match fmt {
        3 => (false, 3),
        4 => (false, 4),
        5 => (true, 1),
        6 => (true, 2),
        7 => (true, 3),
        8 => (true, 4),
        _ => return None,
    })
}

/// `mesh.SIZE`.
fn element_size(fmt: u32) -> Option<usize> {
    Some(match fmt {
        2 | 8 => 8,
        3 => 12,
        4 => 16,
        5 => 2,
        6 | 0x0C | 0x0D | 0x2B => 4,
        7 => 6,
        _ => return None,
    })
}

/// `load_mesh`'s reading of one part: `(lo, n, {usage: (float32 bytes, columns)}, indices (int64 bytes))` - the
/// part's index range (`lo` the lowest), its vertex count, the values of each usage `_values` reads (the first
/// element of a usage, stream 0, the texture coordinates' element `uv` for usage 0), and the indices less `lo`.
/// `None` where the Python must answer (an empty part, an index or element past the data or the stride).
#[pyfunction]
#[allow(clippy::too_many_arguments, reason = "load_mesh's values, as it holds them")]
pub fn meshview_part<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    vbase: u64,
    ibase: u64,
    wide: bool,
    first_index: u64,
    triangles: u64,
    vertex_offset: u64,
    els: Vec<(u32, i64, u64, i64)>,
    stride: u64,
    uv: Option<usize>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let d = b.as_bytes();
    let isz: u64 = if wide { 4 } else { 2 };
    let read = || -> Option<(i64, usize, Vec<(i64, Vec<f32>, usize)>, Vec<i64>)> {
        let start = ibase.checked_add(first_index.checked_mul(isz)?)?;
        let count = triangles.checked_mul(3)?;
        let raw =
            d.get(usize::try_from(start).ok()?..usize::try_from(start.checked_add(count.checked_mul(isz)?)?).ok()?)?;
        let idx: Vec<i64> = if wide {
            raw.chunks_exact(4)
                .map(|c| i64::from(u32::from_le_bytes([c[0], c[1], c[2], c[3]])))
                .collect()
        } else {
            raw.chunks_exact(2)
                .map(|c| i64::from(u16::from_le_bytes([c[0], c[1]])))
                .collect()
        };
        let lo = *idx.iter().min()?;
        let hi = *idx.iter().max()?;
        let n = usize::try_from(hi - lo + 1).ok()?;
        let stride = usize::try_from(stride).ok()?;
        let vstart = usize::try_from(vbase.checked_add(vertex_offset)?)
            .ok()?
            .checked_add(usize::try_from(lo).ok()?.checked_mul(stride)?)?;
        let block = d.get(vstart..vstart.checked_add(n.checked_mul(stride)?)?)?;
        let mut attr: Vec<(i64, Vec<f32>, usize)> = Vec::new();
        for (i, &(f, usage, off, stream)) in els.iter().enumerate() {
            let Some(size) = element_size(f) else { continue };
            if stream != 0 || (usage == 0 && Some(i) != uv) {
                continue;
            }
            let Some((halves, cols)) = values_shape(f) else {
                continue;
            };
            let off = usize::try_from(off).ok()?;
            if off.checked_add(size)? > stride {
                return None; // a slice past the stride: NumPy's shapes, left to the Python
            }
            if attr.iter().any(|a| a.0 == usage) {
                continue;
            }
            let mut v = Vec::with_capacity(n * cols);
            for r in 0..n {
                let e = block.get(r * stride + off..r * stride + off + size)?;
                if halves {
                    for c in e.chunks_exact(2) {
                        v.push(half(u16::from_le_bytes([c[0], c[1]]))?);
                    }
                } else {
                    v.extend(e.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])));
                }
            }
            attr.push((usage, v, cols));
        }
        Some((lo, n, attr, idx.iter().map(|i| i - lo).collect()))
    };
    let Some((lo, n, attr, idx)) = py.detach(read) else {
        return Ok(None);
    };
    let d = PyDict::new(py);
    for (usage, v, cols) in attr {
        let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        d.set_item(usage, (PyByteArray::new(py, &bytes), cols))?;
    }
    let ib: Vec<u8> = idx.iter().flat_map(|x| x.to_le_bytes()).collect();
    Ok(Some(PyTuple::new(
        py,
        [
            lo.into_bound_py_any(py)?,
            n.into_bound_py_any(py)?,
            d.into_any(),
            PyByteArray::new(py, &ib).into_any(),
        ],
    )?))
}

// ------------------------------------------------------------------------------------------------- texqa

/// `texqa.read_pc(res)` / `read_xbox(res)`: `('ok', fmt, flags, typ, w, h, mips, [(slice, level, h, w, RGBA
/// bytes)], [(h, w, float32 bytes)], size_errors, zero_blocks)`, `('error', message)` for `TexQaError`, or `None`
/// where the Python must answer (NumPy's errors, a texture too large to decode here).
#[pyfunction]
pub fn texqa_read<'py>(
    py: Python<'py>,
    res: &Bound<'py, PyBytes>,
    xbox: bool,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    use bc2_convert::texqa::{self as tq, DErr};
    let data = res.as_bytes();
    // the header's size (a decode allocates w x h x 4 per level): a texture past 2^28 pixels is the Python's
    let head = if xbox { data } else { tq::unwrap(data) };
    let word = |o: usize| {
        let b: [u8; 4] = head.get(o..o + 4)?.try_into().ok()?;
        Some(u64::from(if xbox {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        }))
    };
    if let (Some(w), Some(h)) = (word(16), word(20))
        && w.saturating_mul(h) > 1 << 28
    {
        return Ok(None);
    }
    // read_xbox's {1: 0, 2: 1, 8: 3, 16: 4}[bpb] raises KeyError(bpb) for a 4-byte texel, after its header checks
    // (the decoder words it as texqa's command line prints it): ('keyerror', bpb)
    if xbox
        && data.len() >= tq::HEADER
        && let (Some(fmt), Some(typ)) = (word(8), word(4))
        && typ <= 1
        && let Some((bpb, _)) = bc2_convert::texture::format(fmt as u32)
        && !matches!(bpb, 1 | 2 | 8 | 16)
    {
        return Ok(Some(PyTuple::new(
            py,
            [PyString::new(py, "keyerror").into_any(), bpb.into_bound_py_any(py)?],
        )?));
    }
    let got = py.detach(|| if xbox { tq::read_xbox(data) } else { tq::read_pc(data) });
    let t = match got {
        Ok(t) => t,
        Err(DErr::Qa(e)) => {
            return Ok(Some(PyTuple::new(
                py,
                [PyString::new(py, "error"), PyString::new(py, &e.0)],
            )?));
        }
        Err(DErr::Py) => return Ok(None),
    };
    if t.floats.iter().flatten().any(|x| x.is_nan()) {
        return Ok(None); // a half NaN's payload is NumPy's conversion's to keep or quiet
    }
    let levels = PyList::empty(py);
    for (s, m, img) in &t.levels {
        levels.append((*s, *m, img.h, img.w, PyByteArray::new(py, &img.px)))?;
    }
    let floats = PyList::empty(py);
    let float_levels = t.levels.iter().filter(|_| t.fmt == 13);
    for ((_, _, img), f) in float_levels.zip(&t.floats) {
        let bytes: Vec<u8> = f.iter().flat_map(|x| x.to_le_bytes()).collect();
        floats.append((img.h, img.w, PyByteArray::new(py, &bytes)))?;
    }
    Ok(Some(PyTuple::new(
        py,
        [
            "ok".into_bound_py_any(py)?,
            t.fmt.into_bound_py_any(py)?,
            t.flags.into_bound_py_any(py)?,
            t.typ.into_bound_py_any(py)?,
            t.w.into_bound_py_any(py)?,
            t.h.into_bound_py_any(py)?,
            t.mips.into_bound_py_any(py)?,
            levels.into_any(),
            floats.into_any(),
            PyList::new(py, &t.size_errors)?.into_any(),
            t.zero_blocks.into_bound_py_any(py)?,
        ],
    )?))
}
