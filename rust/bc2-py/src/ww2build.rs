//! Round `OR2` ("OPFOR in Rust", stage 2: the level build): Project OPFOR's `wake_install.build` and its parts in the
//! Rust core (`bc2_install::levelbuild`). OPFOR's `wake_install.py` keeps its entry points as thin callers that pass its
//! bindings (the level's constants, as JSON) and its data (our files, the take) and get the same values back.
//!
//! Python's shapes: our files `{lower name: (name, path)}`; a take `{lower name: (name, type or None, source)}`, the
//! source a path (`str`), a stock entry (`(bundle, entry name)`) or `bytes`. A step's failure raises what the Python
//! raised: `ValueError`, `KeyError` or `SystemExit` with its message.

#![allow(
    clippy::many_single_char_names,
    clippy::type_complexity,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::map_unwrap_or,
    clippy::doc_markdown,
    reason = "the Python's names and shapes; every name compared is lower case already"
)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::thread::JoinHandle;

use bc2_install::levelbuild::closure::{self, Src, Take};
use bc2_install::levelbuild::index::{self, OurFile, StockIndex};
use bc2_install::levelbuild::{OMap, describe, gates, levelfiles, stage, terrainfiles};
use pyo3::exceptions::{PyKeyError, PySystemExit, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySet, PyTuple};

fn err(msg: String) -> PyErr {
    if let Some(m) = msg.strip_prefix("ValueError: ") {
        return PyValueError::new_err(m.to_owned());
    }
    if let Some(m) = msg.strip_prefix("KeyError: ") {
        return PyKeyError::new_err(m.trim_matches('\'').to_owned());
    }
    PySystemExit::new_err(msg)
}

fn spec<T: serde::de::DeserializeOwned>(json: &str, what: &str) -> PyResult<T> {
    serde_json::from_str(json).map_err(|e| PyValueError::new_err(format!("{what}: {e}")))
}

/// The stock index (`wake_install.stock_index`): kept by the Rust core for the process.
#[pyclass(module = "bc2_native", frozen)]
pub struct Ww2Index {
    ix: Arc<StockIndex>,
}

#[pymethods]
impl Ww2Index {
    /// `[(bundle, {lower name: (name, type)})]`, the Python index's shape.
    fn items<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let out = PyList::empty(py);
        for b in &self.ix.bundles {
            let d = PyDict::new(py);
            for (low, n, t) in &b.names {
                d.set_item(low, (n, t))?;
            }
            out.append((&b.path, d))?;
        }
        Ok(out)
    }

    /// The kept index's identity (the same while the Rust core keeps the same index).
    fn key(&self) -> usize {
        Arc::as_ptr(&self.ix) as usize
    }

    /// The bundles that could not be read (`<path>: <error>`).
    fn unread(&self) -> Vec<String> {
        self.ix.unread.clone()
    }

    /// An index of the Python shape's items (`[(bundle, {lower name: (name, type)})]`: a test's).
    #[staticmethod]
    fn from_items(items: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            ix: Arc::new(index_from_items(items)?),
        })
    }
}

fn index_from_items(items: &Bound<'_, PyAny>) -> PyResult<StockIndex> {
    let mut bundles = Vec::new();
    for it in items.try_iter()? {
        let it = it?;
        let t = it.cast::<PyTuple>()?;
        let b: String = t.get_item(0)?.extract()?;
        let names = t.get_item(1)?;
        let names = names.cast::<PyDict>()?;
        let mut es = Vec::new();
        for (_, v) in names.iter() {
            let (n, ty): (String, String) = v.extract()?;
            es.push((n, ty));
        }
        bundles.push((b, es));
    }
    Ok(StockIndex::from_bundles(bundles))
}

/// The index a Python value stands for: a [`Ww2Index`], an object whose `native` is one (`wake_install`'s list),
/// or a list of the Python shape.
fn index_of(obj: &Bound<'_, PyAny>) -> PyResult<Arc<StockIndex>> {
    if let Ok(i) = obj.cast::<Ww2Index>() {
        return Ok(Arc::clone(&i.get().ix));
    }
    if let Ok(n) = obj.getattr("native")
        && let Ok(i) = n.cast::<Ww2Index>()
    {
        return Ok(Arc::clone(&i.get().ix));
    }
    Ok(Arc::new(index_from_items(obj)?))
}

