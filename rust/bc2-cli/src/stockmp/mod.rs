//! Round O3's `stockmp` group: `bfbc2 stockmp`, `bfbc2-stockmp-ai\stockmp.py`'s command line on `bc2-stockmp`.
//!
//! Answered here: the usage, finding Project OPFOR's folder (and its error), and `install` in its three modes -
//! `--plan` (out\game against its `build_report.json`, each file new or replacing one), `--apply` (one journalled
//! commit of `stockmp:<lv>` in the game copy's install ledger, round DF2: what each file replaced kept once by sha256,
//! a failure or a kill puts every file back) and `--revert` (that install taken out of the ledger); `summary` and `generate` (`level.rs`:
//! the level's unpack, its model, `mp_common`'s kits; round O5: the full layer placed on the path data,
//! `generate.rs`), `build` (`build.rs`: authored, packed, staged), `validate` (`validate.rs`: the offline checks,
//! DbxEdit in process) and `all`. Handed to the Python: any reading the ports refuse (a damaged report, odd names,
//! links, text not UTF-8, a configuration of a shape the authoring port leaves, a check the reference words itself).

use std::collections::BTreeSet;
use std::fs;

use bc2_formats::gameledger::{self, Ledger, Legacy};
use bc2_stockmp::install::{self, APPLIED, BACKUP, Fault, isfile, join};

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

/// The owner stock-MP installs a level's build as, in the game copy's install ledger.
fn owner(lv: &str) -> String {
    format!("stockmp:{}", lv.to_lowercase())
}

/// The level's own folders in the game copy (where stock-MP installs and its old markers can be).
const LEVEL_FOLDERS: [&[&str]; 3] = [
    &["Dist", "win32", "levels"],
    &["Package", "levels"],
    &["Output", "win32", "ai", "levels"],
];

/// The legacy stock-MP marker suffixes (`.opfor-stockmp-*`, from before the install ledger).
const LEGACY: [&str; 4] = [BACKUP, APPLIED, ".opfor-stockmp-new", ".opfor-stockmp-prev"];

