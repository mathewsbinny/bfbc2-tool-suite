//! Round OR14: `bfbc2-menu-tools\aptgraft.py`'s movie API for Project OPFOR's WW2 and Onslaught tools
//! (`ww2\type95_icon.py`, `weapon_icons.py`, `duplicates.py`, `wake_ui.py`, `ww2_crosshairs.py`,
//! `bf1943_hud_icons.py`, `qa_checks.py`, `onslaught\console_icons.py`) over `bc2_menu::graft::Movie`, proven byte for
//! byte against aptgraft.py (`rust\parity_menu_movies.py`).
//!
//! `AptMovie.load(spec)` / `AptMovie.from_bytes(res, label, where)` parse a movie; `to_res()` writes it in
//! `GRAFT_LAYOUT` (`.apt` / `.geo` kept in place, `.const` packed) in its own byte order. The API speaks ids and
//! plain values, not nodes: a timeline is `None` (the movie's) or a sprite's character id (`movie.char(id)` as the
//! reference passes it: an id without a node is the movie's timeline); a frame item is `(tag, fields)` and a
//! geometry part its `fields`, each field a plain value - a number `int`, a pointer to a string `str` (Latin-1), to
//! a list of fields `list` (its numbers; a pointer inside it `None`), to anything else `None`. `aptgraft.AptError`
//! is `bc2_native.AptError` (a `ValueError`) with the reference's words; a damaged input the reference fails on with
//! an exception of its own is a `ValueError` naming the movie.

use std::collections::HashMap;

use bc2_formats::aptmodel::{Content, Rid, Val};
use bc2_menu::error::MenuError;
use bc2_menu::graft::tiles::{Tile, split_values};
use bc2_menu::graft::{self, GRAFT_LAYOUT, Movie, consts, crosshair, edit, icon};
use pyo3::create_exception;
use pyo3::exceptions::{PyFileNotFoundError, PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyInt, PyList, PyString};

create_exception!(
    bc2_native,
    AptError,
    PyValueError,
    "aptgraft.AptError: a movie the graft cannot read or change (the reference's words)."
);

/// The prefix `console_icons.add_icon` renames the labels it hides with.
const HIDDEN: &str = "__opfor_hidden_";

fn err(label: &str, e: MenuError) -> PyErr {
    match e {
        MenuError::Apt(m) => AptError::new_err(m),
        MenuError::NotFound(p) => PyFileNotFoundError::new_err(p),
        MenuError::Io { path, source } => PyOSError::new_err(format!("{path}: {source}")),
        MenuError::Menu(m) | MenuError::Latin1(m) => PyValueError::new_err(m),
        MenuError::Reference => PyValueError::new_err(format!("{label}: {}", MenuError::Reference)),
    }
}

/// A tile as `aptgraft.Tile`: (x, y, w, h, target image, dx, dy).
type TileArg = (f64, f64, f64, f64, i64, f64, f64);

fn tile(t: &TileArg) -> Tile {
    Tile {
        x: t.0,
        y: t.1,
        w: t.2,
        h: t.3,
        image: t.4,
        dx: t.5,
        dy: t.6,
    }
}

/// `aptgraft.Movie` held in Rust (round OR14; see the module's notes).
#[pyclass(module = "bc2_native", name = "AptMovie")]
pub struct AptMovie {
    m: Movie,
}

impl AptMovie {
    fn e(&self, e: MenuError) -> PyErr {
        err(&self.m.label, e)
    }

    /// A timeline: `movie.char(owner)` (None: the movie's).
    fn owner(&self, owner: Option<i64>) -> PyResult<Option<Rid>> {
        match owner {
            None => Ok(None),
            Some(c) => self.m.char(c).map_err(|e| self.e(e)),
        }
    }

    /// A field as a plain value.
    fn plain<'py>(&self, py: Python<'py>, v: Val) -> PyResult<Bound<'py, PyAny>> {
        Ok(match v {
            Val::Num(x) => x.into_pyobject(py)?.into_any(),
            Val::Ref(r) => match self.m.node(r).map(|n| &n.content) {
                Ok(Content::Str(t)) => PyString::new(py, &graft::latin1(t)).into_any(),
                Ok(Content::Words(vs)) => PyList::new(
                    py,
                    vs.iter().map(|x| match x {
                        Val::Num(n) => Some(*n),
                        Val::Ref(_) => None,
                    }),
                )?
                .into_any(),
                _ => py.None().into_bound(py),
            },
        })
    }

    /// A Words node's fields as plain values.
    fn fields<'py>(&self, py: Python<'py>, rid: Rid) -> PyResult<Bound<'py, PyList>> {
        let vs = self.m.vals(rid).map_err(|e| self.e(e))?;
        let out = PyList::empty(py);
        for v in vs {
            out.append(self.plain(py, *v)?)?;
        }
        Ok(out)
    }

    fn display<'py>(
        &self,
        py: Python<'py>,
        disp: &[std::collections::BTreeMap<i64, Rid>],
    ) -> PyResult<Bound<'py, PyList>> {
        let out = PyList::empty(py);
        for d in disp {
            let f = PyDict::new(py);
            for (depth, it) in d {
                f.set_item(depth, self.fields(py, *it)?)?;
            }
            out.append(f)?;
        }
        Ok(out)
    }
}

