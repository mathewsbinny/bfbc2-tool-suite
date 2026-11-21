//! texqa's command line (`main`, `run`, `find`, the reports, the baseline): the files it writes, its printed lines.
//!
//! `--help` first (or an abbreviation of it) is argparse's help at the console's width ([`crate::argfmt`], round
//! RT2); option abbreviations resolve as argparse resolves them (round RT3). Handed to the Python
//! ([`Fail::Python`], decided before anything is written or printed): argparse's help after other arguments, errors
//! and the option forms this port does not read; BF1943's bundles as a source (`bf1943import.Source`); DICE's copies
//! (the stock bundles' index; give `--no-dice`); a run whose report would have contact sheets (PIL draws them: some
//! texture flagged among `--worst`); a `--write-baseline` without a folder; anything NumPy itself would raise.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::analyse::{self, Out, composite};
use super::decode::{self, name_of};
use crate::argfmt::Long;
use crate::pyfmt::{float_repr, str_repr};
use crate::pyjson::{self, Value};
use crate::pyos;
use crate::pyproc::{Fail, Py, PyErr, PyResult, os_error, suite_root};

/// texqa.py's option strings in argparse's `_option_string_actions` order (abbreviations resolve over them).
const STRINGS: [&str; 16] = [
    "-h",
    "--help",
    "-o",
    "--out",
    "--bf1943",
    "--no-bf1943",
    "--console",
    "--opfor",
    "--no-dice",
    "--cache",
    "--worst",
    "--skip",
    "--baseline",
    "--write-baseline",
    "--threshold",
    "--workers",
];

const VERSION: i128 = 1;
const WHAT_TO_DO: &str = "Give folders or .res files of PC textures (a build output, an unpacked bundle); texqa.py --help shows the options.";
const SRC: &[u8] = include_bytes!("../../../../bfbc2-console-convert/texqa.py");

fn qa_err(msg: String) -> Fail {
    let mut e = PyErr::new("TexQaError", msg);
    e.what_to_do = Some(WHAT_TO_DO.to_owned());
    Fail::Err(e)
}

/// `CODE`: the first 12 hex digits of the sha1 of texqa.py (line ends normalized).
fn code() -> String {
    let mut v = Vec::with_capacity(SRC.len());
    let mut i = 0;
    while i < SRC.len() {
        if SRC[i] == b'\r' && SRC.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        v.push(SRC[i]);
        i += 1;
    }
    pyos::sha1_hex(&v)[..12].to_owned()
}

#[derive(Default)]
struct Args {
    inputs: Vec<String>,
    out: String,
    bf1943: Option<String>,
    no_bf1943: bool,
    console: Vec<String>,
    no_dice: bool,
    cache: Option<String>,
    worst: i64,
    skip: Vec<String>,
    baseline: Option<String>,
    write_baseline: Option<String>,
    threshold: f64,
    workers: Option<usize>,
}

fn int_arg(v: &str) -> PyResult<i64> {
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) || v.len() > 15 {
        return Err(Fail::Python);
    }
    v.parse().map_err(|_| Fail::Python)
}

fn float_arg(v: &str) -> PyResult<f64> {
    let ok = !v.is_empty()
        && v.bytes()
            .all(|b| b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'E' || b == b'+' || b == b'-')
        && v.bytes().next().is_some_and(|b| b.is_ascii_digit() || b == b'.');
    if !ok {
        return Err(Fail::Python);
    }
    v.parse().map_err(|_| Fail::Python)
}

