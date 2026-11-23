//! Round `OR3c`: the BF1943 shader translation's driver in the Rust core (`bc2_convert::shadertranslate`) for
//! `shaderdb.py`'s `translate_plan`, `missing_entries`, `foreign_entries`, `discards` and the soldier glue
//! (`match_entries`, `perms`, `part_users`, `offer_kinds`, `entry_blocks`), which Project OPFOR's `wake_translate`
//! and `ww2_char_shaders` call. A database comes in as `shaderdb.ShaderDb` (its section lists read as they are now:
//! the caller may have changed them); names are latin-1 strings.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use bc2_convert::shadertranslate::{self as st, Fail, Foreign, ForeignOptions, ForeignSource, Options, PlannedParts};
use bc2_formats::shaderdb::query::lower;
use bc2_formats::shaderdb::{Named, Shader, ShaderDb};
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyList, PySet, PyString, PyTuple};

use crate::shaders::latin1;

fn bytes_of(x: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(b) = x.cast::<PyBytes>() {
        return Ok(b.as_bytes().to_vec());
    }
    if let Ok(b) = x.cast::<PyByteArray>() {
        return Ok(b.to_vec());
    }
    Err(PyTypeError::new_err(format!(
        "expected bytes, not {}",
        x.get_type().name()?
    )))
}

/// A name (`str`) as latin-1 bytes.
fn name_of(x: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let s: Cow<'_, str> = x.cast::<PyString>()?.to_cow()?;
    s.chars()
        .map(|c| u8::try_from(u32::from(c)).map_err(|_| PyValueError::new_err(format!("{s:?} is not latin-1"))))
        .collect()
}

fn names_of(x: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<u8>>> {
    x.try_iter()?.map(|n| name_of(&n?)).collect()
}

fn lowered(x: &Bound<'_, PyAny>) -> PyResult<Vec<Vec<u8>>> {
    Ok(names_of(x)?.into_iter().map(|n| lower(&n).into_owned()).collect())
}

/// A database's sections as the caller's lists hold them now.
#[derive(Default)]
struct Owned {
    big_endian: bool,
    programs: Vec<Vec<u8>>,
    keys: Vec<Vec<u8>>,
    layouts: Vec<Vec<u8>>,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
    fragments: Vec<(Vec<u8>, Vec<u8>)>,
    parts: Vec<(Vec<u8>, Vec<u8>)>,
    groups: Vec<(Vec<u8>, Vec<u16>)>,
    vs: Vec<(Vec<u8>, Vec<u8>, u32)>,
    ps: Vec<u32>,
}

/// What of the shaders is read: none, their params, or the vertex shaders' tails (and codes).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shaders {
    None,
    Params,
    Tails,
    Codes,
}

/// A section's list, or an empty one where the object has no such attribute (a stand-in database of a test).
fn section<'py>(db: &Bound<'py, PyAny>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    match db.getattr(name) {
        Ok(v) => Ok(v),
        Err(e) if e.is_instance_of::<pyo3::exceptions::PyAttributeError>(db.py()) => {
            Ok(PyList::empty(db.py()).into_any())
        }
        Err(e) => Err(e),
    }
}

fn records(db: &Bound<'_, PyAny>, name: &str) -> PyResult<Vec<Vec<u8>>> {
    section(db, name)?.try_iter()?.map(|r| bytes_of(&r?)).collect()
}

fn named(db: &Bound<'_, PyAny>, name: &str) -> PyResult<Vec<(Vec<u8>, Vec<u8>)>> {
    section(db, name)?
        .try_iter()?
        .map(|r| {
            let r = r?;
            Ok((name_of(&r.get_item(0)?)?, bytes_of(&r.get_item(1)?)?))
        })
        .collect()
}

impl Owned {
    fn of(db: &Bound<'_, PyAny>, shaders: Shaders) -> PyResult<Self> {
        let mut o = Self {
            big_endian: match db.getattr("big_endian") {
                Ok(b) => b.is_truthy()?,
                Err(_) => false,
            },
            programs: records(db, "programs")?,
            keys: records(db, "keys")?,
            layouts: records(db, "layouts")?,
            entries: named(db, "entries")?,
            fragments: named(db, "fragments")?,
            parts: named(db, "parts")?,
            ..Self::default()
        };
        for g in section(db, "groups")?.try_iter()? {
            let g = g?;
            o.groups.push((bytes_of(&g.get_item(0)?)?, g.get_item(1)?.extract()?));
        }
        if shaders != Shaders::None {
            for s in section(db, "vs")?.try_iter()? {
                let s = s?;
                let param: u32 = s.getattr("param")?.extract()?;
                let tail = if matches!(shaders, Shaders::Tails | Shaders::Codes) {
                    bytes_of(&s.getattr("tail")?)?
                } else {
                    Vec::new()
                };
                let code = if shaders == Shaders::Codes {
                    bytes_of(&s.getattr("code")?)?
                } else {
                    Vec::new()
                };
                o.vs.push((code, tail, param));
            }
            if shaders == Shaders::Params {
                for s in section(db, "ps")?.try_iter()? {
                    o.ps.push(s?.getattr("param")?.extract()?);
                }
            }
        }
        Ok(o)
    }

