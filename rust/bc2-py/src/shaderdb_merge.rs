//! `bc2_native.ShaderMerger` (round OR3a): the shader databases' merge (`bc2_formats::shaderdb::merge::Merger`) on
//! `shaderdb.ShaderDb` objects, in place of the Python `shaderdb.Merger` it retired. `shaderdb.merger(base)` makes
//! one; Project OPFOR's WW2 tools clone entries, register mesh parts and add donors' entries with it.
//!
//! It keeps the Python merger's contract with the base: the base is changed in place, and between calls it holds
//! what the Python merger left there - every new shader, table, vertex layout, fragment, entry and mesh-part name
//! appended to the base's lists as it is added, the programs, keys and entry groups still the base's until
//! `finish()` re-sorts the programs and renumbers the groups. A clone or part whose donor is the base itself reads
//! the base as merged so far, as the Python did. The base must not be changed by anything else while the merger is
//! open (the merger works on the copy it read when it was made).
//!
//! A donor is read from its bytes (`to_bytes()`, parsed in Rust); the last few read are kept (`DONORS`), each known
//! by its object and a stamp of its sections (the lists and their items, by identity: a donor changed between calls
//! is read again), never keeping the object alive. What the merge takes from a donor is copied into the merger's
//! arena, beside its copy of the base; `finish()` lets them go. Errors are `shaderdb.ShaderDbError` with the Python merger's messages; a
//! database whose indices point outside its sections is an `IndexError`, as the Python raised.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Mutex;

use bc2_formats::shaderdb::merge::{Kind, MergeError, Merger, Store};
use bc2_formats::shaderdb::{self, Named, Shader, ShaderDb};
use bumpalo::Bump;
use pyo3::exceptions::{PyIndexError, PyRuntimeError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySet, PyTuple, PyWeakrefReference};

use crate::shaders::latin1;

/// The merger's arena as the merge's store.
struct Arena<'a>(&'a Bump);

impl<'a> Store<'a> for Arena<'a> {
    fn keep(&self, b: &[u8]) -> &'a [u8] {
        self.0.alloc_slice_copy(b)
    }
}

/// The merge, borrowing the arena (its copy of the base, and every record it took or made).
struct State<'a> {
    m: Merger<'a>,
}

self_cell::self_cell!(
    struct Cell {
        owner: Bump,
        #[covariant]
        dependent: State,
    }
);

self_cell::self_cell!(
    struct DonorDb {
        owner: Vec<u8>,
        #[covariant]
        dependent: ShaderDb,
    }
);

/// A donor read: the object (weakly), its id and stamp, its kind and its database.
struct Donor {
    id: usize,
    stamp: u64,
    weak: Option<Py<PyWeakrefReference>>,
    kind: Kind,
    db: DonorDb,
}

/// How many donors a merger keeps read (OPFOR's merges take one donor level at a time, or alternate a few).
const DONORS: usize = 4;

/// The lengths of the sections the merge appends to, and of its report lists.
#[derive(Clone, Copy)]
struct Marks {
    sections: [usize; 9],
    added: usize,
    report: usize,
    conflicts: usize,
    part_conflicts: usize,
}

impl Marks {
    fn of(m: &Merger<'_>) -> Self {
        let o = m.current();
        Self {
            sections: [
                o.params.len(),
                o.bindings.len(),
                o.regs.len(),
                o.vs.len(),
                o.ps.len(),
                o.layouts.len(),
                o.entries.len(),
                o.fragments.len(),
                o.parts.len(),
            ],
            added: m.added.len(),
            report: m.report.len(),
            conflicts: m.conflicts.len(),
            part_conflicts: m.part_conflicts.len(),
        }
    }
}

/// The section attributes the merge appends to, in `Marks::sections`' order.
const APPENDED: [&str; 9] = [
    "params",
    "bindings",
    "regs",
    "vs",
    "ps",
    "layouts",
    "entries",
    "fragments",
    "parts",
];

/// Every section attribute of a `ShaderDb`.
const SECTIONS: [&str; 12] = [
    "params",
    "bindings",
    "regs",
    "vs",
    "ps",
    "programs",
    "keys",
    "layouts",
    "entries",
    "fragments",
    "parts",
    "groups",
];

