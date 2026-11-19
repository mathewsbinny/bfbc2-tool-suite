//! `bfbc2 harness show|compare|list|import-log|ingame ...`: the map and bot test harness's command line
//! (`harness.py`). `run` tests path data (NumPy's seeded draws, NavKit's measures and builds in Python) and is handed
//! to the Python; the harness never starts the game in any command. Its stdout is line-buffered
//! (`sys.stdout.reconfigure(line_buffering=True)`): each line leaves as it is printed.

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

use bc2_harness::oplog;

use super::pycommon::{find_opfor, ljust, py_float, py_int};
use super::runs::{self, BAD, label};
use crate::pyfmt::docstring;
use crate::pyjson::Value;
use crate::pyos;
use crate::pyproc::{Fail, Py, PyErr, PyResult, os_error};

const SOURCE: &str = include_str!("../../../../bfbc2-automated-map-and-bot-test-harness/harness.py");

/// `print(text)` on the line-buffered stdout.
fn say(py: &mut Py, text: &str) -> PyResult<()> {
    py.print(text)?;
    py.flush_out();
    Ok(())
}

fn value_error(msg: impl Into<String>) -> Fail {
    Fail::Err(PyErr::new("ValueError", msg))
}

/// `opt(args, k)`.
fn opt<'a>(args: &'a [String], k: &str) -> PyResult<Option<&'a str>> {
    match args.iter().position(|a| a == k) {
        None => Ok(None),
        Some(i) => args
            .get(i + 1)
            .map(|s| Some(s.as_str()))
            .ok_or_else(|| value_error(format!("{k} needs a value"))),
    }
}

/// `positional(args)`.
fn positional(args: &[String]) -> Vec<&str> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
        } else if a.starts_with("--") {
            skip = true;
        } else {
            out.push(a.as_str());
        }
    }
    out
}

/// `runs_folder(args)`: --runs, else <OPFOR>\work\harness\runs (made), else the working folder.
fn runs_folder(args: &[String]) -> PyResult<String> {
    if let Some(g) = opt(args, "--runs")?.filter(|g| !g.is_empty()) {
        return Ok(g.to_owned());
    }
    let found = (|| -> PyResult<String> {
        let root = find_opfor(opt(args, "--opfor")?, "NavKitError")?;
        let d = pyos::join(&root, &["work", "harness", "runs"])?;
        pyos::makedirs(&d)?;
        Ok(d)
    })();
    match found {
        Ok(d) => Ok(d),
        Err(Fail::Python) => Err(Fail::Python),
        Err(Fail::Err(_)) => pyos::getcwd(),
    }
}

fn cmd_show(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let p = positional(args);
    let Some(first) = p.first() else {
        return Err(value_error("show needs a run name or a .json path"));
    };
    let run = runs::load(&runs::path_of(first, &runs_folder(args)?)?)?;
    let details = match opt(args, "--details")? {
        None => 3,
        Some(d) => py_int(d)?,
    };
    for line in runs::report(&run, details)? {
        say(py, &line)?;
    }
    Ok(0)
}

fn cmd_compare(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let p = positional(args);
    if p.len() < 2 {
        return Err(value_error("compare needs two runs: harness.py compare <base> <new>"));
    }
    let folder = runs_folder(args)?;
    let base = runs::load(&runs::path_of(p[0], &folder)?)?;
    let new = runs::load(&runs::path_of(p[1], &folder)?)?;
    let scale = match opt(args, "--scale")? {
        None => 1.0,
        Some(s) => py_float(s)?,
    };
    let found = runs::compare(&base, &new, scale)?;
    let (lines, bad) = runs::compare_report(&base, &new, &found)?;
    for l in lines {
        say(py, &l)?;
    }
    Ok(i32::from(bad > 0))
}

fn cmd_list(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let folder = runs_folder(args)?;
    let rows = runs::listing(&folder)?;
    say(py, &format!("{} saved run(s) in {folder}", rows.len()))?;
    for (name, date, n, c) in rows {
        say(
            py,
            &format!("  {} {date}  {n} level(s)  {}", ljust(&name, 24), runs::counts_text(&c)),
        )?;
    }
    Ok(0)
}