/// `stock_index()`: the index of the stock bundles (`stock_bundles`), kept for the process while every bundle keeps
/// its size and date.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_stock_index(py: Python<'_>, game: &str, lv: &str, ww2_levels: Vec<String>) -> Ww2Index {
    Ww2Index {
        ix: py.detach(|| index::stock_index(game, lv, &ww2_levels)),
    }
}

/// Forgets the kept stock index.
#[pyfunction]
pub fn ww2build_forget_index() {
    index::forget_index();
}

/// `stock_bundles()`.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_stock_bundles(game: &str, lv: &str, ww2_levels: Vec<String>) -> Vec<String> {
    index::stock_bundles(game, lv, &ww2_levels)
}

/// `ondemand_bundles()`.
#[pyfunction]
pub fn ww2build_ondemand_bundles(game: &str) -> Vec<String> {
    index::ondemand_bundles(game)
}

/// `masked_names()`: the lower names `masked_by`'s deleted entries hide.
#[pyfunction]
pub fn ww2build_masked_names<'py>(py: Python<'py>, masked_by: &str) -> PyResult<Bound<'py, PySet>> {
    let mut v: Vec<String> = index::masked_names(masked_by).into_iter().collect();
    v.sort_unstable();
    PySet::new(py, v)
}

/// `live_entries(b)`: (name, type) of a loaded bundle's entries the level can use.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_live_entries(bundle: &str, masked: Vec<String>, masked_by: &str) -> PyResult<Vec<(String, String)>> {
    let hide = index::masked_names(masked_by);
    index::live_entries(bundle, &masked, &hide).map_err(err)
}

fn ours_from(d: &Bound<'_, PyAny>) -> PyResult<OMap<OurFile>> {
    let d = d.cast::<PyDict>()?;
    let mut out = OMap::new();
    for (k, v) in d.iter() {
        let k: String = k.extract()?;
        let (rel, path): (String, String) = v.extract()?;
        out.insert(&k, (rel, path));
    }
    Ok(out)
}

fn ours_to<'py>(py: Python<'py>, o: &OMap<OurFile>) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    for (k, (rel, path)) in o.iter() {
        d.set_item(k, (rel, path))?;
    }
    Ok(d)
}

fn take_from(d: &Bound<'_, PyAny>) -> PyResult<OMap<Take>> {
    let d = d.cast::<PyDict>()?;
    let mut out = OMap::new();
    for (k, v) in d.iter() {
        let k: String = k.extract()?;
        let t = v.cast::<PyTuple>()?;
        let name: String = t.get_item(0)?.extract()?;
        let kind: Option<String> = t.get_item(1)?.extract()?;
        let s = t.get_item(2)?;
        let src = if let Ok(b) = s.cast::<PyBytes>() {
            Src::Bytes(b.as_bytes().to_vec())
        } else if let Ok(p) = s.extract::<String>() {
            Src::Path(p)
        } else {
            let (b, n): (String, String) = s.extract()?;
            Src::Stock(b, n)
        };
        out.insert(&k, Take { name, kind, src });
    }
    Ok(out)
}

fn src_to<'py>(py: Python<'py>, s: &Src) -> PyResult<Bound<'py, PyAny>> {
    Ok(match s {
        Src::Path(p) => p.into_pyobject(py)?.into_any(),
        Src::Stock(b, n) => (b, n).into_pyobject(py)?.into_any(),
        Src::Bytes(b) => PyBytes::new(py, b).into_any(),
    })
}

fn take_to<'py>(py: Python<'py>, o: &OMap<Take>) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    for (k, t) in o.iter() {
        d.set_item(k, (&t.name, &t.kind, src_to(py, &t.src)?))?;
    }
    Ok(d)
}

fn str_set(s: &Bound<'_, PyAny>) -> PyResult<HashSet<String>> {
    let mut out = HashSet::new();
    for x in s.try_iter()? {
        out.insert(x?.extract()?);
    }
    Ok(out)
}

/// `our_files()`: `({lower name: (name, path)}, [clash])` (spec: `index::OursSpec` as JSON).
#[pyfunction]
pub fn ww2build_our_files<'py>(py: Python<'py>, spec_json: &str) -> PyResult<(Bound<'py, PyDict>, Vec<String>)> {
    let s: index::OursSpec = spec(spec_json, "our_files")?;
    let (o, clash) = py.detach(|| index::our_files(&s));
    Ok((ours_to(py, &o)?, clash))
}

