//! Bindings for the menu tools' movie model and scans (round E4c): `AptModel` (`bfbc2-menu-tools/aptgraft.py`'s
//! `NativeMovie` holds one), `apt_roundtrip_many` (`aptgraft roundtrip`), `apt_check_many` (`aptcheck check`) and
//! `apt_refs_many` (`aptrefs.movie_refs`).
//!
//! Nodes cross as numbers: a node's *code* is `rid * 4 + kind` (kind 0 raw, 1 string, 2 words, 3 code), so Python
//! makes an object of the right class without asking. In what `AptModel.node` gives, a pointer is `~code` (a
//! negative int) among the numbers; in what `set_nodes` takes, a words field's pointer is `~rid` and an
//! instruction argument's pointer is `(rid,)`. A refusal (`None`, `False`) means: run the Python code on it.

use bc2_formats::apt::MovieRef;
use bc2_formats::aptmodel::{self, Arg, Code, Content, Ins, Layout, Model, Node, Target, Val};
use bc2_formats::aptscan;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};
use rayon::prelude::*;

fn code_of(m: &Model, rid: u32) -> i64 {
    i64::from(rid) * 4 + m.node(rid).map_or(0, |n| i64::from(n.content.kind()))
}

/// A parsed movie held in Rust (`aptgraft.NativeMovie`).
#[pyclass(module = "bc2_native")]
pub struct AptModel {
    m: Model,
    /// Each node's offset in the last write that placed it (`Node.new_at`).
    placed: Vec<Option<i64>>,
}

#[pymethods]
impl AptModel {
    /// `Movie._parse` of the members' bytes, or `None` where the port refuses (the Python parse runs).
    #[staticmethod]
    #[pyo3(signature = (apt, c, geo, big))]
    fn parse(
        py: Python<'_>,
        apt: &Bound<'_, PyBytes>,
        c: &Bound<'_, PyBytes>,
        geo: Option<&Bound<'_, PyBytes>>,
        big: bool,
    ) -> Option<AptModel> {
        let (a, c, g) = (apt.as_bytes(), c.as_bytes(), geo.map(PyBytesMethods::as_bytes));
        py.detach(|| Model::parse(a, c, g, big))
            .ok()
            .map(|m| AptModel { m, placed: Vec::new() })
    }

    /// Why `parse` refuses these members (None: it does not) - for diagnostics.
    #[staticmethod]
    #[pyo3(signature = (apt, c, geo, big))]
    fn refusal(
        apt: &Bound<'_, PyBytes>,
        c: &Bound<'_, PyBytes>,
        geo: Option<&Bound<'_, PyBytes>>,
        big: bool,
    ) -> Option<String> {
        Model::parse(apt.as_bytes(), c.as_bytes(), geo.map(PyBytesMethods::as_bytes), big)
            .err()
            .map(|e| e.0)
    }

