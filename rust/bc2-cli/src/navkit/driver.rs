//! `navkit.py`'s command line (`main`: levels, prepare, compare, check, install, restore; the rest handed to the
//! Python) and `prepare_level.py` (the editor's prepare: NavKit's prepare and the terrain unpack, its progress as
//! `@step` / `@log` / `@error` / `@done` lines flushed as they come).

use std::path::Path;

use bc2_formats::gameledger::{self, Ledger, Legacy};
use regex::Regex;

use super::navcompare;
use super::prepare::{heightfield_of, prepare};
use super::project::{BACKUP_SUFFIX, Log, NEW_SUFFIX, Print, Project, suite};
use super::{navkit_error, py_float, splitlines, strip};
use crate::pyfmt::{self, str_repr};
use crate::pyjson::{self, Value};
use crate::pyos::{self, join, join2};
use crate::pyproc::{Fail, Py, PyErr, PyResult};

fn index_error() -> Fail {
    Fail::Err(PyErr::new("IndexError", "list index out of range"))
}

/// `game_running()`: `tasklist` names BFBC2Game.exe. A `tasklist` that does not answer in 30 s (the service host
/// busy, as during the game's crash dump) counts as the game running: a refusal, never a hang (AU4-BUG-20).
pub(crate) fn game_running() -> bool {
    let mut c = std::process::Command::new("tasklist");
    c.args(["/FI", "IMAGENAME eq BFBC2Game.exe", "/NH"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        c.creation_flags(0x0800_0000);
    }
    let Ok(mut child) = c.spawn() else {
        return false;
    };
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < std::time::Duration::from_secs(30) => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return true;
            }
        }
    }
    child
        .wait_with_output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("BFBC2Game.exe"))
}

fn traversal_path(folder: &str, lv: &str) -> PyResult<String> {
    join2(folder, &format!("{lv}.traversal.json"))
}

/// `levels(pr)`.
fn levels(py: &mut Py, pr: &Project) -> PyResult<()> {
    py.print("level        AI path data   NavKit")?;
    for (lv, _) in pr.level_dirs()? {
        let ai = if pr.dice_pathdata(&lv)?.is_some() {
            "yes (samplable)"
        } else {
            "no"
        };
        let mut done = Vec::new();
        if pyos::isfile(&join(&pr.work, &["navkit", &lv, "navkit.json"])?) {
            done.push("prepared");
        }
        if pyos::isfile(&join(&pr.work, &["navkit", &lv, "pathdata.hpd"])?) {
            done.push("built");
        }
        if installed(pr, &lv)? {
            done.push("INSTALLED");
        }
        py.print(&format!("{lv:<12} {ai:<14} {}", done.join(", ")))?;
    }
    Ok(())
}

/// The owner NavKit installs a level's path data as, in the game copy's install ledger.
pub(crate) fn owner(lv: &str) -> String {
    format!("navkit:{}", lv.to_lowercase())
}

fn ledger_fail(e: &gameledger::Error) -> Fail {
    navkit_error(e.to_string())
}

/// The game-relative path of a game file.
fn game_rel(pr: &Project, p: &str) -> PyResult<String> {
    Ok(pyos::relpath(p, &pr.game)?.replace('\\', "/"))
}

/// `installed(pr, lv)`: NavKit's install of the level is in the game copy's install ledger, or its markers from
/// before the ledger are beside the path data.
fn installed(pr: &Project, lv: &str) -> PyResult<bool> {
    let pd = pr.pathdata(lv)?;
    if pyos::isfile(&format!("{pd}{BACKUP_SUFFIX}")) || pyos::isfile(&format!("{pd}{NEW_SUFFIX}")) {
        return Ok(true);
    }
    let led = Ledger::new(Path::new(&pr.game));
    Ok(!led.owner_files(&owner(lv)).map_err(|e| ledger_fail(&e))?.is_empty())
}

/// NavKit's own legacy markers beside the path data (`.opfor-orig` / `.opfor-new`, from before the install ledger)
/// adopted into the ledger; another tool's refused in words.
fn adopt_legacy(py: &mut Py, led: &mut Ledger, lv: &str, rel: &str) -> PyResult<()> {
    match led.adopt_legacy(&owner(lv), &[rel.to_owned()], Legacy::NavKit) {
        Ok(Some(_)) => py.print(&format!(
            "  NavKit's markers from before the install ledger moved into it ({rel})"
        )),
        Ok(None) => Ok(()),
        Err(e) => Err(ledger_fail(&e)),
    }
}

