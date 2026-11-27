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

/// How a merge takes a donor's record into the merged file: borrowed as it is (a donor that lives as long as the
/// file), or copied into the merge's store (a donor read for one call).
type Take<'t, 'd, 'a> = &'t dyn Fn(&'d [u8]) -> &'a [u8];

/// A record of a section, or `Malformed`.
fn rec<'a>(list: &[&'a [u8]], i: usize) -> Result<&'a [u8]> {
    list.get(i).copied().ok_or(MergeError::Malformed)
}

/// A cloned program's id: MD5 of the new entry's name and the donor program's id (a program id must not repeat).
fn program_id(name: &[u8], prog: &[u8]) -> [u8; 16] {
    let mut v = name.to_vec();
    v.extend_from_slice(field(prog, 0, 16));
    query::md5(&v)
}

/// `MAX_PROGRAMS`: entry groups index programs with `u16`.
const MAX_PROGRAMS: usize = 0x10000;
const ZERO_GUID: [u8; 16] = [0; 16];
/// A texture name's zero-padded field in a parameter block (`TEXTURE_FIELD`).
const TEXTURE_FIELD: usize = 0x80;

/// Where a merge keeps the records it makes (`clone_entry`'s new program ids, keys and entry GUID, its retextured
/// parameter blocks; `add_part`'s layout GUID): they must live as long as the merged file's other
/// records. The Python binding keeps them in the merger's own arena.
pub trait Store<'a> {
    /// A copy of `b` that lives for `'a`.
    fn keep(&self, b: &[u8]) -> &'a [u8];
}

/// A [`Store`] that owns its arena (round OR11: a merge in a crate with no arena of its own - Project OPFOR's Mark V
/// stage, `bc2_install::onslaught::markv`). `&ArenaStore` is the store; the merged file's records live as long as it.
#[derive(Default)]
pub struct ArenaStore(bumpalo::Bump);

impl ArenaStore {
    /// An empty arena.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a> Store<'a> for &'a ArenaStore {
    fn keep(&self, b: &[u8]) -> &'a [u8] {
        self.0.alloc_slice_copy(b)
    }
}

/// `retexture(block, textures, where, hits)`: a parameter block with texture names replaced in place. Each old name
/// (a whole field's text, starting the field: the byte before it a zero, or the block's start; NUL-terminated)
/// becomes the new one, padded with zeros to the same span (the old name, its terminator and the zeros after it,
/// within the field's 0x80 bytes). The renames are simultaneous: every field is matched against the block's
/// original names (`{A: B, B: A}` swaps them). `hits` counts each old name's places, only once every rename fits.
///
/// # Errors
/// A new name longer than the span of the old one it replaces (the first such in `textures`' order).
pub fn retexture<'b, S: std::hash::BuildHasher>(
    block: &'b [u8],
    textures: &[(Vec<u8>, Vec<u8>)],
    where_: &str,
    hits: Option<&mut HashMap<Vec<u8>, usize, S>>,
) -> Result<Cow<'b, [u8]>> {
    if textures.is_empty() {
        return Ok(Cow::Borrowed(block));
    }
    let mut found: Vec<(usize, usize, usize)> = Vec::new();
    for (k, (old, new)) in textures.iter().enumerate() {
        let mut pat = old.clone();
        pat.push(0);
        let mut at = 0;
        while let Some(i) = find(block, &pat, at) {
            at = i + 1;
            if i > 0 && block.get(i - 1) != Some(&0) {
                continue; // the tail of a longer name: not this texture
            }
            let mut end = i + old.len();
            while end < block.len() && end < i + TEXTURE_FIELD && block.get(end) == Some(&0) {
                end += 1;
            }
            if new.len() + 1 > end - i {
                return Err(MergeError::Msg(format!(
                    "{where_}: texture name {} longer than the field of {}",
                    latin(new),
                    latin(old)
                )));
            }
            found.push((i, end, k));
        }
    }
    let mut out = block.to_vec();
    for &(i, end, k) in &found {
        let new = &textures.get(k).ok_or(MergeError::Malformed)?.1;
        let span = out.get_mut(i..end).ok_or(MergeError::Malformed)?;
        span.fill(0);
        span.get_mut(..new.len())
            .ok_or(MergeError::Malformed)?
            .copy_from_slice(new);
    }
    if let Some(hits) = hits {
        for &(_, _, k) in &found {
            if let Some((old, _)) = textures.get(k) {
                *hits.entry(old.clone()).or_insert(0) += 1;
            }
        }
    }
    Ok(Cow::Owned(out))
}

/// Python's `bytes.find(sub, start)`: the first place of `pat` at or after `start` (an empty `pat` is found at
/// `start` while `start` is within the block, its end included).
fn find(hay: &[u8], pat: &[u8], start: usize) -> Option<usize> {
    if start > hay.len() {
        return None;
    }
    if pat.is_empty() {
        return Some(start);
    }
    hay.get(start..)?
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|p| p + start)
}