/// `closure(level_data, ours, index, loaded, preloaded, report, starts)` (spec: `closure::ClosureSpec` as JSON):
/// `(take, unresolved {p: (by, kind)}, shaders set, unreached, dup_loaded set, seen set, dangling, report lines)`.
#[pyfunction]
#[allow(clippy::too_many_arguments, reason = "the Python closure's arguments")]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_closure<'py>(
    py: Python<'py>,
    spec_json: &str,
    level_data: &[u8],
    ours: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
    loaded: &Bound<'py, PyAny>,
    preloaded: &Bound<'py, PyAny>,
    starts: Vec<(String, String)>,
) -> PyResult<Bound<'py, PyTuple>> {
    let s: closure::ClosureSpec = spec(spec_json, "closure")?;
    let ours = ours_from(ours)?;
    let ix = index_of(index)?;
    let loaded = str_set(loaded)?;
    let preloaded = str_set(preloaded)?;
    let mut report = Vec::new();
    let c = py
        .detach(|| {
            let inp = closure::Inputs {
                level_data,
                ours: &ours,
                index: &ix,
                loaded: &loaded,
                preloaded: &preloaded,
                starts: &starts,
            };
            closure::closure(&s, &inp, &mut report)
        })
        .map_err(err)?;
    let unresolved = PyDict::new(py);
    for (p, by, kind) in &c.unresolved {
        unresolved.set_item(p, (by, kind))?;
    }
    let dangling: Vec<(String, String, String, String)> = c.dangling;
    PyTuple::new(
        py,
        [
            take_to(py, &c.take)?.into_any(),
            unresolved.into_any(),
            PySet::new(py, &c.shaders)?.into_any(),
            c.unreached.into_pyobject(py)?.into_any(),
            PySet::new(py, &c.dup_loaded)?.into_any(),
            PySet::new(py, &c.seen)?.into_any(),
            dangling.into_pyobject(py)?.into_any(),
            report.into_pyobject(py)?.into_any(),
        ],
    )
}

/// `primary_type(data)`: (type, guid) of the partition's primary instance, each `None` when there is none.
#[pyfunction]
pub fn ww2build_primary_type(data: &[u8]) -> PyResult<(Option<String>, Option<String>)> {
    index::primary_type(data).map_err(err)
}

static STAGED_KEEP: std::sync::Mutex<Vec<Py<PyBytes>>> = std::sync::Mutex::new(Vec::new());