/// `traversal_made_for(trav)`: the `pathdata_sha1` the annotations were made for (None: no file, not JSON, no key).
fn made_for(trav: &str) -> PyResult<Option<String>> {
    if !pyos::isfile(trav) {
        return Ok(None);
    }
    Ok(match std::fs::read(trav).ok().and_then(|d| pyjson::loads_utf8(&d)) {
        Some(Value::Dict(items)) => items
            .into_iter()
            .find(|(k, _)| k == "pathdata_sha1")
            .and_then(|(_, v)| v.as_str().map(str::to_owned)),
        Some(other) => {
            let t = match other {
                Value::List(_) => "list",
                Value::Str(_) => "str",
                Value::Int(_) => "int",
                Value::Float(_) => "float",
                Value::Bool(_) => "bool",
                _ => "NoneType",
            };
            return Err(Fail::Err(PyErr::new(
                "AttributeError",
                format!("'{t}' object has no attribute 'get'"),
            )));
        }
        None => None,
    })
}

/// `install(pr, lv, src=src)`: the built file (and its traversal annotations, when made for it) installed as
/// `navkit:<lv>` in the game copy's install ledger (round DF2): what was there is kept there once, by its sha256, and
/// `restore` gives it back whatever was installed over NavKit since.
pub(super) fn install(py: &mut Py, pr: &Project, lv: &str, src: Option<&str>) -> PyResult<()> {
    let ours = match src.filter(|s| !s.is_empty()) {
        Some(s) => s.to_owned(),
        None => join2(&pr.out_dir(lv)?, "pathdata.hpd")?,
    };
    if !pyos::isfile(&ours) {
        return Err(navkit_error(format!("{lv}: not built yet")));
    }
    if game_running() {
        return Err(navkit_error("the game is running - close it first"));
    }
    let dest = pr.pathdata(lv)?;
    let rel = game_rel(pr, &dest)?;
    let mut led = Ledger::new(Path::new(&pr.game));
    adopt_legacy(py, &mut led, lv, &rel)?;
    let me = owner(lv);
    let holders = led.holders(&rel).map_err(|e| ledger_fail(&e))?;
    let data = pyos::read(&ours)?;
    let trav = traversal_path(&pyos::dirname(&ours)?, lv)?;
    let trav_dest = traversal_path(&pyos::dirname(&dest)?, lv)?;
    let trav_rel = game_rel(pr, &trav_dest)?;
    let with_trav = made_for(&trav)?.as_deref() == Some(pyos::sha1_hex(&data).as_str());
    let mut files = vec![(rel.clone(), data)];
    if with_trav {
        files.push((trav_rel.clone(), pyos::read(&trav)?));
    }
    if !holders.contains(&me) {
        match holders.last() {
            Some(top) => py.print(&format!(
                "  installed over {top}'s file (either can be reverted first: the ledger keeps what each one replaced)"
            ))?,
            None if pyos::isfile(&dest) => py.print(&format!(
                "  the original {} kept in the game copy's install ledger (restore puts it back)",
                pyos::basename(&dest)?
            ))?,
            None => py.print(&format!(
                "  noting {} had no original (restore will remove it)",
                pyos::basename(&dest)?
            ))?,
        }
    }
    let plan = led
        .install(&me, &files, false, "NavKit path data", false)
        .map_err(|e| ledger_fail(&e))?;
    for n in &plan.notes {
        py.print(&format!("  {n}"))?;
    }
    py.print(&format!(
        "== installed {} as {}",
        pyos::relpath(&ours, &pr.root)?,
        pyos::relpath(&dest, &pr.root)?
    ))?;
    if with_trav {
        py.print(&format!(
            "  traversal annotations installed as {}",
            pyos::relpath(&trav_dest, &pr.root)?
        ))?;
    } else {
        if pyos::isfile(&trav) {
            py.print(&format!(
                "  traversal annotations {} not installed: made for another path-data file",
                pyos::basename(&trav)?
            ))?;
        }
        // stale annotations nobody installed through the ledger (from before it): none for another file
        if pyos::isfile(&trav_dest) && led.holders(&trav_rel).map_err(|e| ledger_fail(&e))?.is_empty() {
            std::fs::remove_file(&trav_dest).map_err(|e| crate::pyproc::os_error(&trav_dest, &e))?;
        }
    }
    Ok(())
}