/// argparse's reading of the options this port takes; anything else hands off.
fn parse(argv: &[String]) -> PyResult<Args> {
    let mut a = Args {
        out: "texqa_out".to_owned(),
        worst: 24,
        threshold: 1.0,
        ..Args::default()
    };
    let mut i = 0;
    let mut runs = 0;
    let mut in_run = false;
    while i < argv.len() {
        let arg = &argv[i];
        if !arg.starts_with('-') || arg == "-" {
            if !in_run {
                runs += 1;
                in_run = true;
            }
            a.inputs.push(arg.clone());
            i += 1;
            continue;
        }
        in_run = false;
        let (mut opt, inline) = match arg.split_once('=') {
            Some((o, v)) if o.starts_with("--") => (o.to_owned(), Some(v.to_owned())),
            _ => (arg.clone(), None),
        };
        if arg.starts_with("--") && arg.len() > 2 {
            // round RT3: an abbreviation resolved as argparse resolves it; its errors stay argparse's (below)
            match crate::argfmt::long_option(arg, &STRINGS) {
                Long::Found(s) => s.clone_into(&mut opt),
                _ => return Err(Fail::Python),
            }
        }
        let flag = matches!(opt.as_str(), "--no-bf1943" | "--no-dice");
        if flag {
            if inline.is_some() {
                return Err(Fail::Python);
            }
            if opt == "--no-bf1943" {
                a.no_bf1943 = true;
            } else {
                a.no_dice = true;
            }
            i += 1;
            continue;
        }
        let known = [
            "-o",
            "--out",
            "--bf1943",
            "--console",
            "--opfor",
            "--cache",
            "--worst",
            "--skip",
            "--baseline",
            "--write-baseline",
            "--threshold",
            "--workers",
        ];
        if !known.contains(&opt.as_str()) {
            return Err(Fail::Python);
        }
        let value = if let Some(v) = inline {
            i += 1;
            v
        } else {
            let Some(v) = argv.get(i + 1) else {
                return Err(Fail::Python);
            };
            i += 2;
            v.clone()
        };
        if value.starts_with('-') {
            return Err(Fail::Python);
        }
        match opt.as_str() {
            "-o" | "--out" => a.out = value,
            "--bf1943" => a.bf1943 = Some(value),
            "--console" => a.console.push(value),
            "--opfor" => {}
            "--cache" => a.cache = Some(value),
            "--worst" => a.worst = int_arg(&value)?,
            "--skip" => a.skip.push(value),
            "--baseline" => a.baseline = Some(value),
            "--write-baseline" => a.write_baseline = Some(value),
            "--threshold" => a.threshold = float_arg(&value)?,
            _ => a.workers = Some(int_arg(&value)? as usize),
        }
    }
    if a.inputs.is_empty() || runs > 1 {
        return Err(Fail::Python);
    }
    Ok(a)
}

struct Item {
    name: String,
    path: String,
    root: String,
    orig: Option<String>,
}

fn sorted_dir(dir: &str) -> Option<(Vec<String>, Vec<String>)> {
    let entries = pyos::scandir(dir).ok()?;
    let (mut d, mut f) = (Vec::new(), Vec::new());
    for (n, is_dir) in entries {
        if is_dir {
            d.push(n);
        } else {
            f.push(n);
        }
    }
    Some((d, f))
}

/// `os.walk(top)` with `dn[:] = sorted(...)` applied by `f` before descending.
fn walk_sorted(top: &str, skip: &[String], out: &mut Vec<(String, Vec<String>)>) -> PyResult<()> {
    let Some((dirs, files)) = sorted_dir(top) else {
        return Ok(());
    };
    let mut dn: Vec<String> = dirs.into_iter().filter(|d| !skip.contains(&d.to_lowercase())).collect();
    dn.sort();
    out.push((top.to_owned(), files));
    for d in dn {
        let p = pyos::join2(top, &d)?;
        if !pyos::is_symlink(&p) {
            walk_sorted(&p, skip, out)?;
        }
    }
    Ok(())
}

/// Universal-newline lines with their `\n` (iterating a text file).
fn text_lines(data: &[u8]) -> Vec<String> {
    let t = String::from_utf8_lossy(data).replace("\r\n", "\n").replace('\r', "\n");
    t.split_inclusive('\n').map(str::to_owned).collect()
}

