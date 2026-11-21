//! Merging shader databases (round SD1's `shaderdb.py merge` port, moved here from bc2-cli in round XF1 so the map
//! export can add the entries a level needs): a base database and donors of the same kind; every donor entry whose
//! name a needed mesh covers and the base lacks is added with its programs, shaders, parameter blocks, fragments and
//! vertex layouts (by content, shared where the base has them), and the mesh-part names of those meshes. The base's
//! records are kept as its bytes; only the new program records are made (their four indices renumbered into the
//! merged file), the programs re-sorted and the entry groups renumbered as the reference does.

#![allow(
    clippy::missing_errors_doc,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::must_use_candidate,
    reason = "moved as it was from bc2-cli's port of the Python reference (round XF1); the errors are MergeError's"
)]

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use super::query::{self, Coverage, Needed};
use super::{Named, Shader, ShaderDb};

/// Why a merge stopped: a message in words (the reference's `ShaderDbError`), or a database whose indices point
/// outside its sections (the Python reference raises there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeError {
    /// A message for the user.
    Msg(String),
    /// A database index out of its section.
    Malformed,
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Msg(m) => f.write_str(m),
            Self::Malformed => f.write_str("a shader database index points outside its section"),
        }
    }
}

impl std::error::Error for MergeError {}

type Result<T> = std::result::Result<T, MergeError>;

/// A latin-1 byte string as a `str`.
#[must_use]
pub fn latin(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// A `str` as latin-1 bytes, or None when it holds a character past U+00FF.
#[must_use]
pub fn to_latin(s: &str) -> Option<Vec<u8>> {
    s.chars().map(|c| u8::try_from(u32::from(c)).ok()).collect()
}

/// Lower-case hex of bytes.
#[must_use]
pub fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

/// `ShaderDb.entry_programs()`: {entry GUID: program indices}, the last group of a GUID kept.
#[must_use]
pub fn entry_programs<'d>(db: &'d ShaderDb<'_>) -> HashMap<&'d [u8], &'d [u16]> {
    db.groups.iter().map(|(g, idx)| (*g, idx.as_slice())).collect()
}

/// A program's four indices (vertex shader, pixel shader, two parameter blocks).
///
/// # Errors
/// The program is not in the database or its record is short.
pub fn program_refs(db: &ShaderDb<'_>, i: usize) -> Result<[usize; 4]> {
    let p = db.programs.get(i).ok_or(MergeError::Malformed)?;
    Ok(query::program_refs(p, db.big_endian)
        .ok_or(MergeError::Malformed)?
        .map(|x| x as usize))
}

/// `coverage(db, meshes)`: per mesh (lower case, as given) its entries.
#[must_use]
pub fn coverage(db: &ShaderDb<'_>, meshes: &[String]) -> Vec<u32> {
    let bytes: Vec<Option<Vec<u8>>> = meshes.iter().map(|m| to_latin(m)).collect();
    // a mesh with a character past latin-1 matches no latin-1 entry: its own counter, never counted
    let keyed: Vec<Vec<u8>> = bytes
        .iter()
        .enumerate()
        .map(|(i, b)| {
            b.clone()
                .unwrap_or_else(|| format!("\u{0}{i}\u{0}not latin-1").into_bytes())
        })
        .collect();
    let (mut c, at) = Coverage::new(keyed.iter().map(Vec::as_slice));
    for e in &db.entries {
        c.add(&e.name);
    }
    at.iter()
        .zip(&bytes)
        .map(|(&k, b)| {
            if b.is_some() {
                c.counts.get(k).copied().unwrap_or(0)
            } else {
                0
            }
        })
        .collect()
}

/// `MAX_PROGRAMS`: entry groups index programs with `u16`.
const MAX_PROGRAMS: usize = 0x10000;
const ZERO_GUID: [u8; 16] = [0; 16];

/// A program while merging: the base's (its index there) or a new one (its index among the new).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ProgRef {
    Old(usize),
    New(usize),
}

