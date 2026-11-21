//! `shaderdb.py merge`: `Merger` (`__init__`, `add_donor`, `finish`), `entry_contents`, `verify_merge` and `cmd_merge`
//! in Rust - the same records added in the same order, the programs re-sorted and the entry groups renumbered as the
//! reference does, the same report and messages. The base is kept as `bc2_formats::shaderdb::ShaderDb` over the files'
//! bytes; only the new program records are made (their four indices renumbered into the merged file).

use std::collections::{BTreeSet, HashMap, HashSet};

use bc2_formats::shaderdb::merge::{Kind, MergeError, Merger, field, shader_key};
use bc2_formats::shaderdb::{self, ShaderDb};

use super::shaderdb::{
    Loaded, check, coverage, dx_from_name, entry_programs, latin, load, parse_kind, program_refs, read_need,
    shader_error,
};
use crate::pyos;
use crate::pyproc::{Fail, Py, PyResult, os_error};

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

/// The library's merge error as the port's: a message is the reference's `ShaderDbError`, a malformed database the
/// Python's own failure.
fn fail(e: MergeError) -> Fail {
    match e {
        MergeError::Msg(m) => shader_error(m),
        MergeError::Malformed => Fail::Python,
    }
}

fn kind_of(l: &Loaded, db: &ShaderDb<'_>) -> Kind {
    Kind {
        dx: l.dx,
        big: l.big,
        key_size: db.key_size(),
        version: db.version,
    }
}

/// `entry_contents(db, names)`: {entry name: its programs by content}, each program one byte string (the record up
/// to its indices, the key, both shaders with their tables, both parameter blocks; lengths first, so no two differ
/// and read the same), in the entries' order (a name twice: its last entry, at its first place).
fn entry_contents(
    db: &ShaderDb<'_>,
    names: Option<&HashSet<Vec<u8>>>,
) -> PyResult<(Vec<Vec<u8>>, HashMap<Vec<u8>, BTreeSet<Vec<u8>>>)> {
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
            let vk = shader_key(db, db.vs.get(v).ok_or(Fail::Python)?).map_err(fail)?;
            let fk = shader_key(db, db.ps.get(f).ok_or(Fail::Python)?).map_err(fail)?;
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
    let mut m = Merger::new(base, kind_of(&base_l, &base_l.db()?), need.clone(), a.all_names).map_err(fail)?;
    for (l, db) in donors_l.iter().zip(&donor_dbs) {
        m.add_donor(db, &kind_of(l, db), &l.label).map_err(fail)?;
    }
    let out = m.finish().map_err(fail)?;
    let parse_dx = if base_l.dx == 11 { 10 } else { base_l.dx };
    let (again_dx, again_big) = parse_kind(&out, Some(parse_dx), "the merged file")?;
    let again = shaderdb::parse(&out, again_dx, again_big).map_err(|_| Fail::Python)?;
    let donors: Vec<(&ShaderDb<'_>, &str)> = donor_dbs
        .iter()
        .zip(&donors_l)
        .map(|(d, l)| (d, l.label.as_str()))
        .collect();
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
