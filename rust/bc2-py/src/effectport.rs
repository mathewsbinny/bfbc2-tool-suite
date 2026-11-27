//! BF1943's effects and sounds in BFBC2 (round OR8, "OPFOR in Rust": `bc2_convert::effectport` and
//! `bc2_convert::soundport`) for `bfbc2-bf1943-import/effectport.py`: the walk, the templates' search, an effect's
//! and many effects' ports, the sound porter and its two passes.
//!
//! The game data stays the caller's objects: a `bf1943import.Source` (its `entries` read once a call, `data(key)`
//! asked for each resource) and a `meshview.Index` or a view of one (its `where` read once a call, `read(name)`
//! asked for each partition). A ported effect's answer is `('ok', files, lines, records)` or `('error', kind,
//! payload)` for the Python's own exception classes (`port`, `schema`, `unsupported`, `missing_template`,
//! `texture`, `value`, `exit`, `io`); an exception a callback raised is raised again.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use bc2_convert::dbxport::{CatView, Fail, SiKey, SiVal};
use bc2_convert::effectport::{self as ep, Entry, Error, PortContext, Res, Schema};
use bc2_convert::soundport;
use pyo3::exceptions::PyValueError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString, PyTuple};

use crate::dbxport::{DbxCatalogue, DbxRules, poisoned};

/// The exception a callback raised, kept until the core returns.
#[derive(Default)]
struct Raised(Mutex<Option<PyErr>>);

impl Raised {
    fn keep(&self, e: PyErr) -> Error {
        if let Ok(mut s) = self.0.lock()
            && s.is_none()
        {
            *s = Some(e);
        }
        Error::Callback
    }

    fn take(&self) -> PyErr {
        self.0
            .lock()
            .ok()
            .and_then(|mut s| s.take())
            .unwrap_or_else(|| PyValueError::new_err("effectport: a callback raised"))
    }
}

fn bytes_of(b: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    Ok(b.cast::<PyBytes>()?.as_bytes().to_vec())
}

/// A `bf1943import.Source`.
struct PySource<'r> {
    entries: HashMap<String, Entry>,
    obj: Py<PyAny>,
    raised: &'r Raised,
}

impl<'r> PySource<'r> {
    fn of(src: &Bound<'_, PyAny>, raised: &'r Raised) -> PyResult<Self> {
        let py = src.py();
        let entries = src.getattr(intern!(py, "entries"))?;
        let d = entries.cast::<PyDict>()?;
        let mut out = HashMap::with_capacity(d.len());
        for (k, v) in d.iter() {
            let key: String = k.extract()?;
            let e = v.get_item(1)?;
            let name: String = e.get_item(intern!(py, "name"))?.extract()?;
            let kind: Option<String> = e.call_method1(intern!(py, "get"), (intern!(py, "type"),))?.extract()?;
            out.insert(key, Entry { name, kind });
        }
        Ok(Self {
            entries: out,
            obj: src.clone().unbind(),
            raised,
        })
    }
}

impl ep::Source for PySource<'_> {
    fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries.get(key)
    }

    fn keys(&self) -> Vec<&str> {
        self.entries.keys().map(String::as_str).collect()
    }

    fn data(&self, key: &str) -> Res<Vec<u8>> {
        Python::attach(|py| {
            let got = self.obj.bind(py).call_method1(intern!(py, "data"), (key,));
            got.and_then(|b| bytes_of(&b)).map_err(|e| self.raised.keep(e))
        })
    }
}

/// A `meshview.Index` (or a view of one: `where` and `read`).
struct PyStock<'r> {
    where_: HashMap<String, (String, String)>,
    obj: Py<PyAny>,
    raised: &'r Raised,
}

impl<'r> PyStock<'r> {
    fn of(ix: &Bound<'_, PyAny>, raised: &'r Raised) -> PyResult<Self> {
        let table = ix.getattr(intern!(ix.py(), "where"))?;
        let d = table.cast::<PyDict>()?;
        let mut out = HashMap::with_capacity(d.len());
        for (k, v) in d.iter() {
            let Ok(k) = k.extract::<String>() else { continue };
            let place: (String, String) = v.extract()?;
            out.insert(k, place);
        }
        Ok(Self {
            where_: out,
            obj: ix.clone().unbind(),
            raised,
        })
    }
}