    fn view(&self) -> ShaderDb<'_> {
        fn nm(v: &[(Vec<u8>, Vec<u8>)]) -> Vec<Named<'_>> {
            v.iter()
                .map(|(n, g)| Named {
                    name: Cow::Borrowed(n.as_slice()),
                    guid: g.as_slice(),
                })
                .collect()
        }
        ShaderDb {
            big_endian: self.big_endian,
            programs: self.programs.iter().map(Vec::as_slice).collect(),
            keys: self.keys.iter().map(Vec::as_slice).collect(),
            layouts: self.layouts.iter().map(Vec::as_slice).collect(),
            entries: nm(&self.entries),
            fragments: nm(&self.fragments),
            parts: nm(&self.parts),
            groups: self.groups.iter().map(|(g, v)| (g.as_slice(), v.clone())).collect(),
            vs: self
                .vs
                .iter()
                .map(|(code, tail, param)| Shader {
                    code,
                    tail,
                    param: *param,
                    ..Shader::default()
                })
                .collect(),
            ps: self
                .ps
                .iter()
                .map(|&param| Shader {
                    param,
                    ..Shader::default()
                })
                .collect(),
            ..ShaderDb::default()
        }
    }
}

/// A [`Fail`] as `(exception name, argument)`.
fn fail_py<'py>(py: Python<'py>, f: &Fail) -> PyResult<Bound<'py, PyTuple>> {
    Ok(match f {
        Fail::Translate(m) => PyTuple::new(
            py,
            [
                PyString::new(py, "TranslateError").into_any(),
                PyString::new(py, m).into_any(),
            ],
        )?,
        Fail::Xenos(m) => PyTuple::new(
            py,
            [
                PyString::new(py, "XenosError").into_any(),
                PyString::new(py, m).into_any(),
            ],
        )?,
        Fail::Value(m) => PyTuple::new(
            py,
            [
                PyString::new(py, "ValueError").into_any(),
                PyString::new(py, m).into_any(),
            ],
        )?,
        Fail::KeyBytes(b) => PyTuple::new(
            py,
            [PyString::new(py, "KeyError").into_any(), PyBytes::new(py, b).into_any()],
        )?,
        Fail::KeyInt(n) => PyTuple::new(
            py,
            [
                PyString::new(py, "KeyError").into_any(),
                n.into_pyobject(py)?.into_any(),
            ],
        )?,
    })
}

fn planned_parts(got: &Bound<'_, PyAny>) -> PyResult<PlannedParts> {
    let mut out = PlannedParts::new();
    for item in got.cast::<PyDict>()?.iter() {
        let (mesh, list) = item;
        let mut v = Vec::new();
        for t in list.try_iter()? {
            let t = t?;
            v.push((
                name_of(&t.get_item(0)?)?,
                bytes_of(&t.get_item(1)?)?,
                t.get_item(2)?.is_truthy()?,
            ));
        }
        out.insert(name_of(&mesh)?, v);
    }
    Ok(out)
}

/// `translate_db`'s plan (`bc2_convert::shadertranslate::plan`): `[(entry, guid, programs, error or None)]`, each
/// program `(layout, instanced, perm, template, source, (vs, ps, block, block), inputs [(stream, offset, format,
/// semantic)], {semantic: BF1943 offset}, rank, by, kind, strip, patch, fragment or error, key, pid)` and an error
/// `(exception name, argument)`. `planned()` -> `{mesh: [(part key, layout, instanced)]}`, called at most once.
#[pyfunction]
#[pyo3(signature = (w, db, dx, filters, exclude, keep_donor, only, dice_twins, bf_layouts, strip_layout,
                    patch_layouts, patches, terrain_fragments, soldier, planned))]