/// `_STAGED.clear()`: a new build (the staging records forgotten).
#[pyfunction]
pub fn ww2build_stage_reset() {
    stage::STAGED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
    STAGED_KEEP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

/// `write_stage(stage, files, last)`: the files written into the folder with its manifest; the manifest lines. A
/// file of bytes already written from the same bytes object, and not changed since, is not written again.
#[pyfunction]
#[pyo3(signature = (folder, files, last=None))]
pub fn ww2build_write_stage(folder: &str, files: &Bound<'_, PyAny>, last: Option<&str>) -> PyResult<Vec<String>> {
    let d = files.cast::<PyDict>()?;
    let mut keep: Vec<Bound<'_, PyTuple>> = Vec::new();
    let mut lows: Vec<String> = Vec::new();
    for (k, v) in d.iter() {
        lows.push(k.extract()?);
        keep.push(v.cast_into::<PyTuple>()?);
    }
    let mut names: Vec<String> = Vec::with_capacity(keep.len());
    let mut kinds: Vec<Option<String>> = Vec::with_capacity(keep.len());
    let mut paths: Vec<Option<String>> = Vec::with_capacity(keep.len());
    let mut held: Vec<Option<Bound<'_, PyBytes>>> = Vec::with_capacity(keep.len());
    for t in &keep {
        names.push(t.get_item(0)?.extract()?);
        kinds.push(t.get_item(1)?.extract()?);
        let s = t.get_item(2)?;
        if let Ok(b) = s.cast::<PyBytes>() {
            held.push(Some(b.clone()));
            paths.push(None);
        } else {
            held.push(None);
            paths.push(s.extract::<String>().ok());
        }
    }
    let mut files_v = Vec::with_capacity(keep.len());
    for i in 0..keep.len() {
        let src = match (
            held.get(i).and_then(Option::as_ref),
            paths.get(i).and_then(Option::as_deref),
        ) {
            (Some(b), _) => stage::StageSrc::Bytes(b.as_bytes(), b.as_ptr() as u64),
            (None, Some(p)) => stage::StageSrc::Path(p),
            _ => stage::StageSrc::None,
        };
        files_v.push(stage::StageFile {
            low: lows.get(i).map_or("", String::as_str),
            name: names.get(i).map_or("", String::as_str),
            kind: kinds.get(i).and_then(Option::as_deref),
            src,
        });
    }
    let mut st = stage::STAGED.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let lines = stage::write_stage(folder, &files_v, last, &mut st).map_err(err)?;
    // the bytes objects stay alive while their records do, so an identity is never another object's
    let mut k = STAGED_KEEP.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    k.extend(held.into_iter().flatten().map(Bound::unbind));
    Ok(lines)
}

/// `fetch_stock(files)`: the stock sources of the take read into bytes, in place.
#[pyfunction]
pub fn ww2build_fetch_stock(py: Python<'_>, files: &Bound<'_, PyDict>) -> PyResult<()> {
    let mut by: Vec<(String, Vec<(Bound<'_, PyAny>, String)>)> = Vec::new();
    for (k, v) in files.iter() {
        let t = v.cast::<PyTuple>()?;
        let s = t.get_item(2)?;
        if s.cast::<PyBytes>().is_ok() || s.extract::<String>().is_ok() {
            continue;
        }
        let (b, n): (String, String) = s.extract()?;
        match by.iter_mut().find(|(x, _)| *x == b) {
            Some((_, v)) => v.push((k, n)),
            None => by.push((b, vec![(k, n)])),
        }
    }
    for (b, items) in by {
        let names: Vec<String> = items.iter().map(|(_, n)| n.clone()).collect();
        let data = py
            .detach(|| bc2_install::levelbuild::bundle_read(&b, &names))
            .map_err(err)?;
        for (k, n) in items {
            let v = files.get_item(&k)?.ok_or_else(|| PyKeyError::new_err("gone"))?;
            let t = v.cast::<PyTuple>()?;
            let d = data.get(&n).map_or(&[][..], Vec::as_slice);
            files.set_item(&k, (t.get_item(0)?, t.get_item(1)?, PyBytes::new(py, d)))?;
        }
    }
    Ok(())
}

/// A pack started on its own thread (`pack_level` with `PACK_LATER`): `wait()` gives `None` or the failure's text.
#[pyclass(module = "bc2_native")]
pub struct Ww2PackJob {
    handle: Option<JoinHandle<Result<(), String>>>,
}

#[pymethods]
impl Ww2PackJob {
    /// Waits for the pack: `None` when every bundle was packed, else the first failure's text.
    fn wait(&mut self, py: Python<'_>) -> Option<String> {
        let h = self.handle.take()?;
        py.detach(|| match h.join() {
            Ok(r) => r.err(),
            Err(_) => Some("the packing thread panicked".to_owned()),
        })
    }
}

/// `cc.pack_many(items)` on a thread of its own (the packs of a level while the next one builds).
#[pyfunction]
pub fn ww2build_pack_start(items: Vec<(String, String)>) -> Ww2PackJob {
    Ww2PackJob {
        handle: Some(std::thread::spawn(move || stage::pack_many(&items))),
    }
}

/// `cc.pack_many(items)`: `SystemExit` naming the first folder that could not be packed.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_pack_many(py: Python<'_>, items: Vec<(String, String)>) -> PyResult<()> {
    py.detach(|| stage::pack_many(&items)).map_err(err)
}

/// `_pack_lines(bundles)`: the report's line per packed bundle.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_pack_lines(bundles: Vec<(String, usize, String)>) -> PyResult<Vec<String>> {
    bundles
        .iter()
        .map(|(k, n, o)| stage::pack_line(k, *n, o))
        .collect::<Result<_, _>>()
        .map_err(err)
}

/// `patch_bundle(bundle, files, report, label)` on `source` (the bundle as the install builds on it): (the built
/// bundle, the report's lines). `files`: `{name: (type, path)}`.
#[pyfunction]
pub fn ww2build_patch_bundle(
    py: Python<'_>,
    game: &str,
    work: &str,
    bundle: &str,
    source: &str,
    files: HashMap<String, (String, String)>,
    label: &str,
) -> PyResult<(String, Vec<String>)> {
    let list: Vec<(String, String, String)> = files.into_iter().map(|(n, (t, p))| (n, t, p)).collect();
    let mut report = Vec::new();
    let out = py
        .detach(|| stage::patch_bundle(game, work, bundle, source, &list, label, &mut report))
        .map_err(err)?;
    Ok((out, report))
}

/// `build_ww2_common(files, report)`: ({kind: bundle}, the report's lines). `files`: `{kind: {lower name: (name,
/// type, path)}}`.
#[pyfunction]
pub fn ww2build_ww2_common(
    py: Python<'_>,
    game: &str,
    work: &str,
    files: HashMap<String, HashMap<String, (String, String, String)>>,
) -> PyResult<(Vec<(String, String)>, Vec<String>)> {
    let f: HashMap<String, Vec<(String, String, String)>> =
        files.into_iter().map(|(k, v)| (k, v.into_values().collect())).collect();
    let mut report = Vec::new();
    let built = py
        .detach(|| stage::build_ww2_common(game, work, &f, &mut report))
        .map_err(err)?;
    Ok((built, report))
}

/// `loaded_names(bundles, masked_by, masked)`.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_loaded_names(
    bundles: Vec<String>,
    masked_by: &str,
    masked: Vec<String>,
) -> Vec<(String, HashMap<String, String>)> {
    gates::loaded_names(&bundles, masked_by, &masked)
}