impl ep::Stock for PyStock<'_> {
    fn locate(&self, low: &str) -> Option<(&str, &str)> {
        self.where_.get(low).map(|(b, e)| (b.as_str(), e.as_str()))
    }

    fn names(&self) -> Vec<&str> {
        self.where_.keys().map(String::as_str).collect()
    }

    fn read(&self, name: &str) -> Res<Option<Vec<u8>>> {
        Python::attach(|py| {
            let got = self.obj.bind(py).call_method1(intern!(py, "read"), (name,));
            let got = got.and_then(|b| if b.is_none() { Ok(None) } else { bytes_of(&b).map(Some) });
            got.map_err(|e| self.raised.keep(e))
        })
    }
}

/// The catalogue's tables and the class defaults of every type `defaults` names.
struct Cat<'a> {
    handle: &'a DbxCatalogue,
    defaults: HashMap<String, (String, bc2_convert::dbxport::Id)>,
}

fn cat_of<'a>(handle: Option<&'a DbxCatalogue>, defaults: Option<&Bound<'_, PyDict>>) -> PyResult<Option<Cat<'a>>> {
    let (Some(h), Some(d)) = (handle, defaults) else {
        return Ok(None);
    };
    let types: HashSet<String> = d.keys().iter().filter_map(|k| k.extract::<String>().ok()).collect();
    Ok(Some(Cat {
        handle: h,
        defaults: h.defaults(d, &types)?,
    }))
}

/// Runs `f` with the port context (the rules and the catalogue's view), without the interpreter's lock.
fn with_context<T: Send>(
    py: Python<'_>,
    rules: &DbxRules,
    cat: Option<&Cat<'_>>,
    f: impl FnOnce(&PortContext<'_>) -> T + Send,
) -> PyResult<T> {
    let guard = cat.map(|c| c.handle.els.read()).transpose().map_err(poisoned)?;
    let view = match (cat, &guard) {
        (Some(c), Some(els)) => Some(CatView {
            cat: &c.handle.tables,
            els,
            defaults: &c.defaults,
        }),
        _ => None,
    };
    let ctx = PortContext {
        rules: &rules.rules,
        catalogue: view,
    };
    Ok(py.detach(|| f(&ctx)))
}

/// An error as the Python wrapper raises it: `('error', kind, payload)`, or raised here (a callback's exception,
/// a bundle that does not read).
fn error<'py>(py: Python<'py>, e: Error, raised: &Raised) -> PyResult<Bound<'py, PyTuple>> {
    let tag = |kind: &str, payload: Bound<'py, PyAny>| ("error", kind, payload).into_pyobject(py);
    let s = |m: &str| PyString::new(py, m).into_any();
    match e {
        Error::Callback | Error::Port(Fail::Callback) => Err(raised.take()),
        Error::Bundle(b, e) => Err(crate::fbrb_error(e, &b, &[])),
        Error::Value(m) | Error::Port(Fail::Parse(m)) => tag("value", s(&m)),
        Error::MissingTemplate(m) => tag("missing_template", s(&m)),
        Error::Texture(m) => tag("texture", s(&m)),
        Error::Io(m) => tag("io", s(&m)),
        Error::Exit(m) => tag("exit", s(&m)),
        Error::Port(Fail::Port(m)) => tag("port", s(&m)),
        Error::Port(Fail::Schema(p)) => tag("schema", p.into_pyobject(py)?.into_any()),
        Error::Port(Fail::Unsupported(m)) => tag("unsupported", s(&m)),
    }
}

