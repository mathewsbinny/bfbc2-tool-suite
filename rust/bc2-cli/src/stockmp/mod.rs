//! Round O3's `stockmp` group: `bfbc2 stockmp`, `bfbc2-stockmp-ai\stockmp.py`'s command line on `bc2-stockmp`.
//!
//! Answered here: the usage, finding Project OPFOR's folder (and its error), and `install` in its three modes -
//! `--plan` (out\game against its `build_report.json`, each file new or replacing one), `--apply` (one transaction:
//! staged copies, the originals' backups, markers; put back on a failure) and `--revert` (the backups restored, the
//! added files removed, from the build output and the game folder's markers); `summary` and `generate` (`level.rs`:
//! the level's unpack, its model, `mp_common`'s kits; round O5: the full layer placed on the path data,
//! `generate.rs`), `build` (`build.rs`: authored, packed, staged), `validate` (`validate.rs`: the offline checks,
//! DbxEdit in process) and `all`. Handed to the Python: any reading the ports refuse (a damaged report, odd names,
//! links, text not UTF-8, a configuration of a shape the authoring port leaves, a check the reference words itself).

use std::fs;
use std::io::{self, Read, Write as _};

use bc2_stockmp::install::{self, APPLIED, BACKUP, Fault, isfile, join};
use bc2_stockmp::sha256;

mod build;
mod generate;
mod level;
mod pyx;
mod validate;
use crate::pyproc::{Fail, Py, PyErr, PyResult, Tool, os_error, suite_root};

/// The usage `bfbc2 stockmp` prints: `stockmp.py`'s docstring, reworded when round RT3 retired that command
/// line (the library is `stockmplib.py`).
const USAGE: &str = include_str!("stockmp_usage.txt");
const COMMANDS: [&str; 6] = ["summary", "generate", "build", "validate", "all", "install"];
/// `stockmp.STAGED`, `stockmp.PREV`.
const STAGED: &str = ".opfor-stockmp-new";
const PREV: &str = ".opfor-stockmp-prev";

/// The group's tools.
pub const TOOLS: &[Tool] = &[Tool {
    name: "stockmp",
    script: "bfbc2-stockmp-ai\\stockmp.py",
    log: Some("stockmp"),
    run,
}];

fn fault(f: Fault) -> Fail {
    match f {
        Fault::Io(path, e) => os_error(&path, &e),
        Fault::Refuse(_) => Fail::Python,
    }
}

/// `raise SystemExit(message)` under `errors.guard` (which lets it through): the message on stderr, exit code 1.
fn system_exit(py: &mut Py, message: &str) -> i32 {
    py.eprint(message);
    1
}

/// `opt(argv, name)`: the value after the first `name` (argv[0], the script, is never an option).
fn opt<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == name)?;
    args.get(i + 1).map(String::as_str)
}

/// `paths.find_opfor(None, start=SUITE)`: `OPFOR_ROOT`, then `project-opfor-bfbc2` in the suite folder and each
/// folder above it; the first with a `game` folder, absolute.
fn find_opfor() -> PyResult<String> {
    let mut cands: Vec<String> = Vec::new();
    if let Ok(v) = std::env::var("OPFOR_ROOT")
        && !v.is_empty()
    {
        cands.push(v);
    }
    let mut d = bc2_pyrt::py::abspath(&suite_root().to_string_lossy());
    cands.push(join(&d, "project-opfor-bfbc2"));
    loop {
        let up = bc2_pyrt::py::dirname(&d);
        if up == d {
            break;
        }
        d = up;
        cands.push(join(&d, "project-opfor-bfbc2"));
    }
    for c in &cands {
        if bc2_pyrt::py::isdir(&join(c, "game")) {
            return Ok(bc2_pyrt::py::abspath(c));
        }
    }
    Err(Fail::Err(PyErr::new(
        "GameCopyNotFound",
        format!(
            "Project OPFOR BFBC2 not found - pass --opfor <folder> or set OPFOR_ROOT (looked for a game\\ folder in: {})",
            cands.join(", ")
        ),
    )))
}

