//! `bfbc2 aptrefs movie|table|xref|find ...`: `bfbc2-menu-tools\aptrefs.py`'s command line over
//! `bc2_formats::aptscan::refs_many` and the exe's data (`bc2_harness::pe::raw`). A movie the decoder or the scan
//! refuses, and the Python's own exceptions, hand the command line to the Python.

// The Python it ports indexes and slices freely; each index here is bounded by the check before it (as the
// reference's), and the menu group's 372 recorded lines and OPFOR's replay run them.
#![allow(clippy::indexing_slicing)]

use std::collections::BTreeSet;

use bc2_formats::aptscan::{self, Names, hex6};

use super::common::{
    MOVIE_SETS, Movie, OMap, PeImage, bundle_layers, bundle_movies, find_exe, find_opfor, join, menu_err,
    movie_from_members, out_dir, parse_bigf, read_entries, resolve, write_text,
};
use super::json::{J, dumps};
use crate::pyproc::{Fail, Py, PyResult};

/// The usage `bfbc2 aptrefs` prints: `aptrefs.py`'s docstring, reworded when round RT3 retired that command
/// line (the library is `aptrefslib.py`).
const USAGE: &str = include_str!("aptrefs_usage.txt");

const BUILD_STAMP: u32 = 0x4E55_445D;
type Switch = (u64, usize, i64, u64);
const GETTERS: [(&str, u64, &[Switch]); 2] = [
    (
        "menu",
        0x0076_FD20,
        &[
            (0x0076_FE2C, 0xE6, 0x35, 0x0076_FD78),
            (0x0076_FF4C, 0x3B, 0x123, 0x0076_FF14),
        ],
    ),
    ("generic", 0x0095_D934, &[(0x0095_DB68, 0x15F, 1, 0x0095_D98C)]),
];
const KINDS: [&str; 5] = ["extern", "extern_param", "extern_write", "fscommand", "events"];

type Refs = [Names; 5];
type Table = Vec<(String, i64, Option<u64>)>;
type Cases = OMap<&'static str, OMap<i64, u64>>;

fn ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_') && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
}

/// `extern_table(img)`.
fn extern_table(img: &PeImage) -> PyResult<Table> {
    let pair = |off: i64| -> Option<(String, i64)> {
        let o = usize::try_from(off).ok()?;
        if o + 8 > img.b.len() {
            return None;
        }
        let p = u32::from_le_bytes([img.b[o], img.b[o + 1], img.b[o + 2], img.b[o + 3]]);
        let i = u32::from_le_bytes([img.b[o + 4], img.b[o + 5], img.b[o + 6], img.b[o + 7]]);
        let s = img.cstr(u64::from(p), 80)?;
        (ident(&s) && i < 0x1000).then_some((s, i64::from(i)))
    };
    let mut best: Table = Vec::new();
    for va in img.find_cstring("presenceRequestOnline") {
        let va = u32::try_from(va).map_err(|_| Fail::Python)?;
        for j in img.find_dword(va) {
            let j = j as i64;
            if pair(j).is_none() {
                continue;
            }
            let mut lo = j;
            while pair(lo - 8).is_some() {
                lo -= 8;
            }
            let mut hi = j;
            while pair(hi + 8).is_some() {
                hi += 8;
            }
            let mut run = Vec::new();
            let mut o = lo;
            while o < hi + 8 {
                let (s, i) = pair(o).ok_or(Fail::Python)?;
                run.push((s, i, img.va(o as usize)));
                o += 8;
            }
            if run.len() > best.len() {
                best = run;
            }
        }
    }
    if best.len() < 50 {
        return Err(menu_err(format!(
            "{}: the extern / FSCommand name table was not found (is it BFBC2Game.exe?)",
            img.path
        )));
    }
    Ok(best)
}