/// `restore(pr, lv)`: NavKit's install taken out of the game copy's install ledger - the file from before NavKit put
/// back, or, when another tool installed over NavKit since, NavKit's layer taken out from under it (that tool's
/// revert then gives back what was there before NavKit).
fn restore(py: &mut Py, pr: &Project, lv: &str) -> PyResult<()> {
    let dest = pr.pathdata(lv)?;
    let rel = game_rel(pr, &dest)?;
    let me = owner(lv);
    let legacy = gameledger::legacy_markers(Path::new(&dest))
        .iter()
        .any(|m| m.eq_ignore_ascii_case(BACKUP_SUFFIX) || m.eq_ignore_ascii_case(NEW_SUFFIX));
    let mut led = Ledger::new(Path::new(&pr.game));
    let installed = !led.owner_files(&me).map_err(|e| ledger_fail(&e))?.is_empty();
    if !installed && !legacy {
        return py.print(&format!("{lv}: nothing to restore (no NavKit file installed)"));
    }
    if game_running() {
        return Err(navkit_error("the game is running - close it first"));
    }
    adopt_legacy(py, &mut led, lv, &rel)?;
    if led.owner_files(&me).map_err(|e| ledger_fail(&e))?.is_empty() {
        return py.print(&format!("{lv}: nothing to restore (no NavKit file installed)"));
    }
    let holders = led.holders(&rel).map_err(|e| ledger_fail(&e))?;
    let above = holders
        .iter()
        .position(|h| *h == me)
        .and_then(|i| holders.get(i + 1))
        .cloned();
    let plan = led.uninstall(&me, false).map_err(|e| ledger_fail(&e))?;
    for n in plan.notes.iter().filter(|n| !n.contains("taken out from under")) {
        py.print(&format!("  {n}"))?;
    }
    if let Some(o) = above {
        py.print(&format!(
            "== NavKit's {} taken out from under {o}'s install: the file stays {o}'s, and its revert now puts back \
             what was there before NavKit",
            pyos::relpath(&dest, &pr.root)?
        ))?;
    } else if pyos::isfile(&dest) {
        py.print(&format!("== restored the original {}", pyos::relpath(&dest, &pr.root)?))?;
    } else {
        py.print(&format!(
            "== removed the installed {} (the level had no path data before)",
            pyos::relpath(&dest, &pr.root)?
        ))?;
    }
    let trav = traversal_path(&pyos::dirname(&dest)?, lv)?;
    if pyos::isfile(&trav)
        && led
            .holders(&game_rel(pr, &trav)?)
            .map_err(|e| ledger_fail(&e))?
            .is_empty()
    {
        std::fs::remove_file(&trav).map_err(|e| crate::pyproc::os_error(&trav, &e))?;
    }
    Ok(())
}

/// The converted level partition `<work>\levels\<lv>\dist\levels\<s['level']>.xml`.
fn level_xml(pr: &Project, lv: &str, s: &Value) -> PyResult<String> {
    let name = match s.get("level") {
        Some(Value::Str(n)) => n.clone(),
        Some(_) => return Err(Fail::Python),
        None => return Err(Fail::Err(PyErr::new("KeyError", "'level'"))),
    };
    join(&pr.level_work(lv)?, &["dist", "levels", &format!("{name}.xml")])
}

/// `compare(pr, lv, reference=, candidate=)`.
fn compare(py: &mut Py, pr: &Project, lv: &str, reference: Option<&str>, candidate: Option<&str>) -> PyResult<()> {
    let s = pr.settings(lv)?;
    let reference = match reference.filter(|r| !r.is_empty()) {
        Some(r) => Some(r.to_owned()),
        None => pr.dice_pathdata(lv)?,
    };
    let candidate = match candidate.filter(|c| !c.is_empty()) {
        Some(c) => c.to_owned(),
        None => join2(&pr.out_dir(lv)?, "pathdata.hpd")?,
    };
    let Some(reference) = reference.filter(|r| pyos::isfile(r)) else {
        return Err(navkit_error(format!(
            "{lv}: no reference file (the level has no original path data; pass --reference)"
        )));
    };
    if !pyos::isfile(&candidate) {
        return Err(navkit_error(format!(
            "{lv}: no candidate file {candidate} (build first, or pass --candidate)"
        )));
    }
    let xml = level_xml(pr, lv, &s)?;
    py.print(&format!(
        "== compare {lv}: reference {}, candidate {}",
        pyos::relpath(&reference, &pr.root)?,
        pyos::relpath(&candidate, &pr.root)?
    ))?;
    for l in navcompare::report(&navcompare::compare(&xml, &reference, &candidate)?) {
        py.print(&format!("  {l}"))?;
    }
    Ok(())
}

