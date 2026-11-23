//! The harness's saved runs (`testharness\runs.py`): load / save, the listing, the report and the comparison of two
//! runs, as the Python prints them. A run whose shape Python would treat in its own way (a value of an unexpected
//! type where the report formats it, a file that is not plain JSON) gives `Fail::Python`.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::format_push_string,
    clippy::type_complexity,
    reason = "a port of Python: indexes after their lengths are checked, ints as Python turns them into floats, == as Python compares floats, the Python's names and its functions' shape kept"
)]

use super::pycommon::{fixed, head_len, ljust, py_str, repr};
use crate::pyfmt::float_g;
use crate::pyjson::{self, Value};
use crate::pyos;
use crate::pyproc::{Fail, PyErr, PyResult, read_file};

pub const FORMAT: &str = "bfbc2-harness-run";
pub const VERSION: i128 = 1;
pub const BAD: [&str; 2] = ["fail", "error"];

/// `LABEL.get(status, status)`.
pub fn label(status: &Value) -> String {
    match status.as_str() {
        Some("pass") => "ok  ".to_owned(),
        Some("info") => "info".to_owned(),
        Some("warn") => "warn".to_owned(),
        Some("fail") => "FAIL".to_owned(),
        Some("skip") => "skip".to_owned(),
        Some("hold") => "hold".to_owned(),
        Some("error") => "ERR ".to_owned(),
        _ => py_str(status),
    }
}

/// navregress's HIGHER / LOWER (runs.py takes them from it). Since round NKS-deep D the old exact-match measures
/// (`nodes_same`, `edges_same`, `costs`) are navregress's INFO, never failing: here they are counts.
const PARITY_HIGHER: [&str; 19] = [
    "ground",
    "areas_same",
    "portals",
    "outline",
    "borders",
    "nodes_near",
    "nodes_paired",
    "edges_paired",
    "costs_paired",
    "nodes_paired_candidate",
    "connectivity",
    "runtime_connectivity",
    "strong_share",
    "ai_coverage",
    "ai_pairwise",
    "df_d_precision",
    "df_d_recall",
    "df_f_precision",
    "df_f_recall",
];
const PARITY_LOWER: [&str; 3] = ["extra_ground", "over_connection", "sectors_missing"];
const PARITY_TOL: f64 = 0.05;

#[derive(Clone, Copy, PartialEq)]
pub enum Dir {
    Higher,
    Lower,
    One,
    Count,
}

/// `spec(test, metric)`: (direction, tolerance, the tolerance as Python formats it with 'g').
pub fn spec(test: &str, metric: &str) -> (Dir, f64) {
    let base = metric.rsplit('.').next().unwrap_or(metric);
    match test {
        "parity" => {
            if PARITY_HIGHER.contains(&base) {
                (Dir::Higher, PARITY_TOL)
            } else if PARITY_LOWER.contains(&base) {
                (Dir::Lower, PARITY_TOL)
            } else {
                (Dir::Count, 0.0)
            }
        }
        "structure" => (Dir::Lower, 0.0),
        "battery" => match base {
            "found_pct" | "found_of_ref_pct" => (Dir::Higher, 1.0),
            "extra_pct" | "no_node_pct" | "other_island_pct" | "one_way_pct" => (Dir::Lower, 1.0),
            "length_ratio_median" => (Dir::One, 0.02),
            "length_ratio_p90" => (Dir::One, 0.05),
            _ => (Dir::Count, 0.0),
        },
        "reachability" => {
            if base == "reach_over_pct" {
                (Dir::Lower, 0.5)
            } else if base == "targets_lost" {
                (Dir::Lower, 0.0)
            } else if base.ends_with("_pct") && base != "main_island_nodes_pct" {
                (Dir::Higher, 0.5)
            } else {
                (Dir::Count, 0.0)
            }
        }
        "orders" => {
            if base.ends_with("_pct") {
                (Dir::Higher, 0.5)
            } else {
                (Dir::Count, 0.0)
            }
        }
        "offsets" => {
            if base == "matches" {
                (Dir::Higher, 0.0)
            } else {
                (Dir::Count, 0.0)
            }
        }
        _ => (Dir::Count, 0.0),
    }
}