fn record_tuple<'py>(py: Python<'py>, r: &ep::Record) -> PyResult<Bound<'py, PyTuple>> {
    (
        &r.path,
        &r.action,
        r.old.as_deref(),
        r.new.as_deref(),
        r.source.as_deref(),
        r.step.as_deref(),
        r.note.as_deref(),
        r.partition.as_deref(),
        r.text.as_deref(),
    )
        .into_pyobject(py)
}

fn ported<'py>(py: Python<'py>, files: &ep::Files, report: &ep::Report) -> PyResult<Bound<'py, PyTuple>> {
    let fl = PyList::empty(py);
    for (n, d) in &files.items {
        fl.append((n, PyBytes::new(py, d)))?;
    }
    let recs = PyList::empty(py);
    for r in &report.records {
        recs.append(record_tuple(py, r)?)?;
    }
    ("ok", fl, &report.lines, recs).into_pyobject(py)
}

fn schema_of(schema: &str) -> PyResult<Schema> {
    match schema {
        "error" => Ok(Schema::Error),
        "report" => Ok(Schema::Report),
        _ => Err(PyValueError::new_err(format!("schema {schema:?}: 'error' or 'report'"))),
    }
}

/// `effectport.port_many(src, ix, effects, level, log, catalogue, schema)`: `('ok', [(file, bytes)], lines,
/// [(path, action, old, new, source, step, note, partition, text)])` or `('error', kind, payload)`; `log(line)`
/// hears each effect's line. `catalogue` / `defaults`: `dbxport._native_catalogue`'s handle and the catalogue's
/// `defaults` (both or neither).
///
/// # Errors
/// What `log`, `src.data` or `ix.read` raised; a bundle that does not read.
#[allow(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts the arguments as owned values"
)]
#[pyfunction]
#[pyo3(signature = (src, ix, effects, level, rules, catalogue=None, defaults=None, schema="error", log=None))]
#[expect(
    clippy::too_many_arguments,
    reason = "effectport.port_many's arguments and the port's tables"
)]
pub fn effectport_port_many<'py>(
    py: Python<'py>,
    src: &Bound<'py, PyAny>,
    ix: &Bound<'py, PyAny>,
    effects: Vec<String>,
    level: &str,
    rules: &DbxRules,
    catalogue: Option<&DbxCatalogue>,
    defaults: Option<&Bound<'py, PyDict>>,
    schema: &str,
    log: Option<Py<PyAny>>,
) -> PyResult<Bound<'py, PyTuple>> {
    let raised = Raised::default();
    let s = PySource::of(src, &raised)?;
    let x = PyStock::of(ix, &raised)?;
    let cat = cat_of(catalogue, defaults)?;
    let schema = schema_of(schema)?;
    let got = with_context(py, rules, cat.as_ref(), |ctx| {
        let mut say = |line: &str| -> Res<()> {
            let Some(f) = &log else { return Ok(()) };
            Python::attach(|py| f.bind(py).call1((line,)).map(|_| ()).map_err(|e| raised.keep(e)))
        };
        ep::port_many(&s, &x, &effects, level, ctx, schema, &mut say)
    })?;
    match got {
        Ok((files, report)) => ported(py, &files, &report),
        Err(e) => error(py, e, &raised),
    }
}

/// `effectport.port(src, ix, effect, level, catalogue, schema)`: as [`effectport_port_many`] for one effect, its
/// templates found for it alone.
///
/// # Errors
/// As [`effectport_port_many`].
#[pyfunction]
#[pyo3(signature = (src, ix, effect, level, rules, catalogue=None, defaults=None, schema="error"))]
#[expect(
    clippy::too_many_arguments,
    reason = "effectport.port's arguments and the port's tables"
)]
pub fn effectport_port<'py>(
    py: Python<'py>,
    src: &Bound<'py, PyAny>,
    ix: &Bound<'py, PyAny>,
    effect: &str,
    level: &str,
    rules: &DbxRules,
    catalogue: Option<&DbxCatalogue>,
    defaults: Option<&Bound<'py, PyDict>>,
    schema: &str,
) -> PyResult<Bound<'py, PyTuple>> {
    let raised = Raised::default();
    let s = PySource::of(src, &raised)?;
    let x = PyStock::of(ix, &raised)?;
    let cat = cat_of(catalogue, defaults)?;
    let schema = schema_of(schema)?;
    let got = with_context(py, rules, cat.as_ref(), |ctx| {
        let bc2 = ep::bc2_partitions(&x);
        let mut seen = HashMap::new();
        ep::port(&s, &x, effect, level, ctx, None, None, schema, &mut seen, &bc2)
    })?;
    match got {
        Ok((files, report)) => ported(py, &files, &report),
        Err(e) => error(py, e, &raised),
    }
}