#[expect(clippy::too_many_arguments, reason = "translate_db's arguments")]
#[expect(clippy::needless_pass_by_value, reason = "PyO3 extracts owned arguments")]
pub fn shadertranslate_plan<'py>(
    py: Python<'py>,
    w: &Bound<'py, PyAny>,
    db: &Bound<'py, PyAny>,
    dx: u32,
    filters: &Bound<'py, PyAny>,
    exclude: &Bound<'py, PyAny>,
    keep_donor: &Bound<'py, PyAny>,
    only: Option<&Bound<'py, PyAny>>,
    dice_twins: &Bound<'py, PyAny>,
    bf_layouts: &Bound<'py, PyDict>,
    strip_layout: Vec<u8>,
    patch_layouts: Vec<Vec<u8>>,
    patches: &Bound<'py, PyAny>,
    terrain_fragments: (Bound<'py, PyAny>, Bound<'py, PyAny>),
    soldier: bool,
    planned: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyList>> {
    let wo = Owned::of(w, Shaders::Codes)?;
    let dbo = Owned::of(db, Shaders::Tails)?;
    let mut layouts = Vec::with_capacity(bf_layouts.len());
    for (k, r) in bf_layouts.iter() {
        layouts.push((name_of(&k)?, bytes_of(&r)?));
    }
    let o = Options {
        dx,
        filters: lowered(filters)?,
        exclude: lowered(exclude)?,
        keep_donor: names_of(keep_donor)?,
        only: only.map(|x| names_of(x).map(|v| v.into_iter().collect())).transpose()?,
        dice_twins: names_of(dice_twins)?.into_iter().collect(),
        bf_layouts: layouts,
        strip_layout,
        patch_layouts,
        patches: name_of(patches)?,
        terrain_fragments: (name_of(&terrain_fragments.0)?, name_of(&terrain_fragments.1)?),
        soldier,
    };
    let mut err: Option<PyErr> = None;
    let mut call = || -> Result<PlannedParts, String> {
        match planned.call0().and_then(|got| planned_parts(&got)) {
            Ok(p) => Ok(p),
            Err(e) => {
                let m = e.to_string();
                err = Some(e);
                Err(m)
            }
        }
    };
    let got = st::plan(&wo.view(), &dbo.view(), &o, &mut call);
    let plan = match got {
        Ok(p) => p,
        Err(st::Fatal::Planned(m)) => return Err(err.take().unwrap_or_else(|| PyRuntimeError::new_err(m))),
        Err(e) => return Err(PyValueError::new_err(e.to_string())),
    };
    let out = PyList::empty(py);
    for e in &plan {
        let progs = PyList::empty(py);
        for p in &e.programs {
            let src = PyDict::new(py);
            for (ui, bo) in &p.src {
                src.set_item(ui, bo)?;
            }
            let fragment = match &p.fragment {
                Ok(f) => PyBytes::new(py, f).into_any(),
                Err(f) => fail_py(py, f)?.into_any(),
            };
            let row: [Bound<'py, PyAny>; 16] = [
                PyBytes::new(py, &p.layout).into_any(),
                p.instanced.into_pyobject(py)?.to_owned().into_any(),
                PyBytes::new(py, &p.perm).into_any(),
                p.template.into_pyobject(py)?.into_any(),
                p.source.into_pyobject(py)?.into_any(),
                PyTuple::new(py, p.refs)?.into_any(),
                PyList::new(py, p.inputs.iter().map(|&(s, o, f, u)| (s, o, f, u, false)))?.into_any(),
                src.into_any(),
                p.rank.into_pyobject(py)?.into_any(),
                PyString::new(py, p.by).into_any(),
                p.kind.into_pyobject(py)?.into_any(),
                p.strip.into_pyobject(py)?.to_owned().into_any(),
                p.patch.into_pyobject(py)?.to_owned().into_any(),
                fragment,
                PyBytes::new(py, &p.key).into_any(),
                PyBytes::new(py, &p.pid).into_any(),
            ];
            progs.append(PyTuple::new(py, row)?)?;
        }
        let error = match &e.error {
            Some(f) => fail_py(py, f)?.into_any(),
            None => py.None().into_bound(py),
        };
        out.append((latin1(py, &e.name), PyBytes::new(py, &e.guid), progs, error))?;
    }
    Ok(out)
}