/// `check(pr, lv)`: ordercheck as its own process (bfbc2.exe's port, as navkit.py runs ordercheck.py), its report
/// kept as check.txt, the summary against the original.
fn check(py: &mut Py, pr: &Project, lv: &str) -> PyResult<()> {
    let s = pr.settings(lv)?;
    let ours = join2(&pr.out_dir(lv)?, "pathdata.hpd")?;
    if !pyos::isfile(&ours) {
        return Err(navkit_error(format!("{lv}: not built yet")));
    }
    let xml = level_xml(pr, lv, &s)?;
    let rf = pr.dice_pathdata(lv)?;
    let script = join(&suite()?, &["bfbc2-auto-navkit", "tools", "ordercheck.py"])?;
    let mut args = vec![xml.clone()];
    args.extend(rf.iter().cloned());
    args.push(ours.clone());
    // the summary first (no side effects): where the core refuses, the line goes to the Python before the ordercheck
    // process has written its log
    // (an error it raises is raised where the Python meets it, after ordercheck's run)
    let summary = match &rf {
        Some(rf) => match navcompare::compare(&xml, rf, &ours) {
            Ok(r) => Some(Ok(navcompare::report(&r))),
            Err(Fail::Python) => return Err(Fail::Python),
            Err(e) => Some(Err(e)),
        },
        None => None,
    };
    py.print(&format!(
        "== check {lv}{}",
        if rf.is_some() {
            " against the original path data"
        } else {
            " (self-check: no original path data)"
        }
    ))?;
    let exe = std::env::current_exe().map_err(|_| Fail::Python)?;
    let mut c = std::process::Command::new(exe);
    c.arg("ordercheck").args(&args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        c.creation_flags(0x0800_0000);
    }
    let o = c.output().map_err(|_| Fail::Python)?;
    let mut text = Vec::new();
    text.extend_from_slice(&o.stdout);
    text.extend_from_slice(&o.stderr);
    if !text.is_ascii() {
        return Err(Fail::Python); // the locale decoding with errors='replace': left to Python
    }
    let text = super::universal(&String::from_utf8_lossy(&text));
    let out: Vec<String> = splitlines(strip(&text)).into_iter().map(str::to_owned).collect();
    if o.status.code() != Some(0) {
        let tail = &out[out.len().saturating_sub(3)..];
        return Err(navkit_error(format!(
            "python.exe {script} failed: {}",
            tail.join(" | ")
        )));
    }
    let report = join2(&pr.out_dir(lv)?, "check.txt")?;
    pyos::write_text_utf8(&report, &format!("{}\n", out.join("\n")))?;
    let key = Regex::new(r"positions:|on an area|route check|self-check").map_err(|_| Fail::Python)?;
    for l in &out {
        if key.is_match(l) && !l.starts_with("  ") {
            py.print(&format!("  {l}"))?;
        }
    }
    if let Some(lines) = summary {
        py.print("  summary against the original:")?;
        for l in lines? {
            py.print(&format!("    {l}"))?;
        }
    }
    py.print(&format!("  full report: {}", pyos::relpath(&report, &pr.root)?))
}