/// `getter_cases(img)`.
fn getter_cases(img: &PeImage) -> PyResult<Cases> {
    let mut out = Cases::default();
    if img.h.timestamp != BUILD_STAMP {
        return Ok(out);
    }
    for (key, default, switches) in GETTERS {
        let mut cases = OMap::default();
        for &(index, count, first, jumps) in switches {
            let io = img.off(index).ok_or(Fail::Python)?;
            for k in 0..count {
                let t = u64::from(*img.b.get(io + k).ok_or(Fail::Python)?);
                let o = img.off(jumps + 4 * t).ok_or(Fail::Python)?;
                let w = img.b.get(o..o + 4).ok_or(Fail::Python)?;
                let target = u64::from(u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
                if target != default {
                    cases.insert(first + k as i64, target);
                }
            }
        }
        out.insert(key, cases);
    }
    Ok(out)
}

/// `scan_movies(specs, sets, root)`: [(label, refs)].
fn scan_movies(specs: &[String], sets: Option<&BTreeSet<String>>, root: Option<&str>) -> PyResult<Vec<(String, Refs)>> {
    let mut found = Vec::new();
    if !specs.is_empty() {
        let mut got: Vec<(String, Movie)> = Vec::new();
        for spec in specs {
            let (label, members) = resolve(spec, root)?;
            let m = movie_from_members(&members, &label)?;
            got.push((label, m));
        }
        let refs: Vec<_> = got.iter().map(|(_, m)| m.mref()).collect();
        for ((label, _), r) in got.iter().zip(aptscan::refs_many(&refs)) {
            found.push((label.clone(), r.ok_or(Fail::Python)?));
        }
        return Ok(found);
    }
    let root = match root.filter(|r| !r.is_empty()) {
        Some(r) => r.to_owned(),
        None => find_opfor(None)?,
    };
    for (key, rel) in MOVIE_SETS {
        if sets.is_some_and(|s| !s.contains(key)) {
            continue;
        }
        let layers = bundle_layers(&root, rel);
        if layers.is_empty() {
            continue;
        }
        let movies = bundle_movies(&layers)?;
        let mut by_bundle: OMap<String, Vec<String>> = OMap::default();
        for (_, (bundle, entry)) in &movies.items {
            by_bundle.entry_or(bundle.clone(), Vec::new).push(entry.clone());
        }
        for (bundle, entries) in &by_bundle.items {
            let mut keys: Vec<String> = Vec::new();
            for e in entries {
                if !keys.contains(e) {
                    keys.push(e.clone());
                }
            }
            let data = read_entries(bundle, &keys)?;
            let mut sorted = entries.clone();
            sorted.sort();
            let mut read = Vec::new();
            for entry in &sorted {
                let k = keys.iter().position(|x| x == entry).unwrap_or(0);
                let b = data.get(k).map_or(&[][..], Vec::as_slice);
                let m = parse_bigf(b, entry).and_then(|members| movie_from_members(&members, entry));
                match m {
                    Ok(m) => read.push(m),
                    Err(Fail::Err(e)) if e.kind == "MenuToolError" => {}
                    Err(e) => return Err(e),
                }
            }
            let refs: Vec<_> = read.iter().map(Movie::mref).collect();
            for (m, r) in read.iter().zip(aptscan::refs_many(&refs)) {
                found.push((format!("{key}:{}", m.name), r.ok_or(Fail::Python)?));
            }
        }
    }
    Ok(found)
}

/// A cross-reference row while it is gathered.
struct Acc {
    movies: OMap<String, usize>,
    param: bool,
    params: BTreeSet<i64>,
}

/// A cross-reference row.
struct Row {
    kind: &'static str,
    name: String,
    id: Option<i64>,
    case: Option<String>,
    in_exe: Option<bool>,
    params: Vec<i64>,
    movies: OMap<String, usize>,
}

fn cross_reference(
    found: &[(String, Refs)],
    table: &Table,
    cases: &Cases,
    strings: Option<&BTreeSet<String>>,
) -> Vec<Row> {
    let mut ids: OMap<String, i64> = OMap::default();
    for (name, i, _) in table {
        if !ids.contains(name) {
            ids.insert(name.clone(), *i);
        }
    }
    let mut rows: OMap<(&'static str, String), Acc> = OMap::default();
    for (label, refs) in found {
        for (kind, k) in [
            ("extern", 0),
            ("extern", 1),
            ("extern-write", 2),
            ("fscommand", 3),
            ("event", 4),
        ] {
            for (name, uses) in &refs[k].items {
                let r = rows.entry_or((kind, name.clone()), || Acc {
                    movies: OMap::default(),
                    param: false,
                    params: BTreeSet::new(),
                });
                r.movies.insert(label.clone(), uses.len());
                if k == 1 {
                    r.param = true;
                }
                if k == 4 {
                    r.params
                        .extend(uses.iter().filter_map(|u| u.event.as_ref().and_then(|e| e.0)));
                }
            }
        }
    }
    let mut keys: Vec<usize> = (0..rows.items.len()).collect();
    keys.sort_by(|&a, &b| {
        let (ka, kb) = (&rows.items[a].0, &rows.items[b].0);
        (ka.0, ka.1.to_lowercase()).cmp(&(kb.0, kb.1.to_lowercase()))
    });
    let mut out = Vec::new();
    for k in keys {
        let ((kind, name), r) = &rows.items[k];
        let i = ids.get(name).copied();
        let mut case = None;
        if *kind == "extern"
            && let Some(i) = i
        {
            for (key, _, _) in GETTERS {
                if let Some(t) = cases.get(&key).and_then(|c| c.get(&i)) {
                    case = Some(format!("{key} {t:#x}"));
                    break;
                }
            }
            if case.is_none() && !cases.is_empty() {
                let hud_only =
                    r.movies.keys().all(|m| !m.starts_with("menu:")) && r.movies.keys().any(|m| m.contains(':'));
                case = Some(if hud_only { "none - HUD getter not read" } else { "none" }.to_owned());
            }
        }
        out.push(Row {
            kind,
            name: format!("{name}{}", if r.param { "$" } else { "" }),
            id: i,
            case,
            in_exe: strings.map(|s| s.contains(name)),
            params: r.params.iter().copied().collect(),
            movies: r.movies.clone(),
        });
    }
    out
}

fn sorted_movies(m: &OMap<String, usize>) -> String {
    let mut v: Vec<&(String, usize)> = m.items.iter().collect();
    v.sort();
    v.iter()
        .map(|(m, n)| if *n > 1 { format!("{m} x{n}") } else { m.clone() })
        .collect::<Vec<_>>()
        .join(", ")
}

fn print_rows(rows: &[Row], f: &mut Vec<String>) {
    for (kind, title) in [
        ("extern", "extern values read"),
        ("extern-write", "extern values written"),
        ("fscommand", "FSCommands sent"),
        ("event", "engine events handled"),
    ] {
        let sel: Vec<&Row> = rows.iter().filter(|r| r.kind == kind).collect();
        if sel.is_empty() {
            continue;
        }
        f.push(format!("\n== {title} ({})", sel.len()));
        for r in sel {
            let mut bits = vec![match r.id {
                Some(i) => format!("id {i:#x}"),
                None if kind != "event" => "id -".to_owned(),
                None => String::new(),
            }];
            if let Some(c) = r.case.as_ref().filter(|c| !c.is_empty()) {
                bits.push(format!("case {c}"));
            }
            if kind == "event" {
                let p: Vec<String> = r.params.iter().map(ToString::to_string).collect();
                bits.push(format!(
                    "params {}",
                    if p.is_empty() { "?".to_owned() } else { p.join("/") }
                ));
            }
            if r.in_exe == Some(false) {
                bits.push("NOT in the exe".to_owned());
            }
            let bits: Vec<&str> = bits.iter().map(String::as_str).filter(|b| !b.is_empty()).collect();
            f.push(format!(
                "  {:36} {:48} {}",
                r.name,
                bits.join("  "),
                sorted_movies(&r.movies)
            ));
        }
    }
}

fn print_movie(label: &str, refs: &Refs, f: &mut Vec<String>) {
    f.push(format!("\n{label}"));
    for (k, title) in [
        (0, "extern read"),
        (1, "extern read with a parameter (name$param)"),
        (2, "extern written"),
        (3, "FSCommand sent"),
    ] {
        if !refs[k].items.is_empty() {
            let mut v: Vec<&(String, Vec<aptscan::Use>)> = refs[k].items.iter().collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            let parts: Vec<String> = v
                .iter()
                .map(|(n, u)| {
                    if u.len() > 1 {
                        format!("{n} x{}", u.len())
                    } else {
                        n.clone()
                    }
                })
                .collect();
            f.push(format!("  {title}: {}", parts.join(", ")));
        }
    }
    if !refs[4].items.is_empty() {
        f.push("  events handled:".to_owned());
        let mut v: Vec<&(String, Vec<aptscan::Use>)> = refs[4].items.iter().collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        for (n, uses) in v {
            for u in uses {
                let (params, how) = u.event.clone().unwrap_or((None, String::new()));
                let p = params.map_or_else(|| "?".to_owned(), |p| p.to_string());
                f.push(format!("    {n:32} {p} param(s)   {how} at {}", hex6(u.at)));
            }
        }
    }
}

fn refs_json(refs: &Refs) -> J {
    J::Obj(
        KINDS
            .iter()
            .zip(refs)
            .map(|(k, names)| {
                let o = names
                    .items
                    .iter()
                    .map(|(n, uses)| {
                        let list = uses
                            .iter()
                            .map(|u| match &u.event {
                                None => J::Int(u.at),
                                Some((p, how)) => {
                                    J::Arr(vec![J::Int(u.at), p.map_or(J::Null, J::Int), J::Str(how.clone())])
                                }
                            })
                            .collect();
                        (n.clone(), J::Arr(list))
                    })
                    .collect();
                ((*k).to_owned(), J::Obj(o))
            })
            .collect(),
    )
}

fn by_label(found: &[(String, Refs)]) -> J {
    let mut m: OMap<String, usize> = OMap::default();
    for (k, (label, _)) in found.iter().enumerate() {
        m.insert(label.clone(), k);
    }
    J::Obj(
        m.items
            .iter()
            .map(|(l, k)| (l.clone(), refs_json(&found[*k].1)))
            .collect(),
    )
}

/// aptrefs' `take`: every occurrence taken out, the last value kept.
fn take_all(args: &mut Vec<String>, flag: &str) -> PyResult<Vec<String>> {
    let mut vals = Vec::new();
    while let Some(i) = args.iter().position(|a| a == flag) {
        if i + 1 >= args.len() {
            return Err(menu_err(format!("{flag} needs a value")));
        }
        vals.push(args.remove(i + 1));
        args.remove(i);
    }
    Ok(vals)
}

fn take_last(args: &mut Vec<String>, flag: &str) -> PyResult<Option<String>> {
    Ok(take_all(args, flag)?.pop())
}

fn print_lines(py: &mut Py, lines: &[String]) -> PyResult<()> {
    for l in lines {
        py.print(l)?;
    }
    Ok(())
}

#[expect(clippy::too_many_lines, reason = "aptrefs.main, command by command")]
pub fn run(py: &mut Py, cmdline: &[String]) -> PyResult<i32> {
    let mut args = cmdline.to_vec();
    if args
        .first()
        .is_none_or(|a| !matches!(a.as_str(), "movie" | "table" | "xref" | "find"))
    {
        py.print(&USAGE.replace("\r\n", "\n"))?;
        return Ok(i32::from(!args.is_empty()));
    }
    let cmd = args.remove(0);
    let root_given = take_last(&mut args, "--opfor")?;
    let root = root_given.as_deref();
    let exe = take_last(&mut args, "--exe")?;
    let mut js = take_last(&mut args, "--json")?;
    if cmd == "movie" {
        if args.is_empty() {
            return Err(menu_err("movie needs one or more movies"));
        }
        let found = scan_movies(&args, None, root)?;
        let mut lines = Vec::new();
        for (label, refs) in &found {
            print_movie(label, refs, &mut lines);
        }
        print_lines(py, &lines)?;
        if let Some(js) = js.filter(|j| !j.is_empty()) {
            write_text(&js, &dumps(&by_label(&found)))?;
        }
        return Ok(0);
    }
    let img = super::open_exe(&find_exe(root, exe.as_deref())?)?;
    let table = extern_table(&img)?;
    let cases = getter_cases(&img)?;
    if cmd == "table" {
        let at = table.first().and_then(|t| t.2).ok_or(Fail::Python)?;
        py.print(&format!(
            "{}: {} names, table at {at:#x}{}",
            img.path,
            table.len(),
            if cases.is_empty() {
                " (not build 795745: getter cases not read)"
            } else {
                ""
            }
        ))?;
        for (name, i, _) in &table {
            let c: Vec<String> = GETTERS
                .iter()
                .filter_map(|(k, _, _)| cases.get(k).and_then(|m| m.get(i)).map(|t| format!("{k} {t:#x}")))
                .collect();
            py.print(&format!("  {i:#6x}  {name:40} {}", c.join("  ")))?;
        }
        return Ok(0);
    }
    let sets = take_last(&mut args, "--sets")?;
    let movies = take_all(&mut args, "--movie")?;
    let mut out = take_last(&mut args, "--out")?;
    let sets: Option<BTreeSet<String>> = sets
        .filter(|s| !s.is_empty())
        .map(|s| s.split(',').map(str::to_owned).collect());
    let found = scan_movies(&movies, sets.as_ref(), root)?;
    let strings: BTreeSet<String> = img.strings(3).into_iter().map(|(s, _)| s).collect();
    let rows = cross_reference(&found, &table, &cases, Some(&strings));
    if cmd == "find" {
        let Some(name) = args.first() else {
            return Err(menu_err("find needs a name"));
        };
        let want = name.to_lowercase();
        let sel: Vec<Row> = rows
            .into_iter()
            .filter(|r| r.name.to_lowercase().contains(&want))
            .collect();
        let mut lines = Vec::new();
        print_rows(&sel, &mut lines);
        print_lines(py, &lines)?;
        let tab: Vec<String> = table
            .iter()
            .filter(|(n, _, _)| n.to_lowercase().contains(&want))
            .map(|(n, i, _)| format!("{n} {i:#x}"))
            .collect();
        if !tab.is_empty() {
            py.print(&format!("\n== in the exe's name table: {}", tab.join(", ")))?;
        }
        if sel.is_empty() && tab.is_empty() {
            py.print(&format!(
                "\"{name}\": not used by the {} movies read, not in the exe's name table",
                found.len()
            ))?;
        }
        return Ok(0);
    }
    if out.as_deref().is_none_or(str::is_empty) {
        let root = match root.filter(|r| !r.is_empty()) {
            Some(r) => r.to_owned(),
            None => find_opfor(None)?,
        };
        out = Some(join(&out_dir(&root)?, &["aptrefs.txt"]));
        if js.as_deref().is_none_or(str::is_empty) {
            js = Some(join(&out_dir(&root)?, &["aptrefs.json"]));
        }
    }
    let out = out.unwrap_or_default();
    let mut text = vec![format!(
        "aptrefs: {} movies, exe {} ({} names{})",
        found.len(),
        img.path,
        table.len(),
        if cases.is_empty() {
            "; not build 795745: getter cases not read"
        } else {
            ""
        }
    )];
    print_rows(&rows, &mut text);
    text.push("\n\n== per movie".to_owned());
    for (label, refs) in &found {
        print_movie(label, refs, &mut text);
    }
    write_text(&out, &(text.join("\n") + "\n"))?;
    let js = js.filter(|j| !j.is_empty());
    if let Some(js) = &js {
        let names = J::Arr(
            rows.iter()
                .map(|r| {
                    J::Obj(vec![
                        ("kind".to_owned(), J::Str(r.kind.to_owned())),
                        ("name".to_owned(), J::Str(r.name.clone())),
                        ("id".to_owned(), r.id.map_or(J::Null, J::Int)),
                        ("getter_case".to_owned(), r.case.clone().map_or(J::Null, J::Str)),
                        ("in_exe".to_owned(), r.in_exe.map_or(J::Null, J::Bool)),
                        (
                            "params".to_owned(),
                            J::Arr(r.params.iter().map(|&p| J::Int(p)).collect()),
                        ),
                        (
                            "movies".to_owned(),
                            J::Obj(
                                r.movies
                                    .items
                                    .iter()
                                    .map(|(m, n)| (m.clone(), J::Int(*n as i64)))
                                    .collect(),
                            ),
                        ),
                    ])
                })
                .collect(),
        );
        let tab = J::Arr(
            table
                .iter()
                .map(|(n, i, _)| {
                    J::Obj(vec![
                        ("name".to_owned(), J::Str(n.clone())),
                        ("id".to_owned(), J::Int(*i)),
                    ])
                })
                .collect(),
        );
        let doc = J::Obj(vec![
            ("movies".to_owned(), by_label(&found)),
            ("names".to_owned(), names),
            ("table".to_owned(), tab),
        ]);
        write_text(js, &dumps(&doc))?;
    }
    let mut kinds: OMap<&str, usize> = OMap::default();
    for r in &rows {
        let n = kinds.get(&r.kind).copied().unwrap_or(0);
        kinds.insert(r.kind, n + 1);
    }
    let mut ks: Vec<&(&str, usize)> = kinds.items.iter().collect();
    ks.sort();
    let parts: Vec<String> = ks.iter().map(|(k, v)| format!("{v} {k}")).collect();
    py.print(&format!(
        "{} movies: {} -> {out}{}",
        found.len(),
        parts.join(", "),
        js.as_ref().map_or(String::new(), |j| format!(" and {j}"))
    ))?;
    let unanswered: Vec<&str> = rows
        .iter()
        .filter(|r| r.kind == "extern" && r.case.as_deref() == Some("none"))
        .map(|r| r.name.as_str())
        .collect();
    if !unanswered.is_empty() {
        py.print(&format!(
            "{} extern name(s) main menu movies read with no case in the menu or generic getter: {}{}",
            unanswered.len(),
            unanswered.iter().take(15).copied().collect::<Vec<_>>().join(", "),
            if unanswered.len() > 15 { " ..." } else { "" }
        ))?;
    }
    Ok(0)
}