/// `model_group_check(level_dir, loaded, report)`: (all fit, the report's lines).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_model_group_check(
    py: Python<'_>,
    level_dir: &str,
    loaded: Vec<(String, HashMap<String, String>)>,
) -> PyResult<(bool, Vec<String>)> {
    let mut report = Vec::new();
    let ok = py
        .detach(|| gates::model_group_check(level_dir, &loaded, &mut report))
        .map_err(err)?;
    Ok((ok, report))
}

/// `level_names(reached, ours, index)`: `{'vehicle': set, 'weapon': set, 'from': {Name: partition}, 'unheld': set}`.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_level_names<'py>(
    py: Python<'py>,
    reached: Vec<String>,
    ours: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let ours = ours_from(ours)?;
    let ix = index_of(index)?;
    let n = py.detach(|| gates::level_names(&reached, &ours, &ix)).map_err(err)?;
    let d = PyDict::new(py);
    d.set_item("vehicle", PySet::new(py, &n.vehicle)?)?;
    d.set_item("weapon", PySet::new(py, &n.weapon)?)?;
    d.set_item("from", n.from)?;
    d.set_item("unheld", PySet::new(py, &n.unheld)?)?;
    Ok(d)
}

/// `icon_gate(names, movie, report, reclaimed, reclaim_re)`: (passed, the report's line). `names`: `level_names`'
/// dictionary; `reclaim_re`: the pattern's text or `None`.
#[pyfunction]
#[pyo3(signature = (names, movie, reclaimed, reclaim_re=None))]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_icon_gate(
    names: &Bound<'_, PyDict>,
    movie: &str,
    reclaimed: Vec<String>,
    reclaim_re: Option<&str>,
) -> PyResult<(bool, String)> {
    let set = |k: &str| -> PyResult<std::collections::BTreeSet<String>> {
        match names.get_item(k)? {
            Some(v) => Ok(str_set(&v)?.into_iter().collect()),
            None => Ok(std::collections::BTreeSet::new()),
        }
    };
    let n = gates::LevelNames {
        vehicle: set("vehicle")?,
        weapon: set("weapon")?,
        unheld: set("unheld")?,
        from: match names.get_item("from")? {
            Some(v) => v.extract()?,
            None => HashMap::new(),
        },
    };
    let mut report = Vec::new();
    let ok = gates::icon_gate(&n, movie, &reclaimed, reclaim_re, &mut report).map_err(err)?;
    Ok((ok, report.concat()))
}

/// `description(ours, report)` (spec: `describe::DescSpec` as JSON): (the built partition, the report's lines).
#[pyfunction]
pub fn ww2build_description(py: Python<'_>, spec_json: &str) -> PyResult<(String, Vec<String>)> {
    let s: describe::DescSpec = spec(spec_json, "description")?;
    let mut report = Vec::new();
    let out = py.detach(|| describe::description(&s, &mut report)).map_err(err)?;
    Ok((out, report))
}