/// `k[a:b]` as Python slices bytes.
pub fn field(k: &[u8], a: usize, b: usize) -> &[u8] {
    k.get(a.min(k.len())..b.min(k.len())).unwrap_or_default()
}

/// A shader by content: its GUID, code, tail and its three tables' records.
pub type ShaderKey<'a> = (&'a [u8], &'a [u8], &'a [u8], &'a [u8], &'a [u8], &'a [u8]);

/// `Merger._shader_key(db, s)`: a shader by content (its tables resolved).
pub fn shader_key<'a>(db: &ShaderDb<'a>, s: &Shader<'a>) -> Result<ShaderKey<'a>> {
    Ok((
        s.guid,
        s.code,
        s.tail,
        *db.params.get(s.param as usize).ok_or(MergeError::Malformed)?,
        *db.bindings.get(s.binding as usize).ok_or(MergeError::Malformed)?,
        *db.regs.get(s.regs as usize).ok_or(MergeError::Malformed)?,
    ))
}

/// What a donor's file says about itself (`_same_kind`, the report's label).
pub struct Kind {
    /// 9, 10 or 11.
    pub dx: u32,
    /// Xbox 360 byte order.
    pub big: bool,
    /// The program key's size.
    pub key_size: usize,
    /// The file's version.
    pub version: u32,
}

fn platform(k: &Kind) -> String {
    if k.big {
        "Xbox 360".into()
    } else {
        format!("Dx{}", k.dx)
    }
}

/// `Merger(base, need, all_names)` and its state.
pub struct Merger<'a> {
    out: ShaderDb<'a>,
    kind: Kind,
    need: Vec<String>,
    all_names: bool,
    in_base: HashSet<String>,
    /// Mesh-part names a donor has with another vertex layout (the existing one kept).
    pub part_conflicts: Vec<String>,
    /// Per donor: entries and mesh-part names taken.
    pub report: Vec<String>,
    /// The entries added and the donor each came from.
    pub added: Vec<(Vec<u8>, String)>,
    /// Donor programs the file already had (same GUID and content).
    pub shared_programs: usize,
    /// Donor programs whose GUID the file has with other content (the existing one kept).
    pub conflicts: Vec<String>,
    param_ix: HashMap<&'a [u8], usize>,
    binding_ix: HashMap<&'a [u8], usize>,
    regs_ix: HashMap<&'a [u8], usize>,
    vs_ix: HashMap<ShaderKey<'a>, usize>,
    ps_ix: HashMap<ShaderKey<'a>, usize>,
    prog_ix: HashMap<&'a [u8], ProgRef>,
    prog_body: HashMap<&'a [u8], (&'a [u8], &'a [u8])>,
    entry_names: HashSet<Vec<u8>>,
    frag_g: HashSet<&'a [u8]>,
    lay_g: HashSet<&'a [u8]>,
    part_names: HashMap<Vec<u8>, &'a [u8]>,
    /// `self.groups`: {entry GUID: programs}, in insertion order
    group_keys: Vec<&'a [u8]>,
    groups: HashMap<&'a [u8], Vec<ProgRef>>,
    new_programs: Vec<(Vec<u8>, &'a [u8])>,
}

fn first_index<T: std::hash::Hash + Eq + Copy>(items: impl Iterator<Item = T>) -> HashMap<T, usize> {
    let mut ix = HashMap::new();
    for (i, it) in items.enumerate() {
        ix.entry(it).or_insert(i);
    }
    ix
}

