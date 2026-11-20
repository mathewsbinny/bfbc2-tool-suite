//! `shaderdb.py merge`: `Merger` (`__init__`, `add_donor`, `finish`), `entry_contents`, `verify_merge` and `cmd_merge`
//! in Rust - the same records added in the same order, the programs re-sorted and the entry groups renumbered as the
//! reference does, the same report and messages. The base is kept as `bc2_formats::shaderdb::ShaderDb` over the files'
//! bytes; only the new program records are made (their four indices renumbered into the merged file).

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};

use bc2_formats::shaderdb::query::{self, Needed};
use bc2_formats::shaderdb::{self, Named, Shader, ShaderDb};

use super::shaderdb::{
    Loaded, check, coverage, dx_from_name, entry_programs, hex, latin, load, parse_kind, program_refs, read_need,
    shader_error, to_latin,
};
use crate::pyos;
use crate::pyproc::{Fail, Py, PyResult, os_error};

/// `MAX_PROGRAMS`: entry groups index programs with `u16`.
const MAX_PROGRAMS: usize = 0x10000;
const ZERO_GUID: [u8; 16] = [0; 16];

/// `cmd_merge`'s arguments.
pub struct MergeArgs {
    pub base: String,
    pub donors: Vec<String>,
    pub need: String,
    pub out: String,
    pub all_names: bool,
    pub dx: Option<u32>,
    pub opfor: Option<String>,
}

/// A program while merging: the base's (its index there) or a new one (its index among the new).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ProgRef {
    Old(usize),
    New(usize),
}

/// `k[a:b]` as Python slices bytes.
fn field(k: &[u8], a: usize, b: usize) -> &[u8] {
    k.get(a.min(k.len())..b.min(k.len())).unwrap_or_default()
}

/// `Merger._shader_key(db, s)`: the shader by content (its tables resolved).
type ShaderKey<'a> = (&'a [u8], &'a [u8], &'a [u8], &'a [u8], &'a [u8], &'a [u8]);

fn shader_key<'a>(db: &ShaderDb<'a>, s: &Shader<'a>) -> PyResult<ShaderKey<'a>> {
    Ok((
        s.guid,
        s.code,
        s.tail,
        *db.params.get(s.param as usize).ok_or(Fail::Python)?,
        *db.bindings.get(s.binding as usize).ok_or(Fail::Python)?,
        *db.regs.get(s.regs as usize).ok_or(Fail::Python)?,
    ))
}

/// What a donor's file says about itself (`_same_kind`, the report's label).
struct Kind {
    dx: u32,
    big: bool,
    key_size: usize,
    version: u32,
}

fn kind_of(l: &Loaded, db: &ShaderDb<'_>) -> Kind {
    Kind {
        dx: l.dx,
        big: l.big,
        key_size: db.key_size(),
        version: db.version,
    }
}

fn platform(k: &Kind) -> String {
    if k.big { "Xbox 360".into() } else { format!("Dx{}", k.dx) }
}

/// `Merger(base, need, all_names)` and its state.
struct Merger<'a> {
    out: ShaderDb<'a>,
    kind: Kind,
    need: Vec<String>,
    all_names: bool,
    in_base: HashSet<String>,
    part_conflicts: Vec<String>,
    report: Vec<String>,
    added: Vec<(Vec<u8>, String)>,
    shared_programs: usize,
    conflicts: Vec<String>,
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

fn first_index<'a, T: std::hash::Hash + Eq + Copy>(items: impl Iterator<Item = T>) -> HashMap<T, usize> {
    let mut ix = HashMap::new();
    for (i, it) in items.enumerate() {
        ix.entry(it).or_insert(i);
    }
    ix
}

impl<'a> Merger<'a> {
    fn new(base: ShaderDb<'a>, kind: Kind, need: Vec<String>, all_names: bool) -> PyResult<Self> {
        let cover = coverage(&base, &need);
        let in_base: HashSet<String> = need
            .iter()
            .zip(&cover)
            .filter(|(_, c)| **c > 0)
            .map(|(n, _)| n.clone())
            .collect();
        let b = &base;
        let vs_keys: Vec<ShaderKey<'a>> = b.vs.iter().map(|s| shader_key(b, s)).collect::<PyResult<_>>()?;
        let ps_keys: Vec<ShaderKey<'a>> = b.ps.iter().map(|s| shader_key(b, s)).collect::<PyResult<_>>()?;
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
    fn shader(&mut self, donor: &ShaderDb<'a>, s: &Shader<'a>, vs: bool) -> PyResult<usize> {
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
            param: u32::try_from(param).map_err(|_| Fail::Python)?,
            binding: u32::try_from(binding).map_err(|_| Fail::Python)?,
            regs: u32::try_from(regs).map_err(|_| Fail::Python)?,
            tail: s.tail,
        };
        let (lst, ix) = if vs { (&mut o.vs, &mut self.vs_ix) } else { (&mut o.ps, &mut self.ps_ix) };
        let i = lst.len();
        ix.insert(key, i);
        lst.push(new);
        Ok(i)
    }

