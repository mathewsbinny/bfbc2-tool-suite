//! Round E4b: a shader database kept in the Rust core (`ShaderDbHandle`), and the queries `shaderdb.py` asks of it.
//!
//! `ShaderDb.parse` keeps the file's `bytes` and the offsets of its records here instead of making ~70,000 Python
//! objects: a section becomes a Python list (the objects `shaderdb_parse` made) only when the caller reads that
//! attribute (`materialize`). The queries (`entry_textures`, `entry_signatures`, `entry_layouts`, `coverage`,
//! `select_needed`, `mesh_names`) and the writer read a section from the file while it was never made, and from the
//! caller's list once it was - so what a caller changed is what they see. The entries' and entry groups' lookups are
//! kept between calls and checked against the list each call (the list object, its length and every item, by
//! identity: a list changed in place or replaced is read again).
//!
//! Every method returns `None` where the Python reference would do something else than the plain case - an item of
//! another type, a name that is not latin-1, an index past its section, a record too short - and `shaderdb.py` then
//! runs its Python, which answers or raises as it always did.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use bc2_formats::shaderdb::{self, Named, Shader, ShaderDb, query};
use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyFrozenSet, PyList, PySet, PyString, PyTuple};

use crate::shaders::{
    bytes_items, latin1, latin1_bytes, name_items, named, names, records, shader, shader_items, shaders, slices,
};

/// The sections, in file order, by their `ShaderDb` attribute names.
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

/// The record sections (raw records), by attribute name.
const RECORDS: [&str; 6] = ["params", "bindings", "regs", "programs", "keys", "layouts"];
/// The name sections.
const NAMES: [&str; 3] = ["entries", "fragments", "parts"];

/// A record's place in the file.
#[derive(Debug, Clone, Copy, Default)]
struct Span {
    at: usize,
    len: usize,
}

impl Span {
    fn of(data: &[u8], s: &[u8]) -> Self {
        Self {
            at: s.as_ptr().addr() - data.as_ptr().addr(),
            len: s.len(),
        }
    }

