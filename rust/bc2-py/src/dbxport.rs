//! `bfbc2-bf1943-import/dbxport.py`'s port in Rust (round E4d, `bc2_convert::dbxport`): `DbxTemplates` (BFBC2's
//! template partitions, parsed and kept here), `DbxCatalogue` (`dbxcatalogue.Catalogue`'s tables and elements,
//! copied once), `dbxport_port` (one port) and `dbx_instance_guids` (`dbxport.guids_of` without a Python tree).
//!
//! `dbxport.port` calls these when the module is there and its arguments are ones the port takes; a result
//! `('unsupported', why)` sends it to its Python reference.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError, RwLock};

use bc2_convert::dbxport::rules::Rules;
use bc2_convert::dbxport::{
    self, Arena, Bones, CatView, Catalogue, Fail, Id, Node, Options, Ours, SiKey, SiVal, Templates,
};
use bc2_formats::dbx;
use pyo3::exceptions::PyValueError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods, PyDict, PyFrozenSet, PyList, PySet, PyString, PyTuple};
use rayon::prelude::*;

#[expect(clippy::needless_pass_by_value, reason = "map_err passes the error by value")]
pub(crate) fn poisoned<T>(e: PoisonError<T>) -> PyErr {
    PyValueError::new_err(format!("dbxport: a lock poisoned ({e})"))
}

/// `dbxport.py`'s data tables as the caller's process has them (`ONE_TO_ARRAY`, `RENAMED`, `NESTED`,
/// `ENUM_NUMBERS`, `NUMBERS_TO_ITEMS`, `TYPE_AS`, `DICE_MOVED_CLASSES`, `HINT_ACTIONS`, `HINT_TEXTS`): the port reads
/// these, not built-in copies (a caller changes them: Project OPFOR's planes' gear input).
#[pyclass(frozen)]
pub struct DbxRules {
    pub(crate) rules: Rules,
}

/// A table keyed by (type, field), as rows.
type FieldRows<V> = Vec<(String, String, V)>;

fn by_field<V>(rows: FieldRows<V>) -> dbxport::rules::ByField<V> {
    let mut m: dbxport::rules::ByField<V> = HashMap::new();
    for (t, n, v) in rows {
        m.entry(t).or_default().insert(n, v);
    }
    m
}

#[pymethods]
impl DbxRules {
    /// The tables, from `dbxport._rules_args()` (keyed tables as `(type, field, value)` rows; `NUMBERS_TO_ITEMS`'
    /// others in their order).
    #[new]
    #[expect(clippy::too_many_arguments, reason = "one argument per table")]
    fn new(
        one_to_array: HashMap<String, String>,
        renamed: HashMap<String, String>,
        nested: FieldRows<Vec<String>>,
        enum_numbers: FieldRows<Vec<(i64, String)>>,
        numbers_to_items: FieldRows<(String, Vec<(String, i64)>)>,
        type_as: HashMap<String, String>,
        dice_moved: HashSet<String>,
        hint_actions: HashMap<String, String>,
        hint_texts: HashMap<String, String>,
    ) -> Self {
        let enum_numbers = enum_numbers
            .into_iter()
            .map(|(t, n, m)| (t, n, m.into_iter().map(|(k, v)| (i128::from(k), v)).collect()))
            .collect();
        let numbers_to_items = numbers_to_items
            .into_iter()
            .map(|(t, n, (key, others))| {
                (
                    t,
                    n,
                    (key, others.into_iter().map(|(f, v)| (f, i128::from(v))).collect()),
                )
            })
            .collect();
        Self {
            rules: Rules {
                one_to_array,
                renamed,
                nested: by_field(nested),
                enum_numbers: by_field(enum_numbers),
                numbers_to_items: by_field(numbers_to_items),
                type_as,
                dice_moved,
                hint_actions,
                hint_texts,
            },
        }
    }

    /// Whether these are `dbxport.py`'s own tables, as it defines them.
    fn is_default(&self) -> bool {
        self.rules == Rules::default()
    }
}

/// BFBC2's template partitions (`dbxport.Templates`' instances), kept on the Rust side.
#[pyclass(frozen)]
pub struct DbxTemplates {
    inner: RwLock<Templates>,
}

#[pymethods]
impl DbxTemplates {
    #[new]
    fn new() -> Self {
        Self {
            inner: RwLock::new(Templates::new()),
        }
    }