/// `effectport.walk(src, ix, effect, level, fallback)`: `[(name, source key or None, kind)]`.
///
/// # Errors
/// As [`effectport_port_many`]; `ValueError` for a partition that does not read.
#[pyfunction]
#[pyo3(signature = (src, ix, effect, level, fallback=true))]
pub fn effectport_walk<'py>(
    py: Python<'py>,
    src: &Bound<'py, PyAny>,
    ix: &Bound<'py, PyAny>,
    effect: &str,
    level: &str,
    fallback: bool,
) -> PyResult<Vec<(String, Option<String>, &'static str)>> {
    let raised = Raised::default();
    let s = PySource::of(src, &raised)?;
    let x = PyStock::of(ix, &raised)?;
    match py.detach(|| ep::walk(&s, &x, effect, level, fallback)) {
        Ok(plan) => Ok(plan.into_iter().map(|i| (i.name, i.key, i.kind.name())).collect()),
        Err(Error::Callback) => Err(raised.take()),
        Err(e) => Err(PyValueError::new_err(e.to_string())),
    }
}

/// `effectport.templates(ix, types)`'s search: `(pool, types still needed, sorted)`, and the emitter index's counts
/// `(documents, batches read, from the disk cache)`.
///
/// # Errors
/// As [`effectport_port_many`].
#[pyfunction]
#[expect(clippy::type_complexity, reason = "the answer's tuple, as the Python takes it")]
pub fn effectport_templates<'py>(
    py: Python<'py>,
    ix: &Bound<'py, PyAny>,
    types: Vec<String>,
    rules: &DbxRules,
) -> PyResult<(Vec<String>, Vec<String>, (usize, usize, bool))> {
    let raised = Raised::default();
    let x = PyStock::of(ix, &raised)?;
    let types: HashSet<Option<String>> = types.into_iter().map(Some).collect();
    let got = py.detach(|| ep::templates(&x, &types, &rules.rules));
    match got {
        Ok(c) => {
            let mut need: Vec<String> = c.need.into_iter().flatten().collect();
            need.sort_unstable();
            let idx = ep::EmitterIndex::of_index(&x);
            let i = idx.lock().map_err(poisoned)?;
            Ok((c.pool, need, (i.cands.len(), i.batches_read, i.from_disk)))
        }
        Err(Error::Callback) => Err(raised.take()),
        Err(Error::Bundle(b, e)) => Err(crate::fbrb_error(e, &b, &[])),
        Err(e) => Err(PyValueError::new_err(e.to_string())),
    }
}

/// `effectport.levels_of(src)`: BF1943's level folders (`levels/<name>`) holding level copies, sorted.
///
/// # Errors
/// A `src.entries` that is not a dict of `(bundle, entry)` pairs.
#[pyfunction]
pub fn effectport_levels(src: &Bound<'_, PyAny>) -> PyResult<Vec<String>> {
    let raised = Raised::default();
    Ok(ep::levels_of(&PySource::of(src, &raised)?))
}

/// Forgets the process's emitter indexes (the next search reads the disk cache or the bundles again).
#[pyfunction]
pub fn effectport_forget() {
    ep::EmitterIndex::forget();
}

