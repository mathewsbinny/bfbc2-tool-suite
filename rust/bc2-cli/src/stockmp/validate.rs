//! `validate.validate` (round O5): the offline checks of a built AI layer - DbxEdit's check and round trips (in
//! process, logged as `bfbc2core.dbxedit` logs them), the structure, the changes and the references read by
//! `bc2_stockmp::checks` (round E4a), the packed assets and bundle, the Rush stages, the navigation on NavKit's
//! path-data model and the staged path data - each result printed and `validation.json` written as the reference
//! words and writes them.
//!
//! Handed to the Python (whatever was done is done again there; nothing is printed or logged here before): a
//! report, configuration or file of a shape the readings refuse or the reference words itself (an exception text only
//! Python makes), DbxEdit output without its counts.

use std::fs;

use bc2_formats::dbx::{self, parse_tree};
use bc2_formats::dbxedit;
use bc2_pyrt::json::{self as pyjson, Value};
use bc2_stockmp::checks::{self, Shown};
use bc2_stockmp::files;
use bc2_stockmp::guid::new_guid;
use bc2_stockmp::install::join;
use regex::Regex;

use super::Paths;
use super::build::{KINDS, sector_offsets};
use super::generate::{Snapper, py_str};
use super::level::{common, find_entry};
use super::pyx;
use crate::pyfmt::{float_repr, str_repr};
use crate::pylog;
use crate::pyproc::{Fail, Py, PyResult, os_error};

const ORDER_NODE: f64 = 15.0;
const SPAWN_NODE: f64 = 6.5;
const SPAWN_LIFT: f64 = 0.3;
const PATHFINDING: [&str; 2] = bc2_stockmp::author::PATHFINDING;

/// DbxEdit's runs, their log records held until the validation is the port's (a hand-off logs its own).
struct DbxEdit {
    done: Vec<(Vec<String>, i32, String)>,
    log: Vec<(Vec<String>, Vec<(String, String)>)>,
}

impl DbxEdit {
    /// `_prefetch_dbxedit(runs)`: the read-only runs at once.
    fn prefetch(&mut self, runs: Vec<Vec<String>>) {
        let path = pylog::log_path("dbxedit");
        let outs = dbxedit::run_many(&runs, path.as_deref());
        for (args, o) in runs.into_iter().zip(outs) {
            self.log.push((args.clone(), o.log));
            self.done.push((args, o.code, o.stdout + &o.stderr));
        }
    }

    /// `_dbxedit(*args)`: a prefetched run's result (taken out), else the run now.
    fn run(&mut self, args: &[&str]) -> (i32, String) {
        let key: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        if let Some(i) = self.done.iter().position(|(a, _, _)| *a == key) {
            let (_, code, out) = self.done.remove(i);
            return (code, out);
        }
        let path = pylog::log_path("dbxedit");
        let o = dbxedit::run(&key, path.as_deref());
        self.log.push((key, o.log));
        (o.code, o.stdout + &o.stderr)
    }

    /// The log records, in the runs' order.
    fn flush(&self) {
        let path = pylog::log_path("dbxedit");
        for (args, lines) in &self.log {
            pylog::append(path.as_deref(), "dbxedit", args, lines);
        }
    }
}

/// `_counts(out)`.
fn counts(out: &str) -> (Option<u64>, Option<u64>) {
    let Ok(re) = Regex::new(r"(\d+) error\(s\), (\d+) warning\(s\)") else {
        return (None, None);
    };
    match re.captures(out) {
        Some(m) => (
            m.get(1).and_then(|x| x.as_str().parse().ok()),
            m.get(2).and_then(|x| x.as_str().parse().ok()),
        ),
        None => (None, None),
    }
}

fn opt_num(n: Option<u64>) -> String {
    n.map_or_else(|| "None".to_owned(), |v| v.to_string())
}

type Check = (bool, String);