impl<'a> Merger<'a> {
    /// `Merger(base, need, all_names)`: `need` the mesh names (lower case) whose entries the merge may add.
    pub fn new(base: ShaderDb<'a>, kind: Kind, need: Vec<String>, all_names: bool) -> Result<Self> {
        let cover = coverage(&base, &need);
        let in_base: HashSet<String> = need
            .iter()
            .zip(&cover)
            .filter(|(_, c)| **c > 0)
            .map(|(n, _)| n.clone())
            .collect();
        let b = &base;
        let vs_keys: Vec<ShaderKey<'a>> = b.vs.iter().map(|s| shader_key(b, s)).collect::<Result<_>>()?;
        let ps_keys: Vec<ShaderKey<'a>> = b.ps.iter().map(|s| shader_key(b, s)).collect::<Result<_>>()?;
        let mut prog_ix = HashMap::new();
        for (i, p) in b.programs.iter().enumerate() {
            prog_ix.insert(field(p, 0, 16), ProgRef::Old(i));
        }
        let mut prog_body = HashMap::new();
        for (p, k) in b.programs.iter().zip(&b.keys) {
            prog_body.insert(field(p, 0, 16), (field(p, 0, 0x20), *k));
        }
        let mut group_keys = Vec::new();
        let mut groups: HashMap<&'a [u8], Vec<ProgRef>> = HashMap::new();
        for (g, idx) in &b.groups {
            if !groups.contains_key(g) {
                group_keys.push(*g);
            }
            groups.insert(*g, idx.iter().map(|&i| ProgRef::Old(usize::from(i))).collect());
        }
        Ok(Self {
            param_ix: first_index(b.params.iter().copied()),
            binding_ix: first_index(b.bindings.iter().copied()),
            regs_ix: first_index(b.regs.iter().copied()),
            vs_ix: first_index(vs_keys.into_iter()),
            ps_ix: first_index(ps_keys.into_iter()),
            prog_ix,
            prog_body,
            entry_names: b.entries.iter().map(|n| n.name.to_vec()).collect(),
            frag_g: b.fragments.iter().map(|n| n.guid).collect(),
            lay_g: b.layouts.iter().map(|r| field(r, 0, 16)).collect(),
            part_names: b.parts.iter().map(|n| (n.name.to_vec(), n.guid)).collect(),
            group_keys,
            groups,
            new_programs: Vec::new(),
            in_base,
            part_conflicts: Vec::new(),
            report: Vec::new(),
            added: Vec::new(),
            shared_programs: 0,
            conflicts: Vec::new(),
            need,
            all_names,
            kind,
            out: base,
        })
    }

    fn table(rec: &'a [u8], lst: &mut Vec<&'a [u8]>, ix: &mut HashMap<&'a [u8], usize>) -> usize {
        *ix.entry(rec).or_insert_with(|| {
            lst.push(rec);
            lst.len() - 1
        })
    }

    /// `_shader(donor, s, lst, ix)` for the vertex (`vs`) or pixel shaders.
    fn shader(&mut self, donor: &ShaderDb<'a>, s: &Shader<'a>, vs: bool) -> Result<usize> {
        let key = shader_key(donor, s)?;
        let found = if vs { self.vs_ix.get(&key) } else { self.ps_ix.get(&key) };
        if let Some(&i) = found {
            return Ok(i);
        }
        let o = &mut self.out;
        let param = Self::table(key.3, &mut o.params, &mut self.param_ix);
        let binding = Self::table(key.4, &mut o.bindings, &mut self.binding_ix);
        let regs = Self::table(key.5, &mut o.regs, &mut self.regs_ix);
        let new = Shader {
            guid: s.guid,
            code: s.code,
            param: u32::try_from(param).map_err(|_| MergeError::Malformed)?,
            binding: u32::try_from(binding).map_err(|_| MergeError::Malformed)?,
            regs: u32::try_from(regs).map_err(|_| MergeError::Malformed)?,
            tail: s.tail,
        };
        let (lst, ix) = if vs {
            (&mut o.vs, &mut self.vs_ix)
        } else {
            (&mut o.ps, &mut self.ps_ix)
        };
        let i = lst.len();
        ix.insert(key, i);
        lst.push(new);
        Ok(i)
    }

    fn same_kind(&self, k: &Kind, label: &str) -> Result<()> {
        let o = &self.kind;
        if (k.dx, k.big, k.key_size) != (o.dx, o.big, o.key_size) {
            return Err(MergeError::Msg(format!(
                "{label} is a {} database (version {}) and the base is {} (version {}): merge files of the same kind",
                platform(k),
                k.version,
                platform(o),
                o.version
            )));
        }
        Ok(())
    }