/// `air_superiority_mode(report)` (spec: `describe::ModeSpec` as JSON): ((path, guid), the report's lines).
#[pyfunction]
pub fn ww2build_air_superiority_mode(py: Python<'_>, spec_json: &str) -> PyResult<((String, String), Vec<String>)> {
    let s: describe::ModeSpec = spec(spec_json, "air_superiority_mode")?;
    let mut report = Vec::new();
    let out = py
        .detach(|| describe::air_superiority_mode(&s, &mut report))
        .map_err(err)?;
    Ok((out, report))
}

/// `level_list_query(report, levels)` (spec: `describe::ListSpec` as JSON): (the built partition, the lines).
#[pyfunction]
pub fn ww2build_level_list_query(py: Python<'_>, spec_json: &str) -> PyResult<(String, Vec<String>)> {
    let s: describe::ListSpec = spec(spec_json, "level_list_query")?;
    let mut report = Vec::new();
    let out = py.detach(|| describe::level_list_query(&s, &mut report)).map_err(err)?;
    Ok((out, report))
}

/// `level_list_items(items, levels)` (levels: their `lv`s).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_level_list_items(
    items: Vec<String>,
    levels: Vec<String>,
    ww2_levels: Vec<String>,
) -> PyResult<Vec<String>> {
    let re = describe::ww2_list_item(&ww2_levels).map_err(err)?;
    Ok(describe::level_list_items(&items, &levels, &re))
}

/// `level_list_xml(x, levels)` (levels: their `lv`s).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_level_list_xml(x: &str, levels: Vec<String>, ww2_levels: Vec<String>, entry: &str) -> PyResult<String> {
    let re = describe::ww2_list_item(&ww2_levels).map_err(err)?;
    describe::level_list_xml(x, &levels, &re, entry).map_err(err)
}

/// `edit_level(data, ours, report)` (spec: `levelfiles::LevelSpec` as JSON): (the level partition, the lines).
#[pyfunction]
pub fn ww2build_edit_level<'py>(
    py: Python<'py>,
    data: &[u8],
    ours: &Bound<'py, PyAny>,
    spec_json: &str,
) -> PyResult<(Bound<'py, PyBytes>, Vec<String>)> {
    let s: levelfiles::LevelSpec = spec(spec_json, "edit_level")?;
    let ours = ours_from(ours)?;
    let mut report = Vec::new();
    let out = py
        .detach(|| levelfiles::edit_level(data, &ours, &s, &mut report))
        .map_err(err)?;
    Ok((PyBytes::new(py, &out), report))
}

/// `remap_targets(index, ours)` (spec: `levelfiles::RemapSpec` as JSON): `{old: 'new/guid' or None}`.
#[pyfunction]
pub fn ww2build_remap_targets<'py>(
    py: Python<'py>,
    spec_json: &str,
    index: &Bound<'py, PyAny>,
    ours: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let s: levelfiles::RemapSpec = spec(spec_json, "remap_targets")?;
    let ix = index_of(index)?;
    let ours = ours_from(ours)?;
    let to = py.detach(|| levelfiles::remap_targets(&s, &ix, &ours)).map_err(err)?;
    let d = PyDict::new(py);
    for (k, v) in to {
        d.set_item(k, v)?;
    }
    Ok(d)
}

/// `patch_refs(data, to)`: (bytes, count) - `refs`: `REMAP_REFS`.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a dict argument as an owned HashMap"
)]
pub fn ww2build_patch_refs<'py>(
    py: Python<'py>,
    data: &[u8],
    to: HashMap<String, Option<String>>,
    refs: HashMap<String, Option<String>>,
) -> PyResult<(Bound<'py, PyBytes>, usize)> {
    let (b, n) = levelfiles::patch_refs(data, &to, &refs).map_err(err)?;
    Ok((PyBytes::new(py, &b), n))
}

/// `patch_ours(ours, index, report)` (spec: `levelfiles::RemapSpec`; `weapons`: `WEAPON_PERSISTENCE` in order):
/// (our files with the patched copies, the lines).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_patch_ours<'py>(
    py: Python<'py>,
    spec_json: &str,
    weapons: Vec<(String, Vec<String>)>,
    patched: &str,
    ours: &Bound<'py, PyAny>,
    index: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyDict>, Vec<String>)> {
    let s: levelfiles::RemapSpec = spec(spec_json, "patch_ours")?;
    let mut o = ours_from(ours)?;
    let ix = index_of(index)?;
    let mut report = Vec::new();
    py.detach(|| levelfiles::patch_ours(&s, &weapons, patched, &mut o, &ix, &mut report))
        .map_err(err)?;
    Ok((ours_to(py, &o)?, report))
}