fn find(roots: &[String], skip: &[String]) -> PyResult<Vec<Item>> {
    let skip_l: Vec<String> = skip.iter().map(|s| s.to_lowercase()).collect();
    let mut out = Vec::new();
    for root in roots {
        let root = pyos::abspath(root)?;
        if pyos::isfile(&root) {
            out.push(Item {
                name: pyos::basename(&root)?,
                path: root.clone(),
                root: pyos::dirname(&root)?,
                orig: None,
            });
            continue;
        }
        if !pyos::isdir(&root) {
            return Err(qa_err(format!("{root}: no such file or folder")));
        }
        let mut manifests: Vec<(String, HashMap<String, String>)> = Vec::new();
        let mut rows = Vec::new();
        walk_sorted(&root, &skip_l, &mut rows)?;
        for (dp, mut fn_) in rows {
            if fn_.iter().any(|f| f == "fbrb.manifest") {
                let data = pyos::read(&pyos::join2(&dp, "fbrb.manifest")?)?;
                let mut types = HashMap::new();
                for line in text_lines(&data) {
                    if line.starts_with('#') || !line.contains('|') {
                        continue;
                    }
                    let cols: Vec<&str> = line.split('|').collect();
                    types.insert(cols[0].replace('\\', "/").to_lowercase(), cols[1].to_owned());
                }
                if let Some(slot) = manifests.iter_mut().find(|(m, _)| *m == dp) {
                    slot.1 = types;
                } else {
                    manifests.push((dp.clone(), types));
                }
            }
            let base = manifests
                .iter()
                .position(|(m, _)| dp == *m || dp.starts_with(&format!("{m}\\")));
            fn_.sort();
            for f in fn_ {
                if !f.to_lowercase().ends_with(".res") {
                    continue;
                }
                let p = pyos::join2(&dp, &f)?;
                let rel = pyos::relpath(&p, base.map_or(&root, |b| &manifests[b].0))?.replace('\\', "/");
                if let Some(b) = base {
                    if manifests[b].1.get(&rel.to_lowercase()).map(String::as_str) != Some("ITexture") {
                        continue;
                    }
                } else {
                    let data = pyos::read(&p)?;
                    let mut head = &data[..data.len().min(64 + decode::HEADER)];
                    let mut size = data.len() as u64;
                    if head.len() >= 4 && &head[..4] == b"RES\x02" {
                        head = head.get(64..).unwrap_or(&[]);
                        size = size.wrapping_sub(64);
                    }
                    if !decode::sized_texture(head, size) {
                        continue;
                    }
                }
                let orig = format!("{p}.orig");
                let orig = pyos::isfile(&orig).then_some(orig);
                out.push(Item {
                    name: rel,
                    path: p,
                    root: root.clone(),
                    orig,
                });
            }
        }
    }
    Ok(out)
}

/// `LooseSource`.
struct Loose {
    entries: HashMap<String, String>,
}

impl Loose {
    fn new(folder: &str) -> PyResult<Self> {
        let mut entries = HashMap::new();
        for (dp, _, fs) in pyos::walk(folder)? {
            for f in fs {
                if f.to_lowercase().ends_with(".res") {
                    let p = pyos::join2(&dp, &f)?;
                    entries.insert(pyos::relpath(&p, folder)?.replace('\\', "/").to_lowercase(), p);
                }
            }
        }
        Ok(Self { entries })
    }

    fn ident(&self, key: &str) -> PyResult<String> {
        let p = &self.entries[key];
        // int(os.path.getmtime(p)): st_mtime as CPython makes it (seconds + nanoseconds * 1e-9), truncated
        let secs = pyos::getmtime(p)?.trunc() as i128;
        Ok(format!("{p}|{}|{secs}", pyos::getsize(p)?))
    }
}

fn candidates(name: &str) -> Vec<String> {
    let low = name.to_lowercase().replace('\\', "/");
    let parts: Vec<&str> = low.split('/').collect();
    (0..parts.len()).map(|i| parts[i..].join("/")).collect()
}

fn lookup(name: &str, entries: &HashMap<String, String>) -> Option<String> {
    candidates(name).into_iter().find(|c| entries.contains_key(c))
}

fn workers(n: Option<usize>) -> usize {
    if let Some(n) = n.filter(|&n| n > 0) {
        return n;
    }
    std::env::var("BFBC2_TEXQA_WORKERS")
        .ok()
        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|v| v.parse::<usize>().ok())
        .map_or_else(
            || std::thread::available_parallelism().map_or(1, usize::from),
            |v| v.max(1),
        )
}

fn get<'a>(row: &'a Value, k: &str) -> Option<&'a Value> {
    row.get(k)
}

fn set(row: &mut Value, k: &str, v: Value) {
    if let Value::Dict(items) = row {
        if let Some(slot) = items.iter_mut().find(|(key, _)| key == k) {
            slot.1 = v;
        } else {
            items.push((k.to_owned(), v));
        }
    }
}

fn opt_str(s: Option<&String>) -> Value {
    s.map_or(Value::Null, |x| Value::Str(x.clone()))
}

/// `run(...)`: the rows (with path, root, orig), the cache to write (path, text), and the log lines.
/// One texture of the run: its cache key and its source (which `--console` folder, the entry).
struct Plan {
    key: String,
    src: Option<(usize, String)>,
}

/// `run`'s outcome before its cache is written (a hand-off decided after it must leave no cache behind).
struct Ran {
    rows: Vec<Value>,
    cache: Option<(String, Value)>,
    line: String,
}