    /// Adds `(key, data)` partitions in order (parsed on all cores). Each item's answer: `None` (added), or the
    /// parse error's message (not added), or `('unsupported', why)` (a partition the port cannot hold: the caller
    /// stops using this object).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "PyO3 extracts a list argument as an owned Vec"
    )]
    fn add_many<'py>(
        &self,
        py: Python<'py>,
        items: Vec<(String, Bound<'py, PyBytes>)>,
        rules: &DbxRules,
    ) -> PyResult<Bound<'py, PyList>> {
        let slices: Vec<&[u8]> = items.iter().map(|(_, b)| b.as_bytes()).collect();
        let parsed: Vec<Result<Arena, String>> = py.detach(|| {
            slices
                .par_iter()
                .map(|d| {
                    dbx::parse_tree(d)
                        .map(|t| Arena::from_tree(&t))
                        .map_err(|e| e.to_string())
                })
                .collect()
        });
        let out = PyList::empty(py);
        let mut t = self.inner.write().map_err(poisoned)?;
        for ((key, _), r) in items.iter().zip(parsed) {
            match r {
                Ok(arena) => match t.add(key.clone(), arena, &rules.rules) {
                    Ok(()) => out.append(py.None())?,
                    Err(e) => out.append(("unsupported", e.to_string()))?,
                },
                Err(msg) => out.append(msg)?,
            }
        }
        Ok(out)
    }

    /// The instance types the partitions hold.
    fn types(&self) -> PyResult<Vec<String>> {
        Ok(self.inner.read().map_err(poisoned)?.cands.keys().cloned().collect())
    }

    /// Whether an instance of type `t` is among them (`t in templates`).
    fn has(&self, t: &str) -> PyResult<bool> {
        Ok(self.inner.read().map_err(poisoned)?.has(t))
    }

    fn __len__(&self) -> PyResult<usize> {
        Ok(self.inner.read().map_err(poisoned)?.parts.len())
    }
}

/// A Python `dbx.Element` tree copied into `ar`.
fn element(ar: &mut Arena, e: &Bound<'_, PyAny>) -> PyResult<Id> {
    let py = e.py();
    let tag: String = e.getattr(intern!(py, "tag"))?.extract()?;
    let kind: u8 = e.getattr(intern!(py, "kind"))?.extract()?;
    let attrs: Vec<(String, String)> = e.getattr(intern!(py, "attributes"))?.extract()?;
    let text: String = e.getattr(intern!(py, "text"))?.extract()?;
    let raw: Vec<u8> = e.getattr(intern!(py, "raw"))?.extract()?;
    let kids = e.getattr(intern!(py, "children"))?;
    let mut children = Vec::new();
    if !kids.is_none() {
        for k in kids.try_iter()? {
            children.push(element(ar, &k?)?);
        }
    }
    let tag = ar.intern(&tag);
    let attrs = attrs.iter().map(|(k, v)| (ar.intern(k), ar.intern(v))).collect();
    let text = ar.intern(&text);
    Ok(ar.push(Node {
        tag,
        kind,
        attrs,
        text,
        raw,
        children,
    }))
}

/// `dbxcatalogue.Catalogue`'s tables and elements on the Rust side. Elements are known by their Python object
/// (kept alive here, so an id is never another object's); a class default the port meets that is not known yet is
/// copied in then.
#[pyclass(frozen)]
pub struct DbxCatalogue {
    pub(crate) tables: Catalogue,
    pub(crate) els: RwLock<Arena>,
    ids: Mutex<HashMap<usize, Id>>,
    keep: Mutex<Vec<Py<PyAny>>>,
}

impl DbxCatalogue {
    /// The node of a Python element, copied in the first time.
    fn node(&self, e: &Bound<'_, PyAny>) -> PyResult<Id> {
        let key = e.as_ptr() as usize;
        if let Some(&id) = self.ids.lock().map_err(poisoned)?.get(&key) {
            return Ok(id);
        }
        let id = {
            let mut els = self.els.write().map_err(poisoned)?;
            element(&mut els, e)?
        };
        self.ids.lock().map_err(poisoned)?.insert(key, id);
        self.keep.lock().map_err(poisoned)?.push(e.clone().unbind());
        Ok(id)
    }