/// `pipeline.Paths(lv, opfor, work, variant)`.
struct Paths {
    lv: String,
    opfor: String,
    game: String,
    work: String,
    variant: Option<String>,
    root: String,
}

impl Paths {
    /// `paths.src(variant)`: the level's cached unpack of a bundle, shared by the variants.
    fn src(&self, v: &str) -> String {
        join(&join(&join(&self.work, &self.lv), "src"), v)
    }

    /// `paths.cache`: the common bundles' unpacks.
    fn cache(&self) -> String {
        join(&self.work, "_cache")
    }

    /// `paths.common`: NavKit's shared unpacks.
    fn common(&self) -> String {
        join(&join(&join(&self.opfor, "work"), "navkit"), "common")
    }

    /// `paths.config()`.
    fn config(&self) -> String {
        join(&self.root, "config.json")
    }

    /// `paths.report()`.
    fn report(&self) -> String {
        join(&self.root, "build_report.json")
    }

    /// `paths.out_bundle(variant, out)`.
    fn out_bundle(&self, v: &str, out: &str) -> String {
        let rel: &[&str] = if v == "dist" {
            &["Dist", "win32", "levels", &self.lv]
        } else {
            &["Package", "levels", &self.lv]
        };
        let base = join(&join(&self.root, out), "game");
        join(&rel.iter().fold(base, |a, n| join(&a, n)), "level-00.fbrb")
    }

    /// `paths.out_pathdata(kind, out)`.
    fn out_pathdata(&self, kind: &str, out: &str) -> String {
        [
            "game",
            "Output",
            "win32",
            "ai",
            "levels",
            &self.lv,
            kind,
            "pathdata.hpd",
        ]
        .iter()
        .fold(join(&self.root, out), |a, n| join(&a, n))
    }

    /// `paths.bundles()`: (variant, stock bundle) of the level's level-00 bundles that exist, as before stock-MP's
    /// install.
    fn bundles(&self) -> PyResult<Vec<(&'static str, String)>> {
        let mut out = Vec::new();
        for (v, rel) in [("dist", "Dist\\win32\\levels"), ("pkg", "Package\\levels")] {
            let p = join(&join(&join(&self.game, rel), &self.lv), "level-00.fbrb");
            if let Some(s) = level::stock_file(&p)? {
                out.push((v, s));
            }
        }
        Ok(out)
    }
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    let (Some(cmd), Some(lv)) = (args.first(), args.get(1)) else {
        py.print(&USAGE.replace("\r\n", "\n"))?;
        return Ok(1);
    };
    if !COMMANDS.contains(&cmd.as_str()) {
        py.print(&USAGE.replace("\r\n", "\n"))?;
        return Ok(1);
    }
    // os.path.join(work, lv) of a level name with a drive or a root, str.lower of one not ASCII: the Python's
    if lv.contains(':') || lv.starts_with(['\\', '/']) || !lv.is_ascii() {
        return Err(Fail::Python);
    }
    let lv = lv.to_lowercase();
    let opfor = match opt(args, "--opfor") {
        Some(o) => o.to_owned(),
        None => find_opfor()?,
    };
    let work = match opt(args, "--work") {
        Some(w) if !w.is_empty() => w.to_owned(),
        _ => join(&join(&opfor, "work"), "stockmp"),
    };
    let variant = opt(args, "--variant").filter(|v| !v.is_empty()).map(str::to_owned);
    let root = join(
        &work,
        &format!("{lv}{}", variant.as_ref().map_or(String::new(), |v| format!("-{v}"))),
    );
    let paths = Paths {
        game: join(&opfor, "game"),
        opfor,
        work,
        lv,
        variant,
        root,
    };
    match cmd.as_str() {
        "summary" => level::summary(py, &paths),
        "install" => install(py, &paths, args),
        _ => {
            let r = pipeline(py, &paths, args, cmd);
            level::exits(py, r)
        }
    }
}