/// A latin-1 name's GUID in a file of this byte order (`ShaderDb.name_guid`): MD5 of the name, its fields swapped
/// for an Xbox 360 file.
#[must_use]
pub fn name_guid(name: &[u8], big_endian: bool) -> Vec<u8> {
    let g = query::md5(name);
    if big_endian { query::swap_guid(&g) } else { g.to_vec() }
}

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
#[derive(Debug, Clone, PartialEq, Eq)]
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

    /// `_table(rec, lst, ix)`: the index of a record of a table (parameter blocks, bindings, registers) by content,
    /// the record taken (`take`) and appended when the file lacks it.
    fn table<'d>(
        rec: &'d [u8],
        lst: &mut Vec<&'a [u8]>,
        ix: &mut HashMap<&'a [u8], usize>,
        take: Take<'_, 'd, 'a>,
    ) -> usize {
        if let Some(&i) = ix.get(rec) {
            return i;
        }
        let r = take(rec);
        lst.push(r);
        ix.insert(r, lst.len() - 1);
        lst.len() - 1
    }

    /// `_shader(donor, s, lst, ix)` for the vertex (`vs`) or pixel shaders.
    fn shader<'d>(&mut self, donor: &ShaderDb<'d>, s: &Shader<'d>, vs: bool, take: Take<'_, 'd, 'a>) -> Result<usize>
    where
        'a: 'd,
    {
        let key = shader_key(donor, s)?;
        {
            let ix: &HashMap<ShaderKey<'d>, usize> = if vs { &self.vs_ix } else { &self.ps_ix };
            if let Some(&i) = ix.get(&key) {
                return Ok(i);
            }
        }
        let o = &mut self.out;
        let param = Self::table(key.3, &mut o.params, &mut self.param_ix, take);
        let binding = Self::table(key.4, &mut o.bindings, &mut self.binding_ix, take);
        let regs = Self::table(key.5, &mut o.regs, &mut self.regs_ix, take);
        let new = Shader {
            guid: take(s.guid),
            code: take(s.code),
            param: u32::try_from(param).map_err(|_| MergeError::Malformed)?,
            binding: u32::try_from(binding).map_err(|_| MergeError::Malformed)?,
            regs: u32::try_from(regs).map_err(|_| MergeError::Malformed)?,
            tail: take(s.tail),
        };
        let kept: ShaderKey<'a> = (
            new.guid,
            new.code,
            new.tail,
            *o.params.get(param).ok_or(MergeError::Malformed)?,
            *o.bindings.get(binding).ok_or(MergeError::Malformed)?,
            *o.regs.get(regs).ok_or(MergeError::Malformed)?,
        );
        let (lst, ix) = if vs {
            (&mut o.vs, &mut self.vs_ix)
        } else {
            (&mut o.ps, &mut self.ps_ix)
        };
        let i = lst.len();
        ix.insert(kept, i);
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

    /// `_layout(lg, layouts, label)`: a vertex layout the file lacks, taken from the donor's.
    fn layout<'d>(
        &mut self,
        lg: &[u8],
        layouts: &HashMap<&'d [u8], &'d [u8]>,
        label: &str,
        take: Take<'_, 'd, 'a>,
    ) -> Result<()> {
        if self.lay_g.contains(lg) {
            return Ok(());
        }
        let Some(r) = layouts.get(lg) else {
            return Err(MergeError::Msg(format!(
                "{label}: a vertex layout it uses is not in the file"
            )));
        };
        let r = take(r);
        self.out.layouts.push(r);
        self.lay_g.insert(field(r, 0, 16));
        Ok(())
    }

    /// `add_donor(donor, label)`: a donor that lives as long as the merged file (its records are borrowed).
    pub fn add_donor(&mut self, donor: &ShaderDb<'a>, kind: &Kind, label: &str) -> Result<()> {
        self.add_donor_with(donor, kind, label, &|b| b)
    }

    /// `add_donor(donor, label)` for a donor read for this call only: what the merge takes from it is copied into
    /// the store.
    pub fn add_donor_kept(
        &mut self,
        store: &dyn Store<'a>,
        donor: &ShaderDb<'_>,
        kind: &Kind,
        label: &str,
    ) -> Result<()> {
        self.add_donor_with(donor, kind, label, &|b| store.keep(b))
    }

    fn add_donor_with<'d>(
        &mut self,
        donor: &ShaderDb<'d>,
        kind: &Kind,
        label: &str,
        take: Take<'_, 'd, 'a>,
    ) -> Result<()>
    where
        'a: 'd,
    {
        self.same_kind(kind, label)?;
        let groups = entry_programs(donor);
        let mut frag_names: HashMap<&'d [u8], &Named<'d>> = HashMap::new();
        for n in &donor.fragments {
            frag_names.insert(n.guid, n);
        }
        let mut layouts: HashMap<&'d [u8], &'d [u8]> = HashMap::new();
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
                let prog: &'d [u8] = donor.programs.get(pi).ok_or(MergeError::Malformed)?;
                let key: &'d [u8] = donor.keys.get(pi).ok_or(MergeError::Malformed)?;
                let pg = field(prog, 0, 16);
                if let Some(&r) = self.prog_ix.get(pg) {
                    let same = self
                        .prog_body
                        .get(pg)
                        .is_some_and(|(h, k)| *h == field(prog, 0, 0x20) && *k == key);
                    if same {
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
                let nv = self.shader(donor, donor.vs.get(vi).ok_or(MergeError::Malformed)?, true, take)?;
                let nf = self.shader(donor, donor.ps.get(fi).ok_or(MergeError::Malformed)?, false, take)?;
                let pa = rec(&donor.params, ai)?;
                let pb = rec(&donor.params, bi)?;
                let na = Self::table(pa, &mut self.out.params, &mut self.param_ix, take);
                let nb = Self::table(pb, &mut self.out.params, &mut self.param_ix, take);
                let mut new = field(prog, 0, 0x20).to_vec();
                for ix in [nv, nf, na, nb] {
                    let ix = u32::try_from(ix).map_err(|_| MergeError::Malformed)?;
                    new.extend(if be { ix.to_be_bytes() } else { ix.to_le_bytes() });
                }
                let head = take(field(prog, 0, 0x20));
                let kept_key = take(key);
                let made = ProgRef::New(self.new_programs.len());
                self.prog_ix.insert(field(head, 0, 16), made);
                self.prog_body.insert(field(head, 0, 16), (head, kept_key));
                progs.push(made);
                self.new_programs.push((new, kept_key));
                let (fg, lg) = (field(key, 16, 32), field(key, 32, 48));
                if fg != ZERO_GUID && !self.frag_g.contains(fg) {
                    let Some(n) = frag_names.get(fg) else {
                        return Err(MergeError::Msg(format!(
                            "{label}: a program of {} names a fragment the file has no name for",
                            latin(&e.name)
                        )));
                    };
                    let g = take(n.guid);
                    self.out.fragments.push(Named {
                        name: Cow::Owned(n.name.to_vec()),
                        guid: g,
                    });
                    self.frag_g.insert(g);
                }
                self.layout(lg, &layouts, label, take)?;
            }
            let g = take(e.guid);
            self.out.entries.push(Named {
                name: Cow::Owned(e.name.to_vec()),
                guid: g,
            });
            self.entry_names.insert(e.name.to_vec());
            if !self.groups.contains_key(g) {
                self.group_keys.push(g);
            }
            self.groups.insert(g, progs);
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
            self.layout(p.guid, &layouts, label, take)?;
            let g = take(p.guid);
            self.out.parts.push(Named {
                name: Cow::Owned(p.name.to_vec()),
                guid: g,
            });
            self.part_names.insert(p.name.to_vec(), g);
            parts += 1;
        }
        self.report
            .push(format!("{label}: {took} entries, {parts} mesh-part names"));
        Ok(())
    }

    /// The database as merged so far, before `finish`: the base's records with the new shaders, tables, layouts,
    /// fragments, entries and part names appended, its programs, keys and entry groups still the base's (what the
    /// Python reference's `out` held between calls, and what a clone from the base itself reads).
    #[must_use]
    pub fn current(&self) -> &ShaderDb<'a> {
        &self.out
    }

    /// The file's kind (the base's).
    #[must_use]
    pub fn kind(&self) -> &Kind {
        &self.kind
    }

    /// Is this entry name in the merged file (`name in Merger.entry_names`)?
    #[must_use]
    pub fn has_entry(&self, name: &[u8]) -> bool {
        self.entry_names.contains(name)
    }

    /// `clone_entry(donor, entry, new_name, textures, label)` (round OR3a): adds the donor's entry `entry` again
    /// under `new_name` - its programs with new ids (MD5 of the new name and the old id), their keys naming the new
    /// entry, and copies of their parameter blocks with texture names replaced (`textures`: `[(old, new)]`, in the
    /// caller's order, see [`retexture`]). The shaders, tables, fragments and vertex layouts are shared as
    /// `add_donor` shares them; what is taken from the donor and every record made is copied into the store.
    /// Returns the number of programs.
    ///
    /// Atomic: everything that can refuse the clone (the entry, its programs' indices and new ids, the texture
    /// renames, its fragments' names, its vertex layouts) is checked before anything is added, so an error leaves
    /// the merger as it was. The messages are the Python reference's.
    ///
    /// # Errors
    /// The refusals above as [`MergeError::Msg`]; a name or texture name that is not latin-1.
    #[expect(
        clippy::too_many_arguments,
        reason = "the Python reference's parameters, plus the kind and the store"
    )]
    pub fn clone_entry(
        &mut self,
        store: &dyn Store<'a>,
        donor: &ShaderDb<'_>,
        donor_kind: &Kind,
        entry: &str,
        new_name: &str,
        textures: &[(String, String)],
        label: &str,
    ) -> Result<usize> {
        self.clone_entry_with(store, donor, donor_kind, entry, new_name, textures, label, &|b| {
            store.keep(b)
        })
    }

    /// `clone_entry` with the merged file itself as the donor (the Python reference's `clone_entry(base, ...)`): the
    /// donor is the file as merged so far ([`Merger::current`]: entries added earlier are there, their programs not
    /// yet - they join the file at `finish`).
    ///
    /// # Errors
    /// As [`Merger::clone_entry`].
    pub fn clone_entry_from_self(
        &mut self,
        store: &dyn Store<'a>,
        entry: &str,
        new_name: &str,
        textures: &[(String, String)],
        label: &str,
    ) -> Result<usize> {
        let view = self.out.clone();
        let kind = self.kind.clone();
        self.clone_entry_with(store, &view, &kind, entry, new_name, textures, label, &|b| b)
    }

    #[expect(
        clippy::too_many_arguments,
        clippy::too_many_lines,
        clippy::many_single_char_names,
        reason = "the Python reference's parameters (plus the kind, the store and the copier) and its names (v, f, a, b)"
    )]
    fn clone_entry_with<'d>(
        &mut self,
        store: &dyn Store<'a>,
        donor: &ShaderDb<'d>,
        donor_kind: &Kind,
        entry: &str,
        new_name: &str,
        textures: &[(String, String)],
        label: &str,
        take: Take<'_, 'd, 'a>,
    ) -> Result<usize>
    where
        'a: 'd,
    {
        self.same_kind(donor_kind, label)?;
        let Some(new_b) = to_latin(new_name) else {
            return Err(MergeError::Msg(format!("{new_name}: the name is not latin-1")));
        };
        if self.entry_names.contains(&new_b) {
            return Err(MergeError::Msg(format!(
                "{new_name}: the database has this entry already"
            )));
        }
        let groups = entry_programs(donor);
        // dict(donor.entries).get(entry): the last entry of that name
        let guid_old =
            to_latin(entry).and_then(|eb| donor.entries.iter().rev().find(|e| *e.name == *eb).map(|e| e.guid));
        let Some(guid_old) = guid_old else {
            return Err(MergeError::Msg(format!("{label} has no entry {entry}")));
        };
        let be = self.out.big_endian;
        let new_guid = name_guid(&new_b, be);
        if self.groups.contains_key(new_guid.as_slice()) {
            return Err(MergeError::Msg(format!(
                "{new_name}: the database has an entry group with this name's id already"
            )));
        }
        let mut frag_names: HashMap<&'d [u8], &Named<'d>> = HashMap::new();
        for n in &donor.fragments {
            frag_names.insert(n.guid, n);
        }
        let mut layouts: HashMap<&'d [u8], &'d [u8]> = HashMap::new();
        for r in &donor.layouts {
            layouts.insert(field(r, 0, 16), *r);
        }
        let mut renames: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(textures.len());
        for (old, new) in textures {
            match (to_latin(old), to_latin(new)) {
                (Some(o), Some(n)) => renames.push((o, n)),
                _ => {
                    return Err(MergeError::Msg(format!(
                        "{new_name}: a texture name is not latin-1 ({old} -> {new})"
                    )));
                }
            }
        }
        let progs_old: &[u16] = groups.get(guid_old).copied().unwrap_or_default();
        // every program whole, every texture to rename one the entry uses: checked before anything is added
        let mut seen: HashMap<Vec<u8>, usize> = HashMap::new();
        let mut ids: HashSet<[u8; 16]> = HashSet::new();
        for &pi in progs_old {
            let pi = usize::from(pi);
            let where_ = format!("{new_name}: {label}:{entry} program {pi}");
            if pi >= donor.programs.len().min(donor.keys.len()) {
                return Err(MergeError::Msg(format!("{where_}: not in the file")));
            }
            let (prog, key) = (rec(&donor.programs, pi)?, rec(&donor.keys, pi)?);
            if prog.len() < 0x30 || key.len() < 48 {
                return Err(MergeError::Msg(format!("{where_}: a short program or key record")));
            }
            let [v, f, a, b] = program_refs(donor, pi)?;
            if v >= donor.vs.len() || f >= donor.ps.len() || a >= donor.params.len() || b >= donor.params.len() {
                return Err(MergeError::Msg(format!(
                    "{where_}: a shader or parameter block index past its table"
                )));
            }
            for s in [donor.vs.get(v), donor.ps.get(f)] {
                let s = s.ok_or(MergeError::Malformed)?;
                if s.param as usize >= donor.params.len()
                    || s.binding as usize >= donor.bindings.len()
                    || s.regs as usize >= donor.regs.len()
                {
                    return Err(MergeError::Msg(format!(
                        "{where_}: a shader's table index past its table"
                    )));
                }
            }
            let pg = program_id(&new_b, prog);
            if self.prog_ix.contains_key(pg.as_slice()) || !ids.insert(pg) {
                return Err(MergeError::Msg(format!("{new_name}: program id {} exists", hex(&pg))));
            }
            let (fg, lg) = (field(key, 16, 32), field(key, 32, 48));
            if fg != ZERO_GUID && !self.frag_g.contains(fg) && !frag_names.contains_key(fg) {
                return Err(MergeError::Msg(format!(
                    "{where_}: names a fragment the file has no name for"
                )));
            }
            if !self.lay_g.contains(lg) && !layouts.contains_key(lg) {
                return Err(MergeError::Msg(format!(
                    "{label}: a vertex layout it uses is not in the file ({where_})"
                )));
            }
            retexture(rec(&donor.params, a)?, &renames, new_name, Some(&mut seen))?;
            retexture(rec(&donor.params, b)?, &renames, new_name, Some(&mut seen))?;
        }
        let unused: Vec<String> = renames
            .iter()
            .filter(|(k, _)| seen.get(k).copied().unwrap_or(0) == 0)
            .map(|(k, _)| latin(k))
            .collect();
        if !unused.is_empty() {
            return Err(MergeError::Msg(format!(
                "{new_name}: {label}:{entry} uses no texture named {} (names are matched exactly, case included)",
                unused.join(", ")
            )));
        }
        // checked: now added
        let new_guid: &'a [u8] = store.keep(&new_guid);
        let mut progs = Vec::with_capacity(progs_old.len());
        for &pi in progs_old {
            let pi = usize::from(pi);
            let (prog, key) = (rec(&donor.programs, pi)?, rec(&donor.keys, pi)?);
            let pg = program_id(&new_b, prog);
            let [v, f, a, b] = program_refs(donor, pi)?;
            let nv = self.shader(donor, donor.vs.get(v).ok_or(MergeError::Malformed)?, true, take)?;
            let nf = self.shader(donor, donor.ps.get(f).ok_or(MergeError::Malformed)?, false, take)?;
            let na = self.block(store, rec(&donor.params, a)?, &renames, new_name, take)?;
            let nb = self.block(store, rec(&donor.params, b)?, &renames, new_name, take)?;
            let mut new = pg.to_vec();
            new.extend_from_slice(field(prog, 16, 0x20));
            for ix in [nv, nf, na, nb] {
                let ix = u32::try_from(ix).map_err(|_| MergeError::Malformed)?;
                new.extend(if be { ix.to_be_bytes() } else { ix.to_le_bytes() });
            }
            let mut nkey = new_guid.to_vec();
            nkey.extend_from_slice(field(key, 16, key.len()));
            let nkey: &'a [u8] = store.keep(&nkey);
            let head: &'a [u8] = store.keep(field(&new, 0, 0x20));
            let pg: &'a [u8] = field(head, 0, 16);
            let made = ProgRef::New(self.new_programs.len());
            self.prog_ix.insert(pg, made);
            self.prog_body.insert(pg, (head, nkey));
            progs.push(made);
            self.new_programs.push((new, nkey));
            let (fg, lg) = (field(key, 16, 32), field(key, 32, 48));
            if fg != ZERO_GUID && !self.frag_g.contains(fg) {
                let n = frag_names.get(fg).ok_or(MergeError::Malformed)?;
                let g = take(n.guid);
                self.out.fragments.push(Named {
                    name: Cow::Owned(n.name.to_vec()),
                    guid: g,
                });
                self.frag_g.insert(g);
            }
            self.layout(lg, &layouts, label, take)?;
        }
        let count = progs.len();
        self.out.entries.push(Named {
            name: Cow::Owned(new_b.clone()),
            guid: new_guid,
        });
        self.entry_names.insert(new_b.clone());
        self.group_keys.push(new_guid);
        self.groups.insert(new_guid, progs);
        self.added.push((new_b, format!("{label}:{entry}")));
        Ok(count)
    }

    /// A clone's parameter block with the renames made: the donor's record (taken) when no name changed, else the
    /// new block kept in the store.
    fn block<'d>(
        &mut self,
        store: &dyn Store<'a>,
        block: &'d [u8],
        renames: &[(Vec<u8>, Vec<u8>)],
        where_: &str,
        take: Take<'_, 'd, 'a>,
    ) -> Result<usize> {
        Ok(
            match retexture::<std::hash::RandomState>(block, renames, where_, None)? {
                Cow::Owned(b) if b != block => {
                    let kept = store.keep(&b);
                    Self::table(kept, &mut self.out.params, &mut self.param_ix, &|x| x)
                }
                _ => Self::table(block, &mut self.out.params, &mut self.param_ix, take),
            },
        )
    }

    /// `add_part(name, layout_guid, donor, label)` (round OR3a): a mesh-part name -> vertex layout, the layout taken
    /// from the donor (copied into the store) when the file lacks it. A name the file has with this layout is left as
    /// it is.
    ///
    /// # Errors
    /// The file has the name with another layout; neither file has the layout; the name is not latin-1.
    pub fn add_part(
        &mut self,
        store: &dyn Store<'a>,
        name: &str,
        layout_guid: &[u8],
        donor: &ShaderDb<'_>,
        label: &str,
    ) -> Result<()> {
        self.add_part_with(store, name, layout_guid, donor, label, &|b| store.keep(b))
    }

    /// `add_part` with the merged file itself as the donor.
    ///
    /// # Errors
    /// As [`Merger::add_part`].
    pub fn add_part_from_self(
        &mut self,
        store: &dyn Store<'a>,
        name: &str,
        layout_guid: &[u8],
        label: &str,
    ) -> Result<()> {
        let view = self.out.clone();
        self.add_part_with(store, name, layout_guid, &view, label, &|b| b)
    }

    fn add_part_with<'d>(
        &mut self,
        store: &dyn Store<'a>,
        name: &str,
        layout_guid: &[u8],
        donor: &ShaderDb<'d>,
        label: &str,
        take: Take<'_, 'd, 'a>,
    ) -> Result<()> {
        let Some(nb) = to_latin(name) else {
            return Err(MergeError::Msg(format!("{name}: the name is not latin-1")));
        };
        if let Some(&g) = self.part_names.get(&nb) {
            if g != layout_guid {
                return Err(MergeError::Msg(format!(
                    "{name}: the database has this part with another layout"
                )));
            }
            return Ok(());
        }
        let mut layouts: HashMap<&'d [u8], &'d [u8]> = HashMap::new();
        for r in &donor.layouts {
            layouts.insert(field(r, 0, 16), *r);
        }
        self.layout(layout_guid, &layouts, label, take)?;
        let g = store.keep(layout_guid);
        self.out.parts.push(Named {
            name: Cow::Owned(nb.clone()),
            guid: g,
        });
        self.part_names.insert(nb, g);
        Ok(())
    }

    /// `finish()`: the programs re-sorted by GUID (the PC byte order), the entry groups renumbered and sorted. The
    /// merged file's bytes.
    ///
    /// # Errors
    /// More programs than an entry group can index.
    pub fn finish(&mut self) -> Result<Vec<u8>> {
        Ok(super::write(&self.finished()?))
    }

    /// `finish()` as a database (its records borrowed from the merger): the programs re-sorted by GUID (the PC byte
    /// order), the entry groups renumbered and sorted.
    ///
    /// # Errors
    /// More programs than an entry group can index.
    pub fn finished(&self) -> Result<ShaderDb<'_>> {
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
        let mut allp: Vec<(&[u8], &[u8], ProgRef)> = o
            .programs
            .iter()
            .zip(&o.keys)
            .enumerate()
            .map(|(i, (p, k))| (*p, *k, ProgRef::Old(i)))
            .collect();
        allp.extend(
            self.new_programs
                .iter()
                .enumerate()
                .map(|(i, (p, k))| (p.as_slice(), *k, ProgRef::New(i))),
        );
        allp.sort_by_cached_key(|t| pc(field(t.0, 0, 16)));
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
        let programs: Vec<&[u8]> = allp.iter().map(|t| t.0).collect();
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
        Ok(db)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]