    /// Each node's offset in the last write that placed it (`Node.new_at`), None where none did.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "PyO3 extracts a list argument as an owned Vec"
    )]
    fn new_ats(&self, rids: Vec<u32>) -> Vec<Option<i64>> {
        rids.iter()
            .map(|&r| self.placed.get(r as usize).copied().flatten())
            .collect()
    }

    /// The codes of (apt header, const header raw, const header words, movie, geo header raw or None, geo header
    /// words or None).
    fn heads(&self) -> (i64, i64, i64, i64, Option<i64>, Option<i64>) {
        let h = &self.m.heads;
        let c = |r| code_of(&self.m, r);
        (
            c(h.apt_head),
            c(h.const_magic),
            c(h.const_head),
            c(h.movie),
            h.geo_magic.map(c),
            h.geo_head.map(c),
        )
    }

    /// Big-endian.
    #[getter]
    fn big(&self) -> bool {
        self.m.big
    }

    /// `Movie.reachable()`: [(file, [codes])].
    fn reachable(&self) -> Vec<(u8, Vec<i64>)> {
        self.m
            .reachable()
            .into_iter()
            .map(|(f, l)| (f, l.into_iter().map(|r| code_of(&self.m, r)).collect()))
            .collect()
    }

    /// One node: (file, at, size0, tag, payload) - module text.
    fn node<'py>(&self, py: Python<'py>, rid: u32) -> PyResult<Bound<'py, PyTuple>> {
        let n = self
            .m
            .node(rid)
            .ok_or_else(|| pyo3::exceptions::PyIndexError::new_err("no such node"))?;
        let tag = self.m.tags.get(usize::from(n.tag)).map_or("", String::as_str);
        let payload: Bound<'py, PyAny> = match &n.content {
            Content::Raw(d) | Content::Str(d) => PyBytes::new(py, d).into_any(),
            Content::Words(v) => PyList::new(
                py,
                v.iter().map(|x| match x {
                    Val::Num(w) => i64::from(*w),
                    Val::Ref(r) => !code_of(&self.m, *r),
                }),
            )?
            .into_any(),
            Content::Code(c) => {
                let ops = PyBytes::new(py, &c.ins.iter().map(|i| i.op).collect::<Vec<u8>>());
                let ats = PyList::new(py, c.ins.iter().map(|i| i.at))?;
                let args = PyList::empty(py);
                for i in &c.ins {
                    let a: Vec<Option<i64>> = i
                        .args
                        .iter()
                        .map(|a| match a {
                            Arg::Int(v) => Some(*v),
                            Arg::Ref(r) => Some(!code_of(&self.m, *r)),
                            Arg::Other => None,
                        })
                        .collect();
                    args.append(PyList::new(py, a)?)?;
                }
                let bodies = PyList::new(py, c.ins.iter().map(|i| i.body.map_or(-1, i64::from)))?;
                let targets = PyList::new(
                    py,
                    c.ins.iter().map(|i| match i.target {
                        Target::Unset => -1,
                        Target::Missing => -2,
                        Target::Ins(k) => i64::from(k),
                        Target::End(s) => -(i64::from(s) + 3),
                    }),
                )?;
                let streams = PyList::new(py, c.streams.iter().map(|s| PyList::new(py, s).ok()))?;
                (ops, ats, args, bodies, targets, streams).into_pyobject(py)?.into_any()
            }
        };
        PyTuple::new(
            py,
            [
                n.file.into_pyobject(py)?.into_any(),
                n.at.into_pyobject(py)?,
                n.size0.into_pyobject(py)?,
                tag.into_pyobject(py)?.into_any(),
                payload,
            ],
        )
    }

    /// The code of the node read at (file, at, kind: 0 words, 1 empty words, 2 string, 3 code), or None.
    fn cache_get(&self, file: u8, at: i64, kind: u8) -> Option<i64> {
        self.m.cache.get(&(file, at, kind)).map(|&r| code_of(&self.m, r))
    }

    /// Every node read: [(file, at, kind, code)] in the order they were read.
    fn cache_items(&self) -> Vec<(u8, i64, u8, i64)> {
        self.m
            .cache_keys
            .iter()
            .filter_map(|k| self.m.cache.get(k).map(|&r| (k.0, k.1, k.2, code_of(&self.m, r))))
            .collect()
    }

    /// How many nodes are read (`len(Movie.cache)`).
    fn cache_len(&self) -> usize {
        self.m.cache.len()
    }

    /// n new nodes: the first one's number.
    fn new_nodes(&mut self, n: usize) -> u32 {
        let first = self.m.nodes.len() as u32;
        for _ in 0..n {
            self.m.new_node();
        }
        first
    }

    /// Nodes sent back from Python: [(rid, file, at, size0, tag, kind, payload)]; False where one is refused.
    fn set_nodes(&mut self, nodes: Vec<Bound<'_, PyTuple>>) -> PyResult<bool> {
        for t in nodes {
            let (rid, file, at, size0, tag, kind, payload): (
                u32,
                u8,
                Option<i64>,
                Option<i64>,
                String,
                u8,
                Bound<'_, PyAny>,
            ) = t.extract()?;
            let content = match kind {
                0 => Content::Raw(payload.cast::<PyBytes>()?.as_bytes().to_vec()),
                1 => Content::Str(payload.cast::<PyBytes>()?.as_bytes().to_vec()),
                2 => {
                    let v: Vec<i64> = payload.extract()?;
                    let mut out = Vec::with_capacity(v.len());
                    for x in v {
                        out.push(if x >= 0 {
                            match u32::try_from(x) {
                                Ok(w) => Val::Num(w),
                                Err(_) => return Ok(false),
                            }
                        } else {
                            Val::Ref((!x) as u32)
                        });
                    }
                    Content::Words(out)
                }
                _ => match code_from(&payload)? {
                    Some(c) => Content::Code(c),
                    None => return Ok(false),
                },
            };
            let tag = self.m.tag(&tag);
            if self
                .m
                .set_node(
                    rid,
                    Node {
                        file,
                        at,
                        size0,
                        tag,
                        content,
                    },
                )
                .is_err()
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `Movie.write`: (apt, const, geo or None) bytes, or None where the Python writer runs instead.
    #[pyo3(signature = (big, layouts, gap, dead))]
    fn write<'py>(
        &mut self,
        py: Python<'py>,
        big: bool,
        layouts: (u8, u8, u8),
        gap: i64,
        dead: bool,
    ) -> PyResult<Option<Bound<'py, PyTuple>>> {
        let lay = |x: u8| if x == 1 { Layout::Pack } else { Layout::Keep };
        let l = [lay(layouts.0), lay(layouts.1), lay(layouts.2)];
        let m = &self.m;
        match py.detach(|| m.write_placed(big, l, gap, dead)) {
            Ok((w, at)) => {
                if self.placed.len() < at.len() {
                    self.placed.resize(at.len(), None);
                }
                for (slot, v) in self.placed.iter_mut().zip(at) {
                    if v.is_some() {
                        *slot = v;
                    }
                }
                let items: Vec<Option<Bound<'py, PyBytes>>> =
                    w.iter().map(|x| x.as_ref().map(|b| PyBytes::new(py, b))).collect();
                Ok(Some(PyTuple::new(py, items)?))
            }
            Err(_) => Ok(None),
        }
    }

    /// `Movie.leftovers()`: [(file, text)], or None where the Python runs instead.
    fn leftovers(&self) -> Option<Vec<(u8, String)>> {
        self.m.leftovers().ok()
    }
}