/// `status(pr, lv)` (round O5): every line made before the first is printed, so a value the port does not write as
/// the Python does (a settings field of another type) hands the whole run to the Python.
fn status(py: &mut Py, pr: &Project, lv: &str) -> PyResult<()> {
    let mut out = vec![format!("{lv}:")];
    let dice = pr.dice_pathdata(lv)?;
    out.push(format!("  original path data : {}", dice.as_deref().unwrap_or("none")));
    let prepared = pyos::isfile(&join(&pr.work, &["navkit", lv, "navkit.json"])?);
    out.push(format!(
        "  prepared           : {}",
        if prepared { "yes" } else { "no" }
    ));
    if prepared {
        let s = match pr.settings(lv) {
            Ok(s) => s,
            Err(Fail::Err(e)) if e.kind == "NavKitError" => {
                // the lines before it are out when the Python raises (a damaged navkit.json)
                for line in &out {
                    py.print(line)?;
                }
                return Err(Fail::Err(e));
            }
            Err(e) => return Err(e),
        };
        let nums = |k: &str| -> PyResult<String> {
            let Some(Value::List(v)) = s.get(k) else {
                return Err(Fail::Python);
            };
            let parts: Vec<String> = v
                .iter()
                .map(|x| match x {
                    Value::Int(i) if i.unsigned_abs() < (1 << 53) => Ok(pyfmt::float_g(*i as f64)),
                    Value::Float(f) => Ok(pyfmt::float_g(*f)),
                    _ => Err(Fail::Python),
                })
                .collect::<PyResult<_>>()?;
            Ok(parts.join(","))
        };
        let int = |k: &str| -> PyResult<String> {
            match s.get(k) {
                Some(Value::Int(i)) => Ok(i.to_string()),
                _ => Err(Fail::Python),
            }
        };
        out.push(format!(
            "    box {}, y {}, offsets {},{}",
            nums("box")?,
            nums("y")?,
            int("xoff")?,
            int("zoff")?
        ));
    }
    let when = |p: &str| -> PyResult<String> {
        if pyos::isfile(p) {
            Ok(crate::clock::ctime(pyos::getmtime(p)?))
        } else {
            Ok("no".to_owned())
        }
    };
    for suf in ["", "_hi"] {
        let p = join(&pr.work, &["navsample", &format!("{lv}_navkit{suf}.bin")])?;
        out.push(format!("  samples{suf:12}: {}", when(&p)?));
    }
    let built = join(&pr.work, &["navkit", lv, "pathdata.hpd"])?;
    out.push(format!("  built              : {}", when(&built)?));
    out.push(format!(
        "  installed in game  : {}",
        if installed(pr, lv)? { "yes" } else { "no" }
    ));
    for line in out {
        py.print(&line)?;
    }
    Ok(())
}

fn floats_arg(v: &str) -> PyResult<Vec<f64>> {
    v.split(',').map(py_float).collect()
}

/// `navkit.py <command> <level> [options]` (`main`).
pub fn navkit(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    let mut args: Vec<String> = argv.to_vec();
    let mut opts: Vec<(&str, String)> = Vec::new();
    for k in [
        "--box",
        "--y",
        "--opfor",
        "--root",
        "--reference",
        "--candidate",
        "--navtune",
        "--out",
        "--from",
        "--pitch",
    ] {
        if let Some(i) = args.iter().position(|a| a == k) {
            let v = args.get(i + 1).cloned().ok_or_else(index_error)?;
            opts.push((k, v));
            args.drain(i..i + 2);
        }
    }
    let opt = |k: &str| opts.iter().find(|(x, _)| *x == k).map(|(_, v)| v.as_str());
    if args.iter().any(|a| a == "--no-detail") {
        return Err(Fail::Python); // navbuild's settings: the build's Python
    }
    let install_flag = args.iter().any(|a| a == "--install");
    args.retain(|a| a != "--yes" && a != "--install");
    if args.is_empty() || matches!(args[0].as_str(), "-h" | "--help" | "help") {
        py.print(&pyfmt::docstring(include_str!(
            "../../../../bfbc2-auto-navkit/navkit.py"
        )))?;
        return Ok(0);
    }
    let bx = opt("--box").map(floats_arg).transpose()?;
    let y = opt("--y").map(floats_arg).transpose()?;
    let r = (|| -> PyResult<i32> {
        let given = opt("--opfor").filter(|v| !v.is_empty()).or(opt("--root"));
        let pr = Project::new(given)?;
        let cmd = args[0].as_str();
        if cmd == "levels" {
            levels(py, &pr)?;
            return Ok(0);
        }
        if args.len() < 2 {
            return Err(navkit_error(format!(
                "{cmd}: which level? (e.g. navkit.py {cmd} SP_01)"
            )));
        }
        let lv = pr.level(&args[1])?;
        match cmd {
            // NavKit's generator (navbuild), the game (sample): the Python's
            "build" | "all" | "sample" => Err(Fail::Python),
            // round NK1: a DICE base here, a NavKit base (a rebuild) the Python's
            "apply" => super::apply::apply(py, &pr, &lv, opt("--navtune"), opt("--out"), install_flag).map(|()| 0),
            "status" => status(py, &pr, &lv).map(|()| 0),
            // unbuffered (navkitGUI: python -u, or PYTHONUNBUFFERED for this binary): each line out as it comes
            "prepare" if std::env::var("PYTHONUNBUFFERED").is_ok_and(|v| !v.is_empty()) => {
                prepare(py, &pr, &lv, &mut Streamed, bx.as_deref(), y.as_deref()).map(|()| 0)
            }
            "prepare" => prepare(py, &pr, &lv, &mut Print, bx.as_deref(), y.as_deref()).map(|()| 0),
            "compare" => compare(py, &pr, &lv, opt("--reference"), opt("--candidate")).map(|()| 0),
            "check" => check(py, &pr, &lv).map(|()| 0),
            "install" => install(py, &pr, &lv, opt("--from")).map(|()| 0),
            "restore" => restore(py, &pr, &lv).map(|()| 0),
            _ => Err(navkit_error(format!("unknown command {}", str_repr(cmd)))),
        }
    })();
    match r {
        Err(Fail::Err(e)) if e.kind == "NavKitError" => {
            py.print(&format!("error: {}", e.msg))?;
            py.log("ERROR", format!("{}: {}", argv.join(" "), e.msg));
            Ok(1)
        }
        other => other,
    }
}