    fn layout(&mut self, lg: &'a [u8], layouts: &HashMap<&'a [u8], &'a [u8]>, label: &str) -> Result<()> {
        if self.lay_g.contains(lg) {
            return Ok(());
        }
        let Some(r) = layouts.get(lg) else {
            return Err(MergeError::Msg(format!(
                "{label}: a vertex layout it uses is not in the file"
            )));
        };
        self.out.layouts.push(r);
        self.lay_g.insert(lg);
        Ok(())
    }

    /// `add_donor(donor, label)`.
    pub fn add_donor(&mut self, donor: &ShaderDb<'a>, kind: &Kind, label: &str) -> Result<()> {
        self.same_kind(kind, label)?;
        let groups = entry_programs(donor);
        let mut frag_names: HashMap<&'a [u8], &Named<'a>> = HashMap::new();
        for n in &donor.fragments {
            frag_names.insert(n.guid, n);
        }
        let mut layouts: HashMap<&'a [u8], &'a [u8]> = HashMap::new();
        for r in &donor.layouts {
            layouts.insert(field(r, 0, 16), *r);
        }
        let want: Vec<Vec<u8>> = self
            .need
            .iter()
            .filter(|n| self.all_names || !self.in_base.contains(*n))
            .filter_map(|n| to_latin(n))
            .collect();
        let mine = Needed::new(want.iter().map(Vec::as_slice));
        let be = self.out.big_endian;
        let mut took = 0;
        for e in &donor.entries {
            if !mine.test(&e.name) || self.entry_names.contains(&*e.name) {
                continue;
            }
            let mut progs = Vec::new();
            for &pi in groups.get(e.guid).copied().unwrap_or_default() {
                let pi = usize::from(pi);
                let prog: &'a [u8] = donor.programs.get(pi).ok_or(MergeError::Malformed)?;
                let key: &'a [u8] = donor.keys.get(pi).ok_or(MergeError::Malformed)?;
                let pg = field(prog, 0, 16);
                if let Some(&r) = self.prog_ix.get(pg) {
                    if self.prog_body.get(pg) == Some(&(field(prog, 0, 0x20), key)) {
                        self.shared_programs += 1;
                    } else {
                        self.conflicts.push(format!(
                            "{}: program {} is in the merged file with other content (that one kept)",
                            latin(&e.name),
                            hex(pg)
                        ));
                    }
                    progs.push(r);
                    continue;
                }
                let [vi, fi, ai, bi] = program_refs(donor, pi)?;
                let nv = self.shader(donor, donor.vs.get(vi).ok_or(MergeError::Malformed)?, true)?;
                let nf = self.shader(donor, donor.ps.get(fi).ok_or(MergeError::Malformed)?, false)?;
                let pa = *donor.params.get(ai).ok_or(MergeError::Malformed)?;
                let pb = *donor.params.get(bi).ok_or(MergeError::Malformed)?;
                let na = Self::table(pa, &mut self.out.params, &mut self.param_ix);
                let nb = Self::table(pb, &mut self.out.params, &mut self.param_ix);
                let mut new = field(prog, 0, 0x20).to_vec();
                for ix in [nv, nf, na, nb] {
                    let ix = u32::try_from(ix).map_err(|_| MergeError::Malformed)?;
                    new.extend(if be { ix.to_be_bytes() } else { ix.to_le_bytes() });
                }
                let made = ProgRef::New(self.new_programs.len());
                self.prog_ix.insert(pg, made);
                self.prog_body.insert(pg, (field(prog, 0, 0x20), key));
                progs.push(made);
                self.new_programs.push((new, key));
                let (fg, lg) = (field(key, 16, 32), field(key, 32, 48));
                if fg != ZERO_GUID && !self.frag_g.contains(fg) {
                    let Some(n) = frag_names.get(fg) else {
                        return Err(MergeError::Msg(format!(
                            "{label}: a program of {} names a fragment the file has no name for",
                            latin(&e.name)
                        )));
                    };
                    self.out.fragments.push((*n).clone());
                    self.frag_g.insert(fg);
                }
                self.layout(lg, &layouts, label)?;
            }
            self.out.entries.push(e.clone());
            self.entry_names.insert(e.name.to_vec());
            if !self.groups.contains_key(e.guid) {
                self.group_keys.push(e.guid);
            }
            self.groups.insert(e.guid, progs);
            self.added.push((e.name.to_vec(), label.to_owned()));
            took += 1;
        }
        let mut parts = 0;
        for p in &donor.parts {
            let Some(pm) = query::part_mesh(&p.name) else { continue };
            if !mine.test(&pm) {
                continue;
            }
            if let Some(&g) = self.part_names.get(&*p.name) {
                if g != p.guid {
                    self.part_conflicts.push(format!(
                        "{}: {label} has another vertex layout (the existing one kept)",
                        latin(&p.name)
                    ));
                }
                continue;
            }
            self.layout(p.guid, &layouts, label)?;
            self.out.parts.push(p.clone());
            self.part_names.insert(p.name.to_vec(), p.guid);
            parts += 1;
        }
        self.report
            .push(format!("{label}: {took} entries, {parts} mesh-part names"));
        Ok(())
    }