    fn same_kind(&self, k: &Kind, label: &str) -> PyResult<()> {
        let o = &self.kind;
        if (k.dx, k.big, k.key_size) != (o.dx, o.big, o.key_size) {
            return Err(shader_error(format!(
                "{label} is a {} database (version {}) and the base is {} (version {}): merge files of the same kind",
                platform(k),
                k.version,
                platform(o),
                o.version
            )));
        }
        Ok(())
    }

    fn layout(&mut self, lg: &'a [u8], layouts: &HashMap<&'a [u8], &'a [u8]>, label: &str) -> PyResult<()> {
        if self.lay_g.contains(lg) {
            return Ok(());
        }
        let Some(r) = layouts.get(lg) else {
            return Err(shader_error(format!("{label}: a vertex layout it uses is not in the file")));
        };
        self.out.layouts.push(r);
        self.lay_g.insert(lg);
        Ok(())
    }

    /// `add_donor(donor, label)`.
    fn add_donor(&mut self, donor: &ShaderDb<'a>, k: &Kind, label: &str) -> PyResult<()> {
        self.same_kind(k, label)?;
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
                let prog: &'a [u8] = *donor.programs.get(pi).ok_or(Fail::Python)?;
                let key: &'a [u8] = *donor.keys.get(pi).ok_or(Fail::Python)?;
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
                let [v, f, a, b] = program_refs(donor, pi)?;
                let nv = self.shader(donor, donor.vs.get(v).ok_or(Fail::Python)?, true)?;
                let nf = self.shader(donor, donor.ps.get(f).ok_or(Fail::Python)?, false)?;
                let pa = *donor.params.get(a).ok_or(Fail::Python)?;
                let pb = *donor.params.get(b).ok_or(Fail::Python)?;
                let na = Self::table(pa, &mut self.out.params, &mut self.param_ix);
                let nb = Self::table(pb, &mut self.out.params, &mut self.param_ix);
                let mut new = field(prog, 0, 0x20).to_vec();
                for x in [nv, nf, na, nb] {
                    let x = u32::try_from(x).map_err(|_| Fail::Python)?;
                    new.extend(if be { x.to_be_bytes() } else { x.to_le_bytes() });
                }
                let r = ProgRef::New(self.new_programs.len());
                self.prog_ix.insert(pg, r);
                self.prog_body.insert(pg, (field(prog, 0, 0x20), key));
                progs.push(r);
                self.new_programs.push((new, key));
                let (fg, lg) = (field(key, 16, 32), field(key, 32, 48));
                if fg != ZERO_GUID && !self.frag_g.contains(fg) {
                    let Some(n) = frag_names.get(fg) else {
                        return Err(shader_error(format!(
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
        self.report.push(format!("{label}: {took} entries, {parts} mesh-part names"));
        Ok(())
    }

    /// `finish()`: the programs re-sorted by GUID (the PC byte order), the entry groups renumbered and sorted. The
    /// merged file's bytes.
    fn finish(&mut self) -> PyResult<Vec<u8>> {
        let o = &self.out;
        let total = o.programs.len() + self.new_programs.len();
        if total >= MAX_PROGRAMS {
            return Err(shader_error(format!(
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
            for r in &self.groups[g] {
                idx.push(*place.get(r).ok_or(Fail::Python)?);
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
        Ok(shaderdb::write(&db))
    }
}

/// `entry_contents(db, names)`: {entry name: its programs by content}, each program one byte string (the record up
/// to its indices, the key, both shaders with their tables, both parameter blocks; lengths first, so no two differ
/// and read the same), in the entries' order (a name twice: its last entry, at its first place).
fn entry_contents(db: &ShaderDb<'_>, names: Option<&HashSet<Vec<u8>>>) -> PyResult<(Vec<Vec<u8>>, HashMap<Vec<u8>, BTreeSet<Vec<u8>>>)> {
    let progs = entry_programs(db);
    let mut order = Vec::new();
    let mut out: HashMap<Vec<u8>, BTreeSet<Vec<u8>>> = HashMap::new();
    for e in &db.entries {
        if names.is_some_and(|n| !n.contains(&*e.name)) {
            continue;
        }
        let mut items = BTreeSet::new();
        for &i in progs.get(e.guid).copied().unwrap_or_default() {
            let i = usize::from(i);
            let [v, f, x, y] = program_refs(db, i)?;
            let vk = shader_key(db, db.vs.get(v).ok_or(Fail::Python)?)?;
            let fk = shader_key(db, db.ps.get(f).ok_or(Fail::Python)?)?;
            let mut item = Vec::new();
            let mut put = |b: &[u8]| {
                item.extend((b.len() as u64).to_le_bytes());
                item.extend_from_slice(b);
            };
            put(field(db.programs.get(i).ok_or(Fail::Python)?, 0, 0x20));
            put(db.keys.get(i).ok_or(Fail::Python)?);
            for k in [vk, fk] {
                for b in [k.0, k.1, k.2, k.3, k.4, k.5] {
                    put(b);
                }
            }
            put(db.params.get(x).ok_or(Fail::Python)?);
            put(db.params.get(y).ok_or(Fail::Python)?);
            items.insert(item);
        }
        if !out.contains_key(&*e.name) {
            order.push(e.name.to_vec());
        }
        out.insert(e.name.to_vec(), items);
    }
    Ok((order, out))
}

/// `verify_merge(merged, sources, m)`.
fn verify_merge(
    merged: &ShaderDb<'_>,
    base_c: &(Vec<Vec<u8>>, HashMap<Vec<u8>, BTreeSet<Vec<u8>>>),
    donors: &[(&ShaderDb<'_>, &str)],
    added: &[(Vec<u8>, String)],
) -> PyResult<Vec<String>> {
    let mut p = Vec::new();
    let (_, got) = entry_contents(merged, None)?;
    let mut by_donor: HashMap<&str, HashSet<Vec<u8>>> = HashMap::new();
    for (name, label) in added {
        by_donor.entry(label.as_str()).or_default().insert(name.clone());
    }
    let mut order = base_c.0.clone();
    let mut expect = base_c.1.clone();
    for (db, label) in donors {
        if let Some(names) = by_donor.get(label).filter(|n| !n.is_empty()) {
            let (o, c) = entry_contents(db, Some(names))?;
            for n in o {
                if !expect.contains_key(&n) {
                    order.push(n.clone());
                }
                let v = c.get(&n).ok_or(Fail::Python)?.clone();
                expect.insert(n, v);
            }
        }
    }
    let gk: HashSet<&Vec<u8>> = got.keys().collect();
    let ek: HashSet<&Vec<u8>> = expect.keys().collect();
    if gk != ek {
        p.push(format!(
            "{} entries differ between the merged file and its sources",
            gk.symmetric_difference(&ek).count()
        ));
    }
    let bad: Vec<&Vec<u8>> = order
        .iter()
        .filter(|n| got.get(*n).is_some_and(|g| Some(g) != expect.get(*n)))
        .collect();
    if let Some(first) = bad.first() {
        p.push(format!(
            "{} entries draw with other programs than in their source, e.g. {}",
            bad.len(),
            latin(first)
        ));
    }
    Ok(p)
}

/// `cmd_merge(a)`.
#[allow(clippy::too_many_lines, reason = "the reference's command, step by step")]
pub fn cmd_merge(py: &mut Py, a: &MergeArgs) -> PyResult<i32> {
    let opfor = a.opfor.as_deref();
    let need = read_need(&a.need)?;
    let base_l = load(&a.base, a.dx, opfor)?;
    let mut donors_l: Vec<Loaded> = Vec::new();
    for spec in &a.donors {
        // the donor's own kind (given, or its name's) - else the base's
        let named = dx_from_name(spec.rsplit(':').next().unwrap_or(spec));
        donors_l.push(load(spec, a.dx.or(named).or(Some(base_l.dx)), opfor)?);
    }
    let base = base_l.db()?;
    let before = (
        base.entries.len(),
        base.programs.len(),
        base.vs.len(),
        base.ps.len(),
        base.params.len(),
    );
    let base_c = entry_contents(&base, None)?;
    let donor_dbs: Vec<ShaderDb<'_>> = donors_l.iter().map(Loaded::db).collect::<PyResult<_>>()?;
    let mut m = Merger::new(base, kind_of(&base_l, &base_l.db()?), need.clone(), a.all_names)?;
    for (l, db) in donors_l.iter().zip(&donor_dbs) {
        m.add_donor(db, &kind_of(l, db), &l.label)?;
    }
    let out = m.finish()?;
    let parse_dx = if base_l.dx == 11 { 10 } else { base_l.dx };
    let (again_dx, again_big) = parse_kind(&out, Some(parse_dx), "the merged file")?;
    let again = shaderdb::parse(&out, again_dx, again_big).map_err(|_| Fail::Python)?;
    let donors: Vec<(&ShaderDb<'_>, &str)> = donor_dbs.iter().zip(&donors_l).map(|(d, l)| (d, l.label.as_str())).collect();
    let mut problems = check(&again)?;
    problems.extend(verify_merge(&again, &base_c, &donors, &m.added)?);
    if shaderdb::write(&again) != out || !problems.is_empty() {
        let first: Vec<&String> = problems.iter().take(5).collect();
        return Err(shader_error(format!(
            "the merged file does not read back consistently: {}",
            first.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("; ")
        )));
    }
    let abs = pyos::abspath(&a.out)?;
    pyos::makedirs(&pyos::dirname(&abs)?)?;
    std::fs::write(&a.out, &out).map_err(|e| os_error(&a.out, &e))?;
    let still = coverage(&again, &need);
    let mut nomesh: Vec<&String> = Vec::new();
    let mut seen: HashSet<&String> = HashSet::new();
    for (k, v) in need.iter().zip(&still) {
        if seen.insert(k) && *v == 0 {
            nomesh.push(k);
        }
    }
    nomesh.sort();
    let mut lines: Vec<String> = vec![
        format!(
            "merged {} (Dx{}, {} bytes)",
            a.out,
            base_l.dx,
            crate::pyfmt::thousands(out.len() as i128)
        ),
        format!("base {}: {} entries, {} programs", base_l.label, before.0, before.1),
    ];
    lines.extend(m.report.iter().cloned());
    lines.push(format!(
        "result: {} entries (+{}), {} programs (+{}), {} vertex shaders (+{}), {} pixel shaders (+{}), {} parameter \
         blocks (+{})",
        again.entries.len(),
        again.entries.len() - before.0,
        again.programs.len(),
        again.programs.len() - before.1,
        again.vs.len(),
        again.vs.len() - before.2,
        again.ps.len(),
        again.ps.len() - before.3,
        again.params.len(),
        again.params.len() - before.4
    ));
    lines.push(
        "read back: consistent, writes identical bytes; every entry uses the same programs, shaders, parameter blocks \
         and tables (by content) as in the file it came from"
            .into(),
    );
    lines.push(format!(
        "{} donor programs were already in the file (same GUID and content): shared",
        m.shared_programs
    ));
    if !m.part_conflicts.is_empty() {
        lines.push(format!(
            "{} mesh-part names a donor has with another vertex layout:",
            m.part_conflicts.len()
        ));
        lines.extend(m.part_conflicts.iter().take(50).map(|c| format!("  {c}")));
    }
    if !m.conflicts.is_empty() {
        lines.push(format!(
            "{} donor programs have a GUID the file has with other content (the existing one kept):",
            m.conflicts.len()
        ));
        lines.extend(m.conflicts.iter().take(50).map(|c| format!("  {c}")));
    }
    lines.push("entries added:".into());
    lines.extend(m.added.iter().map(|(n, l)| format!("  {}   <- {l}", latin(n))));
    lines.push(format!("needed names with no entry in the result: {}", nomesh.len()));
    lines.extend(nomesh.iter().map(|k| format!("  {k}")));
    let report_path = format!("{}.merge.txt", a.out);
    let text = format!("{}\n", lines.join("\n")).replace('\n', "\r\n");
    std::fs::write(&report_path, text.as_bytes()).map_err(|e| os_error(&report_path, &e))?;
    let shown = (m.report.len() + 5 + usize::from(!m.conflicts.is_empty())).min(lines.len());
    py.print(&lines[..shown].join("\n"))?;
    py.print(&format!(
        "{} entries added; {} needed names still have no entry; report: {}.merge.txt",
        m.added.len(),
        nomesh.len(),
        a.out
    ))?;
    Ok(0)
}