/// `kit_voices(ours, report)`: (our files with the patched copies, the lines).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_kit_voices<'py>(
    py: Python<'py>,
    voices: Vec<(String, String)>,
    persons: HashSet<String>,
    patched: &str,
    ours: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyDict>, Vec<String>)> {
    let mut o = ours_from(ours)?;
    let mut report = Vec::new();
    py.detach(|| levelfiles::kit_voices(&voices, &persons, patched, &mut o, &mut report))
        .map_err(err)?;
    Ok((ours_to(py, &o)?, report))
}

/// `sky_patch(ours, report)` without the sky's Option B (spec: `levelfiles::SkySpec`).
#[pyfunction]
pub fn ww2build_sky_patch<'py>(
    py: Python<'py>,
    spec_json: &str,
    patched: &str,
    ours: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyDict>, Vec<String>)> {
    let s: levelfiles::SkySpec = spec(spec_json, "sky_patch")?;
    let mut o = ours_from(ours)?;
    let mut report = Vec::new();
    py.detach(|| levelfiles::sky_patch(&s, patched, &mut o, &mut report))
        .map_err(err)?;
    Ok((ours_to(py, &o)?, report))
}

/// `root_files(stage, report)` (spec: `levelfiles::RootSpec`): ({lower: (name, type, path)}, the lines).
#[pyfunction]
pub fn ww2build_root_files<'py>(
    py: Python<'py>,
    spec_json: &str,
    folder: &str,
) -> PyResult<(Bound<'py, PyDict>, Vec<String>)> {
    let s: levelfiles::RootSpec = spec(spec_json, "root_files")?;
    let mut report = Vec::new();
    let t = py
        .detach(|| levelfiles::root_files(&s, folder, &mut report))
        .map_err(err)?;
    Ok((take_to(py, &t)?, report))
}

fn put_back(py: Python<'_>, files: &Bound<'_, PyDict>, part: &OMap<Take>) -> PyResult<()> {
    for (k, t) in part.iter() {
        files.set_item(k, (&t.name, &t.kind, src_to(py, &t.src)?))?;
    }
    Ok(())
}

/// The partitions of a take (what `mesh_flags`, `ondemand_waves` and `emitter_atlas` read and change).
fn partitions(files: &Bound<'_, PyDict>) -> PyResult<OMap<Take>> {
    let d = PyDict::new(files.py());
    for (k, v) in files.iter() {
        if k.extract::<String>()?.ends_with(".dbx") {
            d.set_item(k, v)?;
        }
    }
    take_from(&d)
}

/// `mesh_flags(files, have, report)`: the take changed in place; the lines.
#[pyfunction]
pub fn ww2build_mesh_flags(
    py: Python<'_>,
    files: &Bound<'_, PyDict>,
    have: &Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    let mut part = partitions(files)?;
    let have = str_set(have)?;
    let mut report = Vec::new();
    py.detach(|| levelfiles::mesh_flags(&mut part, &have, &mut report))
        .map_err(err)?;
    put_back(py, files, &part)?;
    Ok(report)
}

/// `ondemand_waves(files, have, report)`: the take changed in place (the audio added); the lines.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_ondemand_waves(
    py: Python<'_>,
    files: &Bound<'_, PyDict>,
    have: &Bound<'_, PyAny>,
    bundles: Vec<String>,
) -> PyResult<Vec<String>> {
    let mut part = partitions(files)?;
    let have = str_set(have)?;
    let mut report = Vec::new();
    py.detach(|| levelfiles::ondemand_waves(&mut part, &have, &bundles, &mut report))
        .map_err(err)?;
    put_back(py, files, &part)?;
    Ok(report)
}