    /// `{type: (partition, node)}` of a `defaults` dict, for the types asked (the only ones a port can look up:
    /// its instances' types, as they are and as `TYPE_AS` writes them).
    pub(crate) fn defaults(
        &self,
        d: &Bound<'_, PyDict>,
        types: &HashSet<String>,
    ) -> PyResult<HashMap<String, (String, Id)>> {
        let mut out = HashMap::with_capacity(types.len());
        #[allow(clippy::iter_over_hash_type, reason = "collected into a map keyed by type")]
        for t in types {
            let Some(v) = d.get_item(t)? else { continue };
            let (p, e): (String, Bound<'_, PyAny>) = v.extract()?;
            out.insert(t.clone(), (p, self.node(&e)?));
        }
        Ok(out)
    }
}

fn path_map<V>(m: &mut catalogue_paths::Map<V>, t: String, p: String, v: V) {
    m.entry(t).or_default().insert(p, v);
}

mod catalogue_paths {
    /// Per type, per path.
    pub type Map<V> = std::collections::HashMap<String, std::collections::HashMap<String, V>>;
}

#[pymethods]
impl DbxCatalogue {
    /// The tables of a `Catalogue` (`classes`, `exe_names`, `order`, `shapes`, `texts`, `numeric`, `protos`, `guids`,
    /// `primary`) and the elements of its `defaults`, `others` and `protos`.
    #[new]
    fn new(cat: &Bound<'_, PyAny>) -> PyResult<Self> {
        let py = cat.py();
        let get = |n: &str| cat.getattr(PyString::new(py, n));
        let mut t = Catalogue {
            classes: get("classes")?.extract()?,
            exe_names: get("exe_names")?.extract()?,
            order: get("order")?.extract()?,
            primary: get("primary")?.extract()?,
            ..Catalogue::default()
        };
        let pair = |k: &Bound<'_, PyAny>| k.extract::<(String, String)>();
        for (k, v) in get("shapes")?.cast::<PyDict>()?.iter() {
            let (ty, p) = pair(&k)?;
            let keys: Vec<String> = v.cast::<PyDict>()?.keys().extract()?;
            path_map(&mut t.shapes, ty, p, keys);
        }
        let texts = get("texts")?;
        for (k, v) in texts.cast::<PyDict>()?.iter() {
            let (ty, p) = pair(&k)?;
            let keys: Vec<String> = v.cast::<PyDict>()?.keys().extract()?;
            t.set_texts(&ty, &p, &keys);
        }
        for (k, v) in get("numeric")?.cast::<PyDict>()?.iter() {
            let (ty, p) = pair(&k)?;
            let c = v.cast::<PyDict>()?;
            let count = |n: &str| -> PyResult<i64> {
                Ok(match c.get_item(n)? {
                    Some(x) => x.extract()?,
                    None => 0,
                })
            };
            let entry = (!c.is_empty(), count("whole")?, count("float")?);
            path_map(&mut t.numeric, ty, p, entry);
        }
        let mut parts: HashMap<String, u32> = HashMap::new();
        let mut intern_part = |p: String, t: &mut Catalogue| -> u32 {
            *parts.entry(p).or_insert_with_key(|k| {
                t.partitions.push(k.clone());
                (t.partitions.len() - 1) as u32
            })
        };
        let guids = get("guids")?;
        let guids = guids.cast::<PyDict>()?;
        t.guids.reserve(guids.len());
        for (g, w) in guids.iter() {
            let g: String = g.extract()?;
            let list: Vec<u32> = if let Ok(s) = w.extract::<String>() {
                vec![intern_part(s, &mut t)]
            } else {
                let ps: Vec<String> = w.extract()?;
                ps.into_iter().map(|p| intern_part(p, &mut t)).collect()
            };
            t.guids.insert(g, list);
        }
        let me = Self {
            tables: Catalogue::default(),
            els: RwLock::new(Arena::new()),
            ids: Mutex::new(HashMap::new()),
            keep: Mutex::new(Vec::new()),
        };
        for (k, v) in get("protos")?.cast::<PyDict>()?.iter() {
            let (ty, p) = pair(&k)?;
            let id = me.node(&v)?;
            path_map(&mut t.protos, ty, p, id);
        }
        // the class defaults are copied when a port first asks for one (few types fall back to them)
        Ok(Self { tables: t, ..me })
    }