/// `check_dbxedit(orig, authored, bundle_dir, orig_bundle_dir, mp_common)`.
fn check_dbxedit(dx: &mut DbxEdit, orig: &str, auth: &str, bdir: &str, sdir: &str, mp: &str) -> PyResult<Check> {
    let (_, a) = dx.run(&["check", auth, "--bundle", bdir, "--bundle", mp]);
    let (_, o) = dx.run(&["check", orig, "--bundle", sdir, "--bundle", mp]);
    let (ea, wa) = counts(&a);
    let (eo, wo) = counts(&o);
    let mut ok = match ea {
        None => false,
        Some(ea) => {
            // wa <= wo with wo None: the reference's TypeError
            let (Some(wa), Some(wo)) = (wa, wo) else {
                return Err(Fail::Python);
            };
            ea <= eo.unwrap_or(0) && wa <= wo
        }
    };
    let errs = |t: &str| {
        let mut v: Vec<&str> = bc2_pyrt::py::splitlines(t)
            .into_iter()
            .filter(|l| l.starts_with("ERROR"))
            .collect();
        v.sort_unstable();
        v.into_iter().map(str::to_owned).collect::<Vec<String>>()
    };
    ok = ok && errs(&a) == errs(&o);
    Ok((
        ok,
        format!(
            "authored {} error(s), {} warning(s); stock {} / {}",
            opt_num(ea),
            opt_num(wa),
            opt_num(eo),
            opt_num(wo)
        ),
    ))
}

/// `check_roundtrip(authored, tmp)`.
fn check_roundtrip(dx: &mut DbxEdit, auth: &str, tmp: &str) -> PyResult<Check> {
    let (_, out) = dx.run(&["roundtrip", auth]);
    let ok1 = out.contains("1 identical, 0 different");
    let a = join(tmp, "rt.dbx");
    fs::copy(auth, &a).map_err(|e| os_error(auth, &e))?;
    dx.run(&["xml", &a]);
    let xml = format!("{}.xml", a.get(..a.len() - 4).unwrap_or(""));
    let back = join(tmp, "rt_back.dbx");
    let (rc2, _) = dx.run(&["import", &xml, &back, "--like", &a]);
    let ok2 = rc2 == 0 && {
        let b = fs::read(&back).map_err(|_| Fail::Python)?;
        b == fs::read(&a).map_err(|_| Fail::Python)?
    };
    let second = if ok2 {
        "identical".to_owned()
    } else {
        format!("DIFFERENT (exit {rc2})")
    };
    Ok((
        ok1 && ok2,
        format!(
            "roundtrip {}; xml -> import {second}",
            if ok1 { "identical" } else { "DIFFERENT" }
        ),
    ))
}

fn read(path: &str) -> PyResult<Vec<u8>> {
    fs::read(path).map_err(|e| os_error(path, &e))
}

fn pylist(xs: &[String]) -> String {
    pyx::str_list(xs)
}

/// `check_structure(orig, authored, c)`.
fn check_structure(orig: &[u8], auth: &[u8]) -> PyResult<Check> {
    let s = checks::structure(orig, auth).map_err(|_| Fail::Python)?;
    let mut problems = Vec::new();
    if let Some(first) = s.twice.first() {
        problems.push(format!(
            "{} GUIDs used by more than one instance (e.g. {first}) - authored twice?",
            s.twice.len()
        ));
    }
    for (g, r) in &s.local_unresolved {
        problems.push(format!("{g}: local reference {r} unresolved"));
    }
    if s.objects_twice > 0 {
        problems.push(format!("{} entities listed twice in Objects", s.objects_twice));
    }
    if s.unlisted > 0 {
        problems.push(format!("{} new instances not in Objects", s.unlisted));
    }
    for (side, r) in &s.event_unresolved {
        problems.push(format!("event {side} {r} unresolved"));
    }
    if s.has_ai != "true" {
        problems.push("HasAI is not true".to_owned());
    }
    let low: Vec<String> = s.pathfinding.iter().map(|p| p.to_lowercase()).collect();
    if !s.pathfinding.iter().all(|p| p.is_ascii()) {
        return Err(Fail::Python);
    }
    if low != PATHFINDING.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>() {
        problems.push(format!("PathfindingTypes {}", pylist(&s.pathfinding)));
    }
    let detail = if problems.is_empty() {
        format!(
            "{} new instances, all in Objects; {} event connections resolve; HasAI true; PathfindingTypes infantry + \
             vehicles",
            s.new, s.events
        )
    } else {
        problems.iter().take(8).cloned().collect::<Vec<_>>().join("; ")
    };
    Ok((problems.is_empty(), detail))
}