mod tests {
    use super::*;

    /// A store for tests: each record kept for the rest of the test process.
    struct Leak;

    impl<'a> Store<'a> for Leak {
        fn keep(&self, b: &[u8]) -> &'a [u8] {
            Box::leak(b.to_vec().into_boxed_slice())
        }
    }

    fn leak(v: Vec<u8>) -> &'static [u8] {
        Box::leak(v.into_boxed_slice())
    }

    /// A parameter block naming textures in 0x80-byte fields (FORMAT.md section 1's layout, enough for retexture).
    fn block(names: &[&str]) -> Vec<u8> {
        let mut b = vec![0u8; 0x20];
        for n in names {
            b.extend([1, 0, 0, 0]);
            let mut f = n.as_bytes().to_vec();
            f.resize(0x80, 0);
            b.extend(f);
            b.extend([0; 8]);
        }
        let size = b.len() as u32;
        b[..4].copy_from_slice(&size.to_le_bytes());
        b
    }

    /// The texture names of a block made by `block`.
    fn names(b: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = 0x20;
        while at + 0x8c <= b.len() {
            let f = &b[at + 4..at + 0x84];
            out.push(latin(&f[..f.iter().position(|&c| c == 0).unwrap_or(f.len())]));
            at += 0x8c;
        }
        out
    }

    /// A Dx9 database: entries [(name, programs)], each program its own shaders and its second block naming `tex`.
    fn built(entries: &[(&str, usize)], seed: u8, tex: &[&str]) -> ShaderDb<'static> {
        let mut db = ShaderDb {
            version: 120,
            ..ShaderDb::default()
        };
        db.params.push(leak(vec![8, 0, 0, 0, b'P', b'A', b'R', b'M']));
        db.params.push(leak(block(tex)));
        db.bindings.push(leak(vec![1, 0, 7, 0, 0x24, 9, 0, 1]));
        db.regs.push(leak(vec![1, 0, 0, 0, b'R', b'E', b'G', b'S']));
        let frag: &'static [u8] = leak(vec![seed ^ 0x40; 16]);
        db.fragments.push(Named {
            name: Cow::Owned(format!("Systems/F{seed}").into_bytes()),
            guid: frag,
        });
        let layout: &'static [u8] = leak([vec![seed ^ 0x50; 16], vec![0; 0x49]].concat());
        db.layouts.push(layout);
        let mut progs: Vec<(&'static [u8], &'static [u8])> = Vec::new();
        for (k, (name, n)) in entries.iter().enumerate() {
            let g: &'static [u8] = leak(name_guid(name.as_bytes(), false));
            db.entries.push(Named {
                name: Cow::Owned(name.as_bytes().to_vec()),
                guid: g,
            });
            db.parts.push(Named {
                name: Cow::Owned(format!("{name}_lod0/p/Standard").into_bytes()),
                guid: &layout[..16],
            });
            for i in 0..*n {
                let vs = db.vs.len() as u32;
                db.vs.push(Shader {
                    guid: leak([seed, k as u8, i as u8, 1].repeat(4)),
                    code: leak(vec![1, seed, i as u8]),
                    tail: leak(vec![1, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]),
                    ..Shader::default()
                });
                db.ps.push(Shader {
                    guid: leak([seed, k as u8, i as u8, 2].repeat(4)),
                    code: leak(vec![0, seed, i as u8]),
                    ..Shader::default()
                });
                let mut p = [seed, k as u8, i as u8, 3].repeat(4);
                p.extend([0; 16]);
                for x in [vs, vs, 0, 1] {
                    p.extend(x.to_le_bytes());
                }
                let mut key = g.to_vec();
                key.extend(frag);
                key.extend(&layout[..16]);
                key.extend([0; 40]);
                progs.push((leak(p), leak(key)));
            }
        }
        progs.sort_by(|a, b| a.0[..16].cmp(&b.0[..16]));
        let mut groups: Vec<(&'static [u8], Vec<u16>)> = Vec::new();
        for (i, (p, k)) in progs.iter().enumerate() {
            db.programs.push(p);
            db.keys.push(k);
            match groups.iter_mut().find(|(g, _)| *g == &k[..16]) {
                Some((_, idx)) => idx.push(i as u16),
                None => groups.push((&k[..16], vec![i as u16])),
            }
        }
        groups.sort_by(|a, b| a.0.cmp(b.0));
        db.groups = groups;
        db
    }

    fn kind() -> Kind {
        Kind {
            dx: 9,
            big: false,
            key_size: 0x58,
            version: 120,
        }
    }

    const A: &str = "Objects/A/A_Mesh/Objects/A/Shaders/Metal";
    const B: &str = "Objects/B/B_Mesh/Objects/B/Shaders/Wood";
    const N: &str = "Objects/N/N_Mesh/Objects/N/Shaders/WoodClone";

    fn pair(a: &str, z: &str) -> (Vec<u8>, Vec<u8>) {
        (a.as_bytes().to_vec(), z.as_bytes().to_vec())
    }

    #[test]
    fn retexture_renames_at_once_and_whole_names_only() {
        let b = block(&["Tex/A", "Tex/B", "Tex/C", "Big/Tex/A"]);
        let mut hits = HashMap::new();
        let got = retexture(
            &b,
            &[pair("Tex/A", "Tex/B"), pair("Tex/B", "Tex/A")],
            "swap",
            Some(&mut hits),
        )
        .unwrap();
        assert_eq!(names(&got), ["Tex/B", "Tex/A", "Tex/C", "Big/Tex/A"]);
        assert_eq!(hits.len(), 2);
        let got = retexture::<std::hash::RandomState>(&b, &[pair("Tex/A", "Tex/B"), pair("Tex/B", "Tex/C")], "c", None);
        assert_eq!(names(&got.unwrap()), ["Tex/B", "Tex/C", "Tex/C", "Big/Tex/A"]);
        let mut hits = HashMap::new();
        let long = "X".repeat(0x80);
        let got = retexture(
            &b,
            &[pair("Tex/A", "Tex/Z"), pair("Tex/B", &long)],
            "long",
            Some(&mut hits),
        );
        assert!(matches!(got, Err(MergeError::Msg(m)) if m.starts_with("long: texture name XXX")));
        assert!(hits.is_empty(), "a refused block counts no hits");
    }

    #[test]
    fn clone_entry_renames_ids_keys_and_textures() {
        let base = built(&[(A, 2)], 40, &["Textures/A/Metal_C"]);
        let donor = built(&[(B, 3)], 41, &["Textures/B/Wood_Df", "Textures/B/Wood_Nm"]);
        let mut m = Merger::new(base, kind(), Vec::new(), false).unwrap();
        let tex = [
            ("Textures/B/Wood_Df".to_owned(), "Textures/B/Wood_Nm".to_owned()),
            ("Textures/B/Wood_Nm".to_owned(), "Textures/B/Wood_Df".to_owned()),
        ];
        assert_eq!(m.clone_entry(&Leak, &donor, &kind(), B, N, &tex, "d").unwrap(), 3);
        let data = m.finish().unwrap();
        let out = crate::shaderdb::parse(&data, 9, false).unwrap();
        assert_eq!(out.entries.len(), 2);
        assert_eq!(&*out.entries[1].name, N.as_bytes());
        let ng = name_guid(N.as_bytes(), false);
        let group = &out.groups.iter().find(|(g, _)| *g == ng.as_slice()).unwrap().1;
        assert_eq!(group.len(), 3);
        let old: Vec<&[u8]> = donor.programs.iter().map(|p| &p[..16]).collect();
        for &i in group {
            let p = out.programs[usize::from(i)];
            assert!(
                old.iter().any(|o| program_id(N.as_bytes(), o) == p[..16]),
                "ids MD5(new name + old id)"
            );
            assert_eq!(
                &out.keys[usize::from(i)][..16],
                ng.as_slice(),
                "keys name the new entry"
            );
            let b = program_refs(&out, usize::from(i)).unwrap()[3];
            assert_eq!(names(out.params[b]), ["Textures/B/Wood_Nm", "Textures/B/Wood_Df"]);
        }
        let ids: Vec<&[u8]> = out.programs.iter().map(|p| &p[..16]).collect();
        assert!(ids.is_sorted(), "programs sorted by id");
    }

    #[test]
    fn clone_entry_refusals_add_nothing() {
        let base = built(&[(A, 2)], 40, &["Textures/A/Metal_C"]);
        let donor = built(&[(B, 3)], 41, &["Textures/B/Wood_Df"]);
        let untouched = crate::shaderdb::write(&base);
        let mut m = Merger::new(base, kind(), Vec::new(), false).unwrap();
        let t = |a: &str, z: &str| vec![(a.to_owned(), z.to_owned())];
        let long = "X".repeat(0x80);
        for (entry, new, tex, says) in [
            (B, A, vec![], "has this entry already"),
            (
                "Objects/Q/Shaders/None",
                N,
                vec![],
                "d has no entry Objects/Q/Shaders/None",
            ),
            (
                B,
                N,
                t("Textures/B/wood_df", "x"),
                "uses no texture named Textures/B/wood_df",
            ),
            (B, N, t("Textures/B/Wood_Df", &long), "longer than the field"),
            (B, "Objects/\u{100}/X", vec![], "not latin-1"),
        ] {
            let got = m.clone_entry(&Leak, &donor, &kind(), entry, new, &tex, "d");
            assert!(
                matches!(&got, Err(MergeError::Msg(msg)) if msg.contains(says)),
                "{says}: {got:?}"
            );
        }
        let other = Kind { dx: 10, ..kind() };
        assert!(
            m.clone_entry(&Leak, &donor, &other, B, N, &[], "d10").is_err(),
            "another kind"
        );
        assert_eq!(m.finish().unwrap(), untouched, "nothing added");
    }

    #[test]
    fn a_clone_of_an_entry_cloned_in_the_same_merge_has_no_programs_yet() {
        // the Python reference read the base as it stood: the entry is there, its programs join at finish
        let base = built(&[(A, 2)], 40, &[]);
        let donor = built(&[(B, 3)], 41, &[]);
        let mut m = Merger::new(base, kind(), Vec::new(), false).unwrap();
        assert_eq!(m.clone_entry(&Leak, &donor, &kind(), B, N, &[], "d").unwrap(), 3);
        assert_eq!(
            m.clone_entry_from_self(&Leak, N, "Objects/S/X", &[], "self").unwrap(),
            0
        );
        assert_eq!(
            m.clone_entry_from_self(&Leak, A, "Objects/S/Y", &[], "self").unwrap(),
            2
        );
        assert!(m.has_entry(b"Objects/S/X"));
    }

    #[test]
    fn add_part_takes_the_donor_layout_once() {
        let base = built(&[(A, 1)], 50, &[]);
        let donor = built(&[(B, 1)], 51, &[]);
        let dg = donor.layouts[0][..16].to_vec();
        let bg = base.layouts[0][..16].to_vec();
        let bn = latin(&base.parts[0].name);
        let mut m = Merger::new(base, kind(), Vec::new(), false).unwrap();
        m.add_part(&Leak, "N_lod0/p/Standard", &dg, &donor, "d").unwrap();
        m.add_part(&Leak, "N_lod0/p/Standard", &dg, &donor, "d").unwrap();
        assert!(m.add_part(&Leak, &bn, &dg, &donor, "d").is_err(), "another layout");
        assert!(
            m.add_part(&Leak, "N_lod1/p/Standard", &[9; 16], &donor, "d").is_err(),
            "no file has it"
        );
        m.add_part_from_self(&Leak, "S_lod0/p/Standard", &bg, "self").unwrap();
        assert_eq!(m.current().layouts.len(), 2);
        assert_eq!(m.current().parts.len(), 3);
    }

    #[test]
    fn a_donor_read_for_one_call_may_go() {
        let base = built(&[(A, 1)], 60, &[]);
        let mut m = Merger::new(base, kind(), vec!["objects/b/b_mesh".into()], false).unwrap();
        {
            let data = crate::shaderdb::write(&built(&[(B, 2)], 61, &[]));
            let donor = crate::shaderdb::parse(&data, 9, false).unwrap();
            m.add_donor_kept(&Leak, &donor, &kind(), "d").unwrap();
            m.clone_entry(&Leak, &donor, &kind(), B, N, &[], "d").unwrap();
        }
        let out = m.finish().unwrap();
        let again = crate::shaderdb::parse(&out, 9, false).unwrap();
        assert_eq!(again.entries.len(), 3);
        assert_eq!(again.programs.len(), 1 + 2 + 2);
    }
}