/// `cfgmod.load(path)`: the configuration and its problems raised as `ValueError`.
fn load_config(path: &str) -> PyResult<bc2_pyrt::json::Value> {
    let data = fs::read(path).map_err(|e| os_error(path, &e))?;
    let c = bc2_pyrt::json::loads_utf8(&data).ok_or(Fail::Python)?;
    let problems = generate::check(&c)?;
    if !problems.is_empty() {
        return Err(Fail::Err(PyErr::new(
            "ValueError",
            format!("{path}: {}", problems.join("; ")),
        )));
    }
    Ok(c)
}

/// `stockmp.main`'s generate / build / validate / all.
fn pipeline(py: &mut Py, paths: &Paths, args: &[String], cmd: &str) -> PyResult<i32> {
    if cmd == "generate" || (cmd == "all" && !isfile(&paths.config())) {
        level::generate(py, paths, args)?;
        if cmd == "generate" {
            return Ok(0);
        }
    }
    let config = opt(args, "--config").map_or_else(|| paths.config(), str::to_owned);
    if cmd == "build" || cmd == "all" {
        let c = load_config(&config)?;
        let built = build::build(py, paths, &c, &[])?;
        let shown =
            bc2_pyrt::json::Value::Dict(built.iter().map(|(v, r)| (v.clone(), build::report_value(r))).collect());
        py.print(&bc2_pyrt::json::dumps(&shown, None))?;
    }
    let c = load_config(&config)?;
    let passed = validate::validate(py, paths, &c)?;
    py.print(if passed {
        "all checks passed"
    } else {
        "SOME CHECKS FAILED"
    })?;
    Ok(i32::from(!passed))
}

/// `stockmp.install(paths, argv)`.
fn install(py: &mut Py, paths: &Paths, args: &[String]) -> PyResult<i32> {
    let game = opt(args, "--game").map_or_else(|| paths.game.clone(), str::to_owned);
    let out = join(&join(&paths.root, "out"), "game");
    let has = |f: &str| args.iter().any(|a| a == f);
    let mode = if has("--apply") {
        "--apply"
    } else if has("--revert") {
        "--revert"
    } else {
        "--plan"
    };
    let mut files = install::walk_rel(&out).map_err(fault)?;
    if mode == "--revert" {
        files.extend(install::installed_files(&game, &paths.lv).map_err(fault)?);
        files.sort();
        files.dedup();
        if files.is_empty() {
            py.print(&format!(
                "nothing installed by stock-MP in {game} (no {APPLIED} markers, nothing built)"
            ))?;
            return Ok(0);
        }
    } else {
        if files.is_empty() {
            return Ok(system_exit(py, &format!("nothing built in {out} (run build first)")));
        }
        let problems = install::check_report(&paths.root).map_err(fault)?;
        if !problems.is_empty() {
            let shown: Vec<&str> = problems.iter().take(6).map(String::as_str).collect();
            let msg = format!("{out} is not one complete build: {}", shown.join("; "));
            if mode == "--apply" {
                return Ok(system_exit(py, &format!("{msg} - nothing installed")));
            }
            py.print(&format!("warning: {msg}"))?;
        }
        files.sort();
    }
    if mode == "--apply" || mode == "--revert" {
        // every file checked before any is written
        let mut changed = Vec::new();
        for rel in &files {
            let dst = join(&game, rel);
            let marker = format!("{dst}{APPLIED}");
            if isfile(&dst) && isfile(&marker) && !install::marker_matches(&marker, &dst).map_err(fault)? {
                changed.push(rel.as_str());
            }
        }
        if !changed.is_empty() {
            let what = if mode == "--apply" { "installed" } else { "reverted" };
            return Ok(system_exit(
                py,
                &format!(
                    "changed after the install (by something else) - nothing {what}: {} (delete their {APPLIED} \
                     markers to take them as they are now)",
                    changed.join(", ")
                ),
            ));
        }
    }
    if mode == "--apply" {
        return apply_files(py, &game, &out, &files);
    }
    for rel in &files {
        let dst = join(&game, rel);
        let marker = format!("{dst}{APPLIED}");
        let backup = format!("{dst}{BACKUP}");
        if mode == "--plan" {
            let state = if isfile(&dst) { "replace" } else { "new" };
            py.print(&format!("{state:8} {rel}"))?;
            continue;
        }
        if isfile(&dst) && isfile(&marker) && !install::marker_matches(&marker, &dst).map_err(fault)? {
            return Ok(system_exit(
                py,
                &format!("{rel} changed after the install - not reverting it"),
            ));
        }
        if isfile(&backup) {
            fs::rename(&backup, &dst).map_err(|e| os_error(&backup, &e))?;
            py.print(&format!("restored  {rel}"))?;
        } else if isfile(&marker) {
            if isfile(&dst) {
                fs::remove_file(&dst).map_err(|e| os_error(&dst, &e))?;
            }
            py.print(&format!("removed   {rel}"))?;
        }
        if isfile(&marker) {
            fs::remove_file(&marker).map_err(|e| os_error(&marker, &e))?;
        }
    }
    if mode == "--plan" {
        let var = paths
            .variant
            .as_ref()
            .map_or(String::new(), |v| format!(" --variant {v}"));
        py.print(&format!(
            "(plan only: `install {}{var} --apply` copies these into {game}, keeping *{BACKUP} backups)",
            paths.lv
        ))?;
    }
    Ok(0)
}