/// `repr()` of `_shown(leaf)`: `(kind, 'value', (('name', 'value'), ...))` or `None`.
fn shown(x: Option<&Shown>) -> String {
    let Some((kind, value, attrs)) = x else {
        return "None".to_owned();
    };
    let items: Vec<String> = attrs
        .iter()
        .map(|(k, v)| format!("({}, {})", str_repr(k), str_repr(v)))
        .collect();
    let attrs = match items.len() {
        0 => "()".to_owned(),
        1 => format!("({},)", items[0]),
        _ => format!("({})", items.join(", ")),
    };
    format!("({kind}, {}, {attrs})", str_repr(value))
}

/// `check_changes(orig, authored, c, tmp)`.
fn check_changes(orig: &[u8], auth: &[u8], c: &Value) -> PyResult<Check> {
    let facts = checks::changes(orig, auth).map_err(|_| Fail::Python)?;
    let Ok(idx) = Regex::new(r"\[\d+\]") else {
        return Err(Fail::Python);
    };
    let mut bad = Vec::new();
    let mut changed: Vec<(String, u64)> = Vec::new();
    for ch in &facts {
        if ch.removed {
            bad.push(format!("{} removed", ch.guid));
            continue;
        }
        for arr in &ch.arrays_changed {
            bad.push(format!("LevelData.{arr}: existing items changed"));
        }
        for (k, x, y) in &ch.leaves {
            let field = idx.replace_all(k, "").trim_matches('/').to_owned();
            if field == "HasAI" || field == "PlayerType" {
                match changed.iter_mut().find(|(f, _)| *f == field) {
                    Some(slot) => slot.1 += 1,
                    None => changed.push((field, 1)),
                }
            } else {
                bad.push(format!("{} {k}: {} -> {}", ch.guid, shown(x.as_ref()), shown(y.as_ref())));
            }
        }
    }
    let human = c.get("stock_spawners").and_then(Value::as_str) == Some("human");
    if changed.iter().any(|(f, n)| f == "PlayerType" && *n > 0) && !human {
        bad.push("PlayerType changed although stock_spawners is not \"human\"".to_owned());
    }
    let detail = if bad.is_empty() {
        let items: Vec<String> = changed.iter().map(|(f, n)| format!("{}: {n}", str_repr(f))).collect();
        format!("existing instances unchanged except {{{}}}", items.join(", "))
    } else {
        bad.iter().take(8).cloned().collect::<Vec<_>>().join("; ")
    };
    Ok((bad.is_empty(), detail))
}

/// `check_external(orig, authored, folders)`.
fn check_external(orig: &[u8], auth: &[u8], folders: &[&str]) -> PyResult<Check> {
    let wanted = checks::external(orig, auth).map_err(|_| Fail::Python)?;
    // _partition_index: the first folder's file where two have one
    let mut idx: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for top in folders {
        for (rel, p) in files::folder_index(top).map_err(|_| Fail::Python)? {
            if seen.insert(rel.clone()) {
                idx.push((rel, p));
            }
        }
    }
    let by: std::collections::HashMap<&str, &str> = idx.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut bad = Vec::new();
    let mut cache: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for r in &wanted {
        let (part, guid) = r.rsplit_once('/').ok_or(Fail::Python)?;
        let Some(&p) = by.get(part) else {
            bad.push(format!("{r}: partition not found"));
            continue;
        };
        if !cache.contains_key(p) {
            let data = read(p)?;
            let t = parse_tree(&data).map_err(|_| Fail::Python)?;
            let g = t
                .instances()
                .map(|e| e.attr("guid").unwrap_or("").to_lowercase())
                .collect();
            cache.insert(p.to_owned(), g);
        }
        if !cache.get(p).is_some_and(|g| g.contains(guid)) {
            bad.push(format!(
                "{r}: instance not in {}",
                bc2_pyrt::py::basename(p)
            ));
        }
    }
    if wanted.iter().any(|w| !w.is_ascii()) {
        return Err(Fail::Python);
    }
    let detail = if bad.is_empty() {
        format!(
            "{} new references into other partitions all resolve: {}",
            wanted.len(),
            wanted.join(", ")
        )
    } else {
        bad.join("; ")
    };
    Ok((bad.is_empty(), detail))
}