    /// How many elements are known.
    fn elements(&self) -> PyResult<usize> {
        Ok(self.ids.lock().map_err(poisoned)?.len())
    }
}

/// `ours` as the port takes it; `None` when a value is not a set, list, tuple or dict of guids (the reference's
/// `in` would mean something else: left to it).
fn ours_of(d: &Bound<'_, PyDict>) -> PyResult<Option<Ours>> {
    let mut out = Ours::with_capacity(d.len());
    for (k, v) in d.iter() {
        let Ok(k) = k.extract::<String>() else { continue };
        let ok = v.is_instance_of::<PySet>()
            || v.is_instance_of::<PyFrozenSet>()
            || v.is_instance_of::<PyList>()
            || v.is_instance_of::<PyTuple>()
            || v.is_instance_of::<PyDict>();
        if !ok {
            return Ok(None);
        }
        let mut set = HashSet::new();
        for g in v.try_iter()? {
            if let Ok(g) = g?.extract::<String>() {
                set.insert(g);
            }
        }
        out.insert(k, set);
    }
    Ok(Some(out))
}

fn stand_ins_of(d: &Bound<'_, PyDict>, template: &Bound<'_, PyAny>) -> HashMap<SiKey, SiVal> {
    let mut out = HashMap::with_capacity(d.len());
    for (k, v) in d.iter() {
        let key = if let Ok(s) = k.extract::<String>() {
            SiKey::Target(s)
        } else if let Ok((f, t)) = k.extract::<(String, String)>() {
            if !k.is_instance_of::<PyTuple>() {
                continue;
            }
            SiKey::Field(f, t)
        } else {
            continue;
        };
        let val = if v.is_none() {
            SiVal::Null
        } else if v.is(template) {
            SiVal::Template
        } else if let Ok(s) = v.extract::<String>() {
            SiVal::Ref(s)
        } else {
            SiVal::Other
        };
        out.insert(key, val);
    }
    out
}

/// `BoneTables` as the port takes it: (source names, target count, `at`, skeleton).
type BoneArgs = (Vec<String>, usize, HashMap<String, i64>, String);

fn record_tuple<'py>(py: Python<'py>, r: &dbxport::Record) -> PyResult<Bound<'py, PyTuple>> {
    (
        &r.path,
        r.action,
        r.old.as_deref(),
        r.new.as_deref(),
        r.source.as_deref(),
        r.step.as_deref(),
        r.note.as_deref(),
    )
        .into_pyobject(py)
}

/// `dbxport.port` in Rust. The answer: `('ok', bytes, [(path, action, old, new, source, step, note), ...])`,
/// `('parse', message)` (the partition does not parse: `ValueError`), `('port', message)` (`PortError`),
/// `('schema', [problems])` (`SchemaError`) or `('unsupported', why)` (port in Python). An exception `resolve`
/// raised is raised.
#[pyfunction]
#[pyo3(signature = (data, templates, rules, catalogue, defaults, resolve, ours, stand_ins, template_marker, bones,
                    drop_untemplated, drop_types, prune, required, donors, strict, schema_error, conventions, name))]