/// `log=print` with the line written out at once (an unbuffered prepare: the reader follows it).
struct Streamed;

impl Log for Streamed {
    fn log(&mut self, py: &mut Py, text: &str) -> PyResult<()> {
        py.print(text)?;
        py.flush_now();
        Ok(())
    }
}

/// `prepare_level.py`'s progress lines.
struct Progress {
    terrain_only: bool,
    steps: u32,
    step: u32,
}

fn emit(py: &mut Py, kind: &str, text: &str) -> PyResult<()> {
    let line = format!("@{kind} {text}");
    py.print(line.trim_end_matches(super::py_space))?;
    py.flush_now();
    Ok(())
}

impl Progress {
    fn start(&mut self, py: &mut Py, n: u32, what: &str) -> PyResult<()> {
        if n > self.step {
            self.step = n;
            emit(py, "step", &format!("{n} {} {what}", self.steps))?;
        }
        Ok(())
    }
}

impl Log for Progress {
    fn log(&mut self, py: &mut Py, text: &str) -> PyResult<()> {
        let t = strip(text);
        let low = t.to_lowercase();
        if !self.terrain_only {
            if (low.starts_with("unpacking") || low.ends_with("already unpacked")) && !low.contains("_common") {
                self.start(py, 2, "Unpacking the level")?;
            } else if low.starts_with("extracting objects") {
                self.start(py, 3, "Extracting objects, collision and spawn points")?;
            }
        }
        if !t.is_empty() {
            emit(py, "log", t)?;
        }
        Ok(())
    }
}

/// `prepare_level.py <level> [--opfor <folder>] [--terrain-only]`.
pub fn prepare_level(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    let mut args: Vec<String> = argv.to_vec();
    let mut opfor: Option<String> = None;
    if let Some(i) = args.iter().position(|a| a == "--opfor") {
        opfor = args.get(i + 1).cloned();
        let end = (i + 2).min(args.len());
        args.drain(i..end);
    }
    let terrain_only = args.iter().any(|a| a == "--terrain-only");
    args.retain(|a| a != "--terrain-only");
    if args.len() != 1 {
        emit(
            py,
            "error",
            "Which level? (prepare_level.py <level> [--opfor <folder>] [--terrain-only])",
        )?;
        return Ok(1);
    }
    let mut started = false;
    let r = (|| -> PyResult<()> {
        let pr = Project::new(opfor.as_deref().filter(|o| !o.is_empty()))?;
        let lv = pr.level(&args[0])?;
        let mut p = Progress {
            terrain_only,
            steps: if terrain_only { 1 } else { 4 },
            step: 0,
        };
        started = true;
        if !terrain_only {
            p.start(py, 1, "Unpacking the shared bundles (sp_common, mp_common)")?;
            prepare(py, &pr, &lv, &mut p, None, None)?;
        }
        let steps = p.steps;
        p.start(py, steps, "Unpacking the terrain")?;
        heightfield_of(py, &pr.root, &lv, &mut p)
    })();
    match r {
        Ok(()) => {
            emit(py, "done", "")?;
            Ok(0)
        }
        Err(Fail::Err(e)) => {
            let text = if e.kind == "NavKitError" {
                e.msg.clone()
            } else {
                e.line_str()
            };
            emit(py, "error", &text)?;
            Ok(1)
        }
        Err(Fail::Python) if !started => Err(Fail::Python),
        Err(Fail::Python) => {
            // past the first progress line the Python cannot take the run over: said in words
            emit(
                py,
                "error",
                "the Rust port of the prepare refused an input the Python would read (bfbc2.exe)",
            )?;
            Ok(1)
        }
    }
}

trait LineStr {
    fn line_str(&self) -> String;
}

impl LineStr for PyErr {
    /// `f'{type(e).__name__}: {e}'`.
    fn line_str(&self) -> String {
        format!("{}: {}", self.kind, self.msg)
    }
}