/// The shader merge on `shaderdb.ShaderDb` objects (the module's notes). `ShaderMerger(base, need=(),
/// all_names=False)`: `need` the lower-case mesh paths whose entries `add_donor` may add; `all_names` also for
/// meshes the base already has entries for.
#[pyclass(name = "ShaderMerger", module = "bc2_native")]
pub struct ShaderMerger {
    cell: Mutex<Option<Cell>>,
    base: Py<PyAny>,
    shader_cls: Py<PyAny>,
    error_cls: Py<PyAny>,
    /// The donors read last, the most recent last.
    donors: Mutex<Vec<Donor>>,
    shared: Mutex<usize>,
    /// The base's programs no entry group owned, left out by `finish` (SHDB-MRG-1).
    unowned: Mutex<usize>,
    /// The entry names of the merged file (`Merger.entry_names`).
    #[pyo3(get)]
    entry_names: Py<PySet>,
    /// The mesh-part names of the merged file and their vertex layouts (`Merger.part_names`).
    #[pyo3(get)]
    part_names: Py<PyDict>,
    /// The entries added and where each came from: `[(name, label)]` (a clone's label is `<label>:<entry>`).
    #[pyo3(get)]
    added: Py<PyList>,
    /// Per donor added: its entries and mesh-part names taken.
    #[pyo3(get)]
    report: Py<PyList>,
    /// Donor programs whose GUID the file has with other content (the existing one kept).
    #[pyo3(get)]
    conflicts: Py<PyList>,
    /// Mesh-part names a donor has with another vertex layout (the existing one kept).
    #[pyo3(get)]
    part_conflicts: Py<PyList>,
}

/// A Python object's kind as the merge compares it (`dx`, `big_endian`, `key_size`, `version`).
fn kind_of(db: &Bound<'_, PyAny>) -> PyResult<Kind> {
    Ok(Kind {
        dx: db.getattr("dx")?.extract()?,
        big: db.getattr("big_endian")?.extract()?,
        key_size: db.getattr("key_size")?.extract()?,
        version: db.getattr("version")?.extract()?,
    })
}

/// A `ShaderDb`'s bytes (`to_bytes()`).
fn db_bytes(db: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    Ok(db.call_method0("to_bytes")?.cast_into::<PyBytes>()?.as_bytes().to_vec())
}

/// A stamp of what a `ShaderDb` holds, without making a section: each section list in its `__dict__` (the list, its
/// length and its items, by identity) or the Rust handle it is still read from, and the kind's attributes.
fn stamp(db: &Bound<'_, PyAny>) -> PyResult<u64> {
    let mut h = DefaultHasher::new();
    let d = db.getattr("__dict__")?;
    for s in SECTIONS {
        match d.get_item(s) {
            Ok(list) => {
                list.as_ptr().addr().hash(&mut h);
                if let Ok(l) = list.cast::<PyList>() {
                    l.len().hash(&mut h);
                    for x in l.iter() {
                        x.as_ptr().addr().hash(&mut h);
                    }
                } else {
                    for x in list.try_iter()? {
                        x?.as_ptr().addr().hash(&mut h);
                    }
                }
            }
            Err(_) => 0usize.hash(&mut h),
        }
    }
    d.get_item("_h").map_or(0, |x| x.as_ptr().addr()).hash(&mut h);
    for a in ["dx", "version", "flag", "key_size"] {
        db.getattr(a)?.extract::<i64>()?.hash(&mut h);
    }
    db.getattr("big_endian")?.extract::<bool>()?.hash(&mut h);
    Ok(h.finish())
}

impl ShaderMerger {
    fn err(&self, py: Python<'_>, e: &MergeError) -> PyErr {
        match e {
            MergeError::Msg(m) => match self.error_cls.bind(py).call1((m.as_str(),)) {
                Ok(x) => PyErr::from_value(x),
                Err(e) => e,
            },
            MergeError::Malformed => PyIndexError::new_err(e.to_string()),
        }
    }