const INGAME: [(&str, &str); 3] = [
    ("loads-ai-init", "the level loads, reaches the match, and AI init runs"),
    (
        "offsets-read",
        "both SectorOffsets loads read the asset's values (no [dev] override)",
    ),
    ("pathfinders", "pathfinders appear after spawning (OPFOR unit counters)"),
];
const TELEMETRY: [(&str, &str); 4] = [
    ("bot-traversal", "bots reach given points within a time"),
    ("stuck", "no bot without progress for N seconds"),
    ("squads", "squad members keep together and follow their orders"),
    ("vehicles", "bots enter and drive vehicles"),
];

fn cmd_ingame(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let p = positional(args);
    let lv = p.first().copied().unwrap_or("<level>");
    say(
        py,
        &format!("In-game tests of {lv}: ON HOLD - needs playtest. The harness does not start the game."),
    )?;
    say(
        py,
        "Planned (ingame.py): testrun.bat per level with [dev] unit_counters_dump=1, then the session imported;",
    )?;
    say(py, "until then run it yourself and import the log:")?;
    say(
        py,
        &format!("  testrun.bat {lv}            (Project OPFOR BFBC2; or play the level)"),
    )?;
    say(py, &format!("  harness.py import-log <run> --level {lv}"))?;
    for (name, what) in INGAME {
        say(py, &format!("  hold {} {what}", ljust(name, 16)))?;
    }
    for (name, what) in TELEMETRY {
        say(
            py,
            &format!("  hold {} {what} (needs OPFOR telemetry, M6)", ljust(name, 16)),
        )?;
    }
    Ok(0)
}

/// A `checks.Result(...).as_dict()` with `seconds` 0.0 (the in-game checks are not timed).
fn result(status: &str, summary: String, metrics: Vec<(&str, i128)>, details: Vec<String>) -> Value {
    Value::dict([
        ("status", Value::Str(status.to_owned())),
        ("summary", Value::Str(summary)),
        (
            "metrics",
            Value::dict(metrics.into_iter().map(|(k, v)| (k, Value::Int(v)))),
        ),
        ("details", Value::List(details.into_iter().map(Value::Str).collect())),
        ("seconds", Value::Float(0.0)),
    ])
}

