//! `navkit.Project` and the module functions around it: the OPFOR folder, the game copy's levels, the original path
//! data (`original_file`), the in-process FbrbTool / DbxEdit runs and the stamped unpack.

use bc2_formats::{dbxedit, fbrb};

use super::{navkit_error, splitlines, strip};
use crate::pyfmt::str_repr;
use crate::pyjson::{self, Value};
use crate::pylog;
use crate::pyos::{self, join, join2};
use crate::pyproc::{Fail, Py, PyErr, PyResult};

pub const BACKUP_SUFFIX: &str = ".opfor-orig";
pub const NEW_SUFFIX: &str = ".opfor-new";

/// Where a step's progress goes: `print` (the command line), or `prepare_level`'s progress lines.
pub trait Log {
    /// One line of the step's log.
    fn log(&mut self, py: &mut Py, text: &str) -> PyResult<()>;
}

/// `log=print`.
pub struct Print;

impl Log for Print {
    fn log(&mut self, py: &mut Py, text: &str) -> PyResult<()> {
        py.print(text)
    }
}

pub use crate::navshared::{original_file, suite};

/// `navkit.find_opfor(given)`: `paths.find_opfor(given, start=<suite>)`, its `GameCopyNotFound` as `NavKitError`.
pub fn find_opfor(given: Option<&str>) -> PyResult<String> {
    crate::navshared::find_opfor(given, "NavKitError")
}

/// The output of one in-process tool run: (exit code, stdout + stderr).
fn run_tool(args: &[String]) -> (i32, String) {
    let fbrb_tool = args.first().is_some_and(|a| is_named(a, "fbrbtool"));
    let (name, rest) = (
        if fbrb_tool { "fbrbtool" } else { "dbxedit" },
        args.get(1..).unwrap_or_default(),
    );
    let log = pylog::log_path(name);
    let o = if fbrb_tool {
        let o = fbrb::tool::run(rest, log.as_deref());
        (o.code, o.stdout, o.stderr, o.log)
    } else {
        let o = dbxedit::run(rest, log.as_deref());
        (o.code, o.stdout, o.stderr, o.log)
    };
    pylog::append(log.as_deref(), name, rest, &o.3);
    (o.0, o.1 + &o.2)
}

/// `fbrbtool.is_fbrbtool` / `dbxedit.is_dbxedit`: the program's base name, any case, with or without `.exe`.
fn is_named(program: &str, tool: &str) -> bool {
    let base = pyos::basename(program).unwrap_or_default().to_lowercase();
    base == tool || base == format!("{tool}.exe")
}

/// The game copy and NavKit's folders (`navkit.Project`).
pub struct Project {
    pub root: String,
    pub game: String,
    pub work: String,
}

impl Project {
    /// `Project(root)`.
    pub fn new(given: Option<&str>) -> PyResult<Self> {
        let root = find_opfor(given)?;
        let game = join2(&root, "game")?;
        let work = join2(&root, "work")?;
        if !pyos::isfile(&join2(&game, "BFBC2Game.exe")?) {
            return Err(navkit_error(format!(
                "no game copy in {game} (run OPFOR's setup_game.bat first)"
            )));
        }
        Ok(Self { root, game, work })
    }

    /// `level_dirs()`: {lower-case name: folder} of the game's levels, sorted by name.
    pub fn level_dirs(&self) -> PyResult<Vec<(String, String)>> {
        let base = join(&self.game, &["Dist", "win32", "levels"])?;
        let mut names = pyos::listdir(&base)?;
        names.sort();
        let mut out: Vec<(String, String)> = Vec::new();
        for d in names {
            let low = d.to_lowercase();
            if !pyos::isfile(&join(&base, &[&d, "level-00.fbrb"])?)
                || low.ends_with("_common")
                || low.starts_with("opfor_")
            {
                continue;
            }
            let p = join2(&base, &d)?;
            match out.iter_mut().find(|(k, _)| *k == low) {
                Some(slot) => slot.1 = p,
                None => out.push((low, p)),
            }
        }
        Ok(out)
    }

    pub fn pathdata(&self, lv: &str) -> PyResult<String> {
        join(
            &self.game,
            &["Output", "win32", "ai", "levels", lv, "infantry", "pathdata.hpd"],
        )
    }

    pub fn dice_pathdata(&self, lv: &str) -> PyResult<Option<String>> {
        original_file(&self.pathdata(lv)?)
    }