/// `check_assets(build_dir, var, c, lv_upper)`.
#[allow(clippy::too_many_lines, reason = "check_assets, step for step")]
fn check_assets(bdir: &str, var: &Value, so: &[(String, (i128, i128))], template_pack: &[String], c: &Value, upper: &str) -> PyResult<Check> {
    let mut bad = Vec::new();
    let m = join(bdir, "fbrb.manifest");
    let text = String::from_utf8(read(&m)?).map_err(|_| Fail::Python)?;
    let lines: Vec<&str> = bc2_pyrt::py::splitlines(&text)
        .into_iter()
        .filter(|l| !bc2_pyrt::py::strip(l).is_empty())
        .collect();
    let added: Vec<String> = match var.get("added_entries") {
        None => Vec::new(),
        Some(v) => v
            .as_list()
            .ok_or(Fail::Python)?
            .iter()
            .map(|x| x.as_str().map(str::to_owned).ok_or(Fail::Python))
            .collect::<PyResult<_>>()?,
    };
    let tail: Vec<String> = if added.is_empty() {
        Vec::new()
    } else {
        lines[lines.len().saturating_sub(added.len())..]
            .iter()
            .map(|l| l.split('|').next().unwrap_or("").to_owned())
            .collect()
    };
    if !tail.iter().chain(&added).all(|x| x.is_ascii()) {
        return Err(Fail::Python);
    }
    let lt: Vec<String> = tail.iter().map(|x| x.to_ascii_lowercase()).collect();
    let la: Vec<String> = added.iter().map(|x| x.to_ascii_lowercase()).collect();
    if lt != la {
        bad.push(format!("added entries are not the manifest tail ({})", pylist(&tail)));
    }
    for (kind, _) in KINDS {
        let Some(p) = find_entry(bdir, &format!("Levels/{upper}/Precomputed/{kind}/SectorOffsets.dbx")) else {
            bad.push(format!("{kind} SectorOffsets missing"));
            continue;
        };
        let data = read(&p)?;
        let t = parse_tree(&data).map_err(|_| Fail::Python)?;
        let e = t.instances().next().ok_or(Fail::Python)?;
        let name = e.child("Name").ok_or(Fail::Python)?.value().map_err(|_| Fail::Python)?;
        if name != format!("Levels/{upper}/Precomputed/{kind}/SectorOffsets") {
            bad.push(format!("{kind}: Name {name}"));
        }
        let want = so.iter().find(|(k, _)| k == kind).map(|(_, v)| *v).ok_or(Fail::Python)?;
        for (fld, v) in [("XOffset", want.0), ("ZOffset", want.1)] {
            let c = e.child(fld).ok_or(Fail::Python)?;
            let (size, vals) = dbx::numbers(c.raw()).map_err(|_| Fail::Python)?;
            let differs = size != 4 || {
                let first = vals.first().ok_or(Fail::Python)?;
                let b = <[u8; 4]>::try_from(*first).map_err(|_| Fail::Python)?;
                i128::from(i32::from_be_bytes(b)) != v
            };
            if differs {
                bad.push(format!("{kind} {fld} is not the integer {v}"));
            }
        }
    }
    for p in template_pack {
        if find_entry(bdir, p).is_none() {
            bad.push(format!("{p} not packed"));
        }
    }
    if let Some(first) = template_pack.first() {
        let t = find_entry(bdir, first);
        let tref = c.get("template").and_then(Value::as_str).ok_or(Fail::Python)?;
        let g = tref.rsplit('/').next().unwrap_or("").to_lowercase();
        if !tref.contains('/') || !tref.is_ascii() {
            return Err(Fail::Python);
        }
        if let Some(t) = t {
            let data = read(&t)?;
            let tree = parse_tree(&data).map_err(|_| Fail::Python)?;
            if !tree.instances().any(|e| e.attr("guid").unwrap_or("").to_lowercase() == g) {
                bad.push("the template instance is not in the packed template partition".to_owned());
            }
        }
    }
    let detail = if bad.is_empty() {
        let items: Vec<String> = so.iter().map(|(k, v)| format!("{k} {},{}", v.0, v.1)).collect();
        format!(
            "SectorOffsets {} as 4-byte integers; {} template partitions packed; added entries at the tail",
            items.join(", "),
            template_pack.len()
        )
    } else {
        bad.join("; ")
    };
    Ok((bad.is_empty(), detail))
}