/// `path_of(name_or_path, folder)`.
pub fn path_of(name: &str, folder: &str) -> PyResult<String> {
    if name.to_lowercase().ends_with(".json") || name.contains('\\') || name.contains('/') {
        return Ok(name.to_owned());
    }
    pyos::join2(folder, &format!("{}.json", name.replace(' ', "_")))
}

/// `load(path)`: the run, its errors as Python raises them.
pub fn load(path: &str) -> PyResult<Value> {
    let data = read_file(path)?;
    let run = pyjson::loads_utf8(&data).ok_or(Fail::Python)?;
    let Value::Dict(_) = run else {
        return Err(Fail::Python);
    };
    let format = run.get("format").cloned().unwrap_or(Value::Null);
    if format.as_str() != Some(FORMAT) {
        return Err(PyErr::new(
            "ValueError",
            format!("{path} is not a saved harness run (format {})", repr(&format)),
        )
        .into());
    }
    let version = match run.get("version") {
        None => 0,
        Some(Value::Int(v)) => *v,
        Some(_) => return Err(Fail::Python),
    };
    if version > VERSION {
        return Err(PyErr::new(
            "ValueError",
            format!("{path} was written by a newer harness (version {version}); update the suite"),
        )
        .into());
    }
    Ok(run)
}

/// `save(run, path)`: `json.dump(run, indent=1)` in text mode (CRLF) to a temporary file beside the path, then moved
/// over it ([`pyos::files::write_atomic`]: per process and write, AU4-CQ-18; flushed to disk).
pub fn save(run: &Value, path: &str) -> PyResult<()> {
    let text = pyjson::dumps(run, Some(1)).replace('\n', "\r\n");
    pyos::files::write_atomic(path, text.as_bytes()).map_err(|(at, e)| {
        if at == path {
            Fail::Python // the move: the reference's own message
        } else {
            crate::pyproc::os_error(&at, &e)
        }
    })
}

fn items(v: &Value) -> Option<&[(String, Value)]> {
    match v {
        Value::Dict(d) => Some(d),
        _ => None,
    }
}

/// `d.get(key, {})` of a dict, as its items (None: d is not a dict, or the value is not one).
fn dict_get<'a>(d: &'a Value, key: &str) -> Option<&'a [(String, Value)]> {
    match d {
        Value::Dict(_) => match d.get(key) {
            None => Some(&[]),
            Some(v) => items(v),
        },
        _ => None,
    }
}

/// One result: (level, scope, test, result).
pub type Row<'a> = (&'a str, &'a str, &'a str, &'a Value);

/// `results(run)` (None where Python raises: a level, file or scope that is not a dict, a result without a status).
pub fn results(run: &Value) -> Option<Vec<Row<'_>>> {
    let mut out = Vec::new();
    for (lv, l) in items(run.get("levels")?)? {
        for (key, f) in dict_get(l, "files")? {
            for (test, r) in dict_get(f, "tests")? {
                out.push((lv.as_str(), key.as_str(), test.as_str(), r));
            }
        }
        for scope in ["level", "ingame"] {
            for (test, r) in dict_get(l, scope)? {
                out.push((lv.as_str(), scope, test.as_str(), r));
            }
        }
    }
    for (_, _, _, r) in &out {
        // r['status'] (and a hashable one, for the counts)
        match r {
            Value::Dict(_) => match r.get("status")? {
                Value::List(_) | Value::Dict(_) => return None,
                _ => {}
            },
            _ => return None,
        }
    }
    Some(out)
}

/// `counts(run)`: {status: n} in first-seen order (statuses as Python's dict keys them: by value).
pub fn counts(run: &Value) -> Option<Vec<(Value, usize)>> {
    let mut c: Vec<(Value, usize)> = Vec::new();
    for (_, _, _, r) in results(run)? {
        let s = r.get("status")?.clone();
        if let Some(slot) = c.iter_mut().find(|(k, _)| py_eq(k, &s)) {
            slot.1 += 1;
        } else {
            c.push((s, 1));
        }
    }
    Some(c)
}