/// `atomicio.copy_file(src, dst)`: the bytes copied beside the target, flushed to disk, then moved over it.
fn copy_file(src: &str, dst: &str) -> PyResult<()> {
    let tmp = format!("{dst}.tmp-{}", std::process::id());
    let done = (|| -> Result<(), (String, io::Error)> {
        let mut r = fs::File::open(src).map_err(|e| (src.to_owned(), e))?;
        let mut w = fs::File::create(&tmp).map_err(|e| (tmp.clone(), e))?;
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = r.read(&mut buf).map_err(|e| (src.to_owned(), e))?;
            if n == 0 {
                break;
            }
            w.write_all(buf.get(..n).unwrap_or_default())
                .map_err(|e| (tmp.clone(), e))?;
        }
        w.sync_all().map_err(|e| (tmp.clone(), e))?;
        drop(w);
        crate::pyos::files::replace(&tmp, dst).map_err(|e| (tmp.clone(), e))
    })();
    if let Err((p, e)) = done {
        let _ = fs::remove_file(&tmp);
        return Err(os_error(&p, &e));
    }
    Ok(())
}

/// `atomicio.write_text(path, text)`.
fn write_text(path: &str, text: &str) -> PyResult<()> {
    let tmp = format!("{path}.tmp-{}", std::process::id());
    let done = (|| -> io::Result<()> {
        let mut w = fs::File::create(&tmp)?;
        w.write_all(text.replace('\n', "\r\n").as_bytes())?;
        w.sync_all()?;
        drop(w);
        crate::pyos::files::replace(&tmp, path)
    })();
    if let Err(e) = done {
        let _ = fs::remove_file(&tmp);
        return Err(os_error(path, &e));
    }
    Ok(())
}

fn sha(path: &str) -> PyResult<String> {
    sha256::file(path).map_err(|e| os_error(path, &e))
}

/// A built file to install: (its path below the game folder, target, source, whether the original needs a backup).
type Todo<'a> = (&'a str, String, String, bool);

/// `stockmp.apply_files(game, out, rels)`: decide every file, stage every built file beside its target, then commit
/// file by file (the original's backup once, the target moved aside, the staged file in, its marker) - a failure
/// puts back every target already replaced before it is raised.
fn apply_files(py: &mut Py, game: &str, out: &str, rels: &[String]) -> PyResult<i32> {
    let mut todo: Vec<Todo> = Vec::new();
    for rel in rels {
        let (dst, src) = (join(game, rel), join(out, rel));
        let marker = format!("{dst}{APPLIED}");
        let mut installed = false;
        if isfile(&marker) && isfile(&dst) {
            installed = install::marker_matches(&marker, &dst).map_err(fault)?;
            if !installed {
                return Ok(system_exit(
                    py,
                    &format!(
                        "{rel} changed after the install - not overwriting it (revert it first, or delete {marker})"
                    ),
                ));
            }
        }
        if installed && sha(&dst)? == sha(&src)? {
            py.print(&format!("already   {rel}"))?;
            continue;
        }
        let backup = isfile(&dst) && !isfile(&format!("{dst}{BACKUP}")) && !installed;
        todo.push((rel, dst, src, backup));
    }
    // stage: a failure removes the staged copies, the game copy unchanged
    let mut staged: Vec<String> = Vec::new();
    if let Err(e) = stage(&todo, &mut staged) {
        for s in &staged {
            let _ = fs::remove_file(s);
        }
        return Err(e);
    }
    let mut done: Vec<(&str, bool, Option<String>)> = Vec::new();
    if let Err(e) = commit(&todo, &mut done) {
        if e == Fail::Python {
            return Err(e);
        }
        rollback(py, &todo, &done)?;
        return Err(e);
    }
    for (dst, had, _) in &done {
        let prev = format!("{dst}{PREV}");
        if *had && isfile(&prev) {
            fs::remove_file(&prev).map_err(|e| os_error(&prev, &e))?;
        }
    }
    for (rel, ..) in &todo {
        py.print(&format!("installed {rel}"))?;
    }
    Ok(0)
}