    /// Appends what the merge added since `before` to the base's lists and to the merger's own.
    fn sync(&self, py: Python<'_>, m: &Merger<'_>, before: Marks) -> PyResult<()> {
        let o = m.current();
        let base = self.base.bind(py);
        let cls = self.shader_cls.bind(py);
        let lists: Vec<Bound<'_, PyAny>> = APPENDED.iter().map(|s| base.getattr(*s)).collect::<PyResult<_>>()?;
        let recs = |l: &Bound<'_, PyAny>, items: &[&[u8]]| -> PyResult<()> {
            for r in items {
                l.call_method1("append", (PyBytes::new(py, r),))?;
            }
            Ok(())
        };
        let shaders = |l: &Bound<'_, PyAny>, items: &[Shader<'_>]| -> PyResult<()> {
            for s in items {
                let made = cls.call1((
                    PyBytes::new(py, s.guid),
                    PyBytes::new(py, s.code),
                    s.param,
                    s.binding,
                    s.regs,
                    PyBytes::new(py, s.tail),
                ))?;
                l.call_method1("append", (made,))?;
            }
            Ok(())
        };
        let names = |l: &Bound<'_, PyAny>, items: &[Named<'_>]| -> PyResult<()> {
            for n in items {
                l.call_method1("append", ((latin1(py, &n.name), PyBytes::new(py, n.guid)),))?;
            }
            Ok(())
        };
        let from = |i: usize| before.sections.get(i).copied().unwrap_or(0);
        if let [params, bindings, regs, vs, ps, layouts, entries, fragments, parts] = lists.as_slice() {
            recs(params, o.params.get(from(0)..).unwrap_or_default())?;
            recs(bindings, o.bindings.get(from(1)..).unwrap_or_default())?;
            recs(regs, o.regs.get(from(2)..).unwrap_or_default())?;
            shaders(vs, o.vs.get(from(3)..).unwrap_or_default())?;
            shaders(ps, o.ps.get(from(4)..).unwrap_or_default())?;
            recs(layouts, o.layouts.get(from(5)..).unwrap_or_default())?;
            let new_entries = o.entries.get(from(6)..).unwrap_or_default();
            names(entries, new_entries)?;
            names(fragments, o.fragments.get(from(7)..).unwrap_or_default())?;
            let new_parts = o.parts.get(from(8)..).unwrap_or_default();
            names(parts, new_parts)?;
            let en = self.entry_names.bind(py);
            for e in new_entries {
                en.add(latin1(py, &e.name))?;
            }
            let pn = self.part_names.bind(py);
            for p in new_parts {
                pn.set_item(latin1(py, &p.name), PyBytes::new(py, p.guid))?;
            }
        }
        let added = self.added.bind(py);
        for (n, label) in m.added.get(before.added..).unwrap_or_default() {
            added.append((latin1(py, n), label.as_str()))?;
        }
        for (list, items, at) in [
            (&self.report, &m.report, before.report),
            (&self.conflicts, &m.conflicts, before.conflicts),
            (&self.part_conflicts, &m.part_conflicts, before.part_conflicts),
        ] {
            let l = list.bind(py);
            for s in items.get(at..).unwrap_or_default() {
                l.append(s.as_str())?;
            }
        }
        *self
            .shared
            .lock()
            .map_err(|_| PyRuntimeError::new_err("the merger's lock is poisoned"))? = m.shared_programs;
        Ok(())
    }

    /// The donor read (from the kept ones when it is unchanged), its place in `donors`.
    fn donor(&self, py: Python<'_>, donors: &mut Vec<Donor>, d: &Bound<'_, PyAny>, label: &str) -> PyResult<usize> {
        let id = d.as_ptr().addr();
        let st = stamp(d)?;
        let hit = donors.iter().position(|x| {
            x.id == id
                && x.stamp == st
                && x.weak
                    .as_ref()
                    .and_then(|w| w.bind(py).upgrade())
                    .is_some_and(|o| o.is(d))
        });
        if let Some(i) = hit {
            let x = donors.remove(i);
            donors.push(x);
            return Ok(donors.len() - 1);
        }
        let kind = kind_of(d)?;
        let data = db_bytes(d)?;
        let db = DonorDb::try_new(data, |data| shaderdb::parse(data, kind.dx, kind.big))
            .map_err(|e| self.err(py, &MergeError::Msg(format!("{label}: {e}"))))?;
        donors.retain(|x| x.id != id);
        if donors.len() >= DONORS {
            donors.remove(0);
        }
        donors.push(Donor {
            id,
            stamp: st,
            weak: PyWeakrefReference::new(d).ok().map(Bound::unbind),
            kind,
            db,
        });
        Ok(donors.len() - 1)
    }

    /// Runs `op` on the open merge with the donor read (None: the base itself), then appends what it added.
    fn run<T>(
        &self,
        py: Python<'_>,
        donor: Option<&Bound<'_, PyAny>>,
        label: &str,
        op: impl for<'a, 'd> FnOnce(
            &mut Merger<'a>,
            Option<(&ShaderDb<'d>, &Kind)>,
            &dyn Store<'a>,
        ) -> Result<T, MergeError>,
    ) -> PyResult<T> {
        let mut guard = self.cell.lock().map_err(|_| poisoned())?;
        let Some(cell) = guard.as_mut() else {
            return Err(self.err(
                py,
                &MergeError::Msg("the merge has finished: make a new merger for more".into()),
            ));
        };
        let mut donors = self.donors.lock().map_err(|_| poisoned())?;
        let at = match donor.filter(|d| !d.is(self.base.bind(py))) {
            Some(d) => Some(self.donor(py, &mut donors, d, label)?),
            None => None,
        };
        let mut out: Option<PyResult<T>> = None;
        cell.with_dependent_mut(|bump, st| {
            let before = Marks::of(&st.m);
            let store = Arena(bump);
            let got = match at.and_then(|i| donors.get(i)) {
                Some(x) => x.db.with_dependent(|_, db| op(&mut st.m, Some((db, &x.kind)), &store)),
                None if at.is_some() => Err(MergeError::Malformed),
                None => op(&mut st.m, None, &store),
            };
            let synced = self.sync(py, &st.m, before);
            out = Some(match (got, synced) {
                (Ok(v), Ok(())) => Ok(v),
                (Err(e), _) => Err(self.err(py, &e)),
                (Ok(_), Err(e)) => Err(e),
            });
        });
        out.unwrap_or_else(|| Err(PyRuntimeError::new_err("the merge did not run")))
    }
}