/// `add_missing_entries`' choice: `[(BF1943 material, sibling entry)]`.
#[pyfunction]
pub fn shadertranslate_missing<'py>(
    py: Python<'py>,
    w: &Bound<'py, PyAny>,
    db: &Bound<'py, PyAny>,
    filters: &Bound<'py, PyAny>,
    exclude: &Bound<'py, PyAny>,
    keep_donor: &Bound<'py, PyAny>,
    ported: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyList>> {
    let (wo, dbo) = (Owned::of(w, Shaders::None)?, Owned::of(db, Shaders::None)?);
    let ported: HashSet<Vec<u8>> = names_of(ported)?.into_iter().collect();
    let got = st::missing_entries(
        &wo.view(),
        &dbo.view(),
        &lowered(filters)?,
        &lowered(exclude)?,
        &names_of(keep_donor)?,
        &ported,
    );
    PyList::new(py, got.iter().map(|(n, s)| (latin1(py, n), latin1(py, s))))
}

struct Source<'a, 'py> {
    db: &'a Bound<'py, PyAny>,
    materials: &'a Bound<'py, PyAny>,
    err: Option<PyErr>,
}

impl ForeignSource for Source<'_, '_> {
    fn materials(&mut self, mesh: &[u8]) -> Result<HashSet<Vec<u8>>, String> {
        let py = self.db.py();
        let got = self.materials.call1((latin1(py, mesh),)).and_then(|s| {
            s.try_iter()?
                .map(|n| name_of(&n?))
                .collect::<PyResult<HashSet<Vec<u8>>>>()
        });
        got.map_err(|e| {
            let m = e.to_string();
            self.err = Some(e);
            m
        })
    }

    fn pixel_code(&mut self, ps: usize) -> Result<Vec<u8>, String> {
        let got = self
            .db
            .getattr("ps")
            .and_then(|l| l.get_item(ps))
            .and_then(|s| bytes_of(&s.getattr("code")?));
        got.map_err(|e| {
            let m = e.to_string();
            self.err = Some(e);
            m
        })
    }
}

/// `foreign_entries`' choice: `[('make', level, entry, template, cutout) | ('missing', entry, template or None)]` in
/// order. `levels`: `[(level database, [entry names])]`; `materials(mesh)` -> the lower materials its mesh set
/// names; `templates`: `{entry: template}` chosen on another database, or None.
#[pyfunction]
#[pyo3(signature = (wake, db, levels, filters, exclude, ported, variant_tags, templates, materials))]
#[expect(clippy::too_many_arguments, reason = "foreign_entries' arguments")]
pub fn shadertranslate_foreign<'py>(
    py: Python<'py>,
    wake: &Bound<'py, PyAny>,
    db: &Bound<'py, PyAny>,
    levels: &Bound<'py, PyAny>,
    filters: &Bound<'py, PyAny>,
    exclude: &Bound<'py, PyAny>,
    ported: &Bound<'py, PyAny>,
    variant_tags: &Bound<'py, PyAny>,
    templates: Option<&Bound<'py, PyDict>>,
    materials: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyList>> {
    let dbo = Owned::of(db, Shaders::None)?;
    let mut lv = Vec::new();
    for item in levels.try_iter()? {
        let item = item?;
        lv.push((item.get_item(0)?.extract::<String>()?, names_of(&item.get_item(1)?)?));
    }
    let tm = match templates {
        Some(t) => {
            let mut m = HashMap::new();
            for (k, v) in t.iter() {
                if let (Ok(k), Ok(v)) = (name_of(&k), name_of(&v)) {
                    m.insert(k, v);
                }
            }
            Some(m)
        }
        None => None,
    };
    let ported: HashSet<Vec<u8>> = names_of(ported)?.into_iter().collect();
    let tags: HashSet<Vec<u8>> = names_of(variant_tags)?.into_iter().collect();
    let (filters, exclude) = (lowered(filters)?, lowered(exclude)?);
    let o = ForeignOptions {
        filters: &filters,
        exclude: &exclude,
        ported: &ported,
        variant_tags: &tags,
        templates: tm.as_ref(),
    };
    let mut src = Source {
        db,
        materials,
        err: None,
    };
    let got = st::foreign_entries(&names_of(wake)?, &dbo.view(), &lv, &o, &mut src);
    let got = match got {
        Ok(g) => g,
        Err(m) => return Err(src.err.take().unwrap_or_else(|| PyValueError::new_err(m))),
    };
    let out = PyList::empty(py);
    for f in &got {
        match f {
            Foreign::Make(k, n, t, want) => out.append(("make", k, latin1(py, n), latin1(py, t), *want))?,
            Foreign::Missing(n, t) => out.append(("missing", latin1(py, n), t.as_ref().map(|t| latin1(py, t))))?,
        }
    }
    Ok(out)
}