/// The columns of an instruction stream sent from Python.
type CodeCols<'py> = (
    Vec<i64>,
    Vec<Option<i64>>,
    Vec<Vec<Bound<'py, PyAny>>>,
    Vec<i64>,
    Vec<i64>,
    Vec<Vec<u32>>,
);

/// An instruction stream sent from Python: (ops, ats, args, bodies, targets, streams); None where refused.
#[expect(
    clippy::many_single_char_names,
    reason = "one letter per column, as aptgraft's encoder"
)]
fn code_from(p: &Bound<'_, PyAny>) -> PyResult<Option<Code>> {
    let (ops, ats, args, bodies, targets, streams): CodeCols<'_> = p.extract()?;
    let n = ops.len();
    if ats.len() != n || args.len() != n || bodies.len() != n || targets.len() != n {
        return Ok(None);
    }
    let mut ins = Vec::with_capacity(n);
    for k in 0..n {
        let (Some(&op), Some(&at), Some(a), Some(&b), Some(&t)) =
            (ops.get(k), ats.get(k), args.get(k), bodies.get(k), targets.get(k))
        else {
            return Ok(None);
        };
        let Ok(op) = u8::try_from(op) else { return Ok(None) };
        let mut av = Vec::with_capacity(a.len());
        for x in a {
            av.push(if let Ok(v) = x.extract::<i64>() {
                if x.is_instance_of::<pyo3::types::PyBool>() {
                    Arg::Other
                } else {
                    Arg::Int(v)
                }
            } else if let Ok((r,)) = x.extract::<(u32,)>() {
                Arg::Ref(r)
            } else {
                Arg::Other
            });
        }
        ins.push(Ins {
            op,
            args: av,
            body: u32::try_from(b).ok(),
            target: match t {
                -1 => Target::Unset,
                -2 => Target::Missing,
                t if t >= 0 => Target::Ins(t as u32),
                t => Target::End((-t - 3) as u32),
            },
            at,
        });
    }
    Ok(Some(Code { ins, streams }))
}