/// The game files under the level's own folders with a stock-MP marker beside them (game-relative, `/`): read only
/// there, never the whole game folder - a junction or an odd name elsewhere in the game copy is no concern of the
/// revert (AU4-DS-9). Folders that cannot be read are skipped.
fn legacy_files(game: &str, lv: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    for parts in LEVEL_FOLDERS {
        let top = parts
            .iter()
            .fold(std::path::PathBuf::from(game), |p, x| p.join(x))
            .join(lv);
        let mut stack = vec![top];
        while let Some(d) = stack.pop() {
            let Ok(rd) = fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let name = e.file_name().to_string_lossy().into_owned();
                let low = name.to_lowercase();
                if let Some(suf) = LEGACY.iter().find(|s| low.ends_with(*s))
                    && let Some(base) = p.to_string_lossy().get(..p.to_string_lossy().len() - suf.len())
                    && let Ok(rel) = std::path::Path::new(base).strip_prefix(game)
                {
                    out.insert(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    out.into_iter().collect()
}

fn shown(rel: &str) -> String {
    rel.replace('/', "\\")
}

/// `stockmp.install(paths, argv)` (round DF2): `--plan` lists the built files; `--apply` installs them as one
/// journalled commit of `stockmp:<lv>` in the game copy's install ledger (each file's previous bytes kept once by
/// sha256; a failure, or a kill, puts every file back); `--revert` takes that install out (the files from before it
/// back, or stock-MP's layer taken out from under a later install). stock-MP's markers from before the ledger are
/// adopted first.
#[allow(
    clippy::too_many_lines,
    reason = "the three modes and their messages, in the reference's order"
)]
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
    let me = owner(&paths.lv);
    let mut led = Ledger::new(std::path::Path::new(&game));
    let built: Vec<String> = install::walk_rel(&out).map_err(fault)?;
    if mode != "--revert" {
        if built.is_empty() {
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
    }
    if mode == "--plan" {
        let mut files = built;
        files.sort();
        for rel in &files {
            let state = if isfile(&join(&game, rel)) { "replace" } else { "new" };
            py.print(&format!("{state:8} {rel}"))?;
        }
        let var = paths
            .variant
            .as_ref()
            .map_or(String::new(), |v| format!(" --variant {v}"));
        py.print(&format!(
            "(plan only: `install {}{var} --apply` copies these into {game}, keeping what they replace in the game \
             copy's install ledger)",
            paths.lv
        ))?;
        return Ok(0);
    }
    // stock-MP's markers from before the ledger: adopted (the built files' and any in the level's own folders)
    let mut legacy: BTreeSet<String> = legacy_files(&game, &paths.lv).into_iter().collect();
    legacy.extend(built.iter().map(|r| r.replace('\\', "/")));
    match led.adopt_legacy(&me, &legacy.into_iter().collect::<Vec<_>>(), Legacy::StockMp) {
        Ok(Some(p)) => py.print(&format!(
            "stock-MP's markers from before the install ledger moved into it ({} file(s))",
            p.writes.iter().filter(|w| w.starts_with("adopt ")).count()
        ))?,
        Ok(None) => {}
        Err(e) => {
            let what = if mode == "--apply" { "installed" } else { "reverted" };
            return Ok(system_exit(py, &format!("nothing {what}: {e}")));
        }
    }
    if mode == "--apply" {
        let mut files = Vec::new();
        for rel in &built {
            let src = join(&out, rel);
            files.push((rel.replace('\\', "/"), fs::read(&src).map_err(|e| os_error(&src, &e))?));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        return match led.install(&me, &files, false, "stock-MP AI layer", false) {
            Ok(plan) => {
                for n in &plan.notes {
                    match n.strip_suffix(": already installed") {
                        Some(rel) => py.print(&format!("already   {}", shown(rel)))?,
                        None => py.print(&format!("  {n}"))?,
                    }
                }
                for w in &plan.writes {
                    if let Some(rel) = w.strip_prefix("write ") {
                        py.print(&format!("installed {}", shown(rel)))?;
                    } else if let Some(rel) = w.strip_prefix("remove ") {
                        py.print(&format!("removed   {} (no longer in the build)", shown(rel)))?;
                    }
                }
                Ok(0)
            }
            Err(e) => Ok(system_exit(py, &format!("nothing installed: {e}"))),
        };
    }
    // --revert
    let mine = led.owner_files(&me).map_err(|e| ledger(&e))?;
    if mine.is_empty() {
        py.print(&format!(
            "nothing installed by stock-MP in {game} (no install of {me} in its install ledger)"
        ))?;
        return Ok(0);
    }
    let mut what: Vec<(String, String)> = Vec::new();
    for rel in &mine {
        let holders = led.holders(rel).map_err(|e| ledger(&e))?;
        let above = holders
            .iter()
            .position(|h| *h == me)
            .and_then(|i| holders.get(i + 1))
            .cloned();
        let before = led.before(&me, rel).map_err(|e| ledger(&e))?.flatten();
        let line = match (above, before) {
            (Some(o), _) => format!(
                "kept      {} (stock-MP's layer taken out from under {o}'s install: its revert now puts back what was \
                 there before stock-MP)",
                shown(rel)
            ),
            (None, Some(_)) => format!("restored  {}", shown(rel)),
            (None, None) => format!("removed   {}", shown(rel)),
        };
        what.push((rel.clone(), line));
    }
    match led.uninstall(&me, false) {
        Ok(plan) => {
            for n in plan.notes.iter().filter(|n| !n.contains("taken out from under")) {
                py.print(&format!("  {n}"))?;
            }
            for (_, line) in &what {
                py.print(line)?;
            }
            Ok(0)
        }
        Err(e) => Ok(system_exit(py, &format!("nothing reverted: {e}"))),
    }
}

fn ledger(e: &gameledger::Error) -> Fail {
    Fail::Err(PyErr::new("LedgerError", e.to_string()))
}