/// Whether a compiled Dx10 / Dx11 shader discards (`bc2_convert::shadertranslate::discards_code`).
#[pyfunction]
pub fn shadertranslate_discards(code: &[u8]) -> bool {
    st::discards_code(code)
}

/// `ww2_char_shaders.match_entries`: `[(our entry, BF1943's)]` in `db`'s order; `weapons`: `{our mesh: BF1943's}`.
#[pyfunction]
pub fn shadertranslate_match_entries<'py>(
    py: Python<'py>,
    w: &Bound<'py, PyAny>,
    db: &Bound<'py, PyAny>,
    keep_donor: &Bound<'py, PyAny>,
    weapons: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyList>> {
    let (wo, dbo) = (Owned::of(w, Shaders::None)?, Owned::of(db, Shaders::None)?);
    let mut wm = Vec::new();
    for (k, v) in weapons.iter() {
        wm.push((name_of(&k)?, name_of(&v)?));
    }
    let got = st::match_entries(&wo.view(), &dbo.view(), &names_of(keep_donor)?, &wm);
    PyList::new(py, got.iter().map(|(n, h)| (latin1(py, n), latin1(py, h))))
}

/// `ww2_char_shaders.perms`: `{entry: {(pass, layout, instanced, point light, kind, fragment)}}`.
#[pyfunction]
pub fn shadertranslate_perms<'py>(
    py: Python<'py>,
    db: &Bound<'py, PyAny>,
    names: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let dbo = Owned::of(db, Shaders::None)?;
    let names: HashSet<Vec<u8>> = names_of(names)?.into_iter().collect();
    let got = st::perms(&dbo.view(), &names).map_err(PyValueError::new_err)?;
    let out = PyDict::new(py);
    let mut rows: Vec<_> = got.into_iter().collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    for (n, set) in rows {
        let s = PySet::empty(py)?;
        let mut set: Vec<_> = set.into_iter().collect();
        set.sort();
        for (a, l, b, c, d, f) in set {
            s.add((a, PyBytes::new(py, &l), b, c, d, PyBytes::new(py, &f)))?;
        }
        out.set_item(latin1(py, &n), s)?;
    }
    Ok(out)
}

/// `ww2_char_shaders.part_users`: `{entry: {layout guid}}` in the names' order.
#[pyfunction]
pub fn shadertranslate_part_users<'py>(
    py: Python<'py>,
    db: &Bound<'py, PyAny>,
    names: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyDict>> {
    let dbo = Owned::of(db, Shaders::None)?;
    let out = PyDict::new(py);
    for (n, set) in st::part_users(&dbo.view(), &names_of(names)?) {
        let mut v: Vec<_> = set.into_iter().collect();
        v.sort();
        out.set_item(latin1(py, &n), PySet::new(py, v.iter().map(|g| PyBytes::new(py, g)))?)?;
    }
    Ok(out)
}

/// `ww2_char_shaders.offer_kinds`' answer: `([(program copied, new key)], [(group guid, [programs])], {entry: why},
/// [log lines])`. `entries`: `{our entry: BF1943's name in w}`; `need`: `{our entry: {(pass, kind)}}`; `single`:
/// `{our entry: bool}`.
#[pyfunction]
pub fn shadertranslate_offer_kinds<'py>(
    py: Python<'py>,
    w: &Bound<'py, PyAny>,
    entries: &Bound<'py, PyDict>,
    need: &Bound<'py, PyDict>,
    single: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyTuple>> {
    let wo = Owned::of(w, Shaders::None)?;
    let mut en = Vec::new();
    for (k, v) in entries.iter() {
        en.push((name_of(&k)?, name_of(&v)?));
    }
    let mut nd = HashMap::new();
    for (k, v) in need.iter() {
        let mut pairs: Vec<(u8, u8)> = Vec::new();
        for p in v.try_iter()? {
            pairs.push(p?.extract()?);
        }
        nd.insert(name_of(&k)?, pairs);
    }
    let mut sg = HashMap::new();
    for (k, v) in single.iter() {
        sg.insert(name_of(&k)?, v.is_truthy()?);
    }
    let got = st::offer_kinds(&wo.view(), &en, &nd, &sg).map_err(PyValueError::new_err)?;
    let added = PyList::new(py, got.added.iter().map(|(i, k)| (*i, PyBytes::new(py, k))))?;
    let groups = PyList::empty(py);
    for (g, v) in &got.groups {
        groups.append((PyBytes::new(py, g), PyList::new(py, v)?))?;
    }
    let bad = PyDict::new(py);
    for (n, why) in &got.bad {
        bad.set_item(latin1(py, n), why)?;
    }
    PyTuple::new(
        py,
        [
            added.into_any(),
            groups.into_any(),
            bad.into_any(),
            PyList::new(py, &got.log)?.into_any(),
        ],
    )
}