    /// `finish()`: the programs re-sorted by GUID (the PC byte order), the entry groups renumbered and sorted. The
    /// merged file's bytes.
    pub fn finish(&mut self) -> Result<Vec<u8>> {
        let o = &self.out;
        let total = o.programs.len() + self.new_programs.len();
        if total >= MAX_PROGRAMS {
            return Err(MergeError::Msg(format!(
                "the merged file would have {total} programs; entry groups index at most {}: take fewer donor entries",
                MAX_PROGRAMS - 1
            )));
        }
        let be = o.big_endian;
        let pc = |g: &[u8]| -> Vec<u8> { if be { query::swap_guid(g) } else { g.to_vec() } };
        let mut allp: Vec<(Cow<'_, [u8]>, &[u8], ProgRef)> = o
            .programs
            .iter()
            .zip(&o.keys)
            .enumerate()
            .map(|(i, (p, k))| (Cow::Borrowed(*p), *k, ProgRef::Old(i)))
            .collect();
        allp.extend(
            self.new_programs
                .iter()
                .enumerate()
                .map(|(i, (p, k))| (Cow::Borrowed(p.as_slice()), *k, ProgRef::New(i))),
        );
        allp.sort_by_cached_key(|t| pc(field(&t.0, 0, 16)));
        let place: HashMap<ProgRef, usize> = allp.iter().enumerate().map(|(i, t)| (t.2, i)).collect();
        let mut groups: Vec<(&[u8], Vec<u16>)> = Vec::new();
        for g in &self.group_keys {
            let mut idx: Vec<usize> = Vec::new();
            for r in self.groups.get(g).ok_or(MergeError::Malformed)? {
                idx.push(*place.get(r).ok_or(MergeError::Malformed)?);
            }
            idx.sort_unstable();
            groups.push((g, idx.into_iter().map(|i| i as u16).collect()));
        }
        groups.sort_by_cached_key(|t| pc(t.0));
        let programs: Vec<&[u8]> = allp.iter().map(|t| &*t.0).collect();
        let keys: Vec<&[u8]> = allp.iter().map(|t| t.1).collect();
        let db = ShaderDb {
            big_endian: be,
            version: o.version,
            flag: o.flag,
            params: o.params.clone(),
            bindings: o.bindings.clone(),
            regs: o.regs.clone(),
            vs: o.vs.clone(),
            ps: o.ps.clone(),
            programs,
            keys,
            layouts: o.layouts.clone(),
            entries: o.entries.clone(),
            fragments: o.fragments.clone(),
            parts: o.parts.clone(),
            groups,
        };
        Ok(super::write(&db))
    }
}