fn stage_num(v: Option<&Value>) -> PyResult<i128> {
    match v {
        None | Some(Value::Null) => Ok(0),
        Some(Value::Int(i)) => Ok(*i),
        _ => Err(Fail::Python),
    }
}

/// `check_stages(authored, c)`.
fn check_stages(auth: &[u8], c: &Value) -> PyResult<Check> {
    let stages: &[Value] = match c.get("stages") {
        Some(v) if v.truthy() => v.as_list().ok_or(Fail::Python)?,
        _ => &[],
    };
    let spawners = c.get("spawners").and_then(Value::as_list).ok_or(Fail::Python)?;
    let orders = c.get("orders").and_then(Value::as_list).ok_or(Fail::Python)?;
    let any_stage = spawners.iter().any(|s| s.get("stage").is_some_and(Value::truthy));
    if stages.is_empty() && !any_stage {
        return Ok((true, "no stages (one-stage mode)".to_owned()));
    }
    let lv = c.get("lv").and_then(Value::as_str).ok_or(Fail::Python)?;
    let mut later: Vec<(&Value, String)> = Vec::new();
    for (role, list) in [("spawner", spawners), ("order", orders)] {
        for s in list {
            if stage_num(s.get("stage"))? > 0 {
                let id = py_str(Some(s.get("id").ok_or(Fail::Python)?))?;
                // `s in c['spawners']`: an order equal to a spawner would count as one
                let role = if role == "order" && spawners.contains(s) { "spawner" } else { role };
                later.push((s, new_guid("opfor:stockmp", &[lv, role, &id])));
            }
        }
    }
    let mut srcs = Vec::new();
    for st in stages {
        let src = st
            .get("advance")
            .and_then(|a| a.get("source"))
            .and_then(Value::as_str)
            .ok_or(Fail::Python)?;
        if !src.is_ascii() {
            return Err(Fail::Python);
        }
        srcs.push(src.to_ascii_uppercase());
    }
    let mut guids: Vec<String> = later.iter().map(|(_, g)| g.clone()).collect();
    guids.extend(srcs.iter().cloned());
    let facts = checks::instance_facts(auth, &guids).map_err(|_| Fail::Python)?;
    // the Python reading where it would raise: a later-stage instance missing, or without a readable Enabled
    if !facts.iter().take(later.len()).all(|f| f.as_ref().is_some_and(|f| f.1.is_some())) {
        return Err(Fail::Python);
    }
    let mut bad = Vec::new();
    for ((s, _), f) in later.iter().zip(&facts) {
        if f.as_ref().and_then(|f| f.1.as_deref()) != Some("false") {
            bad.push(format!(
                "{} (stage {}) starts enabled",
                py_str(s.get("id"))?,
                py_str(s.get("stage"))?
            ));
        }
    }
    for ((st, src), f) in stages.iter().zip(&srcs).zip(facts.iter().skip(later.len())) {
        let index = py_str(st.get("index"))?;
        match f {
            None => bad.push(format!("stage {index}: source {src} missing")),
            Some((t, _)) => {
                if !t.as_deref().unwrap_or("").ends_with("MultipleTriggerEntityData") {
                    let shown = t.clone().unwrap_or_else(|| "None".to_owned());
                    bad.push(format!("stage {index}: source is a {shown}"));
                }
            }
        }
    }
    let detail = if bad.is_empty() {
        format!("{} stage switches; later stages start disabled", stages.len())
    } else {
        bad.iter().take(8).cloned().collect::<Vec<_>>().join("; ")
    };
    Ok((bad.is_empty(), detail))
}

/// `check_packed(out_bundle, build_dir, tmp)`.
fn check_packed(bundle: &str, bdir: &str) -> PyResult<Check> {
    let (n, bad) = files::packed_check(bundle, bdir).map_err(|_| Fail::Python)?;
    Ok((
        bad == 0,
        if bad == 0 {
            format!("{n} files unpack identical")
        } else {
            format!("{bad} of {n} files differ after packing")
        },
    ))
}