    /// `level(name)`.
    pub fn level(&self, name: &str) -> PyResult<String> {
        let n = name.replace('\\', "/");
        let lv = n.rsplit('/').next().unwrap_or("").to_lowercase();
        if !self.level_dirs()?.iter().any(|(k, _)| *k == lv) {
            return Err(navkit_error(format!(
                "unknown level {} (see: navkit.py levels)",
                str_repr(name)
            )));
        }
        Ok(lv)
    }

    pub fn level_dir(&self, lv: &str) -> PyResult<String> {
        self.level_dirs()?
            .into_iter()
            .find(|(k, _)| k == lv)
            .map(|(_, p)| p)
            .ok_or_else(|| Fail::Err(PyErr::new("KeyError", str_repr(lv))))
    }

    pub fn level_work(&self, lv: &str) -> PyResult<String> {
        join(&self.work, &["levels", lv])
    }

    /// `out_dir(lv)` (made when missing).
    pub fn out_dir(&self, lv: &str) -> PyResult<String> {
        let d = join(&self.work, &["navkit", lv])?;
        pyos::makedirs(&d)?;
        Ok(d)
    }

    pub fn settings_path(&self, lv: &str) -> PyResult<String> {
        join2(&self.out_dir(lv)?, "navkit.json")
    }

    /// `settings(lv)`.
    pub fn settings(&self, lv: &str) -> PyResult<Value> {
        let p = self.settings_path(lv)?;
        if !pyos::isfile(&p) {
            return Err(navkit_error(format!(
                "{lv}: not prepared yet (navkit.py prepare {})",
                lv.to_uppercase()
            )));
        }
        let data = pyos::read(&p)?;
        // bytes that are not UTF-8, a UTF-8 BOM and invalid JSON are all ValueError in Python: "damaged"
        let text = std::str::from_utf8(&data).unwrap_or("\u{0}");
        match pyjson::loads(&super::universal(
            text.strip_prefix('\u{feff}').map_or(text, |_| "\u{0}"),
        )) {
            Some(v) => Ok(v),
            None => Err(navkit_error(format!(
                "{lv}: {p} is damaged - prepare the level again (navkit.py prepare {})",
                lv.to_uppercase()
            ))),
        }
    }

    /// `run(args, log)`: an in-process FbrbTool / DbxEdit command line's output lines; `NavKitError` on failure.
    #[allow(clippy::unused_self)] // navkit.Project.run: a method there
    pub fn run(&self, args: &[&str]) -> PyResult<Vec<String>> {
        let owned: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        let (code, text) = run_tool(&owned);
        let out: Vec<String> = splitlines(strip(&text)).into_iter().map(str::to_owned).collect();
        if code != 0 {
            let head = std::iter::once(pyos::basename(args.first().copied().unwrap_or(""))?)
                .chain(args.get(1..2).unwrap_or_default().iter().map(|s| (*s).to_owned()))
                .collect::<Vec<_>>()
                .join(" ");
            let tail = &out[out.len().saturating_sub(3)..];
            return Err(navkit_error(format!("{head} failed: {}", tail.join(" | "))));
        }
        Ok(out)
    }

    /// `unpack(bundle, dest, log)`: the bundle unpacked and its DBX converted to XML once, stamped with its SHA-256.
    pub fn unpack(&self, py: &mut Py, bundle: &str, dest: &str, log: &mut dyn Log) -> PyResult<()> {
        let stamp = join2(dest, ".source.sha256")?;
        let source_hash = pyos::sha256_file(bundle)?;
        if pyos::isfile(&join2(dest, "fbrb.manifest")?) && pyos::isfile(&stamp) {
            let data = pyos::read(&stamp)?;
            if data.is_ascii() && strip(&String::from_utf8_lossy(&data)) == source_hash {
                return log.log(py, &format!("  {} already unpacked", pyos::relpath(dest, &self.root)?));
            }
        }
        log.log(py, &format!("  unpacking {} ...", pyos::relpath(bundle, &self.game)?))?;
        if pyos::isdir(dest) {
            pyos::rmtree_quiet(dest);
        }
        self.run(&["FbrbTool", "unpack", bundle, dest])?;
        log.log(py, "  converting binary DBX to XML ...")?;
        let out = self.run(&["DbxEdit", "xml", dest])?;
        let last = out
            .last()
            .ok_or_else(|| Fail::Err(PyErr::new("IndexError", "list index out of range")))?;
        log.log(py, &format!("  {last}"))?;
        pyos::write_text_plain(&stamp, &source_hash)
    }
}