/// One roundtrip job: (members, the container's bytes or None).
type Job<'py> = (Vec<(String, Bound<'py, PyBytes>)>, Option<Bound<'py, PyBytes>>);

/// `aptgraft.roundtrip` of many movies on all cores: (problems, notes) each, or None where the Python runs.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn apt_roundtrip_many(py: Python<'_>, jobs: Vec<Job<'_>>, pack: bool) -> Vec<Option<(Vec<String>, Vec<String>)>> {
    let jobs: Vec<(aptmodel::Members, Option<Vec<u8>>)> = jobs
        .iter()
        .map(|(m, raw)| {
            let m = m.iter().map(|(n, b)| (n.clone(), b.as_bytes().to_vec())).collect();
            (m, raw.as_ref().map(|r| r.as_bytes().to_vec()))
        })
        .collect();
    py.detach(|| {
        jobs.par_iter()
            .map(|(m, raw)| aptmodel::roundtrip(m, raw.as_deref(), pack).ok())
            .collect()
    })
}

/// One movie to scan: (name, apt, big, movie offset, constants) as apt.Movie holds them.
type MovieArg<'py> = (String, Bound<'py, PyBytes>, bool, i64, Vec<String>);

fn refs_of<'py>(movies: &'py [MovieArg<'py>]) -> Vec<MovieRef<'py>> {
    movies
        .iter()
        .map(|(name, data, big, movie, consts)| MovieRef {
            name,
            apt: data.as_bytes(),
            big: *big,
            movie: *movie,
            consts,
        })
        .collect()
}

/// `aptcheck check`'s numbers of many movies: (instructions, functions, constants, [(kind, key, text)]) or None
/// where the decoder refuses (aptcheck.Code runs).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
#[expect(clippy::type_complexity, reason = "the tuple aptcheck unpacks")]
pub fn apt_check_many(
    py: Python<'_>,
    movies: Vec<MovieArg<'_>>,
) -> Vec<Option<(usize, usize, usize, Vec<(&'static str, i64, String)>)>> {
    let refs = refs_of(&movies);
    let got = py.detach(|| aptscan::check_many(&refs));
    got.into_iter()
        .map(|c| c.map(|c| (c.instructions, c.functions, c.constants, c.findings)))
        .collect()
}

/// `aptrefs.movie_refs` of many movies (its dicts, keys in Python's order), or None where the Python runs.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn apt_refs_many<'py>(py: Python<'py>, movies: Vec<MovieArg<'py>>) -> PyResult<Bound<'py, PyList>> {
    let refs = refs_of(&movies);
    let got = py.detach(|| aptscan::refs_many(&refs));
    let out = PyList::empty(py);
    for r in got {
        let Some(r) = r else {
            out.append(py.None())?;
            continue;
        };
        let d = PyDict::new(py);
        for (key, names) in aptscan::REF_KINDS.iter().zip(r) {
            let inner = PyDict::new(py);
            for (name, uses) in names.items {
                let l = PyList::empty(py);
                for u in uses {
                    match u.event {
                        None => l.append(u.at)?,
                        Some((params, how)) => l.append((u.at, params, how))?,
                    }
                }
                inner.set_item(name, l)?;
            }
            d.set_item(key, inner)?;
        }
        out.append(d)?;
    }
    Ok(out)
}