fn poisoned() -> PyErr {
    PyRuntimeError::new_err("the merger's lock is poisoned")
}

#[pymethods]
impl ShaderMerger {
    #[new]
    #[pyo3(signature = (base, need = None, all_names = false))]
    fn new(
        py: Python<'_>,
        base: &Bound<'_, PyAny>,
        need: Option<&Bound<'_, PyAny>>,
        all_names: bool,
    ) -> PyResult<Self> {
        let module = py.import(base.get_type().getattr("__module__")?.extract::<String>()?)?;
        let shader_cls = module.getattr("Shader")?;
        let error_cls = module.getattr("ShaderDbError")?;
        let need: Vec<String> = match need {
            Some(n) => n.try_iter()?.map(|x| x?.extract()).collect::<PyResult<_>>()?,
            None => Vec::new(),
        };
        let kind = kind_of(base)?;
        for s in SECTIONS {
            base.getattr(s)?; // every section made: the merge appends to them (the Python merger read them all)
        }
        let data = db_bytes(base)?;
        let entry_names = PySet::empty(py)?;
        for e in base.getattr("entries")?.try_iter()? {
            entry_names.add(e?.get_item(0)?)?;
        }
        let part_names = PyDict::new(py);
        for p in base.getattr("parts")?.try_iter()? {
            let p = p?;
            part_names.set_item(p.get_item(0)?, p.get_item(1)?)?;
        }
        let made = Cell::try_new(Bump::new(), |bump| -> Result<State<'_>, MergeError> {
            let data: &[u8] = bump.alloc_slice_copy(&data);
            let db = shaderdb::parse(data, kind.dx, kind.big).map_err(|e| MergeError::Msg(format!("the base: {e}")))?;
            Ok(State {
                m: Merger::new(db, kind, need, all_names)?,
            })
        });
        let error = error_cls.clone().unbind();
        let me = Self {
            cell: Mutex::new(None),
            base: base.clone().unbind(),
            shader_cls: shader_cls.unbind(),
            error_cls: error,
            donors: Mutex::new(Vec::new()),
            shared: Mutex::new(0),
            unowned: Mutex::new(0),
            entry_names: entry_names.unbind(),
            part_names: part_names.unbind(),
            added: PyList::empty(py).unbind(),
            report: PyList::empty(py).unbind(),
            conflicts: PyList::empty(py).unbind(),
            part_conflicts: PyList::empty(py).unbind(),
        };
        match made {
            Ok(c) => {
                *me.cell
                    .lock()
                    .map_err(|_| PyRuntimeError::new_err("the merger's lock is poisoned"))? = Some(c);
                Ok(me)
            }
            Err(e) => Err(me.err(py, &e)),
        }
    }

    /// The base database (changed in place; `finish()` returns it too).
    #[getter]
    fn out(&self, py: Python<'_>) -> Py<PyAny> {
        self.base.clone_ref(py)
    }

    /// Donor programs the file already had (same GUID and content).
    #[getter]
    fn shared_programs(&self) -> PyResult<usize> {
        Ok(*self
            .shared
            .lock()
            .map_err(|_| PyRuntimeError::new_err("the merger's lock is poisoned"))?)
    }

    /// The base's programs that no entry group owned, which `finish` left out (0 before `finish`; none in any of
    /// DICE's databases: an entry taken out of the base by hand with its programs left, SHDB-MRG-1).
    #[getter]
    fn unowned_base_programs(&self) -> PyResult<usize> {
        Ok(*self.unowned.lock().map_err(|_| poisoned())?)
    }

    /// `add_donor(donor, label)`: every donor entry of a needed mesh the base has no entry for (any, with
    /// `all_names`: every one the file lacks), with everything it draws with, and the mesh-part names of those
    /// meshes.
    fn add_donor(&self, py: Python<'_>, donor: &Bound<'_, PyAny>, label: &str) -> PyResult<()> {
        self.run(py, Some(donor), label, |m, d, store| {
            if let Some((db, kind)) = d {
                m.add_donor_kept(store, db, kind, label)
            } else {
                let view = m.current().clone();
                let kind = m.kind().clone();
                m.add_donor(&view, &kind, label)
            }
        })
    }

    /// `clone_entry(donor, entry, new_name, textures=None, label='donor')`: the donor's entry again under a new name
    /// (new program ids, keys naming it, its textures renamed: `{old: new}`); the number of programs. Nothing is
    /// added when it is refused.
    #[pyo3(signature = (donor, entry, new_name, textures = None, label = "donor"))]
    fn clone_entry(
        &self,
        py: Python<'_>,
        donor: &Bound<'_, PyAny>,
        entry: &str,
        new_name: &str,
        textures: Option<&Bound<'_, PyAny>>,
        label: &str,
    ) -> PyResult<usize> {
        let mut renames: Vec<(String, String)> = Vec::new();
        if let Some(t) = textures {
            for kv in t.call_method0("items")?.try_iter()? {
                let (k, v): (String, String) = kv?.extract()?;
                renames.push((k, v));
            }
        }
        self.run(py, Some(donor), label, |m, d, store| match d {
            Some((db, kind)) => m.clone_entry(store, db, kind, entry, new_name, &renames, label),
            None => m.clone_entry_from_self(store, entry, new_name, &renames, label),
        })
    }

    /// `add_part(name, layout_guid, donor, label='donor')`: a mesh-part name -> vertex layout, the layout taken from
    /// the donor when the file lacks it.
    #[pyo3(signature = (name, layout_guid, donor, label = "donor"))]
    fn add_part(
        &self,
        py: Python<'_>,
        name: &str,
        layout_guid: &[u8],
        donor: &Bound<'_, PyAny>,
        label: &str,
    ) -> PyResult<()> {
        self.run(py, Some(donor), label, |m, d, store| match d {
            Some((db, _)) => m.add_part(store, name, layout_guid, db, label),
            None => m.add_part_from_self(store, name, layout_guid, label),
        })
    }

    /// `finish()`: the base's programs re-sorted by GUID (PC byte order) and its entry groups renumbered and sorted;
    /// returns the base. The merger lets its records and donors go: it takes no more calls.
    fn finish(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let mut guard = self
            .cell
            .lock()
            .map_err(|_| PyRuntimeError::new_err("the merger's lock is poisoned"))?;
        let Some(cell) = guard.as_ref() else {
            return Err(self.err(
                py,
                &MergeError::Msg("the merge has finished: make a new merger for more".into()),
            ));
        };
        *self.unowned.lock().map_err(|_| poisoned())? = cell.borrow_dependent().m.unowned_base_programs().len();
        let (programs, keys, groups) = match cell.borrow_dependent().m.finished() {
            Ok(db) => {
                let groups = PyList::empty(py);
                for (g, idx) in &db.groups {
                    groups.append(PyTuple::new(
                        py,
                        [PyBytes::new(py, g).into_any(), PyList::new(py, idx)?.into_any()],
                    )?)?;
                }
                (
                    PyList::new(py, db.programs.iter().map(|r| PyBytes::new(py, r)))?,
                    PyList::new(py, db.keys.iter().map(|r| PyBytes::new(py, r)))?,
                    groups,
                )
            }
            Err(e) => return Err(self.err(py, &e)),
        };
        let base = self.base.bind(py);
        base.setattr("programs", programs)?;
        base.setattr("keys", keys)?;
        base.setattr("groups", groups)?;
        *guard = None;
        drop(guard);
        self.donors.lock().map_err(|_| poisoned())?.clear();
        Ok(self.base.clone_ref(py))
    }
}