/// `counts_text(c)`.
pub fn counts_text(c: &[(Value, usize)]) -> String {
    let order = [
        ("pass", "ok"),
        ("info", "info"),
        ("warn", "warn"),
        ("fail", "FAIL"),
        ("error", "error"),
        ("skip", "skipped"),
        ("hold", "on hold"),
    ];
    order
        .iter()
        .filter_map(|(k, w)| {
            c.iter()
                .find(|(s, _)| s.as_str() == Some(k))
                .filter(|(_, n)| *n > 0)
                .map(|(_, n)| format!("{n} {w}"))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Python's `==` of two JSON values (1 == 1.0 == True; NaN never equal).
pub fn py_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::List(x), Value::List(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| py_eq(p, q)),
        (Value::Dict(x), Value::Dict(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.iter().any(|(k2, v2)| k == k2 && py_eq(v, v2)))
        }
        (Value::Int(_) | Value::Float(_) | Value::Bool(_), Value::Int(_) | Value::Float(_) | Value::Bool(_)) => {
            match (a.as_int(), b.as_int()) {
                (Some(x), Some(y)) if !matches!(a, Value::Float(_)) && !matches!(b, Value::Float(_)) => x == y,
                _ => a.as_f64() == b.as_f64(),
            }
        }
        _ => false,
    }
}

/// `listing(folder)`: (name, date as `str()`, levels, counts) of every loadable run.
pub fn listing(folder: &str) -> PyResult<Vec<(String, String, usize, Vec<(Value, usize)>)>> {
    let mut out = Vec::new();
    if !pyos::isdir(folder) {
        return Ok(out);
    }
    let mut names = pyos::listdir(folder)?;
    names.sort();
    for f in names {
        let Some(stem) = f.strip_suffix(".json") else {
            continue;
        };
        let p = pyos::join2(folder, &f)?;
        let r = match load(&p) {
            Ok(r) => r,
            Err(Fail::Python) => return Err(Fail::Python),
            Err(Fail::Err(_)) => continue,
        };
        let Some(date) = r.get("date") else {
            continue;
        };
        let n = match r.get("levels") {
            Some(Value::Dict(d)) => d.len(),
            _ => continue, // len() of a str / list works, but counts() then fails: skipped either way
        };
        let Some(c) = counts(&r) else {
            continue;
        };
        out.push((stem.to_owned(), py_str(date), n, c));
    }
    Ok(out)
}

/// The report's lines (`report(run, details=...)`).
pub fn report(run: &Value, details: i64) -> PyResult<Vec<String>> {
    let mut log: Vec<String> = Vec::new();
    let s = match run.get("suite") {
        None => None,
        Some(v) if !v.truthy() => None,
        Some(v @ Value::Dict(_)) => Some(v),
        Some(_) => return Err(Fail::Python),
    };
    let sget = |k: &str| s.and_then(|s| s.get(k));
    let levels = items(run.get("levels").ok_or(Fail::Python)?).ok_or(Fail::Python)?;
    log.push(format!(
        "harness run '{}' ({}, {}; suite {}{}): {} level(s)",
        py_str(run.get("name").ok_or(Fail::Python)?),
        py_str(run.get("date").ok_or(Fail::Python)?),
        run.get("host").map_or_else(|| "?".to_owned(), py_str),
        sget("commit")
            .filter(|v| v.truthy())
            .map_or_else(|| "?".to_owned(), py_str),
        if sget("changed").is_some_and(Value::truthy) {
            " with changes"
        } else {
            ""
        },
        levels.len()
    ));
    let num = |v: Option<&Value>| -> PyResult<String> {
        match v {
            None => Ok("0".to_owned()),
            Some(v) => Ok(py_str(v)),
        }
    };
    for (lv, l) in levels {
        let Value::Dict(_) = l else {
            return Err(Fail::Python);
        };
        let pts = match l.get("points") {
            None => &Value::Dict(Vec::new()),
            Some(p @ Value::Dict(_)) => p,
            Some(_) => return Err(Fail::Python),
        };
        let name = l.get("name").filter(|v| v.truthy()).map_or_else(|| lv.clone(), py_str);
        log.push(format!(
            "\n{name}   spawns {}, targets {}, orders {}",
            num(pts.get("sources"))?,
            num(pts.get("targets"))?,
            num(pts.get("orders"))?
        ));
        if let Some(ex) = l.get("excluded").filter(|v| v.truthy()) {
            let Value::Dict(_) = ex else {
                return Err(Fail::Python);
            };
            let words = |k: &str| -> PyResult<String> {
                let d = dict_get(ex, k).ok_or(Fail::Python)?;
                let mut v: Vec<(&String, f64, &Value)> = Vec::new();
                for (key, val) in d {
                    let x = match val {
                        Value::Int(i) => *i as f64,
                        Value::Float(f) if !f.is_nan() => *f,
                        _ => return Err(Fail::Python),
                    };
                    v.push((key, x, val));
                }
                v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                let s = v
                    .iter()
                    .map(|(k, _, val)| format!("{} {k}", py_str(val)))
                    .collect::<Vec<_>>()
                    .join(", ");
                Ok(if s.is_empty() { "none".to_owned() } else { s })
            };
            log.push(format!(
                "  infantry points tested: {} spawns, {} targets, {} orders",
                num(pts.get("infantry_sources"))?,
                num(pts.get("infantry_targets"))?,
                num(pts.get("infantry_orders"))?
            ));
            log.push(format!(
                "  not infantry (counted, not tested): spawns {}; targets {}; orders {}",
                words("sources")?,
                words("targets")?,
                words("orders")?
            ));
        }
        match l.get("notes") {
            None => {}
            Some(Value::List(notes)) => {
                for n in notes {
                    log.push(format!("  note: {}", py_str(n)));
                }
            }
            Some(_) => return Err(Fail::Python),
        }
        for (key, f) in dict_get(l, "files").ok_or(Fail::Python)? {
            let Value::Dict(_) = f else {
                return Err(Fail::Python);
            };
            let path = f.get("path").map_or_else(String::new, py_str);
            let sha = match f.get("sha1") {
                None => String::new(),
                Some(v) if !v.truthy() => String::new(),
                Some(Value::Str(s)) => s.chars().take(8).collect(),
                Some(_) => return Err(Fail::Python),
            };
            log.push(format!("  {} {path}  ({sha})", ljust(key, 9)));
            for (test, r) in dict_get(f, "tests").ok_or(Fail::Python)? {
                line(&mut log, test, r, details)?;
            }
        }
        for (scope, title) in [
            ("level", "level checks (AI readiness, from files)"),
            ("ingame", "in game"),
        ] {
            if l.get(scope).is_some_and(Value::truthy) {
                log.push(format!("  {title}"));
                for (test, r) in dict_get(l, scope).ok_or(Fail::Python)? {
                    line(&mut log, test, r, details)?;
                }
            }
        }
    }
    let c = counts(run).ok_or(Fail::Python)?;
    let secs = match run.get("seconds") {
        None => 0.0,
        Some(Value::Int(i)) => *i as f64,
        Some(Value::Float(f)) => *f,
        Some(_) => return Err(Fail::Python),
    };
    log.push(format!("\nsummary: {} ({} s)", counts_text(&c), fixed(secs, 0)));
    Ok(log)
}

fn line(log: &mut Vec<String>, test: &str, r: &Value, details: i64) -> PyResult<()> {
    let Value::Dict(_) = r else {
        return Err(Fail::Python);
    };
    let status = r.get("status").ok_or(Fail::Python)?;
    if matches!(status, Value::List(_) | Value::Dict(_)) {
        return Err(Fail::Python);
    }
    log.push(format!(
        "    {} {} {}",
        label(status),
        ljust(test, 16),
        r.get("summary").map_or_else(String::new, py_str)
    ));
    if matches!(status.as_str(), Some("fail" | "warn" | "error")) {
        match r.get("details") {
            None => {}
            Some(Value::List(d)) => {
                for x in d.iter().take(head_len(d.len(), details)) {
                    log.push(format!("           {}", py_str(x)));
                }
            }
            Some(_) => return Err(Fail::Python),
        }
    }
    Ok(())
}

/// One comparison finding: (kind, level, scope, test, metric, before, after).
pub type Found = (&'static str, String, String, String, String, Value, Value);

fn metric_num(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) if i.unsigned_abs() <= 1 << 53 => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// `compare(base, new, scale)`. The set differences' findings in sorted order (level, scope, test), as runs.compare
/// sorts them since round O4 (Python's set order followed its randomized str hashes).
pub fn compare(base: &Value, new: &Value, scale: f64) -> PyResult<Vec<Found>> {
    let ra = results(base).ok_or(Fail::Python)?;
    let rb = results(new).ok_or(Fail::Python)?;
    let dict = |rows: Vec<Row<'_>>| {
        let mut out: Vec<((String, String, String), Value)> = Vec::new();
        for (lv, sc, t, r) in rows {
            let k = (lv.to_owned(), sc.to_owned(), t.to_owned());
            if let Some(slot) = out.iter_mut().find(|(o, _)| *o == k) {
                slot.1 = r.clone();
            } else {
                out.push((k, r.clone()));
            }
        }
        out
    };
    let a = dict(ra);
    let b = dict(rb);
    let levels = |run: &Value| -> Vec<String> {
        run.get("levels")
            .and_then(items)
            .map_or_else(Vec::new, |d| d.iter().map(|(k, _)| k.clone()).collect())
    };
    let (la, lb) = (levels(base), levels(new));
    let st = |r: &Value| r.get("status").cloned().unwrap_or(Value::Null);
    let is_bad = |v: &Value| v.as_str().is_some_and(|s| BAD.contains(&s));
    let mut out: Vec<Found> = Vec::new();
    let mut only_a: Vec<&((String, String, String), Value)> =
        a.iter().filter(|(k, _)| !b.iter().any(|(o, _)| o == k)).collect();
    only_a.sort_by(|x, y| x.0.cmp(&y.0));
    for (k, r) in only_a {
        if lb.contains(&k.0) {
            out.push((
                "missing",
                k.0.clone(),
                k.1.clone(),
                k.2.clone(),
                String::new(),
                st(r),
                Value::Null,
            ));
        }
    }
    let mut only_b: Vec<&((String, String, String), Value)> =
        b.iter().filter(|(k, _)| !a.iter().any(|(o, _)| o == k)).collect();
    only_b.sort_by(|x, y| x.0.cmp(&y.0));
    for (k, r) in only_b {
        if la.contains(&k.0) {
            let kind = if is_bad(&st(r)) { "status" } else { "new" };
            out.push((
                kind,
                k.0.clone(),
                k.1.clone(),
                k.2.clone(),
                String::new(),
                Value::Null,
                st(r),
            ));
        }
    }
    let mut lv_a: Vec<&String> = la.iter().filter(|l| !lb.contains(l)).collect();
    lv_a.sort();
    for lv in lv_a {
        out.push((
            "missing",
            lv.clone(),
            String::new(),
            String::new(),
            String::new(),
            Value::Str("level".into()),
            Value::Null,
        ));
    }
    let mut lv_b: Vec<&String> = lb.iter().filter(|l| !la.contains(l)).collect();
    lv_b.sort();
    for lv in lv_b {
        out.push((
            "new",
            lv.clone(),
            String::new(),
            String::new(),
            String::new(),
            Value::Null,
            Value::Str("level".into()),
        ));
    }
    let mut both: Vec<(&(String, String, String), &Value, &Value)> = a
        .iter()
        .filter_map(|(k, r)| b.iter().find(|(o, _)| o == k).map(|(_, q)| (k, r, q)))
        .collect();
    both.sort_by(|x, y| x.0.cmp(y.0));
    for (k, ra, rb) in both {
        let (sa, sb) = (st(ra), st(rb));
        if !is_bad(&sa) && is_bad(&sb) {
            out.push((
                "status",
                k.0.clone(),
                k.1.clone(),
                k.2.clone(),
                String::new(),
                sa.clone(),
                sb.clone(),
            ));
        } else if is_bad(&sa) && !is_bad(&sb) {
            out.push((
                "fixed",
                k.0.clone(),
                k.1.clone(),
                k.2.clone(),
                String::new(),
                sa.clone(),
                sb.clone(),
            ));
        }
        let test = k.2.as_str();
        let ma = dict_get(ra, "metrics").ok_or(Fail::Python)?;
        let mb = dict_get(rb, "metrics").ok_or(Fail::Python)?;
        let mut names: Vec<&String> = ma
            .iter()
            .map(|(m, _)| m)
            .filter(|m| mb.iter().any(|(o, _)| o == *m))
            .collect();
        names.sort();
        names.dedup();
        for m in names {
            let x = &ma.iter().rev().find(|(o, _)| o == m).ok_or(Fail::Python)?.1;
            let y = &mb.iter().rev().find(|(o, _)| o == m).ok_or(Fail::Python)?.1;
            if matches!(x, Value::Bool(_)) || matches!(y, Value::Bool(_)) {
                return Err(Fail::Python);
            }
            let (Some(xf), Some(yf)) = (metric_num(x), metric_num(y)) else {
                if matches!(x, Value::Int(_) | Value::Float(_)) && matches!(y, Value::Int(_) | Value::Float(_)) {
                    return Err(Fail::Python); // ints past 2**53: Python's exact arithmetic
                }
                continue;
            };
            if py_eq(x, y) {
                continue;
            }
            let (dir, tol) = spec(test, m);
            let tol = tol * scale;
            if dir == Dir::Count {
                out.push((
                    "changed",
                    k.0.clone(),
                    k.1.clone(),
                    k.2.clone(),
                    m.clone(),
                    x.clone(),
                    y.clone(),
                ));
                continue;
            }
            let gain = match dir {
                Dir::Higher => yf - xf,
                Dir::Lower => xf - yf,
                _ => (xf - 1.0).abs() - (yf - 1.0).abs(),
            };
            if gain < -tol - 1e-9 {
                out.push((
                    "worse",
                    k.0.clone(),
                    k.1.clone(),
                    k.2.clone(),
                    m.clone(),
                    x.clone(),
                    y.clone(),
                ));
            } else if gain > tol + 1e-9 {
                out.push((
                    "better",
                    k.0.clone(),
                    k.1.clone(),
                    k.2.clone(),
                    m.clone(),
                    x.clone(),
                    y.clone(),
                ));
            }
        }
        if test == "battery" {
            let data = |r: &Value| -> PyResult<Vec<(String, Value)>> {
                match r.get("data") {
                    None => Ok(Vec::new()),
                    Some(v) if !v.truthy() => Ok(Vec::new()),
                    Some(Value::Dict(d)) => Ok(d.clone()),
                    Some(_) => Err(Fail::Python),
                }
            };
            let (da_all, db_all) = (data(ra)?, data(rb)?);
            for (name, da) in &da_all {
                let Some(db) = db_all.iter().rev().find(|(o, _)| o == name).map(|(_, v)| v) else {
                    continue;
                };
                let (Value::Dict(_), Value::Dict(_)) = (da, db) else {
                    continue;
                };
                let pa = da.get("points").cloned().unwrap_or(Value::Null);
                let pb = db.get("points").cloned().unwrap_or(Value::Null);
                if !py_eq(&pa, &pb) {
                    continue;
                }
                let bits = |d: &Value| -> PyResult<String> {
                    match d.get("bits") {
                        None => Ok(String::new()),
                        Some(Value::Str(s)) => Ok(s.clone()),
                        Some(_) => Err(Fail::Python),
                    }
                };
                let (ba, bb) = (bits(da)?, bits(db)?);
                if ba.chars().count() != bb.chars().count() {
                    continue;
                }
                let lost = ba
                    .chars()
                    .zip(bb.chars())
                    .filter(|(p, q)| *p == '1' && *q == '0')
                    .count();
                if lost > 0 {
                    out.push((
                        "pairs",
                        k.0.clone(),
                        k.1.clone(),
                        k.2.clone(),
                        format!("{name}.lost_pairs"),
                        Value::Int(0),
                        Value::Int(lost as i128),
                    ));
                }
            }
        }
    }
    Ok(out)
}

/// `compare_report(base, new, found)`: the lines and the regression count.
pub fn compare_report(base: &Value, new: &Value, found: &[Found]) -> PyResult<(Vec<String>, usize)> {
    let mut log = Vec::new();
    let name = |r: &Value, k: &str| r.get(k).map(py_str).ok_or(Fail::Python);
    log.push(format!(
        "run '{}' ({}) against '{}' ({})",
        name(new, "name")?,
        name(new, "date")?,
        name(base, "name")?,
        name(base, "date")?
    ));
    let mut files_a: Vec<((String, String), Value)> = Vec::new();
    for (lv, l) in items(base.get("levels").ok_or(Fail::Python)?).ok_or(Fail::Python)? {
        for (key, f) in dict_get(l, "files").ok_or(Fail::Python)? {
            let Value::Dict(_) = f else {
                return Err(Fail::Python);
            };
            let k = (lv.clone(), key.clone());
            let v = f.get("sha1").cloned().unwrap_or(Value::Null);
            if let Some(slot) = files_a.iter_mut().find(|(o, _)| *o == k) {
                slot.1 = v;
            } else {
                files_a.push((k, v));
            }
        }
    }
    for (lv, l) in items(new.get("levels").ok_or(Fail::Python)?).ok_or(Fail::Python)? {
        for (key, f) in dict_get(l, "files").ok_or(Fail::Python)? {
            let Value::Dict(_) = f else {
                return Err(Fail::Python);
            };
            let was = files_a.iter().find(|(o, _)| o.0 == *lv && o.1 == *key).map(|(_, v)| v);
            let now = match f.get("sha1") {
                Some(v) if v.truthy() => v.clone(),
                _ => Value::Str(String::new()),
            };
            if let Some(was) = was.filter(|v| !matches!(v, Value::Null)) {
                let (Value::Str(w), Value::Str(n)) = (was, &now) else {
                    return Err(Fail::Python);
                };
                let text = if w == n {
                    "same file".to_owned()
                } else {
                    format!(
                        "{} -> {}",
                        w.chars().take(8).collect::<String>(),
                        n.chars().take(8).collect::<String>()
                    )
                };
                log.push(format!("  {} {} {text}", ljust(lv, 12), ljust(key, 9)));
            }
        }
    }
    let words = |k: &str| match k {
        "worse" => "worse  ",
        "status" => "FAILS  ",
        "better" => "better ",
        "fixed" => "fixed  ",
        "changed" => "count  ",
        "missing" => "missing",
        "new" => "new    ",
        _ => "pairs  ",
    };
    for kind in [
        "status", "worse", "pairs", "missing", "fixed", "better", "new", "changed",
    ] {
        for (_, lv, sc, test, m, x, y) in found.iter().filter(|f| f.0 == kind) {
            let where_: Vec<&str> = [lv.as_str(), sc.as_str(), test.as_str()]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect();
            let where_ = where_.join(" ");
            let w = words(kind);
            match kind {
                "status" | "fixed" => log.push(format!("  {w} {where_}: {} -> {}", py_str(x), py_str(y))),
                "pairs" => log.push(format!(
                    "  {w} {where_}: {} pair(s) routed before, not now ({})",
                    py_str(y),
                    m.split('.').next().unwrap_or("")
                )),
                "missing" | "new" => log.push(format!("  {w} {where_}")),
                _ => {
                    let (_, tol) = spec(test, m);
                    let tail = if kind == "worse" || kind == "better" {
                        format!("  (tolerance {})", float_g(tol))
                    } else {
                        String::new()
                    };
                    log.push(format!("  {w} {where_} {m}: {} -> {}{tail}", py_str(x), py_str(y)));
                }
            }
        }
    }
    let bad = found.iter().filter(|f| f.0 == "worse" || f.0 == "status").count();
    log.push(if bad > 0 {
        format!("{bad} regression(s)")
    } else {
        "no regressions".to_owned()
    });
    Ok((log, bad))
}