/// `ingame.evaluate(s, asset, vehicle)`.
fn evaluate(
    s: &oplog::Session,
    asset: Option<(i128, i128)>,
    vehicle: Option<(i128, i128)>,
) -> Vec<(&'static str, Value)> {
    let src = format!("opfor.log session {} (PID {})", s.start, s.pid);
    let in_game = s.states.contains(&3);
    let ai_init = s.events.contains(&"AI init");
    let ok = in_game && ai_init && !s.crashed;
    let mut out = Vec::new();
    let what = if s.crashed {
        "crashed".to_owned()
    } else {
        format!(
            "{}, AI init {}",
            if in_game { "in game" } else { "never in game" },
            if ai_init { "seen" } else { "not seen" }
        )
    };
    let events = if s.events.is_empty() {
        "none".to_owned()
    } else {
        s.events.join(", ")
    };
    out.push((
        "loads-ai-init",
        result(
            if ok { "pass" } else { "fail" },
            format!("{src}: {what}"),
            vec![
                ("in_game", i128::from(in_game)),
                ("ai_init", i128::from(ai_init)),
                ("crashed", i128::from(s.crashed)),
            ],
            vec![format!("events: {events}")],
        ),
    ));
    if s.offset_loads.is_empty() {
        out.push((
            "offsets-read",
            result(
                "hold",
                format!("{src}: no SectorOffsets load lines (needs OPFOR's probes, [dev] probes=1)"),
                vec![],
                vec![],
            ),
        ));
    } else {
        let mut reads = s.offset_loads.clone();
        reads.sort_unstable();
        reads.dedup();
        let as_i = |p: (i64, i64)| (i128::from(p.0), i128::from(p.1));
        let good = asset.is_some()
            && s.offset_loads
                .iter()
                .all(|r| Some(as_i(*r)) == asset || (vehicle.is_some() && Some(as_i(*r)) == vehicle))
            && s.override_.is_none();
        let mut why = format!(
            "loads read {}",
            reads
                .iter()
                .map(|(x, z)| format!("{x},{z}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        if let Some((x, z)) = s.override_ {
            why.push_str(&format!("; override on ({x},{z})"));
        }
        match asset {
            Some((x, z)) => why.push_str(&format!("; the asset says {x},{z}")),
            None => why.push_str("; the asset's values are not known"),
        }
        if let (Some(a), Some(v)) = (asset, vehicle)
            && v != a
        {
            why.push_str(&format!(" (Vehicle {},{})", v.0, v.1));
        }
        let status = if asset.is_none() {
            "hold"
        } else if good {
            "pass"
        } else {
            "fail"
        };
        out.push((
            "offsets-read",
            result(
                status,
                format!("{src}: {why}"),
                vec![
                    ("loads", s.offset_loads.len() as i128),
                    ("override", i128::from(s.override_.is_some())),
                ],
                vec![],
            ),
        ));
    }
    if s.pathfinders.is_empty() {
        out.push((
            "pathfinders",
            result(
                "hold",
                format!("{src}: no unit-counter lines (set [dev] unit_counters_dump=1 for the run)"),
                vec![],
                vec![],
            ),
        ));
    } else {
        let peak = s.pathfinders.iter().copied().max().unwrap_or(0);
        let shown: Vec<String> = s.pathfinders.iter().take(8).map(ToString::to_string).collect();
        let more = if s.pathfinders.len() > 8 { " ..." } else { "" };
        out.push((
            "pathfinders",
            result(
                if peak > 0 { "pass" } else { "fail" },
                format!("{src}: pathfinders {}{more}", shown.join(" -> ")),
                vec![("pathfinders_peak", i128::from(peak))],
                vec![],
            ),
        ));
    }
    out
}

/// A dict's value slot for `key` (`d.setdefault(key, default)`).
fn setdefault<'a>(d: &'a mut Vec<(String, Value)>, key: &str, default: impl FnOnce() -> Value) -> &'a mut Value {
    let i = if let Some(i) = d.iter().position(|(k, _)| k == key) {
        i
    } else {
        d.push((key.to_owned(), default()));
        d.len() - 1
    };
    &mut d[i].1
}

/// `(m['x'], m['z']) if 'x' in m and 'z' in m else None` for int values (others hand off).
fn pair(m: &Value, a: &str, b: &str) -> PyResult<Option<(i128, i128)>> {
    match (m.get(a), m.get(b)) {
        (Some(Value::Int(x)), Some(Value::Int(z))) => Ok(Some((*x, *z))),
        (Some(_), Some(_)) => Err(Fail::Python),
        _ => Ok(None),
    }
}