#[pymethods]
impl AptMovie {
    /// `Movie.load(spec)`: a `.res`, a `.apt` with its `.const` / `.geo` beside it, or `<bundle.fbrb>:<name>`.
    #[staticmethod]
    fn load(spec: &str) -> PyResult<Self> {
        graft::load(spec).map(|m| Self { m }).map_err(|e| err(spec, e))
    }

    /// `Movie(read_bigf_bytes(res, where), label)`.
    #[staticmethod]
    #[pyo3(signature = (res, label = "movie", place = "file"))]
    fn from_bytes(res: &[u8], label: &str, place: &str) -> PyResult<Self> {
        let members = graft::read_bigf(res, place).map_err(|e| err(place, e))?;
        Movie::new(members, label)
            .map(|m| Self { m })
            .map_err(|e| err(label, e))
    }

    /// `Movie.label`.
    #[getter]
    fn label(&self) -> String {
        self.m.label.clone()
    }

    /// `Movie.big`: Xbox 360 big-endian.
    #[getter]
    fn big(&self) -> bool {
        self.m.big
    }

    /// `Movie.to_res(layout=GRAFT_LAYOUT)` in the movie's byte order.
    fn to_res<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let b = self.m.to_res(self.m.big, GRAFT_LAYOUT).map_err(|e| self.e(e))?;
        Ok(PyBytes::new(py, &b))
    }

    /// `len(movie.chartable.vals)`.
    fn char_count(&self) -> PyResult<usize> {
        self.m.char_count().map_err(|e| self.e(e))
    }

    /// `Movie.char_type(cid)` (None: no character there).
    fn char_type(&self, cid: i64) -> PyResult<Option<u32>> {
        self.m.char_type(cid).map_err(|e| self.e(e))
    }

    /// The sprites' character ids (`char_type == 5`), in order.
    fn sprites(&self) -> PyResult<Vec<i64>> {
        let mut out = Vec::new();
        for c in 0..self.char_count()? as i64 {
            if self.char_type(c)? == Some(5) {
                out.push(c);
            }
        }
        Ok(out)
    }

    /// `len(movie.frames(owner).vals) // 2`.
    #[pyo3(signature = (owner = None))]
    fn frame_count(&self, owner: Option<i64>) -> PyResult<usize> {
        let o = self.owner(owner)?;
        self.m.frame_count(o).map_err(|e| self.e(e))
    }

    /// `Movie.frame_items(frame, owner)` as `[(tag, fields)]`.
    #[pyo3(signature = (frame, owner = None))]
    fn frame_items<'py>(&mut self, py: Python<'py>, frame: usize, owner: Option<i64>) -> PyResult<Bound<'py, PyList>> {
        let o = self.owner(owner)?;
        let items = self.m.frame_items(frame, o).map_err(|e| self.e(e))?;
        let out = PyList::empty(py);
        for it in items {
            let tag = self.m.tag(it).map_err(|e| self.e(e))?.to_owned();
            out.append((tag, self.fields(py, it)?))?;
        }
        Ok(out)
    }

    /// `Movie.label_frame(label, owner)`, None where the timeline has no such label.
    #[pyo3(signature = (label, owner = None))]
    fn find_label(&mut self, label: &str, owner: Option<i64>) -> PyResult<Option<usize>> {
        let o = self.owner(owner)?;
        match self.m.label_frame(label, o) {
            Ok(f) => Ok(Some(f)),
            Err(MenuError::Apt(_)) => Ok(None),
            Err(e) => Err(self.e(e)),
        }
    }

    /// `Movie.label_frame(label, owner)` (`AptError`: no such label).
    #[pyo3(signature = (label, owner = None))]
    fn label_frame(&mut self, label: &str, owner: Option<i64>) -> PyResult<usize> {
        let o = self.owner(owner)?;
        self.m.label_frame(label, o).map_err(|e| self.e(e))
    }

    /// `movie.add_item(frame, Words('apt', None, [2, Ref(Str('apt', None, text)), flags, frame], 'label'), owner)`.
    #[pyo3(signature = (frame, text, flags, owner = None))]
    fn add_label(&mut self, frame: usize, text: &str, flags: u32, owner: Option<i64>) -> PyResult<()> {
        let o = self.owner(owner)?;
        let t = graft::latin1_encode(text).map_err(|e| self.e(e))?;
        let s = self.m.new_str(graft::APT, t);
        let w = self.m.new_words(
            graft::APT,
            vec![Val::Num(2), Val::Ref(s), Val::Num(flags), Val::Num(frame as u32)],
            "label",
        );
        self.m.add_item(frame, w, o, None).map_err(|e| self.e(e))
    }

    /// The geometry parts of shape `cid` (`geo_entry`: `table.vals[k + 2].node.vals`), each part's fields; None:
    /// the shape has no geometry entry.
    fn geo_parts<'py>(&self, py: Python<'py>, cid: i64) -> PyResult<Option<Bound<'py, PyList>>> {
        let Some((table, k)) = self.m.geo_entry(cid).map_err(|e| self.e(e))? else {
            return Ok(None);
        };
        let parts = match self.m.val(table, k + 2).map_err(|e| self.e(e))? {
            Val::Ref(p) => self.m.vals(p).map_err(|e| self.e(e))?.clone(),
            Val::Num(_) => return Err(self.e(MenuError::Reference)),
        };
        let out = PyList::empty(py);
        for p in parts {
            let Val::Ref(p) = p else {
                return Err(self.e(MenuError::Reference));
            };
            out.append(self.fields(py, p)?)?;
        }
        Ok(Some(out))
    }

    /// `add_label_alias(movie, like, alias)`: the labels added.
    fn add_label_alias(&mut self, like: &str, alias: &str) -> PyResult<usize> {
        edit::add_label_alias(&mut self.m, like, alias).map_err(|e| self.e(e))
    }

    /// `add_icon_frame(movie, like, label, size)`: (image character, timelines given a frame). `hide_except`: a
    /// sprite id - every other sprite timeline's labels `label` are renamed for the call and put back after it
    /// (`console_icons.add_icon`: DICE's own seat-map frame of the name is kept).
    #[pyo3(signature = (like, label, size, hide_except = None))]
    #[allow(clippy::needless_pass_by_value, reason = "PyO3 extracts a tuple argument by value")]
    fn add_icon_frame(
        &mut self,
        like: &str,
        label: &str,
        size: (Bound<'_, PyAny>, Bound<'_, PyAny>),
        hide_except: Option<i64>,
    ) -> PyResult<(i64, usize)> {
        // `-w / 2`: a Python int 0 negates to +0, a float 0.0 to -0.0
        let neg = |v: &Bound<'_, PyAny>, x: f64| {
            if x == 0.0 && v.is_instance_of::<PyInt>() {
                0.0
            } else {
                -x
            }
        };
        let (w, h): (f64, f64) = (size.0.extract()?, size.1.extract()?);
        let negs = (neg(&size.0, w), neg(&size.1, h));
        let hidden = match hide_except {
            Some(c) => icon::hide_labels(&mut self.m, label, c, HIDDEN).map_err(|e| self.e(e))?,
            None => Vec::new(),
        };
        let r = icon::add_icon_frame_neg(&mut self.m, like, label, (w, h), negs);
        icon::restore_labels(&mut self.m, &hidden).map_err(|e| self.e(e))?;
        r.map_err(|e| self.e(e))
    }

    /// `wake_ui.renamed`'s swap: each string constant whose text is a key of keys given the key's value; {old text:
    /// constants changed} in the order first met.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "PyO3 extracts a dict argument as an owned HashMap"
    )]
    fn rename_constants<'py>(
        &mut self,
        py: Python<'py>,
        keys: HashMap<String, String>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let done = consts::rename_constants(&mut self.m, &keys).map_err(|e| self.e(e))?;
        let d = PyDict::new(py);
        for (k, n) in done {
            d.set_item(k, n)?;
        }
        Ok(d)
    }

    /// `[movie.const_text(k) for k in range(len(movie.const_entries.vals) // 2)]`.
    fn const_texts(&self) -> PyResult<Vec<String>> {
        consts::const_texts(&self.m).map_err(|e| self.e(e))
    }

    /// `deps(movie, cid)`: {'chars': {id: type}, 'imports', 'fonts', 'images', 'geometry', 'constants'}.
    fn deps<'py>(&self, py: Python<'py>, cid: i64) -> PyResult<Bound<'py, PyDict>> {
        let d = edit::deps(&self.m, cid).map_err(|e| self.e(e))?;
        let out = PyDict::new(py);
        let chars = PyDict::new(py);
        for (k, v) in &d.chars {
            chars.set_item(k, v)?;
        }
        out.set_item("chars", chars)?;
        out.set_item("imports", d.imports)?;
        out.set_item("fonts", d.fonts)?;
        out.set_item("images", d.images)?;
        out.set_item("geometry", d.geometry)?;
        out.set_item("constants", d.constants.into_iter().collect::<Vec<u32>>())?;
        Ok(out)
    }

    /// `timeline_display(movie, owner)`: each frame's {depth: the showing place item's fields}.
    #[pyo3(signature = (owner = None))]
    fn timeline_display<'py>(&mut self, py: Python<'py>, owner: Option<i64>) -> PyResult<Bound<'py, PyList>> {
        let o = self.owner(owner)?;
        let disp = crosshair::timeline_display_of(&mut self.m, o).map_err(|e| self.e(e))?;
        self.display(py, &disp)
    }

    /// `crosshair_table(movie)`: {id: {'label', 'clip', 'start', 'end', 'reg'}} in table order.
    fn crosshair_table<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let t = crosshair::crosshair_table(&self.m).map_err(|e| self.e(e))?;
        let out = PyDict::new(py);
        for (k, e) in t {
            let d = PyDict::new(py);
            d.set_item("label", e.label)?;
            d.set_item("clip", e.clip)?;
            d.set_item("start", e.start)?;
            d.set_item("end", e.end)?;
            d.set_item("reg", e.reg)?;
            out.set_item(k, d)?;
        }
        Ok(out)
    }

    /// `crosshair_objects(movie, label, baseline)`: [(depth, place fields)].
    #[pyo3(signature = (label, baseline = "none"))]
    fn crosshair_objects<'py>(&mut self, py: Python<'py>, label: &str, baseline: &str) -> PyResult<Bound<'py, PyList>> {
        let objs = crosshair::crosshair_objects(&mut self.m, label, baseline).map_err(|e| self.e(e))?;
        let out = PyList::empty(py);
        for (d, it) in objs {
            out.append((d, self.fields(py, it)?))?;
        }
        Ok(out)
    }

    /// `add_crosshair(dst=self, src, cid, frame, images)`: the report {'id', 'label', 'frame', 'depths', 'added',
    /// 'registers', 'values', 'notes'}. images: {src image: [(x, y, w, h, dst image, dx, dy)]}.
    #[pyo3(signature = (src, cid, frame = None, images = None))]
    fn add_crosshair<'py>(
        &mut self,
        py: Python<'py>,
        mut src: PyRefMut<'_, AptMovie>,
        cid: &str,
        frame: Option<i64>,
        images: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let mut imgs: Vec<(i64, Vec<Tile>)> = Vec::new();
        if let Some(d) = images {
            for (k, v) in d.iter() {
                let ts: Vec<TileArg> = v.extract()?;
                imgs.push((k.extract()?, ts.iter().map(tile).collect()));
            }
        }
        let r = crosshair::add_crosshair_tiles(&mut self.m, &mut src.m, cid, frame, &imgs).map_err(|e| self.e(e))?;
        let out = PyDict::new(py);
        out.set_item("id", r.id)?;
        out.set_item("label", r.label)?;
        out.set_item("frame", r.frame)?;
        let pairs = |v: &[(i64, i64)]| -> PyResult<Bound<'py, PyDict>> {
            let d = PyDict::new(py);
            for (a, b) in v {
                d.set_item(a, b)?;
            }
            Ok(d)
        };
        out.set_item("depths", pairs(&r.depths)?)?;
        out.set_item("added", r.added)?;
        out.set_item("registers", pairs(&r.registers)?)?;
        let values = PyDict::new(py);
        for (k, v) in r.values {
            values.set_item(k, v)?;
        }
        out.set_item("values", values)?;
        out.set_item("notes", r.notes)?;
        Ok(out)
    }
}