/// Every built file copied whole next to its target (`*.opfor-stockmp-new`).
fn stage(todo: &[Todo], staged: &mut Vec<String>) -> PyResult<()> {
    for (rel, dst, src, _) in todo {
        if let Some(dir) = std::path::Path::new(dst).parent() {
            fs::create_dir_all(dir).map_err(|e| os_error(&dir.to_string_lossy(), &e))?;
        }
        let s = format!("{dst}{STAGED}");
        copy_file(src, &s)?;
        staged.push(s.clone());
        if sha(&s)? != sha(src)? {
            return Err(Fail::Err(PyErr::new(
                "OSError",
                format!("{rel}: the staged copy differs from the build"),
            )));
        }
    }
    Ok(())
}

/// Per file: the original's backup (once), the target moved aside, the staged file renamed in, its marker.
fn commit<'a>(todo: &'a [Todo], done: &mut Vec<(&'a str, bool, Option<String>)>) -> PyResult<()> {
    for (_, dst, _, backup) in todo {
        if *backup {
            copy_file(dst, &format!("{dst}{BACKUP}"))?;
        }
        let had = isfile(dst);
        let marker = format!("{dst}{APPLIED}");
        let old_marker = if isfile(&marker) {
            Some(install::read_marker(&marker).map_err(fault)?)
        } else {
            None
        };
        if had {
            let prev = format!("{dst}{PREV}");
            fs::rename(dst, &prev).map_err(|e| os_error(dst, &e))?;
        }
        done.push((dst.as_str(), had, old_marker));
        let s = format!("{dst}{STAGED}");
        fs::rename(&s, dst).map_err(|e| os_error(&s, &e))?;
        write_text(&marker, &sha(dst)?)?;
    }
    Ok(())
}

/// A failed commit: every target already replaced put back (and its marker), the staged copies removed.
fn rollback(py: &mut Py, todo: &[Todo], done: &[(&str, bool, Option<String>)]) -> PyResult<()> {
    for (dst, had, old_marker) in done.iter().rev() {
        let marker = format!("{dst}{APPLIED}");
        let back = (|| -> PyResult<()> {
            if *had {
                let prev = format!("{dst}{PREV}");
                fs::rename(&prev, dst).map_err(|e| os_error(&prev, &e))?;
            } else if isfile(dst) {
                fs::remove_file(dst).map_err(|e| os_error(dst, &e))?;
            }
            match old_marker {
                None if isfile(&marker) => fs::remove_file(&marker).map_err(|e| os_error(&marker, &e))?,
                None => {}
                Some(t) => write_text(&marker, t)?,
            }
            Ok(())
        })();
        if let Err(Fail::Err(err)) = back {
            py.print(&format!(
                "ROLLBACK FAILED for {dst}: {} (its earlier file is {dst}{PREV})",
                err.msg
            ))?;
        }
    }
    for (_, dst, _, _) in todo {
        let s = format!("{dst}{STAGED}");
        if isfile(&s) {
            fs::remove_file(&s).map_err(|e| os_error(&s, &e))?;
        }
    }
    py.print("nothing installed: every file put back as it was")
}