/// `emitter_atlas(take, report)`: the take changed in place; the lines. `manifest`: the build's `manifest.json`
/// as read (`None` when not built).
#[pyfunction]
pub fn ww2build_emitter_atlas(
    py: Python<'_>,
    files: &Bound<'_, PyDict>,
    folder: &str,
    manifest: Option<&Bound<'_, PyDict>>,
) -> PyResult<Vec<String>> {
    let m = match manifest {
        None => None,
        Some(m) => {
            let mut out = levelfiles::AtlasManifest::default();
            if let Some(e) = m.get_item("emitters")? {
                for (k, v) in e.cast::<PyDict>()?.iter() {
                    let v = v.cast::<PyDict>()?;
                    let sha: String = v.get_item("sha1_in")?.map_or(Ok(String::new()), |x| x.extract())?;
                    let file: String = v.get_item("file")?.map_or(Ok(String::new()), |x| x.extract())?;
                    out.emitters.insert(k.extract()?, (sha, file));
                }
            }
            if let Some(a) = m.get_item("atlases")? {
                for (k, v) in a.cast::<PyDict>()?.iter() {
                    let v = v.cast::<PyDict>()?;
                    let name: String = v.get_item("name")?.map_or(Ok(String::new()), |x| x.extract())?;
                    let file: String = v.get_item("file")?.map_or(Ok(String::new()), |x| x.extract())?;
                    let size: Vec<i64> = v.get_item("size")?.map_or(Ok(Vec::new()), |x| x.extract())?;
                    let wh = (size.first().copied().unwrap_or(0), size.get(1).copied().unwrap_or(0));
                    out.atlases.push((k.extract()?, name, file, wh));
                }
            }
            Some(out)
        }
    };
    // the partitions and the atlases (the only entries it reads or changes), in the take's order
    let atlases: HashSet<&str> = m
        .as_ref()
        .map(|m| m.atlases.iter().map(|a| a.0.as_str()).collect())
        .unwrap_or_default();
    let d = PyDict::new(py);
    for (k, v) in files.iter() {
        let low: String = k.extract()?;
        if low.ends_with(".dbx") || atlases.contains(low.as_str()) {
            d.set_item(k, v)?;
        }
    }
    let mut take = take_from(&d)?;
    let mut report = Vec::new();
    py.detach(|| levelfiles::emitter_atlas(&mut take, folder, m.as_ref(), &mut report))
        .map_err(err)?;
    put_back(py, files, &take)?;
    Ok(report)
}

/// `undergrowth_files(index, report)`: ({lower: (name, type, bytes)}, the lines).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_undergrowth_files<'py>(
    py: Python<'py>,
    standins: Vec<(String, String)>,
    index: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyDict>, Vec<String>)> {
    let ix = index_of(index)?;
    let mut report = Vec::new();
    let t = py
        .detach(|| levelfiles::undergrowth_files(&standins, &ix, &mut report))
        .map_err(err)?;
    Ok((take_to(py, &t)?, report))
}

/// `terrain_files(stage, src, report)` (spec: `terrainfiles::TerrainSpec` as JSON; `items`: (lower key, name, type,
/// bytes) of BF1943's terrain resources, sorted): ({lower: (name, type, path)}, the lines).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_terrain_files<'py>(
    py: Python<'py>,
    spec_json: &str,
    items: Vec<(String, String, String, Vec<u8>)>,
    index: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyDict>, Vec<String>)> {
    let s: terrainfiles::TerrainSpec = spec(spec_json, "terrain_files")?;
    let ix = index_of(index)?;
    let mut report = Vec::new();
    let t = py
        .detach(|| terrainfiles::terrain_files(&s, &items, &ix, &mut report))
        .map_err(err)?;
    Ok((take_to(py, &t)?, report))
}

/// `undergrowth_scales(pc, report)` (`standins`: `UG_STANDIN`; `strips_off`: `TERRAIN_STRIPS_OFF`): (the
/// VisualTerrain, the lines).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn ww2build_undergrowth_scales<'py>(
    py: Python<'py>,
    pc: &[u8],
    standins: Vec<(String, String)>,
    strips_off: Vec<String>,
    index: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyBytes>, Vec<String>)> {
    let ix = index_of(index)?;
    let mut report = Vec::new();
    let out = py
        .detach(|| terrainfiles::undergrowth_scales(pc, &standins, &strips_off, &ix, &mut report))
        .map_err(err)?;
    Ok((PyBytes::new(py, &out), report))
}

/// Forgets the kept undergrowth blocks (a test's other stock index).
#[pyfunction]
pub fn ww2build_forget_undergrowth() {
    terrainfiles::forget_blocks();
}