    fn get(self, data: &[u8]) -> &[u8] {
        data.get(self.at..self.at + self.len).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct FileShader {
    guid: Span,
    code: Span,
    param: u32,
    binding: u32,
    regs: u32,
    tail: Span,
}

/// Every record of the file, as places in it.
#[derive(Debug, Default)]
struct Spans {
    records: [Vec<Span>; 6],
    shaders: [Vec<FileShader>; 2],
    names: [Vec<(Span, Span)>; 3],
    /// An entry group's GUID and its program count (the `u16` indices follow the count).
    groups: Vec<(Span, usize)>,
}

impl Spans {
    fn of(data: &[u8], db: &ShaderDb<'_>) -> Self {
        let recs = |l: &[&[u8]]| l.iter().map(|r| Span::of(data, r)).collect::<Vec<_>>();
        let shaders = |l: &[Shader<'_>]| {
            l.iter()
                .map(|s| FileShader {
                    guid: Span::of(data, s.guid),
                    code: Span::of(data, s.code),
                    param: s.param,
                    binding: s.binding,
                    regs: s.regs,
                    tail: if s.tail.is_empty() {
                        Span::default()
                    } else {
                        Span::of(data, s.tail)
                    },
                })
                .collect()
        };
        let names = |l: &[Named<'_>]| {
            l.iter()
                .map(|n| (Span::of(data, &n.name), Span::of(data, n.guid)))
                .collect()
        };
        Self {
            records: [
                recs(&db.params),
                recs(&db.bindings),
                recs(&db.regs),
                recs(&db.programs),
                recs(&db.keys),
                recs(&db.layouts),
            ],
            shaders: [shaders(&db.vs), shaders(&db.ps)],
            names: [names(&db.entries), names(&db.fragments), names(&db.parts)],
            groups: db
                .groups
                .iter()
                .map(|(g, idx)| (Span::of(data, g), idx.len()))
                .collect(),
        }
    }
}

/// The plain case does not hold: the Python reference answers instead.
struct Decline;

type Q<T> = Result<T, Decline>;

impl From<PyErr> for Decline {
    fn from(_: PyErr) -> Self {
        Self
    }
}

/// Where a cached lookup was made from: the file (the attribute never made), or a Python list as it was then.
enum Src {
    File,
    Live { list: Py<PyList>, items: Vec<Py<PyAny>> },
}

impl Src {
    fn live(list: &Bound<'_, PyList>) -> Self {
        Self::Live {
            list: list.clone().unbind(),
            items: list.iter().map(Bound::unbind).collect(),
        }
    }

    /// Is the attribute's value (`None`: not made) still what this was made from?
    fn current(&self, cur: Option<&Bound<'_, PyAny>>) -> bool {
        match (self, cur) {
            (Self::File, None) => true,
            (Self::Live { list, items }, Some(c)) => {
                c.is(list)
                    && c.cast::<PyList>()
                        .is_ok_and(|l| l.len() == items.len() && l.iter().zip(items).all(|(a, b)| a.is(b)))
            }
            _ => false,
        }
    }
}

/// A name section's names (latin-1) and GUIDs, and where each name is.
struct NameIndex {
    src: Src,
    names: Vec<Vec<u8>>,
    guids: Vec<Vec<u8>>,
    by_name: HashMap<Vec<u8>, Vec<usize>>,
    /// The names lower-cased (Python's `str.lower`), made on first use.
    lowered: std::cell::OnceCell<Vec<Vec<u8>>>,
}

impl NameIndex {
    fn new(src: Src, pairs: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        let mut by_name: HashMap<Vec<u8>, Vec<usize>> = HashMap::with_capacity(pairs.len());
        let (names, guids): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
        for (i, n) in names.iter().enumerate() {
            by_name.entry(n.clone()).or_default().push(i);
        }
        Self {
            src,
            names,
            guids,
            by_name,
            lowered: std::cell::OnceCell::new(),
        }
    }
}

/// The entry groups: each GUID's last group (`entry_programs`' dict).
struct GroupIndex {
    src: Src,
    by_guid: HashMap<Vec<u8>, usize>,
}

/// What the queries keep of a parameter block: its textures and its constants' names.
struct Block {
    item: Option<Py<PyAny>>,
    be: bool,
    textures: Vec<(u8, u8, Vec<u8>)>,
    constants: Vec<Vec<u8>>,
}

#[derive(Default)]
struct Cache {
    names: [Option<NameIndex>; 3],
    groups: Option<GroupIndex>,
    blocks: HashMap<usize, Block>,
}

/// A shader database in the Rust core: the file it was read from (or none, for one made in Python) and the queries.
#[pyclass(frozen, module = "bc2_native")]
pub struct ShaderDbHandle {
    file: Option<Py<PyBytes>>,
    spans: Spans,
    big_endian: bool,
    version: u32,
    flag: u8,
    cache: Mutex<Cache>,
}

/// A section as a query reads it: the file's records, or the caller's list.
enum Recs<'a, 'py> {
    File(&'a [u8], &'a [Span]),
    Live(Bound<'py, PyList>),
}

/// One record of [`Recs`].
enum Rec<'a, 'py> {
    File(&'a [u8]),
    Live(Bound<'py, PyBytes>),
}

impl Rec<'_, '_> {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::File(b) => b,
            Self::Live(b) => b.as_bytes(),
        }
    }
}

impl<'a, 'py> Recs<'a, 'py> {
    fn get(&self, i: usize) -> Q<Rec<'a, 'py>> {
        match self {
            Self::File(data, spans) => Ok(Rec::File(spans.get(i).ok_or(Decline)?.get(data))),
            Self::Live(l) => {
                if i >= l.len() {
                    return Err(Decline);
                }
                Ok(Rec::Live(
                    l.get_item(i)?.cast_into_exact::<PyBytes>().map_err(|_| Decline)?,
                ))
            }
        }
    }
}

/// A `str` as latin-1 bytes, or `None` (not a plain `str`, or a character past U+00FF).
fn latin1_of(x: &Bound<'_, PyAny>) -> Option<Vec<u8>> {
    let s = x.cast_exact::<PyString>().ok()?;
    latin1_bytes(s.to_str().ok()?).map(std::borrow::Cow::into_owned)
}

/// The `str` items of a set, frozenset, dict (keys), list or tuple, as latin-1 bytes; items that cannot equal a
/// latin-1 name (other characters) are left out. `None` for another container or an item that is not a plain `str`.
fn wanted(names: &Bound<'_, PyAny>) -> Option<Vec<Vec<u8>>> {
    let plain = names.is_exact_instance_of::<PySet>()
        || names.is_exact_instance_of::<PyFrozenSet>()
        || names.is_exact_instance_of::<PyDict>()
        || names.is_exact_instance_of::<PyList>()
        || names.is_exact_instance_of::<PyTuple>();
    if !plain {
        return None;
    }
    let mut out = Vec::new();
    for x in names.try_iter().ok()? {
        let x = x.ok()?;
        let s = x.cast_exact::<PyString>().ok()?;
        if let Some(b) = latin1_bytes(s.to_str().ok()?) {
            out.push(b.into_owned());
        }
    }
    Some(out)
}

impl ShaderDbHandle {
    fn data<'a>(&'a self, py: Python<'a>) -> &'a [u8] {
        match &self.file {
            Some(f) => f.bind(py).as_bytes(),
            None => &[],
        }
    }

    /// The attribute's value in the instance dict (`None`: not made).
    fn attr<'py>(d: &Bound<'py, PyDict>, name: &str) -> Q<Option<Bound<'py, PyAny>>> {
        Ok(d.get_item(name)?)
    }

    /// A section to read: the caller's list when made (a plain list), else the file's records (a handle without a
    /// file declines).
    fn recs<'a, 'py>(&'a self, py: Python<'py>, d: &Bound<'py, PyDict>, name: &str) -> Q<Recs<'a, 'py>>
    where
        'py: 'a,
    {
        let k = RECORDS.iter().position(|r| *r == name).ok_or(Decline)?;
        match Self::attr(d, name)? {
            Some(v) => Ok(Recs::Live(v.cast_into_exact::<PyList>().map_err(|_| Decline)?)),
            None if self.file.is_some() => Ok(Recs::File(self.data(py), self.spans.records.get(k).ok_or(Decline)?)),
            None => Err(Decline),
        }
    }

    /// The index of name section `section` for its current value, made again when it changed.
    #[expect(
        clippy::many_single_char_names,
        reason = "a tuple's name and GUID, as the reference unpacks them"
    )]
    fn names_index<'c>(
        &self,
        py: Python<'_>,
        d: &Bound<'_, PyDict>,
        cache: &'c mut Cache,
        section: usize,
    ) -> Q<&'c NameIndex> {
        let k = section;
        let name = NAMES.get(k).ok_or(Decline)?;
        let cur = Self::attr(d, name)?;
        let slot = cache.names.get_mut(k).ok_or(Decline)?;
        if slot.as_ref().is_some_and(|ix| ix.src.current(cur.as_ref())) {
            return slot.as_ref().ok_or(Decline);
        }
        let ix = match cur {
            None => {
                if self.file.is_none() {
                    return Err(Decline);
                }
                let data = self.data(py);
                let spans = self.spans.names.get(k).ok_or(Decline)?;
                let pairs = spans
                    .iter()
                    .map(|(n, g)| (n.get(data).to_vec(), g.get(data).to_vec()))
                    .collect();
                NameIndex::new(Src::File, pairs)
            }
            Some(value) => {
                let list = value.cast_into_exact::<PyList>().map_err(|_| Decline)?;
                let mut pairs = Vec::with_capacity(list.len());
                for item in list.iter() {
                    let t = item.cast_exact::<PyTuple>().map_err(|_| Decline)?;
                    if t.len() != 2 {
                        return Err(Decline);
                    }
                    let n = latin1_of(&t.get_item(0)?).ok_or(Decline)?;
                    let g = t.get_item(1)?;
                    let g = g.cast_exact::<PyBytes>().map_err(|_| Decline)?;
                    pairs.push((n, g.as_bytes().to_vec()));
                }
                NameIndex::new(Src::live(&list), pairs)
            }
        };
        Ok(slot.insert(ix))
    }

    /// The entry groups' index for their current value.
    fn groups_index<'c>(&self, py: Python<'_>, d: &Bound<'_, PyDict>, cache: &'c mut Cache) -> Q<&'c GroupIndex> {
        let cur = Self::attr(d, "groups")?;
        if cache.groups.as_ref().is_some_and(|ix| ix.src.current(cur.as_ref())) {
            return cache.groups.as_ref().ok_or(Decline);
        }
        let mut by_guid = HashMap::new();
        let src = match cur {
            None => {
                if self.file.is_none() {
                    return Err(Decline);
                }
                let data = self.data(py);
                for (i, (g, _)) in self.spans.groups.iter().enumerate() {
                    by_guid.insert(g.get(data).to_vec(), i);
                }
                Src::File
            }
            Some(v) => {
                let list = v.cast_into_exact::<PyList>().map_err(|_| Decline)?;
                for (i, item) in list.iter().enumerate() {
                    let t = item.cast_exact::<PyTuple>().map_err(|_| Decline)?;
                    if t.len() != 2 {
                        return Err(Decline);
                    }
                    let g = t.get_item(0)?;
                    by_guid.insert(g.cast_exact::<PyBytes>().map_err(|_| Decline)?.as_bytes().to_vec(), i);
                }
                Src::live(&list)
            }
        };
        Ok(cache.groups.insert(GroupIndex { src, by_guid }))
    }

    /// Group `i`'s program indices, read now (a made list's inner lists may have changed).
    fn group_programs(&self, py: Python<'_>, ix: &GroupIndex, i: usize) -> Q<Vec<usize>> {
        match &ix.src {
            Src::File => {
                let (g, n) = self.spans.groups.get(i).ok_or(Decline)?;
                let raw = self.data(py).get(g.at + 20..g.at + 20 + 2 * n).ok_or(Decline)?;
                let be = self.big_endian;
                Ok(raw
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| {
                        usize::from(if be {
                            u16::from_be_bytes(*c)
                        } else {
                            u16::from_le_bytes(*c)
                        })
                    })
                    .collect())
            }
            Src::Live { items, .. } => {
                let t = items
                    .get(i)
                    .ok_or(Decline)?
                    .bind(py)
                    .cast::<PyTuple>()
                    .map_err(|_| Decline)?
                    .clone();
                let idx = t.get_item(1)?;
                if !(idx.is_exact_instance_of::<PyList>() || idx.is_exact_instance_of::<PyTuple>()) {
                    return Err(Decline);
                }
                idx.try_iter()?
                    .map(|x| x?.extract::<u32>().map(|v| v as usize).map_err(|_| Decline))
                    .collect()
            }
        }
    }

    /// The programs of the entry with this GUID (`entry_programs().get(g, [])`).
    fn entry_programs(&self, py: Python<'_>, groups: &GroupIndex, guid: &[u8]) -> Q<Vec<usize>> {
        match groups.by_guid.get(guid) {
            Some(&i) => self.group_programs(py, groups, i),
            None => Ok(Vec::new()),
        }
    }

    /// Parameter block `k`'s textures and constant names (kept while the block is the same object).
    fn block(blocks: &mut HashMap<usize, Block>, params: &Recs<'_, '_>, k: usize, be: bool) -> Q<()> {
        let rec = params.get(k)?;
        let item = match &rec {
            Rec::File(_) => None,
            Rec::Live(b) => Some(b.as_any()),
        };
        let fresh = blocks.get(&k).is_some_and(|b| {
            b.be == be
                && match (&b.item, item) {
                    (None, None) => true,
                    (Some(a), Some(c)) => c.is(a),
                    _ => false,
                }
        });
        if !fresh {
            let bytes = rec.bytes();
            let block = Block {
                item: item.map(|b| b.clone().unbind()),
                be,
                textures: query::block_textures(bytes, be)
                    .into_iter()
                    .map(|t| (t.sampler, t.cube, t.name.to_vec()))
                    .collect(),
                constants: query::block_constants(bytes, be)
                    .into_iter()
                    .map(|c| c.name.to_vec())
                    .collect(),
            };
            blocks.insert(k, block);
        }
        Ok(())
    }

    /// The entries a query covers, in file order: all, or those whose name is in `names`.
    fn selected(ix: &NameIndex, names: Option<&[Vec<u8>]>) -> Vec<usize> {
        match names {
            None => (0..ix.names.len()).collect(),
            Some(want) => {
                let mut at: Vec<usize> = want
                    .iter()
                    .filter_map(|n| ix.by_name.get(n))
                    .flatten()
                    .copied()
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                at.sort_unstable();
                at
            }
        }
    }

    fn lock(&self) -> Q<std::sync::MutexGuard<'_, Cache>> {
        self.cache.try_lock().map_err(|_| Decline)
    }

    /// The distinct parameter blocks an entry's programs name (an entry's permutations mostly share theirs: the sets
    /// the queries make are the same from each block once).
    fn program_blocks(programs: &Recs<'_, '_>, progs: &[usize], be: bool) -> Q<Vec<usize>> {
        let mut out = Vec::with_capacity(2 * progs.len());
        for &i in progs {
            let r = query::program_refs(programs.get(i)?.bytes(), be).ok_or(Decline)?;
            out.extend([r[2] as usize, r[3] as usize]);
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    fn entry_textures_q<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        be: bool,
        names: Option<&[Vec<u8>]>,
    ) -> Q<Bound<'py, PyDict>> {
        let mut guard = self.lock()?;
        let cache = &mut *guard;
        self.groups_index(py, d, cache)?;
        self.names_index(py, d, cache, 0)?;
        let (Some(entries), Some(groups)) = (cache.names.first().and_then(Option::as_ref), cache.groups.as_ref())
        else {
            return Err(Decline);
        };
        let programs = self.recs(py, d, "programs")?;
        let params = self.recs(py, d, "params")?;
        let out = PyDict::new(py);
        for i in Self::selected(entries, names) {
            let progs = self.entry_programs(py, groups, entries.guids.get(i).ok_or(Decline)?)?;
            let ks = Self::program_blocks(&programs, &progs, be)?;
            for &k in &ks {
                Self::block(&mut cache.blocks, &params, k, be)?;
            }
            let mut found = BTreeSet::new();
            for k in ks {
                let b = cache.blocks.get(&k).ok_or(Decline)?;
                found.extend(b.textures.iter().map(|(s, c, n)| (*s, *c, n.as_slice())));
            }
            let list = PyList::empty(py);
            for (s, c, n) in found {
                list.append((s, c, latin1(py, n)))?;
            }
            out.set_item(latin1(py, entries.names.get(i).ok_or(Decline)?), list)?;
        }
        Ok(out)
    }

    fn entry_signatures_q<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        be: bool,
        names: Option<&[Vec<u8>]>,
    ) -> Q<Bound<'py, PyDict>> {
        let mut guard = self.lock()?;
        let cache = &mut *guard;
        self.groups_index(py, d, cache)?;
        self.names_index(py, d, cache, 0)?;
        let (Some(entries), Some(groups)) = (cache.names.first().and_then(Option::as_ref), cache.groups.as_ref())
        else {
            return Err(Decline);
        };
        let programs = self.recs(py, d, "programs")?;
        let params = self.recs(py, d, "params")?;
        let out = PyDict::new(py);
        for i in Self::selected(entries, names) {
            let progs = self.entry_programs(py, groups, entries.guids.get(i).ok_or(Decline)?)?;
            let mut consts = BTreeSet::new();
            let mut samplers = HashSet::new();
            let ks = Self::program_blocks(&programs, &progs, be)?;
            for &k in &ks {
                Self::block(&mut cache.blocks, &params, k, be)?;
            }
            for k in ks {
                let b = cache.blocks.get(&k).ok_or(Decline)?;
                consts.extend(b.constants.iter().map(Vec::as_slice));
                samplers.extend(b.textures.iter().map(|t| t.0));
            }
            let c = PyTuple::new(py, consts.into_iter().map(|n| latin1(py, n)))?;
            out.set_item(latin1(py, entries.names.get(i).ok_or(Decline)?), (c, samplers.len()))?;
        }
        Ok(out)
    }

    fn entry_layouts_q<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        names: Option<&[Vec<u8>]>,
    ) -> Q<Bound<'py, PyDict>> {
        let mut guard = self.lock()?;
        let cache = &mut *guard;
        self.groups_index(py, d, cache)?;
        self.names_index(py, d, cache, 0)?;
        let (Some(entries), Some(groups)) = (cache.names.first().and_then(Option::as_ref), cache.groups.as_ref())
        else {
            return Err(Decline);
        };
        let keys = self.recs(py, d, "keys")?;
        let out = PyDict::new(py);
        for i in Self::selected(entries, names) {
            let progs = self.entry_programs(py, groups, entries.guids.get(i).ok_or(Decline)?)?;
            let mut lays = BTreeSet::new();
            for p in progs {
                let k = keys.get(p)?;
                let b = k.bytes();
                lays.insert(b.get(32.min(b.len())..48.min(b.len())).unwrap_or_default().to_vec());
            }
            let set = PySet::new(py, lays.iter().map(|l| PyBytes::new(py, l)))?;
            out.set_item(latin1(py, entries.names.get(i).ok_or(Decline)?), set)?;
        }
        Ok(out)
    }

    /// A section's length: the caller's list (any sized object) or the file's count.
    fn section_len(&self, d: &Bound<'_, PyDict>, name: &str, file_len: usize) -> Q<usize> {
        match Self::attr(d, name)? {
            Some(v) => Ok(v.len()?),
            None if self.file.is_some() => Ok(file_len),
            None => Err(Decline),
        }
    }

    /// The groups as `(GUID, program indices)`, read now from the file or the caller's list.
    fn groups_now<'py>(&self, py: Python<'py>, d: &Bound<'py, PyDict>) -> Q<Vec<(Bound<'py, PyBytes>, Vec<u32>)>> {
        let mut out = Vec::new();
        match Self::attr(d, "groups")? {
            Some(v) => {
                let list = v.cast_into_exact::<PyList>().map_err(|_| Decline)?;
                for item in list.iter() {
                    let t = item.cast_exact::<PyTuple>().map_err(|_| Decline)?;
                    if t.len() != 2 {
                        return Err(Decline);
                    }
                    let g = t.get_item(0)?.cast_into_exact::<PyBytes>().map_err(|_| Decline)?;
                    let idx = t.get_item(1)?;
                    if !(idx.is_exact_instance_of::<PyList>() || idx.is_exact_instance_of::<PyTuple>()) {
                        return Err(Decline);
                    }
                    let idx = idx
                        .try_iter()?
                        .map(|x| x?.extract::<u32>().map_err(|_| Decline))
                        .collect::<Q<_>>()?;
                    out.push((g, idx));
                }
            }
            None if self.file.is_some() => {
                let data = self.data(py);
                let be = self.big_endian;
                for (g, n) in &self.spans.groups {
                    let raw = data.get(g.at + 20..g.at + 20 + 2 * n).ok_or(Decline)?;
                    let idx = raw
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| {
                            u32::from(if be {
                                u16::from_be_bytes(*c)
                            } else {
                                u16::from_le_bytes(*c)
                            })
                        })
                        .collect();
                    out.push((PyBytes::new(py, g.get(data)), idx));
                }
            }
            None => return Err(Decline),
        }
        Ok(out)
    }

    fn check_q(&self, py: Python<'_>, d: &Bound<'_, PyDict>, big_endian: bool, numbers_be: bool) -> Q<Vec<String>> {
        let rec_list = |name: &str| -> Q<Vec<Rec<'_, '_>>> {
            let r = self.recs(py, d, name)?;
            let n = match &r {
                Recs::File(_, spans) => spans.len(),
                Recs::Live(l) => l.len(),
            };
            (0..n).map(|i| r.get(i)).collect()
        };
        let programs = rec_list("programs")?;
        let keys = rec_list("keys")?;
        let layouts = rec_list("layouts")?;
        let [params_count, vs_count, ps_count] = [
            self.section_len(d, "params", self.spans.records.first().map_or(0, Vec::len))?,
            self.section_len(d, "vs", self.spans.shaders.first().map_or(0, Vec::len))?,
            self.section_len(d, "ps", self.spans.shaders.get(1).map_or(0, Vec::len))?,
        ];
        let groups = self.groups_now(py, d)?;
        let mut guard = self.lock()?;
        let cache = &mut *guard;
        for k in 0..3 {
            self.names_index(py, d, cache, k)?;
        }
        let [Some(entries), Some(fragments), Some(parts)] = &cache.names else {
            return Err(Decline);
        };
        let input = query::CheckInput {
            big_endian,
            numbers_big_endian: numbers_be,
            params: params_count,
            vs: vs_count,
            ps: ps_count,
            programs: programs.iter().map(Rec::bytes).collect(),
            keys: keys.iter().map(Rec::bytes).collect(),
            layouts: layouts.iter().map(Rec::bytes).collect(),
            entries: entries
                .names
                .iter()
                .map(Vec::as_slice)
                .zip(entries.guids.iter().map(Vec::as_slice))
                .collect(),
            fragments: fragments.guids.iter().map(Vec::as_slice).collect(),
            parts: parts.guids.iter().map(Vec::as_slice).collect(),
            groups: groups.iter().map(|(g, idx)| (g.as_bytes(), idx.clone())).collect(),
        };
        query::check(&input).ok_or(Decline)
    }

    /// Writes from each section's current source; `None` where only the Python writer takes an item.
    #[expect(clippy::too_many_lines, reason = "every section from its own source, in file order")]
    fn write_q<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        big_endian: bool,
        version: u32,
        flag: u8,
    ) -> Q<Bound<'py, PyBytes>> {
        let data = self.data(py);
        let fbe = self.big_endian;
        let mut held: Vec<Vec<Bound<'py, PyBytes>>> = Vec::new();
        let mut live: [Option<Bound<'py, PyAny>>; 12] = Default::default();
        for (k, s) in SECTIONS.iter().enumerate() {
            let v = Self::attr(d, s)?;
            if v.is_none() && self.file.is_none() {
                return Err(Decline);
            }
            if let Some(slot) = live.get_mut(k) {
                *slot = v;
            }
        }
        // record sections: the caller's bytes or the file's spans
        let mut rec_lists: [Vec<&[u8]>; 6] = Default::default();
        let mut bound: [Option<Vec<Bound<'py, PyBytes>>>; 6] = Default::default();
        for (k, name) in RECORDS.iter().enumerate() {
            let at = SECTIONS.iter().position(|s| s == name).ok_or(Decline)?;
            if let Some(v) = live.get(at).and_then(Option::as_ref)
                && let Some(slot) = bound.get_mut(k)
            {
                *slot = Some(bytes_items(v).ok_or(Decline)?);
            }
        }
        for (k, slot) in rec_lists.iter_mut().enumerate() {
            *slot = match bound.get(k).and_then(Option::as_ref) {
                Some(b) => slices(b),
                None => self
                    .spans
                    .records
                    .get(k)
                    .ok_or(Decline)?
                    .iter()
                    .map(|s| s.get(data))
                    .collect(),
            };
        }
        let shader_fields = [3usize, 4].map(|at| live.get(at).and_then(Option::as_ref).map(shader_items));
        let mut shaders: [Vec<Shader<'_>>; 2] = Default::default();
        for (k, slot) in shaders.iter_mut().enumerate() {
            *slot = match shader_fields.get(k).ok_or(Decline)? {
                Some(Some(f)) => f.iter().map(shader).collect(),
                Some(None) => return Err(Decline),
                None => self
                    .spans
                    .shaders
                    .get(k)
                    .ok_or(Decline)?
                    .iter()
                    .map(|s| Shader {
                        guid: s.guid.get(data),
                        code: s.code.get(data),
                        param: s.param,
                        binding: s.binding,
                        regs: s.regs,
                        tail: s.tail.get(data),
                    })
                    .collect(),
            };
        }
        let name_fields = [8usize, 9, 10].map(|at| live.get(at).and_then(Option::as_ref).map(name_items));
        let mut strs: [Vec<&str>; 3] = Default::default();
        for (k, slot) in strs.iter_mut().enumerate() {
            if let Some(Some(f)) = name_fields.get(k).ok_or(Decline)? {
                *slot = f.iter().map(|(n, _)| n.to_str()).collect::<PyResult<_>>()?;
            }
        }
        let mut name_lists: [Vec<Named<'_>>; 3] = Default::default();
        for (k, slot) in name_lists.iter_mut().enumerate() {
            *slot = match name_fields.get(k).ok_or(Decline)? {
                Some(Some(f)) => named(f, strs.get(k).ok_or(Decline)?).ok_or(Decline)?,
                Some(None) => return Err(Decline),
                None => self
                    .spans
                    .names
                    .get(k)
                    .ok_or(Decline)?
                    .iter()
                    .map(|(n, g)| Named {
                        name: n.get(data).into(),
                        guid: g.get(data),
                    })
                    .collect(),
            };
        }
        let mut group_guids = Vec::new();
        let mut groups = Vec::new();
        match live.get(11).and_then(Option::as_ref) {
            Some(v) => {
                let mut idx_lists = Vec::new();
                for x in v.try_iter()? {
                    let (g, idx): (Bound<'py, PyAny>, Vec<u16>) = x?.extract()?;
                    group_guids.push(g.cast_into::<PyBytes>().map_err(|_| Decline)?);
                    idx_lists.push(idx);
                }
                held.push(group_guids);
                let gs = held.last().ok_or(Decline)?;
                groups = slices(gs).into_iter().zip(idx_lists).collect();
            }
            None => {
                for (g, n) in &self.spans.groups {
                    let raw = data.get(g.at + 20..g.at + 20 + 2 * n).ok_or(Decline)?;
                    let idx = raw
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| {
                            if fbe {
                                u16::from_be_bytes(*c)
                            } else {
                                u16::from_le_bytes(*c)
                            }
                        })
                        .collect();
                    groups.push((g.get(data), idx));
                }
            }
        }
        let [params, bindings, regs, programs, keys, layouts] = rec_lists;
        let [vs, ps] = shaders;
        let [entries, fragments, parts] = name_lists;
        let db = ShaderDb {
            big_endian,
            version,
            flag,
            params,
            bindings,
            regs,
            vs,
            ps,
            programs,
            keys,
            layouts,
            entries,
            fragments,
            parts,
            groups,
        };
        let size = shaderdb::written_size(&db);
        Ok(PyBytes::new_with(py, size, |buf| {
            py.detach(|| shaderdb::write_into(&db, buf));
            Ok(())
        })?)
    }
}

impl ShaderDbHandle {
    /// A handle holding `data`, the file `db` was read from (round SD1: `shaderdb_open`).
    pub(crate) fn of_file(data: &Bound<'_, PyBytes>, db: &ShaderDb<'_>) -> Self {
        Self {
            file: Some(data.clone().unbind()),
            spans: Spans::of(data.as_bytes(), db),
            big_endian: db.big_endian,
            version: db.version,
            flag: db.flag,
            cache: Mutex::new(Cache::default()),
        }
    }
}

/// `Some(v)` for an answer, `None` for a decline.
fn answer<T>(q: Q<T>) -> Option<T> {
    q.ok()
}

#[pymethods]
impl ShaderDbHandle {
    /// A handle without a file: the queries of a database made in Python (every section read from its lists).
    #[new]
    fn empty() -> Self {
        Self {
            file: None,
            spans: Spans::default(),
            big_endian: false,
            version: 0,
            flag: 0,
            cache: Mutex::new(Cache::default()),
        }
    }

    /// `ShaderDb._parse`'s file read into a handle: the file kept, its records as places in it. A file this layout
    /// does not read is a `ValueError` with the Python reader's message (without its `where: ` prefix).
    #[staticmethod]
    fn open(py: Python<'_>, data: &Bound<'_, PyBytes>, dx: u32, big_endian: bool) -> PyResult<Self> {
        let bytes = data.as_bytes();
        let spans = py
            .detach(|| shaderdb::parse(bytes, dx, big_endian).map(|db| (Spans::of(bytes, &db), db.version, db.flag)))
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            file: Some(data.clone().unbind()),
            big_endian,
            version: spans.1,
            flag: spans.2,
            spans: spans.0,
            cache: Mutex::new(Cache::default()),
        })
    }

    /// The file's version.
    #[getter]
    fn version(&self) -> u32 {
        self.version
    }

    /// The byte after the version.
    #[getter]
    fn flag(&self) -> u8 {
        self.flag
    }

    /// Whether the handle holds a file.
    #[getter]
    fn has_file(&self) -> bool {
        self.file.is_some()
    }

    /// The counts of the file's sections, by attribute name.
    fn counts<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        for (k, s) in RECORDS.iter().enumerate() {
            d.set_item(s, self.spans.records.get(k).map_or(0, Vec::len))?;
        }
        d.set_item("vs", self.spans.shaders.first().map_or(0, Vec::len))?;
        d.set_item("ps", self.spans.shaders.get(1).map_or(0, Vec::len))?;
        for (k, s) in NAMES.iter().enumerate() {
            d.set_item(s, self.spans.names.get(k).map_or(0, Vec::len))?;
        }
        d.set_item("groups", self.spans.groups.len())?;
        Ok(d)
    }

    /// Section `name` made as the Python list `shaderdb_parse` gives (the shaders through `shader_cls`) and set in
    /// the instance dict `d`.
    fn materialize<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        name: &str,
        shader_cls: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyList>> {
        let k = SECTIONS
            .iter()
            .position(|s| *s == name)
            .ok_or_else(|| PyAttributeError::new_err(name.to_string()))?;
        if self.file.is_none() {
            return Err(PyAttributeError::new_err(name.to_string()));
        }
        let data = self.data(py);
        let list = match k {
            3 | 4 => {
                let l: Vec<Shader<'_>> = self
                    .spans
                    .shaders
                    .get(k - 3)
                    .map_or_default(Vec::as_slice)
                    .iter()
                    .map(|s| Shader {
                        guid: s.guid.get(data),
                        code: s.code.get(data),
                        param: s.param,
                        binding: s.binding,
                        regs: s.regs,
                        tail: s.tail.get(data),
                    })
                    .collect();
                shaders(py, &l, shader_cls)?
            }
            8..=10 => {
                let l: Vec<Named<'_>> = self
                    .spans
                    .names
                    .get(k - 8)
                    .map_or_default(Vec::as_slice)
                    .iter()
                    .map(|(n, g)| Named {
                        name: n.get(data).into(),
                        guid: g.get(data),
                    })
                    .collect();
                names(py, &l)?
            }
            11 => {
                let out = PyList::empty(py);
                let be = self.big_endian;
                for (g, n) in &self.spans.groups {
                    let raw = data.get(g.at + 20..g.at + 20 + 2 * n).unwrap_or_default();
                    let idx: Vec<u16> = raw
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|c| {
                            if be {
                                u16::from_be_bytes(*c)
                            } else {
                                u16::from_le_bytes(*c)
                            }
                        })
                        .collect();
                    out.append((PyBytes::new(py, g.get(data)), PyList::new(py, idx)?))?;
                }
                out
            }
            _ => {
                let r = RECORDS.iter().position(|s| *s == name).unwrap_or(0);
                let l: Vec<&[u8]> = self
                    .spans
                    .records
                    .get(r)
                    .map_or_default(Vec::as_slice)
                    .iter()
                    .map(|s| s.get(data))
                    .collect();
                records(py, &l)?
            }
        };
        // a lookup made from the file stays valid for the list made from it (the same records)
        if let Ok(mut cache) = self.cache.try_lock() {
            let src = Src::live(&list);
            match k {
                8..=10 => {
                    if let Some(ix) = cache.names.get_mut(k - 8).and_then(Option::as_mut)
                        && matches!(ix.src, Src::File)
                    {
                        ix.src = src;
                    }
                }
                11 => {
                    if let Some(ix) = cache.groups.as_mut()
                        && matches!(ix.src, Src::File)
                    {
                        ix.src = src;
                    }
                }
                0 => cache.blocks.clear(),
                _ => {}
            }
        }
        d.set_item(name, &list)?;
        Ok(list)
    }

    /// `entry_textures(db, names)`: `{entry name: [(sampler, cube, texture name)]}`, or `None` (the reference runs).
    #[pyo3(signature = (d, big_endian, names=None))]
    fn entry_textures<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        big_endian: bool,
        names: Option<&Bound<'py, PyAny>>,
    ) -> Option<Bound<'py, PyDict>> {
        let want = match names {
            Some(n) => Some(wanted(n)?),
            None => None,
        };
        answer(self.entry_textures_q(py, d, big_endian, want.as_deref()))
    }

    /// `entry_signatures(db, names)`: `{entry name: (sorted constant names, sampler count)}`, or `None`.
    #[pyo3(signature = (d, big_endian, names=None))]
    fn entry_signatures<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        big_endian: bool,
        names: Option<&Bound<'py, PyAny>>,
    ) -> Option<Bound<'py, PyDict>> {
        let want = match names {
            Some(n) => Some(wanted(n)?),
            None => None,
        };
        answer(self.entry_signatures_q(py, d, big_endian, want.as_deref()))
    }

    /// `entry_layouts(db, names)`: `{entry name: {key[32:48] of its programs}}`, or `None`.
    #[pyo3(signature = (d, names=None))]
    fn entry_layouts<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        names: Option<&Bound<'py, PyAny>>,
    ) -> Option<Bound<'py, PyDict>> {
        let want = match names {
            Some(n) => Some(wanted(n)?),
            None => None,
        };
        answer(self.entry_layouts_q(py, d, want.as_deref()))
    }

    /// `coverage(db, meshes)`: `{mesh: entries}` with the meshes as given (plain `str`s), or `None`.
    fn coverage<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        meshes: &Bound<'py, PyAny>,
    ) -> Option<Bound<'py, PyDict>> {
        let mut items = Vec::new();
        let mut keys = Vec::new();
        for m in meshes.try_iter().ok()? {
            let m = m.ok()?;
            let s = m.cast_exact::<PyString>().ok()?;
            // a mesh that is not latin-1 matches no name: it keeps its count of 0
            keys.push(latin1_bytes(s.to_str().ok()?).map_or_else(|| vec![0xff, 0], std::borrow::Cow::into_owned));
            items.push(m);
        }
        let mut guard = self.lock().ok()?;
        let entries = self.names_index(py, d, &mut guard, 0).ok()?;
        let (mut cov, at) = query::Coverage::new(keys.iter().map(Vec::as_slice));
        let low = entries
            .lowered
            .get_or_init(|| entries.names.iter().map(|n| query::lower(n).into_owned()).collect());
        for n in low {
            cov.add_lower(n);
        }
        drop(guard);
        let out = PyDict::new(py);
        for (m, k) in items.iter().zip(at) {
            if !out.contains(m).ok()? {
                out.set_item(m, cov.counts.get(k).copied().unwrap_or(0)).ok()?;
            }
        }
        Some(out)
    }

    /// `Merger.add_donor`'s needed names: `([(name, guid)] of the entries whose name passes needed_test(want), [(name,
    /// guid)] of the mesh-part names whose mesh does)`, in file order; or `None`.
    fn select_needed<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        want: &Bound<'py, PyAny>,
    ) -> Option<(Bound<'py, PyList>, Bound<'py, PyList>)> {
        if !want.is_exact_instance_of::<PyList>() {
            return None;
        }
        let w = wanted(want)?;
        let need = query::Needed::new(w.iter().map(Vec::as_slice));
        let mut guard = self.lock().ok()?;
        let entries = PyList::empty(py);
        let ix = self.names_index(py, d, &mut guard, 0).ok()?;
        for (n, g) in ix.names.iter().zip(&ix.guids) {
            if need.test(n) {
                entries.append((latin1(py, n), PyBytes::new(py, g))).ok()?;
            }
        }
        let parts = PyList::empty(py);
        let ix = self.names_index(py, d, &mut guard, 2).ok()?;
        for (n, g) in ix.names.iter().zip(&ix.guids) {
            if query::part_mesh(n).is_some_and(|m| need.test(&m)) {
                parts.append((latin1(py, n), PyBytes::new(py, g))).ok()?;
            }
        }
        Some((entries, parts))
    }

    /// `ShaderDb.check()`: the problems in the reference's words, or `None`. `big_endian`: the GUIDs' order (`ShaderDb.big_endian`);
    /// `numbers_big_endian`: the program records' (`ShaderDb.e`).
    fn check(
        &self,
        py: Python<'_>,
        d: &Bound<'_, PyDict>,
        big_endian: bool,
        numbers_big_endian: bool,
    ) -> Option<Vec<String>> {
        answer(self.check_q(py, d, big_endian, numbers_big_endian))
    }

    /// `ShaderDb.mesh_names()`: the set of meshes (lower case) the mesh-part names name, or `None`.
    fn mesh_names<'py>(&self, py: Python<'py>, d: &Bound<'py, PyDict>) -> Option<Bound<'py, PySet>> {
        let mut guard = self.lock().ok()?;
        let ix = self.names_index(py, d, &mut guard, 2).ok()?;
        let meshes: BTreeSet<Vec<u8>> = ix.names.iter().filter_map(|n| query::part_mesh(n)).collect();
        drop(guard);
        PySet::new(py, meshes.iter().map(|m| latin1(py, m))).ok()
    }

    /// `ShaderDb.to_bytes()` from each section's current source (the file's records for those never made), or `None`
    /// when an item is one only the Python writer takes.
    fn write<'py>(
        &self,
        py: Python<'py>,
        d: &Bound<'py, PyDict>,
        big_endian: bool,
        version: &Bound<'py, PyAny>,
        flag: &Bound<'py, PyAny>,
    ) -> Option<Bound<'py, PyBytes>> {
        let version: u32 = version.extract().ok()?;
        let flag: u8 = flag.extract().ok()?;
        answer(self.write_q(py, d, big_endian, version, flag))
    }
}