/// `ww2_char_shaders.entry_blocks`: the parameter blocks the named entries' programs use.
#[pyfunction]
pub fn shadertranslate_entry_blocks<'py>(
    py: Python<'py>,
    w: &Bound<'py, PyAny>,
    names: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PySet>> {
    let wo = Owned::of(w, Shaders::Params)?;
    let got = st::entry_blocks(&wo.view(), &names_of(names)?).map_err(PyValueError::new_err)?;
    PySet::new(py, got)
}

fn element_map_py(py: Python<'_>, m: Option<st::ElementMap>) -> PyResult<Bound<'_, PyAny>> {
    match m {
        None => Ok(py.None().into_bound(py)),
        Some(m) => {
            let d = PyDict::new(py);
            for (k, e) in m {
                d.set_item(k, e)?;
            }
            Ok(d.into_any())
        }
    }
}

/// `element_map(bf_layout, pc_layout)`: `{BF1943 offset: PC element}` or None.
#[pyfunction]
#[pyo3(signature = (bf, pc))]
pub fn shadertranslate_element_map<'py>(py: Python<'py>, bf: Option<&[u8]>, pc: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let m = st::element_map(bf, pc).map_err(|e| PyValueError::new_err(e.to_string()))?;
    element_map_py(py, m)
}

/// `ww2_char_shaders.soldier_element_map(bf, pc, kind)`: `{BF1943 offset: PC element}` or None; `KeyError` where
/// DICE's layout lacks a wanted element.
#[pyfunction]
pub fn shadertranslate_soldier_element_map<'py>(
    py: Python<'py>,
    bf: &[u8],
    pc: &[u8],
    kind: &str,
) -> PyResult<Bound<'py, PyAny>> {
    match st::soldier_element_map(bf, pc, kind == "1p").map_err(|e| PyValueError::new_err(e.to_string()))? {
        Ok(m) => element_map_py(py, m),
        Err(Fail::KeyInt(k)) => Err(pyo3::exceptions::PyKeyError::new_err(k)),
        Err(_) => Err(PyRuntimeError::new_err("soldier_element_map")),
    }
}

/// `vertex_elements(layout)`: the stream-0 elements in offset order, overlapping ones left out.
#[pyfunction]
pub fn shadertranslate_vertex_elements(layout: &[u8]) -> PyResult<Vec<(u8, u8, u8, u8)>> {
    st::vertex_elements(layout).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// `by_role(bfel, fetch, bf_fmt)`: `{semantic: BF1943 offset}` or None.
#[pyfunction]
#[expect(clippy::needless_pass_by_value, reason = "PyO3 extracts owned arguments")]
pub fn shadertranslate_by_role(
    py: Python<'_>,
    bfel: Vec<(u32, u32)>,
    fetch: HashMap<u32, u8>,
    bf_fmt: std::collections::BTreeMap<u8, u8>,
) -> PyResult<Bound<'_, PyAny>> {
    match st::by_role(&bfel, &fetch, &bf_fmt) {
        None => Ok(py.None().into_bound(py)),
        Some(v) => {
            let d = PyDict::new(py);
            // the dictionary's order: the fetches' (bfel's)
            for (bo, ui) in &bfel {
                let _ = bo;
                if let Some((_, b)) = v.iter().find(|(u, _)| u == ui) {
                    d.set_item(ui, b)?;
                }
            }
            Ok(d.into_any())
        }
    }
}

/// `mesh_of(entry)`: the mesh (lower) an entry's name begins with.
#[pyfunction]
pub fn shadertranslate_mesh_of<'py>(py: Python<'py>, entry: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyString>> {
    Ok(latin1(py, &st::mesh_of(&name_of(entry)?)))
}

/// `patch_inputs(tail)`: `{semantic: (stream, offset, format)}`.
#[pyfunction]
pub fn shadertranslate_patch_inputs(tail: &[u8]) -> PyResult<std::collections::BTreeMap<u32, (u32, u32, u32)>> {
    st::patch_inputs(tail).map_err(|e| PyValueError::new_err(e.to_string()))
}