/// The sound porter (Project OPFOR's `wake_level.port_sounds`): ports the BF1943 sound partitions under `roots`
/// (and those they name under `follow`'s prefixes) that BFBC2 lacks into `out`, their waves converted. `donors`:
/// the template donors (the default ones first), `last_donors` taken last, `stand_ins` `{partition: reference}`,
/// `later` `{partition: step}`. `('ok', [partitions ported])` or `('error', kind, payload)`.
///
/// # Errors
/// As [`effectport_port_many`].
#[allow(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts the arguments as owned values"
)]
#[pyfunction]
#[pyo3(signature = (src, ix, roots, out, donors, last_donors, follow, stand_ins, later, rules, catalogue=None,
                    defaults=None, log=None))]
#[expect(clippy::too_many_arguments, reason = "port_sounds' arguments and the port's tables")]
pub fn soundport_port<'py>(
    py: Python<'py>,
    src: &Bound<'py, PyAny>,
    ix: &Bound<'py, PyAny>,
    roots: Vec<String>,
    out: String,
    donors: Vec<String>,
    last_donors: Vec<String>,
    follow: Vec<String>,
    stand_ins: BTreeMap<String, String>,
    later: BTreeMap<String, String>,
    rules: &DbxRules,
    catalogue: Option<&DbxCatalogue>,
    defaults: Option<&Bound<'py, PyDict>>,
    log: Option<Py<PyAny>>,
) -> PyResult<Bound<'py, PyTuple>> {
    let raised = Raised::default();
    let s = PySource::of(src, &raised)?;
    let x = PyStock::of(ix, &raised)?;
    let cat = cat_of(catalogue, defaults)?;
    let spec = soundport::Spec {
        roots,
        out,
        donors,
        last_donors,
        follow,
        stand_ins: stand_ins
            .into_iter()
            .map(|(k, v)| (SiKey::Target(k), SiVal::Ref(v)))
            .collect(),
        later,
    };
    let got = with_context(py, rules, cat.as_ref(), |ctx| {
        let mut say = |line: &str| -> Res<()> {
            let Some(f) = &log else { return Ok(()) };
            Python::attach(|py| f.bind(py).call1((line,)).map(|_| ()).map_err(|e| raised.keep(e)))
        };
        soundport::port_sounds(&s, &x, &spec, ctx, &mut say)
    })?;
    match got {
        Ok(want) => ("ok", want).into_pyobject(py),
        Err(e) => error(py, e, &raised),
    }
}

/// `sound_fills(data, source)`: `(bytes, fields changed)`.
///
/// # Errors
/// `ValueError`: a partition that does not read or write.
#[pyfunction]
pub fn soundport_fills<'py>(py: Python<'py>, data: &[u8], source: &[u8]) -> PyResult<(Bound<'py, PyBytes>, usize)> {
    let (d, n) = py
        .detach(|| soundport::sound_fills(data, source))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok((PyBytes::new(py, &d), n))
}

/// `not_streaming(data)`: `(bytes, whether they changed)`.
///
/// # Errors
/// `ValueError`: a partition that does not read or write.
#[pyfunction]
pub fn soundport_not_streaming<'py>(py: Python<'py>, data: &[u8]) -> PyResult<(Bound<'py, PyBytes>, bool)> {
    let (d, c) = py
        .detach(|| soundport::not_streaming(data))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok((PyBytes::new(py, &d), c))
}

/// `audio.convert` with the reason a Wave does not convert: `(bytes, None)` or `(None, why)`.
#[allow(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts the arguments as owned values"
)]
#[pyfunction]
#[pyo3(signature = (b, source, streams, word14=0))]
pub fn audio_convert_checked<'py>(
    py: Python<'py>,
    b: &[u8],
    source: &str,
    streams: [Option<Vec<u8>>; 3],
    word14: u32,
) -> (Option<Bound<'py, PyBytes>>, Option<String>) {
    let views = [streams[0].as_deref(), streams[1].as_deref(), streams[2].as_deref()];
    match py.detach(|| bc2_convert::wavport::convert_checked(b, source, &views, word14)) {
        Ok(d) => (Some(PyBytes::new(py, &d)), None),
        Err(e) => (None, Some(e)),
    }
}