/// `aptgraft.split_image_part` of a part's fields (field 2 not read) and its vertex fields: [(fields, vertices)] of
/// the new parts (field 2: 0) - the parity check's handle on `bc2_menu::graft::tiles`.
///
/// # Errors
/// `AptError`: a part that is not one axis-aligned rectangle (the reference's words); `ValueError`: a value the
/// reference's `struct.pack` refuses.
#[pyfunction]
#[allow(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts list arguments as owned Vecs"
)]
pub fn apt_split_image_part(
    fields: Vec<u32>,
    vertices: Vec<u32>,
    tiles: Vec<TileArg>,
) -> PyResult<Vec<(Vec<u32>, Vec<u32>)>> {
    let v: Vec<Val> = fields.into_iter().map(Val::Num).collect();
    let vs: Vec<Val> = vertices.into_iter().map(Val::Num).collect();
    let ts: Vec<Tile> = tiles.iter().map(tile).collect();
    let out = split_values(&v, &vs, &ts).map_err(|e| err("split_image_part", e))?;
    Ok(out
        .into_iter()
        .map(|(f, nv)| {
            let f = f
                .into_iter()
                .map(|x| match x {
                    Val::Num(n) => n,
                    Val::Ref(_) => 0,
                })
                .collect();
            (f, nv)
        })
        .collect())
}