/// `block_textures(block, e)` for a `bytes` block: `[(sampler, cube, name)]`.
#[pyfunction]
pub fn shaderdb_block_textures<'py>(
    py: Python<'py>,
    block: &Bound<'py, PyBytes>,
    big_endian: bool,
) -> PyResult<Bound<'py, PyList>> {
    let out = PyList::empty(py);
    for t in query::block_textures(block.as_bytes(), big_endian) {
        out.append((t.sampler, t.cube, latin1(py, t.name)))?;
    }
    Ok(out)
}

/// `block_constants(block, e)` for a `bytes` block: `[(name, register, components, (4 floats))]`.
#[pyfunction]
pub fn shaderdb_block_constants<'py>(
    py: Python<'py>,
    block: &Bound<'py, PyBytes>,
    big_endian: bool,
) -> PyResult<Bound<'py, PyList>> {
    let out = PyList::empty(py);
    for c in query::block_constants(block.as_bytes(), big_endian) {
        let [a, b, x, y] = c.values.map(f64::from);
        out.append((latin1(py, c.name), c.register, c.components, (a, b, x, y)))?;
    }
    Ok(out)
}

/// `block_engine_textures(block, e)` for a `bytes` block: `[(parameter name, sampler)]`.
#[pyfunction]
pub fn shaderdb_block_engine_textures<'py>(
    py: Python<'py>,
    block: &Bound<'py, PyBytes>,
    big_endian: bool,
) -> PyResult<Bound<'py, PyList>> {
    let out = PyList::empty(py);
    for (n, s) in query::block_engine_textures(block.as_bytes(), big_endian) {
        out.append((latin1(py, n), s))?;
    }
    Ok(out)
}

/// `texture_kind(name)` for a latin-1 `str`, else `None` (the reference lowers other characters).
#[pyfunction]
pub fn shaderdb_texture_kind(name: &Bound<'_, PyAny>) -> Option<&'static str> {
    latin1_of(name).map(|n| query::texture_kind(&n))
}