fn cmd_import_log(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let p = positional(args);
    let lv = opt(args, "--level")?;
    let (Some(first), Some(lv)) = (p.first(), lv.filter(|l| !l.is_empty())) else {
        return Err(value_error("import-log needs a run and --level <level>"));
    };
    if !lv.is_ascii() {
        return Err(Fail::Python);
    }
    let lv = lv.to_lowercase().rsplit('/').next().unwrap_or_default().to_owned();
    let folder = runs_folder(args)?;
    let path = runs::path_of(first, &folder)?;
    let mut run = runs::load(&path)?;
    let log_path = match opt(args, "--log")?.filter(|l| !l.is_empty()) {
        Some(l) => l.to_owned(),
        None => pyos::join(
            &find_opfor(opt(args, "--opfor")?, "NavKitError")?,
            &["game", "logs", "opfor.log"],
        )?,
    };
    let all = match oplog::sessions_of_file(std::path::Path::new(&log_path)) {
        Err(e) => return Err(os_error(&log_path, &e)),
        Ok(Err(_)) => return Err(Fail::Python),
        Ok(Ok(v)) => v,
    };
    let found: Vec<&oplog::Session> = all
        .iter()
        .filter(|s| {
            s.level
                .as_ref()
                .is_some_and(|l| !l.is_empty() && l.to_lowercase().rsplit('/').next().unwrap_or_default() == lv)
        })
        .collect();
    if all.iter().any(|s| s.level.as_ref().is_some_and(|l| !l.is_ascii())) {
        return Err(Fail::Python); // str.lower() beyond ASCII
    }
    if found.is_empty() {
        say(py, &format!("no session of {lv} in {log_path}"))?;
        return Ok(2);
    }
    let k = match opt(args, "--session")? {
        None => -1,
        Some(s) => py_int(s)?,
    };
    let n = found.len() as i64;
    let idx = if k < 0 { k + n } else { k };
    let s = *found
        .get(usize::try_from(idx).unwrap_or(usize::MAX))
        .ok_or_else(|| Fail::Err(PyErr::new("IndexError", "list index out of range")))?;
    let Value::Dict(top) = &mut run else {
        return Err(Fail::Python);
    };
    let Some(Value::Dict(levels)) = top.iter_mut().find(|(k, _)| k == "levels").map(|(_, v)| v) else {
        return Err(Fail::Python);
    };
    let l = setdefault(levels, &lv, || {
        Value::dict([
            ("name", Value::Str(lv.clone())),
            ("notes", Value::List(vec![])),
            ("points", Value::Dict(vec![])),
            ("files", Value::Dict(vec![])),
            ("level", Value::Dict(vec![])),
            ("ingame", Value::Dict(vec![])),
        ])
    });
    let Value::Dict(ld) = l else {
        return Err(Fail::Python);
    };
    // m = L.get('level', {}).get('offsets-asset', {}).get('metrics', {})
    let empty = Value::Dict(vec![]);
    let lvl = ld.iter().find(|(k, _)| k == "level").map_or(&empty, |(_, v)| v);
    let oa = match lvl {
        Value::Dict(_) => lvl.get("offsets-asset").unwrap_or(&empty),
        _ => return Err(Fail::Python),
    };
    let m = match oa {
        Value::Dict(_) => oa.get("metrics").unwrap_or(&empty),
        _ => return Err(Fail::Python),
    };
    let Value::Dict(_) = m else {
        return Err(Fail::Python);
    };
    let asset = pair(m, "x", "z")?;
    let vehicle = pair(m, "vx", "vz")?;
    let ev = evaluate(s, asset, vehicle);
    let Value::Dict(ingame) = setdefault(ld, "ingame", || Value::Dict(vec![])) else {
        return Err(Fail::Python);
    };
    for (name, r) in ev {
        *setdefault(ingame, name, || Value::Null) = r;
    }
    let shown: Vec<(String, String, bool)> = ["loads-ai-init", "offsets-read", "pathfinders"]
        .iter()
        .map(|name| {
            let r = ingame.iter().find(|(k, _)| k == name).map(|(_, v)| v);
            let st = r.and_then(|r| r.get("status")).and_then(Value::as_str).unwrap_or("");
            let summ = r.and_then(|r| r.get("summary")).and_then(Value::as_str).unwrap_or("");
            (
                format!("  {} {} {summ}", label(&Value::Str(st.to_owned())), ljust(name, 16)),
                st.to_owned(),
                BAD.contains(&st),
            )
        })
        .collect();
    runs::save(&run, &path)?;
    say(
        py,
        &format!(
            "{lv}: session {} (PID {}) of {} in {log_path}",
            s.start,
            s.pid,
            found.len()
        ),
    )?;
    for (line, _, _) in &shown {
        say(py, line)?;
    }
    say(py, &format!("saved: {path}"))?;
    Ok(i32::from(shown.iter().any(|(_, _, b)| *b)))
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let Some((cmd, rest)) = args.split_first() else {
        say(py, &docstring(SOURCE))?;
        return Ok(0);
    };
    match cmd.as_str() {
        "run" => Err(Fail::Python),
        "show" => cmd_show(py, rest),
        "compare" => cmd_compare(py, rest),
        "list" => cmd_list(py, rest),
        "import-log" => cmd_import_log(py, rest),
        "ingame" => cmd_ingame(py, rest),
        _ => {
            say(py, &docstring(SOURCE))?;
            Ok(1)
        }
    }
}