fn triple(v: Option<&Value>) -> PyResult<[f64; 3]> {
    let l = v.and_then(Value::as_list).ok_or(Fail::Python)?;
    if l.len() != 3 {
        return Err(Fail::Python);
    }
    let mut out = [0.0; 3];
    for (o, x) in out.iter_mut().zip(l) {
        *o = match x {
            #[allow(clippy::cast_precision_loss, reason = "within 2^53: exact")]
            Value::Int(i) if i.unsigned_abs() < (1u128 << 53) => *i as f64,
            Value::Float(f) if f.is_finite() => *f,
            _ => return Err(Fail::Python),
        };
    }
    Ok(out)
}

/// `check_navigation(c, hpd)`.
#[allow(clippy::too_many_lines, reason = "check_navigation, step for step")]
fn check_navigation(c: &Value, hpd: &str) -> PyResult<Check> {
    if !bc2_pyrt::py::isfile(hpd) {
        return Ok((false, format!("no path data at {hpd}")));
    }
    let snap = Snapper::new(hpd)?;
    let spawners = c.get("spawners").and_then(Value::as_list).ok_or(Fail::Python)?;
    let orders = c.get("orders").and_then(Value::as_list).ok_or(Fail::Python)?;
    let mut bad = Vec::new();
    let mut groups: Vec<(String, u32)> = Vec::new();
    for s in spawners {
        let id = s.get("id").and_then(Value::as_str).ok_or(Fail::Python)?.to_owned();
        let p = triple(s.get("pos"))?;
        let q = [p[0], p[1] - SPAWN_LIFT, p[2]];
        let n = snap.nearest(q);
        let on = snap.pd.on_area(q);
        if !on && n.dist > SPAWN_NODE {
            bad.push(format!(
                "spawner {id}: not on the navigation data (node {} m)",
                pyx::fixed(n.dist, 1)?
            ));
        }
        #[allow(clippy::cast_precision_loss, reason = "group sizes")]
        if (n.size as f64) < 0.25 * snap.biggest as f64 {
            bad.push(format!("spawner {id}: on a small island ({} nodes)", n.size));
        }
        let g = snap.group_of(n.index);
        match groups.iter_mut().find(|(k, _)| *k == id) {
            Some(slot) => slot.1 = g,
            None => groups.push((id, g)),
        }
    }
    let mut listed: Vec<(String, u64)> = Vec::new();
    let mut worst = 0.0f64;
    for o in orders {
        let oid = py_str(o.get("id"))?;
        let p = triple(o.get("pos"))?;
        let n = snap.nearest(p);
        if n.dist > worst {
            worst = n.dist; // max(worst, dist)
        }
        if n.dist > ORDER_NODE {
            bad.push(format!(
                "order {oid}: no route node within {} m ({} m)",
                float_repr(ORDER_NODE),
                pyx::fixed(n.dist, 1)?
            ));
        }
        let og = snap.group_of(n.index);
        let soldiers = o.get("soldiers").and_then(Value::as_list).ok_or(Fail::Python)?;
        for x in soldiers {
            let x = x.as_str().ok_or(Fail::Python)?;
            match listed.iter_mut().find(|(k, _)| k == x) {
                Some(slot) => slot.1 += 1,
                None => listed.push((x.to_owned(), 1)),
            }
            if groups.iter().find(|(k, _)| k == x).map(|(_, g)| *g) != Some(og) {
                bad.push(format!(
                    "order {oid}: spawner {x} is not route-connected to the objective"
                ));
            }
        }
    }
    let mut unlisted = Vec::new();
    for s in spawners {
        let id = s.get("id").and_then(Value::as_str).ok_or(Fail::Python)?;
        if !listed.iter().any(|(k, n)| k == id && *n > 0) {
            unlisted.push(id.to_owned());
        }
    }
    if !unlisted.is_empty() {
        bad.push(format!(
            "{} AI spawners in no order: {}",
            unlisted.len(),
            pylist(&unlisted[..unlisted.len().min(6)])
        ));
    }
    let detail = if bad.is_empty() {
        format!(
            "{} spawners on the navigation data, {} orders with a route node within {} m, every spawner \
             route-connected to its objectives",
            spawners.len(),
            orders.len(),
            pyx::fixed(worst, 1)?
        )
    } else {
        bad.iter().take(8).cloned().collect::<Vec<_>>().join("; ")
    };
    Ok((bad.is_empty(), detail))
}