fn run(py: &mut Py, a: &Args, sources: &[Loose]) -> PyResult<Ran> {
    let items = find(&a.inputs, &a.skip)?;
    let cache_path = match &a.cache {
        Some(c) => Some(pyos::join2(c, &format!("rows_v{VERSION}.json"))?),
        None => None,
    };
    let mut old: Vec<(String, Value)> = Vec::new();
    if let Some(cp) = &cache_path
        && pyos::isfile(cp)
    {
        let data = pyos::read(cp)?;
        if !data.is_ascii() {
            return Err(Fail::Python);
        }
        match pyjson::loads_ansi(&data) {
            Some(Value::Dict(d)) => old = d,
            Some(_) => return Err(Fail::Python),
            None => {
                if String::from_utf8_lossy(&data).contains("\\ud") {
                    return Err(Fail::Python);
                }
            }
        }
    }
    let old_get = |k: &str| old.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    let code = code();
    let mut shas = HashMap::new();
    for it in &items {
        shas.insert(it.path.clone(), pyos::sha1_hex(&pyos::read(&it.path)?));
    }
    let mut origs: HashMap<String, String> = HashMap::new();
    for it in &items {
        if let Some(o) = &it.orig {
            origs.insert(shas[&it.path].clone(), o.clone());
        }
    }
    let mut items = items;
    let mut plan = Vec::new();
    for it in &mut items {
        let osha = shas[&it.path].clone();
        if it.orig.is_none()
            && let Some(o) = origs.get(&osha)
        {
            it.orig = Some(o.clone());
        }
        let mut src = None;
        if !composite(&it.name.to_lowercase()) {
            for (i, s) in sources.iter().enumerate() {
                if let Some(k) = lookup(&it.name, &s.entries) {
                    src = Some((i, k));
                    break;
                }
            }
        }
        let orig_sha = match &it.orig {
            Some(o) => Some(pyos::sha1_hex(&pyos::read(o)?)),
            None => None,
        };
        let ident = Value::List(vec![
            Value::Int(VERSION),
            Value::Str(code.clone()),
            Value::Str(osha),
            match &src {
                Some((i, k)) => Value::Str(sources[*i].ident(k)?),
                None => Value::Null,
            },
            Value::Null,
            orig_sha.map_or(Value::Null, Value::Str),
            Value::Str(it.name.to_lowercase()),
        ]);
        let key = pyos::sha1_hex(pyjson::dumps(&ident, None).as_bytes());
        plan.push(Plan { key, src });
    }
    // the analyses (texqa's worker processes: the same rows in order)
    let todo: Vec<usize> = (0..plan.len()).filter(|&i| old_get(&plan[i].key).is_none()).collect();
    let n = todo.len();
    let nw = workers(a.workers)
        .min(if a.workers.is_some() { n } else { (n / 3).max(1) })
        .clamp(1, 4);
    let results: Mutex<Vec<Option<Result<Out, Fail>>>> = Mutex::new((0..n).map(|_| None).collect());
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..nw {
            s.spawn(|| {
                loop {
                    let j = next.fetch_add(1, Ordering::SeqCst);
                    if j >= n {
                        break;
                    }
                    let i = todo[j];
                    let it = &items[i];
                    let r = (|| -> Result<Out, Fail> {
                        let data = std::fs::read(&it.path).map_err(|_| Fail::Python)?;
                        let src = match &plan[i].src {
                            Some((si, k)) => Some(std::fs::read(&sources[*si].entries[k]).map_err(|_| Fail::Python)?),
                            None => None,
                        };
                        let orig = match &it.orig {
                            Some(o) => Some(std::fs::read(o).map_err(|_| Fail::Python)?),
                            None => None,
                        };
                        analyse::analyse(&it.name, &data, src.as_deref(), orig.as_deref())
                    })();
                    if let Ok(mut g) = results.lock() {
                        g[j] = Some(r);
                    }
                }
            });
        }
    });
    let results = results.into_inner().map_err(|_| Fail::Python)?;
    let mut fresh = results.into_iter();
    let mut new: Vec<(String, Value)> = Vec::new();
    let mut rows = Vec::new();
    let mut done = 0usize;
    for (i, p) in plan.iter().enumerate() {
        let it = &items[i];
        let row = if let Some(r) = old_get(&p.key) {
            if !matches!(r, Value::Dict(_)) {
                return Err(Fail::Python);
            }
            r
        } else {
            let out = fresh.next().flatten().ok_or(Fail::Python)??;
            let mut row = match out {
                Out::Row(v) => v,
                Out::Err(detail) => Value::dict([
                    ("name", Value::Str(it.name.clone())),
                    (
                        "flags",
                        Value::List(vec![Value::dict([
                            ("code", Value::Str("unreadable".into())),
                            ("severity", Value::Str("error".into())),
                            ("cause", Value::Str("unknown".into())),
                            ("detail", Value::Str(detail)),
                        ])]),
                    ),
                    ("score", Value::Float(1000.0)),
                ]),
            };
            set(
                &mut row,
                "source_name",
                p.src.as_ref().map_or(Value::Null, |(_, k)| Value::Str(k.clone())),
            );
            set(
                &mut row,
                "source_from",
                if p.src.is_some() {
                    Value::Str("LooseSource".into())
                } else {
                    Value::Null
                },
            );
            set(&mut row, "dice_name", Value::Null);
            done += 1;
            if done.is_multiple_of(50) {
                py.print(&format!("  {done} textures analysed ..."))?;
                py.flush_out();
            }
            row
        };
        if let Some(slot) = new.iter_mut().find(|(k, _)| *k == p.key) {
            slot.1 = row.clone();
        } else {
            new.push((p.key.clone(), row.clone()));
        }
        let mut r = row;
        set(&mut r, "path", Value::Str(it.path.clone()));
        set(&mut r, "root", Value::Str(it.root.clone()));
        set(&mut r, "orig", opt_str(it.orig.as_ref()));
        rows.push(r);
    }
    let line = format!(
        "{} textures: {done} analysed, {} unchanged (cache)",
        rows.len(),
        rows.len() - done
    );
    Ok(Ran {
        rows,
        cache: cache_path.map(|cp| (cp, Value::Dict(new))),
        line,
    })
}

