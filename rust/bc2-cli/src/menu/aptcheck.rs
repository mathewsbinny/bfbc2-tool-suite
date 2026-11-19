//! `bfbc2 aptcheck verify|lint|check ...`: `bfbc2-menu-tools\aptcheck.py`'s command line. `Code` and its findings
//! over the Rust decoder's records (`bc2_formats::apt`), `check` over `bc2_formats::aptscan::check_many`. A movie
//! the decoder refuses, and the Python's own exceptions (`struct.error` ...), hand the command line to the Python.

// The Python it ports indexes and slices freely; each index here is bounded by the check before it (as the
// reference's), and the menu group's 372 recorded lines and OPFOR's replay run them.
#![allow(clippy::indexing_slicing)]

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use bc2_formats::apt::{self, Instruction, Record};
use bc2_formats::aptscan::{self, hex6};

use super::common::{
    self as mc, Members, Movie, OMap, apply_patch, basename, glob, isfile, menu_err, parse_patch, resolve, splitext,
    take, take_flag,
};
use crate::pyfmt::docstring;
use crate::pyproc::{Fail, Py, PyErr, PyResult};

const SOURCE: &str = include_str!("../../../../bfbc2-menu-tools/aptcheck.py");

// ------------------------------------------------------------------------------------------------ analysis