/// A check's exception as the reference words it (`f'{type(e).__name__}: {e}'`); one only Python words: handed off.
fn caught(r: PyResult<Check>) -> PyResult<Check> {
    match r {
        Err(Fail::Err(e)) => Ok((false, format!("{}: {}", e.kind, e.msg))),
        r => r,
    }
}

/// A unique folder in the temporary folder (`tempfile.mkdtemp(prefix='stockmp_validate_')`).
fn mkdtemp() -> PyResult<String> {
    let base = std::env::temp_dir();
    let pid = std::process::id();
    for n in 0..1000u32 {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let p = base.join(format!("stockmp_validate_{:08x}", (t ^ pid.rotate_left(13)).wrapping_add(n)));
        if fs::create_dir(&p).is_ok() {
            return Ok(p.to_string_lossy().into_owned());
        }
    }
    Err(Fail::Python)
}

/// `validate.validate(paths, c)`: whether every check passed.
#[allow(clippy::too_many_lines, reason = "validate.validate, step for step")]
pub fn validate(py: &mut Py, paths: &Paths, c: &Value) -> PyResult<bool> {
    let report = join(&paths.root, "build_report.json");
    let data = read(&report)?;
    let rep = pyjson::loads_utf8(&data).ok_or(Fail::Python)?;
    let tmp = mkdtemp()?;
    let r = run_checks(py, paths, c, &rep, &tmp);
    let _ = fs::remove_dir_all(&tmp);
    let (results, dx) = r?;
    dx.flush();
    let passed = results.iter().all(|(_, ok, _)| *ok);
    let out = Value::dict([
        ("passed", Value::Bool(passed)),
        (
            "results",
            Value::List(
                results
                    .iter()
                    .map(|(n, ok, d)| {
                        Value::dict([
                            ("check", Value::Str(n.clone())),
                            ("ok", Value::Bool(*ok)),
                            ("detail", Value::Str(d.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
    ]);
    let path = join(&paths.root, "validation.json");
    let text = pyjson::dumps(&out, Some(1)).replace('\n', "\r\n");
    fs::write(&path, text.as_bytes()).map_err(|e| os_error(&path, &e))?;
    Ok(passed)
}

type Results = Vec<(String, bool, String)>;

#[allow(clippy::too_many_lines, reason = "validate.validate's loop, step for step")]
fn run_checks(py: &mut Py, paths: &Paths, c: &Value, rep: &Value, tmp: &str) -> PyResult<(Results, DbxEdit)> {
    let mut results: Results = Vec::new();
    let mp = common(paths, "mp_common")?;
    let mut so: Vec<(String, (i128, i128))> = Vec::new();
    let Some(Value::Dict(sor)) = rep.get("sector_offsets") else {
        return Err(Fail::Python);
    };
    for (k, v) in sor {
        let x = v.get("x").and_then(Value::as_int).ok_or(Fail::Python)?;
        let z = v.get("z").and_then(Value::as_int).ok_or(Fail::Python)?;
        if matches!(v.get("x"), Some(Value::Bool(_))) || matches!(v.get("z"), Some(Value::Bool(_))) {
            return Err(Fail::Python);
        }
        so.push((k.clone(), (x, z)));
    }
    let Some(Value::Dict(variants)) = rep.get("variants") else {
        return Err(Fail::Python);
    };
    let template_pack: Vec<String> = match rep.get("template_pack") {
        None => Vec::new(),
        Some(v) => v
            .as_list()
            .ok_or(Fail::Python)?
            .iter()
            .map(|x| x.as_str().map(str::to_owned).ok_or(Fail::Python))
            .collect::<PyResult<_>>()?,
    };
    let build = join(&paths.root, "build");
    let mut named = Vec::new();
    for (v, var) in variants {
        let entry = var.get("entry").and_then(Value::as_str).ok_or(Fail::Python)?;
        let last = entry.rsplit('/').next().unwrap_or("");
        let upper = last.get(..last.len().saturating_sub(4)).ok_or(Fail::Python)?.to_owned();
        if last.len() < 4 {
            return Err(Fail::Python);
        }
        named.push((v.clone(), var, upper));
    }
    let mut dx = DbxEdit {
        done: Vec::new(),
        log: Vec::new(),
    };
    let mut pre = Vec::new();
    for (v, _, upper) in &named {
        let orig = join(&build, &format!("{upper}_{v}.original.dbx"));
        let auth = join(&build, &format!("{upper}_{v}.authored.dbx"));
        let s = |x: &str| x.to_owned();
        pre.push(vec![s("check"), auth.clone(), s("--bundle"), join(&build, v), s("--bundle"), mp.clone()]);
        pre.push(vec![s("check"), orig, s("--bundle"), paths.src(v), s("--bundle"), mp.clone()]);
        pre.push(vec![s("roundtrip"), auth]);
    }
    dx.prefetch(pre);
    for (v, var, upper) in &named {
        let orig = join(&build, &format!("{upper}_{v}.original.dbx"));
        let auth = join(&build, &format!("{upper}_{v}.authored.dbx"));
        let bdir = join(&build, v);
        let sdir = paths.src(v);
        let checks_of: [(&str, Box<dyn FnOnce(&mut DbxEdit) -> PyResult<Check> + '_>); 8] = [
            ("DbxEdit check", Box::new(|dx: &mut DbxEdit| check_dbxedit(dx, &orig, &auth, &bdir, &sdir, &mp))),
            ("round trips", Box::new(|dx: &mut DbxEdit| check_roundtrip(dx, &auth, tmp))),
            ("structure (second reader)", Box::new(|_: &mut DbxEdit| check_structure(&read(&orig)?, &read(&auth)?))),
            ("only intended changes", Box::new(|_: &mut DbxEdit| check_changes(&read(&orig)?, &read(&auth)?, c))),
            (
                "references into other partitions",
                Box::new(|_: &mut DbxEdit| check_external(&read(&orig)?, &read(&auth)?, &[&bdir, &mp])),
            ),
            (
                "packed assets",
                Box::new(|_: &mut DbxEdit| check_assets(&bdir, var, &so, &template_pack, c, upper)),
            ),
            ("Rush stages", Box::new(|_: &mut DbxEdit| check_stages(&read(&auth)?, c))),
            ("packed bundle", Box::new(|_: &mut DbxEdit| check_packed(&paths.out_bundle(v, "out"), &bdir))),
        ];
        for (name, f) in checks_of {
            let (ok, detail) = caught(f(&mut dx))?;
            py.print(&format!("{} {v}: {name}: {detail}", if ok { "ok  " } else { "FAIL" }))?;
            results.push((format!("{v}: {name}"), ok, detail));
        }
    }
    let layer = c.get("layer");
    let minimal = matches!(layer, Some(Value::Str(s)) if s == "minimal");
    if layer.is_some() && !matches!(layer, Some(Value::Str(_))) {
        return Err(Fail::Python);
    }
    if !minimal {
        let (ok, detail) = check_navigation(c, &paths.out_pathdata("infantry", "out"))?;
        py.print(&format!("{} navigation: {detail}", if ok { "ok  " } else { "FAIL" }))?;
        results.push(("navigation".to_owned(), ok, detail));
    }
    for (kind, folder) in KINDS {
        let r = sector_offsets(&paths.out_pathdata(folder, "out")).and_then(|(xz, _)| {
            let want = so.iter().find(|(k, _)| k == kind).map(|(_, v)| *v).ok_or(Fail::Python)?;
            let ok = i128::from(xz.0) == want.0 && i128::from(xz.1) == want.1;
            Ok((
                ok,
                format!("sector offsets ({}, {}) (asset ({}, {}))", xz.0, xz.1, want.0, want.1),
            ))
        });
        let (ok, detail) = caught(r)?;
        py.print(&format!("{} staged {folder} path data: {detail}", if ok { "ok  " } else { "FAIL" }))?;
        results.push((format!("staged {folder} path data"), ok, detail));
    }
    Ok((results, dx))
}