/// `_write_json(path, data)`: indent 0, through `<path>.tmp-<pid>`.
fn write_json(path: &str, v: &Value) -> PyResult<()> {
    let dir = pyos::dirname(path)?;
    if dir.is_empty() {
        return Err(Fail::Python); // os.makedirs('') raises its own WinError
    }
    pyos::makedirs(&dir)?;
    let tmp = format!("{path}.tmp-{}", std::process::id()); // per process (AU4-CQ-18)
    pyos::write_text_utf8(&tmp, &pyjson::dumps(v, Some(0)))?;
    pyos::files::replace(&tmp, path).map_err(|e| os_error(path, &e))
}

/// `_num`: inf as the string 'inf'.
fn num(v: &Value) -> Value {
    match v {
        Value::Float(f) if *f == f64::INFINITY => Value::Str("inf".into()),
        Value::Dict(d) => Value::Dict(d.iter().map(|(k, x)| (k.clone(), num(x))).collect()),
        Value::List(l) => Value::List(l.iter().map(num).collect()),
        x => x.clone(),
    }
}

/// Python's `str()` of a value in an f-string.
fn py_str(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "None".into(),
        Some(Value::Str(s)) => s.clone(),
        Some(x) => py_repr(x),
    }
}

fn py_repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(b) => if *b { "True" } else { "False" }.into(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => float_repr(*f),
        Value::Str(s) => str_repr(s),
        Value::List(l) => format!("[{}]", l.iter().map(py_repr).collect::<Vec<_>>().join(", ")),
        Value::Dict(d) => format!(
            "{{{}}}",
            d.iter()
                .map(|(k, x)| format!("{}: {}", str_repr(k), py_repr(x)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn flags_of(r: &Value) -> PyResult<Vec<[String; 4]>> {
    let fl = get(r, "flags").and_then(Value::as_list).ok_or(Fail::Python)?;
    fl.iter()
        .map(|f| {
            let s = |k: &str| f.get(k).and_then(Value::as_str).map(str::to_owned).ok_or(Fail::Python);
            Ok([s("code")?, s("severity")?, s("cause")?, s("detail")?])
        })
        .collect()
}

fn key_of(r: &Value) -> PyResult<String> {
    let root = get(r, "root")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_end_matches(['\\', '/'])
        .to_owned();
    let path = get(r, "path").and_then(Value::as_str).unwrap_or("");
    let rel = if !path.is_empty() && !root.is_empty() {
        pyos::relpath(path, &root)?.replace('\\', "/")
    } else {
        get(r, "name").and_then(Value::as_str).ok_or(Fail::Python)?.to_owned()
    };
    Ok(format!("{}/{rel}", pyos::basename(&root)?).to_lowercase())
}

fn errors_of(r: &Value) -> PyResult<Vec<String>> {
    let mut e: Vec<String> = flags_of(r)?
        .into_iter()
        .filter(|f| f[1] == "error")
        .map(|f| f[0].clone())
        .collect();
    e.sort();
    e.dedup();
    Ok(e)
}

fn summary(rows: &[Value]) -> PyResult<Value> {
    let mut by_code: Vec<(String, i128)> = Vec::new();
    let mut by_cause: Vec<(String, i128)> = Vec::new();
    let bump = |v: &mut Vec<(String, i128)>, k: String| {
        if let Some(s) = v.iter_mut().find(|(x, _)| *x == k) {
            s.1 += 1;
        } else {
            v.push((k, 1));
        }
    };
    let mut flagged = 0;
    for r in rows {
        let fl = flags_of(r)?;
        for f in &fl {
            bump(&mut by_code, format!("{}: {}", f[1], f[0]));
            bump(&mut by_cause, f[2].clone());
        }
        flagged += i128::from(fl.iter().any(|f| f[1] != "info"));
    }
    by_code.sort_by(|a, b| a.0.cmp(&b.0));
    let truthy = |r: &Value, k: &str| get(r, k).is_some_and(Value::truthy);
    let count = |f: &dyn Fn(&Value) -> bool| Value::Int(rows.iter().filter(|r| f(r)).count() as i128);
    Ok(Value::dict([
        ("textures", Value::Int(rows.len() as i128)),
        ("with_source", count(&|r| truthy(r, "source"))),
        ("identical_to_source", count(&|r| truthy(r, "identical_source"))),
        ("with_dice_copy", count(&|r| truthy(r, "dice"))),
        (
            "identical_to_dice",
            count(&|r| {
                get(r, "dice")
                    .and_then(|d| d.get("identical"))
                    .is_some_and(Value::truthy)
            }),
        ),
        ("flagged", Value::Int(flagged)),
        (
            "flags",
            Value::Dict(by_code.into_iter().map(|(k, n)| (k, Value::Int(n))).collect()),
        ),
        (
            "causes",
            Value::Dict(by_cause.into_iter().map(|(k, n)| (k, Value::Int(n))).collect()),
        ),
    ]))
}

fn score(r: &Value) -> f64 {
    get(r, "score").and_then(Value::as_f64).unwrap_or(0.0)
}

fn sorted_worst(rows: &[Value]) -> Vec<&Value> {
    let mut v: Vec<&Value> = rows.iter().collect();
    v.sort_by(|a, b| (-score(a)).partial_cmp(&-score(b)).unwrap_or(std::cmp::Ordering::Equal));
    v
}

fn pad(s: &str, n: usize) -> String {
    let k = s.chars().count();
    if k >= n {
        s.to_owned()
    } else {
        format!("{s}{}", " ".repeat(n - k))
    }
}

fn text_report(rows: &[Value], regress: Option<&[String]>) -> PyResult<String> {
    let s = summary(rows)?;
    let n = |k: &str| s.get(k).and_then(Value::as_int).unwrap_or(0);
    let mut out = vec![
        format!(
            "{} textures; {} with a source ({} decoding identically at every level); {} with DICE's PC copy ({} identical); {} flagged (error / warn)",
            n("textures"),
            n("with_source"),
            n("identical_to_source"),
            n("with_dice_copy"),
            n("identical_to_dice"),
            n("flagged")
        ),
        String::new(),
        "flags:".into(),
    ];
    if let Some(Value::Dict(d)) = s.get("flags") {
        for (k, v) in d {
            out.push(format!("  {:5}  {k}", v.as_int().unwrap_or(0)));
        }
    }
    if let Some(reg) = regress {
        out.push(String::new());
        out.push(format!("regressions against the baseline: {}", reg.len()));
        out.extend(reg.iter().map(|x| format!("  {x}")));
    }
    out.push(String::new());
    out.push("flagged textures, worst first (severity, cause, check: detail):".into());
    for r in sorted_worst(rows) {
        let fl: Vec<[String; 4]> = flags_of(r)?
            .into_iter()
            .filter(|f| f[1] != "info" || f[2] == "dice" || f[2] == "intentional")
            .collect();
        if !fl.iter().any(|f| f[1] != "info") {
            continue;
        }
        let o = get(r, "ours").filter(|o| o.truthy());
        let og = |k: &str| o.and_then(|o| o.get(k));
        let fmt = match og("format") {
            None => "None".to_owned(),
            Some(Value::Int(f)) => name_of(*f).map_or_else(|| f.to_string(), str::to_owned),
            Some(x) => py_str(Some(x)),
        };
        out.push(format!(
            "{}  [{}]  {fmt} {} mips {}  PSNR source {} DICE {}",
            get(r, "name").and_then(Value::as_str).ok_or(Fail::Python)?,
            key_of(r)?,
            py_str(og("size")),
            py_str(og("mips")),
            py_str(get(r, "psnr_source")),
            py_str(get(r, "psnr_dice"))
        ));
        for f in fl {
            out.push(format!("    {} {} {}: {}", pad(&f[1], 5), pad(&f[2], 11), f[0], f[3]));
        }
    }
    Ok(out.join("\n") + "\n")
}

fn baseline_of(rows: &[Value]) -> PyResult<Value> {
    let mut t: Vec<(String, Value)> = Vec::new();
    for r in rows {
        let k = key_of(r)?;
        let v = Value::dict([
            ("psnr_source", get(r, "psnr_source").cloned().unwrap_or(Value::Null)),
            ("psnr_dice", get(r, "psnr_dice").cloned().unwrap_or(Value::Null)),
            (
                "flags",
                Value::List(errors_of(r)?.into_iter().map(Value::Str).collect()),
            ),
        ]);
        if let Some(slot) = t.iter_mut().find(|(x, _)| *x == k) {
            slot.1 = v;
        } else {
            t.push((k, v));
        }
    }
    Ok(Value::dict([
        ("version", Value::Int(VERSION)),
        ("textures", Value::Dict(t)),
    ]))
}

fn as_db(v: &Value) -> PyResult<f64> {
    match v {
        Value::Str(s) if s == "inf" => Ok(99.0),
        Value::Float(f) if *f == f64::INFINITY => Ok(99.0),
        Value::Int(_) | Value::Float(_) => v.as_f64().ok_or(Fail::Python),
        _ => Err(Fail::Python),
    }
}

fn regressions(rows: &[Value], base: &Value, threshold: f64) -> PyResult<Vec<String>> {
    let Value::Dict(_) = base else {
        return Err(Fail::Python);
    };
    let empty = Value::Dict(Vec::new());
    let old = base.get("textures").unwrap_or(&empty);
    if !matches!(old, Value::Dict(_)) {
        return Err(Fail::Python);
    }
    let mut out = Vec::new();
    for r in rows {
        let k = key_of(r)?;
        let errs = errors_of(r)?;
        let Some(b) = old.get(&k).filter(|b| !matches!(b, Value::Null)) else {
            if !errs.is_empty() {
                out.push(format!("{k}: new texture with {}", errs.join(", ")));
            }
            continue;
        };
        let bf: Vec<&str> = b
            .get("flags")
            .and_then(Value::as_list)
            .ok_or(Fail::Python)?
            .iter()
            .map(|x| x.as_str().ok_or(Fail::Python))
            .collect::<PyResult<_>>()?;
        let added: Vec<&String> = errs.iter().filter(|e| !bf.contains(&e.as_str())).collect();
        if !added.is_empty() {
            out.push(format!(
                "{k}: new {}",
                added.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        for f in ["psnr_source", "psnr_dice"] {
            let (Some(was), Some(now)) = (b.get(f), get(r, f)) else {
                continue;
            };
            if matches!(was, Value::Null) || matches!(now, Value::Null) {
                continue;
            }
            let (was, now) = (as_db(was)?, as_db(now)?);
            if now < was - threshold {
                out.push(format!("{k}: {f} {} -> {} dB", float_repr(was), float_repr(now)));
            }
        }
    }
    Ok(out)
}

/// texqa's `main` (`errors.guard` reports a `TexQaError`).
pub fn main(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    // the help consumed first (argparse takes the optional at index 0 before anything after it)
    if let Some(h) = argv.first()
        && (h == "-h"
            || (h.starts_with("--")
                && !h.contains('=')
                && crate::argfmt::long_option(h, &STRINGS) == Long::Found("--help")))
    {
        // argparse's help, at the console's width (round RT2: crate::argfmt)
        py.write(&crate::argfmt::help("texqa")?)?;
        return Ok(0);
    }
    let a = parse(argv)?;
    for p in &a.inputs {
        if !pyos::exists(p) {
            return Err(qa_err(format!("{p}: no such file or folder")));
        }
    }
    let sources: Vec<Loose> = a.console.iter().map(|c| Loose::new(c)).collect::<PyResult<_>>()?;
    if !a.no_bf1943 {
        // bf1943import.default_source: BF1943_XENON when set (unchecked), else beside the suite
        let env = std::env::var("BF1943_XENON").ok().filter(|v| !v.is_empty());
        let folder = if let Some(f) = a.bf1943.clone().or(env) {
            f
        } else {
            let suite = pyos::abspath(&suite_root().to_string_lossy())?;
            pyos::join(
                &pyos::dirname(&suite)?,
                &["operation-rising-sun", "work", "extracted", "dist", "xenon"],
            )?
        };
        if pyos::isdir(&folder) {
            return Err(Fail::Python); // BF1943's bundles as a source: bf1943import.Source
        }
        py.print(&format!(
            "no BF1943 source at {folder}: textures are compared with DICE's copies only"
        ))?;
    }
    if !a.no_dice {
        return Err(Fail::Python); // DICE's copies: the stock bundles' index (meshview, fbrb)
    }
    if a.out.is_empty() || pyos::isfile(&a.out) {
        return Err(Fail::Python);
    }
    if let Some(w) = &a.write_baseline
        && pyos::dirname(w)?.is_empty()
    {
        return Err(Fail::Python);
    }
    let ran = run(py, &a, &sources)?;
    let rows = ran.rows;
    // the baseline is read before the sheets are drawn: a baseline that cannot be read ends the run (after the
    // cache) whether there would have been sheets or not
    let reg: PyResult<Option<Vec<String>>> = match &a.baseline {
        Some(b) => match pyos::read(b) {
            Ok(data) => {
                let base = pyjson::loads_ansi(&data).ok_or(Fail::Python)?;
                Ok(Some(regressions(&rows, &base, a.threshold)?))
            }
            Err(Fail::Python) => return Err(Fail::Python),
            Err(e) => Err(e),
        },
        None => Ok(None),
    };
    if let Err(e) = reg {
        if let Some((cp, new)) = &ran.cache {
            write_json(cp, new)?;
        }
        py.print(&ran.line)?;
        py.flush_out();
        return Err(e);
    }
    // contact sheets (PIL): handed off where the report would have any
    let worst = sorted_worst(&rows)
        .into_iter()
        .filter(|r| {
            flags_of(r).is_ok_and(|fl| {
                fl.iter()
                    .any(|f| f[1] != "info" && f[2] != "intentional" && f[2] != "dice")
            })
        })
        .take(usize::try_from(a.worst).unwrap_or(0))
        .count();
    if worst > 0 {
        return Err(Fail::Python);
    }
    if let Some((cp, new)) = &ran.cache {
        write_json(cp, new)?;
    }
    py.print(&ran.line)?;
    py.flush_out();
    let reg = reg?;
    pyos::makedirs(&a.out)?;
    let summ = summary(&rows)?;
    let doc = Value::dict([
        ("summary", summ.clone()),
        (
            "regressions",
            reg.as_ref().map_or(Value::Null, |r| {
                Value::List(r.iter().cloned().map(Value::Str).collect())
            }),
        ),
        ("rows", Value::List(rows.clone())),
    ]);
    write_json(&pyos::join2(&a.out, "texqa.json")?, &num(&doc))?;
    let txt = pyos::join2(&a.out, "texqa.txt")?;
    pyos::write_text_utf8(&txt, &text_report(&rows, reg.as_deref())?)?;
    pyos::makedirs(&pyos::join2(&a.out, "sheets")?)?; // sheets() makes its folder whether it draws a page or not
    if let Some(w) = &a.write_baseline {
        write_json(w, &num(&baseline_of(&rows)?))?;
    }
    let n = |k: &str| summ.get(k).and_then(Value::as_int).unwrap_or(0);
    py.print(&format!(
        "{} textures, {} flagged -> {txt}; 0 contact sheet(s)",
        n("textures"),
        n("flagged")
    ))?;
    if let Some(reg) = reg.filter(|r| !r.is_empty()) {
        py.print(&format!(
            "{} regression(s) against {}:",
            reg.len(),
            a.baseline.as_deref().unwrap_or("")
        ))?;
        for x in reg.iter().take(30) {
            py.print(&format!("  {x}"))?;
        }
        return Ok(1);
    }
    Ok(0)
}