#[expect(
    clippy::too_many_arguments,
    clippy::fn_params_excessive_bools,
    clippy::needless_pass_by_value,
    reason = "dbxport.port's arguments, as PyO3 extracts them"
)]
pub fn dbxport_port<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    templates: &DbxTemplates,
    rules: &DbxRules,
    catalogue: Option<&DbxCatalogue>,
    defaults: Option<&Bound<'py, PyDict>>,
    resolve: Option<Py<PyAny>>,
    ours: Option<&Bound<'py, PyDict>>,
    stand_ins: Option<&Bound<'py, PyDict>>,
    template_marker: &Bound<'py, PyAny>,
    bones: Option<BoneArgs>,
    drop_untemplated: bool,
    drop_types: Vec<String>,
    prune: bool,
    required: HashMap<String, String>,
    donors: Vec<String>,
    strict: bool,
    schema_error: bool,
    conventions: bool,
    name: String,
) -> PyResult<Bound<'py, PyTuple>> {
    let unsupported = |why: &str| ("unsupported", why).into_pyobject(py);
    let ours = match ours {
        Some(d) => match ours_of(d)? {
            Some(o) => Some(o),
            None => return unsupported("ours: a value that is no set of guids"),
        },
        None => None,
    };
    let stand_ins = match stand_ins {
        Some(d) => stand_ins_of(d, template_marker),
        None => HashMap::new(),
    };
    let bones = bones.map(|(source, target_len, at, skeleton)| Bones {
        source,
        target_len,
        at: at.into_iter().map(|(k, v)| (k, i128::from(v))).collect(),
        skeleton,
    });
    let defaults = match (catalogue, defaults) {
        (Some(c), Some(d)) => {
            // the instances' types (a file that does not parse: the port says so, no default is asked)
            let mut types = HashSet::new();
            if let Ok(tree) = dbx::parse_tree(data.as_bytes()) {
                for t in tree.instances().map(|n| n.attr("type").unwrap_or("")) {
                    types.insert(t.to_owned());
                    if let Some(a) = rules.rules.type_as(t) {
                        types.insert(a.to_owned());
                    }
                }
            }
            c.defaults(d, &types)?
        }
        _ => HashMap::new(),
    };
    let els_guard = catalogue.map(|c| c.els.read()).transpose().map_err(poisoned)?;
    let view = match (catalogue, &els_guard) {
        (Some(c), Some(els)) => Some(CatView {
            cat: &c.tables,
            els,
            defaults: &defaults,
        }),
        _ => None,
    };
    let raised: Mutex<Option<PyErr>> = Mutex::new(None);
    let call = |p: &str| -> Result<bool, Fail> {
        let Some(f) = &resolve else { return Ok(false) };
        Python::attach(|py| {
            let r = f.bind(py).call1((p,)).and_then(|v| v.is_truthy());
            r.map_err(|e| {
                if let Ok(mut slot) = raised.lock() {
                    *slot = Some(e);
                }
                Fail::Callback
            })
        })
    };
    let options = Options {
        drop_untemplated,
        drop_types: drop_types.into_iter().collect(),
        prune,
        required,
        catalogue: view,
        ours: ours.as_ref(),
        stand_ins: &stand_ins,
        donors,
        strict,
        schema_error,
        conventions,
        bones: bones.as_ref(),
        name,
        rules: &rules.rules,
        resolve: resolve
            .as_ref()
            .map(|_| &call as &(dyn Fn(&str) -> Result<bool, Fail> + Sync)),
    };
    let t = templates.inner.read().map_err(poisoned)?;
    let result = dbxport::port(data.as_bytes(), &t, &options);
    match result {
        Ok(p) => {
            let recs = PyList::empty(py);
            for r in &p.records {
                recs.append(record_tuple(py, r)?)?;
            }
            ("ok", PyBytes::new(py, &p.data), recs).into_pyobject(py)
        }
        Err(Fail::Parse(m)) => ("parse", m).into_pyobject(py),
        Err(Fail::Port(m)) => ("port", m).into_pyobject(py),
        Err(Fail::Schema(p)) => ("schema", p).into_pyobject(py),
        Err(Fail::Unsupported(m)) => ("unsupported", m).into_pyobject(py),
        Err(Fail::Callback) => Err(raised
            .lock()
            .ok()
            .and_then(|mut s| s.take())
            .unwrap_or_else(|| PyValueError::new_err("dbxport: resolve raised"))),
    }
}

/// Each instance's `guid` attribute (`None` where it has none), the partition's `instance` children in order -
/// `[i.attr('guid') for i in dbx.parse(data).instances]` without the Python tree; `None` for a file without a
/// partition holding children (the reference's tree then raises: its caller asks it). `ValueError` when the file
/// does not parse.
#[pyfunction]
pub fn dbx_instance_guids(py: Python<'_>, data: &Bound<'_, PyBytes>) -> PyResult<Option<Vec<Option<String>>>> {
    let bytes = data.as_bytes();
    let got = py.detach(|| -> Result<Option<Vec<Option<String>>>, dbx::DbxError> {
        let tree = dbx::parse_tree(bytes)?;
        let Some(p) = tree.partition() else { return Ok(None) };
        if !p.is_container() {
            return Ok(None);
        }
        Ok(Some(
            p.children()
                .filter(|n| n.tag() == "instance")
                .map(|n| n.attr("guid").map(str::to_owned))
                .collect(),
        ))
    });
    got.map_err(|e| PyValueError::new_err(e.to_string()))
}