type Key = (&'static str, i64);

/// `Code(movie)`: the decoded records, by offset, the functions and their names.
struct Code {
    recs: Vec<Record>,
    by_at: OMap<i64, usize>,
    starts: Vec<i64>,
    funcs: Vec<(i64, i64, i64, String)>,
    nconst: usize,
}

impl Code {
    fn new(m: &Movie) -> PyResult<Self> {
        let d = apt::decode(m.mref(), true).map_err(|_| Fail::Python)?;
        let recs = d.records;
        let mut by_at = OMap::default();
        let mut funcs = Vec::new();
        let mut last: HashMap<i64, usize> = HashMap::new();
        for (k, r) in recs.iter().enumerate() {
            by_at.insert(r.ins.at, k);
            if let Some(f) = &r.ins.func {
                let name = if f.name.is_empty() {
                    match last.get(&r.start).map(|&p| &recs[p].ins) {
                        Some(p) if matches!(p.op, 0xA1..=0xA3) && p.arg.starts_with('"') => {
                            p.arg.trim_matches('"').to_owned()
                        }
                        _ => "<anon>".to_owned(),
                    }
                } else {
                    f.name.clone()
                };
                funcs.push((f.body, f.body + i64::from(f.size), r.ins.at, name));
            }
            last.insert(r.start, k);
        }
        funcs.sort();
        let mut starts: Vec<i64> = by_at.keys().copied().collect();
        starts.sort_unstable();
        Ok(Self {
            recs,
            by_at,
            starts,
            funcs,
            nconst: m.consts.len(),
        })
    }

    fn ins_at(&self, at: i64) -> Option<&Instruction> {
        self.by_at.get(&at).map(|&k| &self.recs[k].ins)
    }

    fn span(ins: &Instruction) -> (i64, i64) {
        (ins.at, ins.end.max(ins.at + 1))
    }

    /// `containing(off)`.
    fn containing(&self, off: i64) -> Option<&Instruction> {
        let k = self.starts.partition_point(|&s| s <= off);
        let ins = self.ins_at(*self.starts.get(k.checked_sub(1)?)?)?;
        let (a, b) = Self::span(ins);
        (a <= off && off < b).then_some(ins)
    }

    /// `fname(off)`.
    fn fname(&self, off: i64) -> String {
        let mut best: Option<(i64, i64, &str)> = None;
        for (body, end, at, name) in &self.funcs {
            if *body <= off && off < *end && best.is_none_or(|b| *body >= b.0) {
                best = Some((*body, *at, name));
            }
        }
        best.map_or_else(
            || "a frame action".to_owned(),
            |(_, at, name)| format!("function {name} ({})", hex6(at)),
        )
    }

    /// `text(ins)`.
    fn text(ins: &Instruction) -> String {
        let name = apt::display_name(ins.op);
        if let Some(f) = &ins.func {
            let fname = if f.name.is_empty() { "<anon>" } else { &f.name };
            return format!("{name} {fname}({} params) size={}", f.nparams, f.size);
        }
        if ins.arg.is_empty() {
            name
        } else {
            format!("{name} {}", ins.arg)
        }
    }

    /// `findings()`: {(kind, key): (kind, text)} in Python's insertion order.
    #[expect(clippy::too_many_lines, reason = "aptcheck.Code.findings, rule by rule")]
    fn findings(&self) -> OMap<Key, (&'static str, String)> {
        let mut streams: HashMap<i64, HashSet<i64>> = HashMap::new();
        for r in &self.recs {
            streams.entry(r.start).or_default().insert(r.ins.at);
        }
        let mut out: OMap<Key, (&'static str, String)> = OMap::default();
        let mut users: OMap<u32, Vec<i64>> = OMap::default();
        let nconst = self.nconst;
        for r in &self.recs {
            let ins = &r.ins;
            let name = apt::display_name(ins.op);
            if let Some(p) = ins.plist {
                users.entry_or(p, Vec::new).push(ins.at);
            }
            if ins.unknown {
                out.insert(
                    ("unknown-op", ins.at),
                    (
                        "unknown-op",
                        format!(
                            "{} {name}: an opcode with an argument the reader does not know",
                            hex6(ins.at)
                        ),
                    ),
                );
            }
            if ins.op == 0 && r.depth > 0 {
                out.insert(
                    ("end-in-function", ins.at),
                    (
                        "end-in-function",
                        format!("{} End inside {}", hex6(ins.at), self.fname(ins.at)),
                    ),
                );
            }
            if matches!(ins.op, 0x99 | 0x9D | 0xB8)
                && let Some(t) = ins.target
            {
                let ok = streams.get(&r.start).is_some_and(|s| s.contains(&t)) || t == r.limit;
                if !ok {
                    out.insert(
                        ("branch-target", ins.at),
                        (
                            "branch-target",
                            format!(
                                "{} {name} {}: the target is not an instruction of this stream",
                                hex6(ins.at),
                                ins.arg
                            ),
                        ),
                    );
                }
            }
            if matches!(ins.op, 0x96 | 0x88)
                && let Some(items) = &ins.items
                && let Some(bad) = items.iter().find(|&&g| g as usize >= nconst)
            {
                out.insert(
                    ("const-index", ins.at),
                    (
                        "const-index",
                        format!(
                            "{} {name}: constant index {bad} past the {nconst} constants",
                            hex6(ins.at)
                        ),
                    ),
                );
            }
            if let Some(ix) = ins.index
                && ix as usize >= ins.pool
            {
                out.insert(
                    ("const-index", ins.at),
                    (
                        "const-index",
                        format!(
                            "{} {name}: pool index {ix} past the current pool ({} constants)",
                            hex6(ins.at),
                            ins.pool
                        ),
                    ),
                );
            }
        }
        for (plist, ats) in &users.items {
            if ats.len() > 1 {
                let mut s = ats.clone();
                s.sort_unstable();
                let list: Vec<String> = s.iter().map(|&a| hex6(a)).collect();
                out.insert(
                    ("shared-list", i64::from(*plist)),
                    (
                        "shared-list",
                        format!(
                            "PushData list {plist:#x} used by {} PushData: {}",
                            ats.len(),
                            list.join(", ")
                        ),
                    ),
                );
            }
        }
        let mut ends: HashMap<(i64, i64), i64> = HashMap::new();
        for r in &self.recs {
            if r.depth > 0 {
                let e = r.ins.func.as_ref().map_or(r.ins.end, |f| f.body + i64::from(f.size));
                let cur = ends.get(&(r.start, r.limit)).copied().unwrap_or(r.start);
                ends.insert((r.start, r.limit), cur.max(e));
            }
        }
        for (body, end, at, name) in &self.funcs {
            let got = ends.get(&(*body, *end)).copied().unwrap_or(*body);
            if got != *end {
                out.insert(
                    ("body-end", *at),
                    (
                        "body-end",
                        format!(
                            "function {name} at {}: its body decodes to {}, not its end {}",
                            hex6(*at),
                            hex6(got),
                            hex6(*end)
                        ),
                    ),
                );
            }
        }
        out
    }
}

/// `ranges(offsets, gap=4)`.
fn ranges(offsets: &BTreeSet<usize>) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &o in offsets {
        match out.last_mut() {
            Some(last) if o - last.1 < 4 => last.1 = o + 1,
            _ => out.push((o, o + 1)),
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------ verify

/// `Report`.
pub struct Report {
    title: String,
    pub errors: usize,
    warnings: usize,
    notes: usize,
    lines: Vec<String>,
    brief: bool,
}

impl Report {
    fn new(title: String, brief: bool) -> Self {
        Self {
            title,
            errors: 0,
            warnings: 0,
            notes: 0,
            lines: Vec::new(),
            brief,
        }
    }

    fn add(&mut self, level: &str, text: &str, detail: bool) {
        if detail && self.brief {
            return;
        }
        match level {
            "error" => self.errors += 1,
            "warning" => self.warnings += 1,
            "note" => self.notes += 1,
            _ => {}
        }
        self.lines.push(if level == "info" {
            format!("    {text}")
        } else {
            format!("  {level}: {text}")
        });
    }

    fn show(&self, py: &mut Py) -> PyResult<()> {
        py.print(&self.title)?;
        for l in &self.lines {
            py.print(l)?;
        }
        let mut s = format!(
            "  -> {}",
            if self.errors == 0 {
                "OK".to_owned()
            } else {
                format!("{} error(s)", self.errors)
            }
        );
        if self.warnings > 0 {
            let _ = write!(s, ", {} warning(s)", self.warnings);
        }
        if self.notes > 0 {
            let _ = write!(s, ", {} note(s)", self.notes);
        }
        py.print(&s)
    }
}

/// `verify(orig_members, new_members, rep, named)`.
#[expect(clippy::too_many_lines, reason = "aptcheck.verify, step by step")]
fn verify(
    orig: &Members,
    new: &Members,
    rep: &mut Report,
    named: Option<&OMap<String, BTreeSet<usize>>>,
) -> PyResult<()> {
    let mut lower: HashMap<String, String> = HashMap::new();
    for k in new.keys() {
        lower.insert(k.to_lowercase(), k.clone());
    }
    let mut changed: OMap<String, BTreeSet<usize>> = OMap::default();
    for (k, ob) in &orig.items {
        let Some(nk) = lower.get(&k.to_lowercase()) else {
            rep.add("error", &format!("member {k} is missing from the patched movie"), false);
            continue;
        };
        let nb = new.get(nk).map_or(&[][..], Vec::as_slice);
        if nb.len() < ob.len() {
            rep.add(
                "error",
                &format!(
                    "{k}: {} -> {} bytes - the member got shorter (patches keep the size)",
                    ob.len(),
                    nb.len()
                ),
                false,
            );
        } else if nb.len() > ob.len() {
            rep.add(
                "note",
                &format!(
                    "{k}: {} byte(s) appended at the end ({:X} -> {:X})",
                    nb.len() - ob.len(),
                    ob.len(),
                    nb.len()
                ),
                false,
            );
        }
        let n = ob.len().min(nb.len());
        let diff: BTreeSet<usize> = (0..n).filter(|&i| ob[i] != nb[i]).collect();
        changed.insert(k.clone(), diff);
    }
    let olow: HashSet<String> = orig.keys().map(|k| k.to_lowercase()).collect();
    for k in new.keys() {
        if !olow.contains(&k.to_lowercase()) {
            rep.add("error", &format!("member {k} is new in the patched movie"), false);
        }
    }
    let om = mc::movie_from_members(orig, "the original")?;
    let nm = mc::movie_from_members(new, "the patched movie")?;
    if om.big != nm.big {
        rep.add("error", "the byte order differs between the two movies", false);
        return Ok(());
    }
    let (oc, nc) = (Code::new(&om)?, Code::new(&nm)?);
    let aptk = orig
        .keys()
        .find(|k| k.to_lowercase().ends_with(".apt"))
        .cloned()
        .unwrap_or_default();
    let constk_low = format!(
        "{}.const",
        aptk.chars()
            .take(aptk.chars().count().saturating_sub(4))
            .collect::<String>()
    )
    .to_lowercase();
    let constk = orig
        .keys()
        .find(|k| k.to_lowercase() == constk_low)
        .cloned()
        .unwrap_or_default();
    let total: usize = changed.items.iter().map(|(_, v)| v.len()).sum();
    if total > 0 {
        let parts: Vec<String> = changed
            .items
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| format!("{k} {}", v.len()))
            .collect();
        rep.add("info", &format!("{total} byte(s) changed: {}", parts.join(", ")), false);
    } else {
        rep.add("info", "no byte changed in place", false);
    }

    // the changed ranges, named
    for (k, diff) in &changed.items {
        for (a, b) in ranges(diff) {
            let ob = &orig.get(k).map_or(&[][..], Vec::as_slice)[a..b];
            let nk = lower.get(&k.to_lowercase()).cloned().unwrap_or_default();
            let nb = &new.get(&nk).map_or(&[][..], Vec::as_slice)[a..b];
            let head = format!("{k} {a:X}: {} -> {}", mc::hex_spaced(ob), mc::hex_spaced(nb));
            if *k == aptk {
                let before: BTreeSet<i64> = (a..b).filter_map(|o| oc.containing(o as i64)).map(|i| i.at).collect();
                let after: BTreeSet<i64> = (a..b).filter_map(|o| nc.containing(o as i64)).map(|i| i.at).collect();
                if !before.is_empty() || !after.is_empty() {
                    rep.add("info", &format!("{head}  in {}", nc.fname(a as i64)), true);
                    for at in &before {
                        let ins = oc.ins_at(*at).ok_or(Fail::Python)?;
                        rep.add("info", &format!("      was {}  {}", hex6(*at), Code::text(ins)), true);
                    }
                    for at in &after {
                        let ins = nc.ins_at(*at).ok_or(Fail::Python)?;
                        rep.add("info", &format!("      now {}  {}", hex6(*at), Code::text(ins)), true);
                    }
                } else {
                    rep.add("info", &format!("{head}  (data, not code)"), true);
                }
                let napt = new.get(&aptk).ok_or(Fail::Python)?;
                for o in diff.range(a..b) {
                    if napt.get(*o) == Some(&0) && nc.by_at.contains(&(*o as i64)) {
                        rep.add(
                            "error",
                            &format!(
                                "zero-opcode: {aptk} {o:X} is now opcode 00 (End) in {} - use 59 17 (push 0 / pop) \
                                 as filler",
                                nc.fname(*o as i64)
                            ),
                            false,
                        );
                    }
                }
            } else if *k == constk {
                let (count, entries) = (om.cu(24)?, om.cu(28)?);
                let (lo, hi) = (entries, entries + 8 * count);
                let fields: BTreeSet<(u64, u64)> = diff
                    .range(a..b)
                    .map(|&o| o as u64)
                    .filter(|&o| lo <= o && o < hi)
                    .map(|o| ((o - lo) / 8, (o - lo) % 8 / 4))
                    .collect();
                let desc: Vec<String> = fields
                    .iter()
                    .map(|&(idx, field)| {
                        let get = |m: &Movie| m.consts.get(idx as usize).cloned().unwrap_or_else(|| "?".to_owned());
                        format!(
                            "constant {idx} {}: {} -> {}",
                            if field == 0 { "type" } else { "value" },
                            get(&om),
                            get(&nm)
                        )
                    })
                    .collect();
                let tail = if desc.is_empty() {
                    "  (constant data)".to_owned()
                } else {
                    format!("  {}", desc.join("; "))
                };
                rep.add("info", &format!("{head}{tail}"), true);
            } else {
                rep.add("info", &head, true);
            }
        }
    }

    // every instruction outside the changed bytes decodes as before
    let cs: Vec<i64> = changed
        .get(&aptk)
        .map_or_else(Vec::new, |s| s.iter().map(|&o| o as i64).collect());
    let touched = |ins: &Instruction| {
        let (a, b) = Code::span(ins);
        let k = cs.partition_point(|&c| c < a);
        k < cs.len() && cs[k] < b
    };
    let mut shifted: Vec<i64> = Vec::new();
    for &(at, k) in &oc.by_at.items {
        let ins = &oc.recs[k].ins;
        if touched(ins) {
            continue;
        }
        let differs = match nc.ins_at(at) {
            None => true,
            Some(n) => {
                let fdiff = match (&ins.func, &n.func) {
                    (Some(f), Some(g)) => f.size != g.size,
                    (Some(_), None) => return Err(Fail::Python), // Python: AttributeError
                    _ => false,
                };
                n.op != ins.op || n.end != ins.end || n.target != ins.target || n.plist != ins.plist || fdiff
            }
        };
        if differs {
            shifted.push(at);
        }
    }
    for &(at, k) in &nc.by_at.items {
        if !oc.by_at.contains(&at) && !touched(&nc.recs[k].ins) {
            shifted.push(at);
        }
    }
    if let Some(&first) = shifted.iter().min() {
        rep.add(
            "error",
            &format!(
                "decode-shift: {} instruction(s) away from the changed bytes decode differently, first at {} in {} - \
                 an operand or the alignment moved",
                shifted.len(),
                hex6(first),
                nc.fname(first)
            ),
            false,
        );
    } else {
        rep.add(
            "info",
            &format!(
                "{} instructions decode; every one away from the changed bytes as before",
                nc.by_at.len()
            ),
            false,
        );
    }

    // the rules, on the patched movie, against the original's own
    let (of, nf) = (oc.findings(), nc.findings());
    let mut sorted: Vec<&(Key, (&'static str, String))> = nf.items.iter().collect();
    sorted.sort_by(|x, y| x.0.cmp(&y.0));
    for (key, (kind, text)) in sorted {
        let was = of.get(key);
        if was.is_none() || (*kind == "shared-list" && was.is_some_and(|w| w.1 != *text)) {
            rep.add("error", &format!("{kind}: {text}"), false);
        }
    }
    for (key, (kind, text)) in &of.items {
        if nf.contains(key) {
            rep.add("note", &format!("already in the original ({kind}): {text}"), false);
        }
    }
    for (key, (kind, text)) in &of.items {
        if !nf.contains(key) {
            rep.add("info", &format!("gone in the patched movie ({kind}): {text}"), false);
        }
    }

    // only the bytes the patch names
    if let Some(named) = named {
        for (k, diff) in &changed.items {
            let empty = BTreeSet::new();
            let want = named.get(k).unwrap_or(&empty);
            let extra: Vec<&usize> = diff.difference(want).collect();
            if let Some(first) = extra.first() {
                rep.add(
                    "error",
                    &format!(
                        "unexpected-change: {k} differs at {} byte(s) the patch does not name, first at {first:X}",
                        extra.len()
                    ),
                    false,
                );
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ commands

fn cmd_verify(py: &mut Py, args: &mut Vec<String>, opfor: Option<&str>) -> PyResult<usize> {
    let patch = take(args, "--patch")?;
    let brief = take_flag(args, "--brief");
    if args.len() < 2 {
        return Err(menu_err("verify needs <original> <patched>"));
    }
    let (ol, om) = resolve(&args[0], opfor)?;
    let (nl, nm) = resolve(&args[1], opfor)?;
    let mut rep = Report::new(format!("verify {ol}\n    -> {nl}"), brief);
    let mut named = None;
    if let Some(patch) = patch {
        let (lines, problems) = parse_patch(&patch)?;
        let (_, nm_, applied) = apply_patch(&om, &lines);
        for (n, p) in problems.iter().chain(&applied) {
            rep.add("error", &format!("{} line {n}: {p}", basename(&patch)), false);
        }
        named = Some(nm_);
    }
    verify(&om, &nm, &mut rep, named.as_ref())?;
    rep.show(py)?;
    Ok(rep.errors)
}

#[expect(clippy::too_many_lines, reason = "aptcheck.cmd_lint, step by step")]
fn cmd_lint(py: &mut Py, args: &mut Vec<String>, opfor: Option<&str>) -> PyResult<usize> {
    let source = take(args, "--source")?;
    let installed = take_flag(args, "--installed");
    let brief = take_flag(args, "--brief");
    let mut files = Vec::new();
    for a in args.iter() {
        if a.contains(['*', '?']) {
            files.extend(glob(a)?);
        } else {
            files.push(a.clone());
        }
    }
    if files.is_empty() {
        return Err(menu_err("lint needs one or more .patch files"));
    }
    let mut errors = 0;
    for path in files {
        if !isfile(&path) {
            return Err(Fail::Err(PyErr::file_not_found(&path)));
        }
        let movie = splitext(&basename(&path)).0;
        let spec = source.clone().unwrap_or_else(|| format!("pristine:{movie}"));
        let mut rep = Report::new(format!("lint {path}  (source: {spec})"), brief);
        let (lines, problems) = parse_patch(&path)?;
        for (n, p) in &problems {
            rep.add("error", &format!("line {n}: {p}"), false);
        }
        let mut spans: HashMap<(String, usize), usize> = HashMap::new();
        for pl in &lines {
            if pl.comment.is_empty() {
                rep.add(
                    "warning",
                    &format!(
                        "line {}: no comment, on the line or just above it (OPFOR comments every patch)",
                        pl.number
                    ),
                    false,
                );
            }
            if !pl.append {
                let plain = pl.new.iter().all(|t| matches!(t, mc::NewByte::Byte(_)));
                let bytes: Vec<u8> = pl
                    .new
                    .iter()
                    .filter_map(|t| if let mc::NewByte::Byte(b) = t { Some(*b) } else { None })
                    .collect();
                if plain && bytes == pl.expected {
                    rep.add(
                        "warning",
                        &format!("line {}: changes nothing (new bytes = expected bytes)", pl.number),
                        false,
                    );
                }
                for o in pl.offset..pl.offset + pl.expected.len() {
                    let k = (pl.member.to_lowercase(), o);
                    if let Some(prev) = spans.get(&k) {
                        rep.add(
                            "error",
                            &format!("line {}: {} {o:X} is also patched on line {prev}", pl.number, pl.member),
                            false,
                        );
                        break;
                    }
                    spans.insert(k, pl.number);
                }
            }
        }
        let (label, members) = resolve(&spec, opfor)?;
        let (new, named, applied) = apply_patch(&members, &lines);
        for (n, p) in &applied {
            rep.add("error", &format!("line {n}: {p}"), false);
        }
        let np = lines.iter().filter(|p| !p.append).count();
        let na = lines.iter().filter(|p| p.append).count();
        rep.add(
            "info",
            &format!("{np} patch line(s), {na} append(s) applied to {label}"),
            false,
        );
        if applied.is_empty() {
            verify(&members, &new, &mut rep, Some(&named))?;
        }
        if installed {
            match resolve(&movie, opfor) {
                Ok((il, im)) => {
                    let same = new.items.iter().all(|(k, v)| im.get(k) == Some(v));
                    rep.add(
                        if same { "info" } else { "note" },
                        &format!(
                            "installed movie ({il}): {}",
                            if same {
                                "is this result"
                            } else {
                                "differs from this result - the build is stale, or later steps (onslaught_art.py) \
                                 changed it"
                            }
                        ),
                        false,
                    );
                }
                Err(Fail::Err(e)) if e.kind == "MenuToolError" => {
                    rep.add("note", &format!("installed movie not compared: {}", e.msg), false);
                }
                Err(e) => return Err(e),
            }
        }
        rep.show(py)?;
        errors += rep.errors;
    }
    Ok(errors)
}

fn cmd_check(py: &mut Py, args: &[String], opfor: Option<&str>) -> PyResult<usize> {
    if args.is_empty() {
        return Err(menu_err("check needs a movie"));
    }
    let mut got: Vec<(String, Movie)> = Vec::new();
    let mut failure = None;
    for spec in args {
        match resolve(spec, opfor).and_then(|(label, members)| {
            let m = mc::movie_from_members(&members, &label)?;
            Ok((label, m))
        }) {
            Ok(x) => got.push(x),
            Err(e) => {
                failure = Some(e);
                break;
            }
        }
    }
    let refs: Vec<_> = got.iter().map(|(_, m)| m.mref()).collect();
    let results = aptscan::check_many(&refs);
    let mut errors = 0;
    for ((label, _), r) in got.iter().zip(results) {
        let r = r.ok_or(Fail::Python)?;
        let mut rep = Report::new(format!("check {label}"), false);
        rep.add(
            "info",
            &format!(
                "{} instructions, {} functions, {} constants",
                r.instructions, r.functions, r.constants
            ),
            false,
        );
        for (kind, _, text) in &r.findings {
            rep.add("error", &format!("{kind}: {text}"), false);
        }
        rep.show(py)?;
        errors += rep.errors;
    }
    if let Some(e) = failure {
        return Err(e);
    }
    Ok(usize::from(errors > 0))
}

pub fn run(py: &mut Py, cmdline: &[String]) -> PyResult<i32> {
    let mut args = cmdline.to_vec();
    if args
        .first()
        .is_none_or(|a| !matches!(a.as_str(), "verify" | "lint" | "check"))
    {
        py.print(&docstring(SOURCE))?;
        return Ok(i32::from(!args.is_empty()));
    }
    let cmd = args.remove(0);
    let opfor = take(&mut args, "--opfor")?;
    let opfor = opfor.as_deref();
    let n = match cmd.as_str() {
        "verify" => cmd_verify(py, &mut args, opfor)?,
        "lint" => cmd_lint(py, &mut args, opfor)?,
        _ => cmd_check(py, &args, opfor)?,
    };
    Ok(i32::from(n > 0))
}
