//! The install transaction (Project OPFOR's `ww2_level_install.install_all` / `revert_all`, `wake_install.revert`,
//! `set_ini`, `register`, `install_patch`, `mark_stings` / `mark_tips`, `verify`): the game copy's side of the WW2
//! install, after OPFOR's build staged every bundle.
//!
//! The order is OPFOR's: the gates and the backup guard before the first write; each level's bundles copied in with
//! its record, then `ww2_common`, the DLL's markers, the shared bundles (one ledger commit each), the ini files, and the
//! registration (package.mft, bundleManifest) last - a level registered with bundles that are not there is the
//! start-up checksum crash; then every level verified. Every shared file goes through the install ledger as owner
//! `wake` with the records OPFOR's Python wrote ([`Ledger::install_keep`]). The printed lines and report files are
//! the Python's.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;
use std::time::UNIX_EPOCH;

use bc2_formats::{dbx, fbrb};
use bc2_gamefs::ledger::{Ledger, LedgerError};
use bc2_gamefs::sha256;
use regex::Regex;

use crate::marks::Guard;
use crate::plan::{Level, Plan};
use crate::pytext::{basename, latin1, to_latin1};
use crate::staged;
use crate::text::{
    bm_line, bm_with_level, ini_with_key, ini_with_level, manifest_line_broken, maps_with_levels, mft_with_level,
    py_list_repr, py_repr,
};

/// What a run ends with: the exit code (0, or 1 when a gate, the guard or the verification failed).
pub type Code = u8;

/// Python's `SystemExit(message)`: the run stopped (exit 1, the message on stderr). The writes before it stay (as in
/// the Python: each commit is whole, the level folders are outside the ledger).
pub type Stop = String;

/// `stages.LEGACY`: the markers of the protocol before the install ledger.
static LEGACY: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used, reason = "a constant pattern, checked by the tests")]
    Regex::new(r"(?i)\.opfor-(orig|new|last|[a-z0-9_.-]+-(orig|applied|new|prev|prevlast))$").expect("valid")
});
static FIELD: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used, reason = "a constant pattern")]
    Regex::new(r"<item>([^<]*)</item>").expect("valid")
});

const MIGRATE_HINT: &str = r"python src\tools\opforcore\migrate.py --apply";
/// Windows FILETIME of the Unix epoch (100 ns ticks since 1601).
const FILETIME_EPOCH: u128 = 116_444_736_000_000_000;

/// One run over a plan.
pub struct Run<'a> {
    plan: &'a Plan,
    guard: Guard,
    game: PathBuf,
    log: &'a mut dyn FnMut(&str),
    resumed: bool,
    /// `BC2_INSTALL_CRASH_AT=<n>` (tests): the process ends (exit 9) before the n-th write step of the run, as a
    /// kill would - the level copies, records and markers, and every ledger step (its journal, blobs, files, records).
    crash_at: Option<u32>,
    steps: Rc<Cell<u32>>,
}

impl std::fmt::Debug for Run<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Run").field("game", &self.game).finish_non_exhaustive()
    }
}

fn read_text(p: &str) -> Result<String, Stop> {
    let d = fs::read(p).map_err(|e| format!("{p}: {e}"))?;
    String::from_utf8(d).map_err(|e| format!("{p}: not UTF-8 ({e})"))
}

fn read_latin1(p: &str) -> Result<String, Stop> {
    fs::read(p).map(|d| latin1(&d)).map_err(|e| format!("{p}: {e}"))
}

fn sha(p: &str) -> Result<String, Stop> {
    sha256::file(p).map_err(|e| format!("{p}: {e}"))
}

/// `cc.write_atomic`: [`bc2_gamefs::atomic::write`] (a temporary file beside it, flushed, renamed over it, retried
/// while another process holds it).
fn write_atomic(dst: &Path, data: &[u8]) -> Result<(), Stop> {
    bc2_gamefs::atomic::write(dst, &[data]).map_err(|e| format!("{}: {}", dst.display(), e.io()))
}

/// `cc.copy_atomic`: the copy made beside `dst` ([`bc2_gamefs::atomic::temp_name`]), flushed, renamed over it.
fn copy_atomic(src: &Path, dst: &Path) -> Result<(), Stop> {
    let tmp = bc2_gamefs::atomic::temp_name(dst);
    let r = (|| -> std::io::Result<()> {
        fs::copy(src, &tmp)?;
        fs::OpenOptions::new().write(true).open(&tmp)?.sync_all()?;
        bc2_gamefs::atomic::replace(&tmp, dst)
    })();
    if r.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    r.map_err(|e| format!("{} -> {}: {e}", src.display(), dst.display()))
}

/// The JSON of a `{kind: sha}` record as Python's `json.dumps(indent=1)` writes it.
fn record_json(items: &[(String, String)]) -> String {
    if items.is_empty() {
        return "{}".to_owned();
    }
    let rows: Vec<String> = items
        .iter()
        .map(|(k, v)| {
            format!(
                " {}: {}",
                serde_json::Value::from(k.as_str()),
                serde_json::Value::from(v.as_str())
            )
        })
        .collect();
    format!("{{\n{}\n}}", rows.join(",\n"))
}

/// `cc.bundle_marker_text`: `<size> <last write time as a FILETIME>` (what the DLL compares).
fn marker_text(bundle: &Path) -> Result<String, Stop> {
    let m = fs::metadata(bundle).map_err(|e| format!("{}: {e}", bundle.display()))?;
    let ns = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    Ok(format!("{} {}", m.len(), ns / 100 + FILETIME_EPOCH))
}

/// The lower-case names of a bundle's live entries; Ok(None) when it is not a bundle; Err when it cannot be read.
fn bundle_names(p: &Path) -> Result<Option<Vec<String>>, String> {
    match fbrb::entries_path(p) {
        Ok(h) => Ok(Some(
            h.entries
                .iter()
                .filter(|e| !e.is_deleted())
                .map(|e| e.name.to_ascii_lowercase())
                .collect(),
        )),
        Err(fbrb::FbrbError::Io(e)) => Err(e.to_string()),
        Err(_) => Ok(None),
    }
}

fn join(folder: &str, kind: &str) -> String {
    format!("{folder}\\{kind}-00.fbrb")
}

impl<'a> Run<'a> {
    /// A run over the plan, its lines given to `log`.
    ///
    /// # Errors
    /// The plan's guard patterns do not compile.
    pub fn new(plan: &'a Plan, log: &'a mut dyn FnMut(&str)) -> Result<Self, Stop> {
        let game = PathBuf::from(&plan.game);
        let guard = Guard::new(&game, &plan.owner, &plan.ini.file, &plan.marks)?;
        Ok(Self {
            plan,
            guard,
            game,
            log,
            resumed: false,
            crash_at: std::env::var("BC2_INSTALL_CRASH_AT")
                .ok()
                .and_then(|v| v.trim().parse().ok()),
            steps: Rc::new(Cell::new(0)),
        })
    }

    fn say(&mut self, line: &str) {
        (self.log)(line);
    }

    /// The guard's marks of a file (`ww2_marks`).
    ///
    /// # Errors
    /// A bundle that cannot be read.
    pub fn marks(&self, path: &str) -> Result<Vec<String>, Stop> {
        self.guard.marks(Path::new(path))
    }

    fn ledger(&self) -> Ledger {
        let mut led = Ledger::new(&self.game);
        if let Some(at) = self.crash_at {
            let steps = Rc::clone(&self.steps);
            led.set_hook(Some(Box::new(move |label: &str| {
                crash_step(&steps, at, label);
                Ok(())
            })));
        }
        led
    }

    /// A write step outside the ledger (the crash hook's count).
    fn step(&self, label: &str) {
        if let Some(at) = self.crash_at {
            crash_step(&self.steps, at, label);
        }
    }

    fn rel(&self, path: &str) -> Result<String, Stop> {
        let r = Path::new(path)
            .strip_prefix(&self.game)
            .map_err(|_| format!("{path}: not inside the game copy {}", self.game.display()))?;
        bc2_gamefs::ledger::norm_rel(&r.to_string_lossy()).map_err(|e| e.to_string())
    }

    /// `Stages._ready`: an interrupted commit rolled back first (once a run), then the files' old markers refused.
    fn ready(&mut self, paths: &[&str]) -> Result<Vec<String>, Stop> {
        if !self.resumed {
            let mut led = self.ledger();
            if led.lock_present() || led.interrupted() {
                if let Some(m) = led.live_lock() {
                    return Err(m);
                }
                let done = led
                    .recover(false)
                    .map_err(|e| format!("an interrupted install could not be rolled back: {e}"))?;
                if done.is_empty() {
                    self.say("  resumed: a lock left by an interrupted install was removed (nothing had been written)");
                }
                for d in done {
                    self.say(&format!("  resumed: an interrupted install was rolled back - {d}"));
                }
            }
            self.resumed = true;
        }
        let mut out = Vec::new();
        for p in paths {
            let rel = self.rel(p)?;
            let full = self.ledger().path(&rel);
            let old = legacy_markers(&full);
            if !old.is_empty() {
                return Err(format!(
                    "{} has install markers from before the install ledger ({}): migrate them first - {MIGRATE_HINT} \
                     (nothing was written)",
                    full.display(),
                    old.join(", ")
                ));
            }
            out.push(rel);
        }
        Ok(out)
    }

    /// `cc.install(owner, {path: data})`: one commit through the ledger, the owner's other files kept.
    fn install_files(&mut self, owner: &str, files: &[(&str, Vec<u8>)]) -> Result<(), Stop> {
        let paths: Vec<&str> = files.iter().map(|(p, _)| *p).collect();
        let rels = self.ready(&paths)?;
        let items: Vec<(String, Vec<u8>)> = rels.into_iter().zip(files.iter().map(|(_, d)| d.clone())).collect();
        match self.ledger().install_keep(owner, &items, true, "", false) {
            Ok(_) => Ok(()),
            Err(LedgerError::Refused(c)) => Err(format!(
                "install of {owner} refused - nothing was written:\n{}",
                c.iter().map(|x| format!("  {x}")).collect::<Vec<_>>().join("\n")
            )),
            Err(e) => Err(format!("install of {owner} refused: {e}")),
        }
    }

    /// `Stages.check_install`: why installing the owner's write of path would be refused.
    fn check_install(&mut self, path: &str) -> Result<Vec<String>, Stop> {
        let owner = self.plan.owner.clone();
        let rel = self.ready(&[path])?.remove(0);
        let led = self.ledger();
        let mixed = led.slot_holders(&rel).map_err(|e| e.to_string())?;
        if let Some((o, what)) = mixed.into_iter().find(|(o, _)| *o != owner) {
            return Ok(vec![format!("{rel}: {o} holds {what} in it (revert {o} first)")]);
        }
        let lay = led.file_layers(&rel).map_err(|e| e.to_string())?;
        if let Some(i) = lay.iter().position(|(o, _)| *o == owner)
            && i + 1 != lay.len()
            && let Some((above, _)) = lay.get(i + 1)
        {
            return Ok(vec![format!(
                "{rel}: {above} was installed after {owner} - revert {above} first (an install of {owner} now \
                 would go under a layer built on it)"
            )]);
        }
        if let Some((o, c)) = lay.last() {
            let p = led.path(&rel);
            let now = if p.is_file() {
                sha256::file(&p.to_string_lossy()).ok()
            } else {
                None
            };
            let wrote = c
                .get("value")
                .and_then(|v| v.get("sha"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            if now.as_deref() != Some(wrote) {
                return Ok(vec![format!(
                    "{rel}: changed outside the install ledger since {o} installed it ({}, {o} wrote sha {}) - put \
                     back what {o} wrote, or revert {o}",
                    now.map_or_else(
                        || "missing".to_owned(),
                        |n| format!("sha {}", n.get(..12).unwrap_or(&n))
                    ),
                    wrote.get(..12).unwrap_or(wrote)
                )]);
            }
        }
        Ok(Vec::new())
    }

    /// `refresh_ini`: opfor.ini edited since the install - the ledger's copy from before it gets those edits.
    fn refresh_ini(&mut self, path: &str) -> Result<(), Stop> {
        let Some(new) = self.guard.ini_refresh(path)? else {
            return Ok(());
        };
        let rel = self.ready(&[path])?.remove(0);
        let owner = self.plan.owner.clone();
        self.ledger()
            .restate(
                &owner,
                &rel,
                false,
                Some(new.as_bytes()),
                "opfor.ini edited since the WW2 install",
            )
            .map_err(|e| e.to_string())?;
        self.say(&format!(
            "  {}: the copy from before the WW2 install refreshed with the edits made since (its WW2 keys at their \
             pre-WW2 values)",
            basename(path)
        ));
        Ok(())
    }

    /// `check_backup`: the ledger's check and the guard, before an apply's first write.
    fn check_backup(&mut self, path: &str) -> Result<(), Stop> {
        self.refresh_ini(path)?;
        let why = self.check_install(path)?;
        if !why.is_empty() {
            return Err(format!("{}: not installed - {}", self.plan.owner, why.join("; ")));
        }
        self.guard.check(path)
    }

    // ------------------------------------------------------------------------------------------- text files

    /// `ini_text_new`: (path, old, new) of opfor.ini and (when there) maps.ini.
    fn ini_texts(&self) -> Result<Vec<(String, String, String)>, Stop> {
        let p = self.plan;
        let text = read_text(&p.ini.file)?;
        let new = ini_with_level(&text, &p.ini.dev_level).map_err(|e| e.0)?;
        let new = ini_with_key(&new, "gamemode", "pacific_levels", &p.ini.pacific_levels);
        let mut out = vec![(p.ini.file.clone(), text, new)];
        if Path::new(&p.maps.file).is_file() {
            let text = read_text(&p.maps.file)?;
            let new = maps_with_levels(&text, &p.maps.levels, &p.maps.strip, &p.maps.mark);
            out.push((p.maps.file.clone(), text, new));
        }
        Ok(out)
    }

    /// `all_text_writes`: the text files the set's registration would change (sorted).
    fn text_writes(&self) -> Result<Vec<String>, Stop> {
        let mut out: Vec<String> = self
            .ini_texts()?
            .into_iter()
            .filter(|(_, a, b)| a != b)
            .map(|(p, _, _)| p)
            .collect();
        let mft = read_latin1(&self.plan.mft)?;
        let bm = read_latin1(&self.plan.bm)?;
        for r in &self.plan.registration {
            if mft_with_level(&mft, &r.lv, &r.mft_blocks).0.is_some() && !out.contains(&self.plan.mft) {
                out.push(self.plan.mft.clone());
            }
            if bm_with_level(&bm, &r.lv, &r.kinds).0.is_some() && !out.contains(&self.plan.bm) {
                out.push(self.plan.bm.clone());
            }
        }
        out.sort();
        Ok(out)
    }

    /// `set_ini`: `[dev] level`, `[gamemode] pacific_levels`, the maps.ini sections.
    fn set_ini(&mut self, write: bool, report: &mut Vec<String>) -> Result<(), Stop> {
        let p = self.plan;
        let names: Vec<String> = p.maps.levels.iter().map(|l| l.name.clone()).collect();
        for (path, text, new) in self.ini_texts()? {
            let what = if path == p.ini.file {
                format!(
                    "[dev] level={}, [gamemode] pacific_levels={}",
                    p.ini.dev_level, p.ini.pacific_levels
                )
            } else {
                format!("sections {}", names.join(", "))
            };
            report.push(format!(
                "{}: {what}{}",
                basename(&path),
                if new == text { " (already)" } else { "" }
            ));
            if write && new != text {
                self.guard.check(&path)?;
                let owner = p.owner.clone();
                self.install_files(&owner, &[(path.as_str(), new.into_bytes())])?;
            }
        }
        if !Path::new(&p.maps.file).is_file() {
            report.push(
                "maps.ini: not in the game copy (build.bat copies it) - the browser names the WW2 rows by level"
                    .to_owned(),
            );
        }
        Ok(())
    }

    /// `register`: the level's package.mft blocks and bundleManifest lines (once each); written last.
    fn register(&mut self, level: &Level, write: bool, report: &mut Vec<String>) -> Result<(), Stop> {
        let p = self.plan;
        let (new_m, line) = mft_with_level(&read_latin1(&p.mft)?, &level.lv, &level.mft_blocks);
        report.push(line);
        let (new_b, add, broken) = bm_with_level(&read_latin1(&p.bm)?, &level.lv, &level.kinds);
        report.push(format!(
            "bundleManifest: {} lines to add; {} line(s) without their third field repaired ({})",
            add.len(),
            broken.len(),
            py_list_repr(&broken)
        ));
        if !write {
            return Ok(());
        }
        let mut writes: Vec<(&str, Vec<u8>)> = Vec::new();
        if let Some(m) = new_m {
            self.guard.check(&p.mft)?;
            writes.push((
                p.mft.as_str(),
                to_latin1(&m).ok_or("package.mft: a character outside latin-1")?,
            ));
        }
        if let Some(b) = new_b {
            self.guard.check(&p.bm)?;
            writes.push((
                p.bm.as_str(),
                to_latin1(&b).ok_or("bundleManifest: a character outside latin-1")?,
            ));
        }
        if !writes.is_empty() {
            let owner = p.owner.clone();
            self.install_files(&owner, &writes)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------------------------------- markers

    fn marker(m: &crate::plan::Marker) -> Result<String, Stop> {
        let bundle = Path::new(&m.bundle);
        let have = if bundle.is_file() {
            match bundle_names(bundle) {
                Ok(Some(n)) => n,
                Ok(None) => Vec::new(),
                Err(e) => {
                    return Err(format!(
                        "{}: cannot be read ({e}) - the {} marker is left as it is",
                        m.bundle,
                        basename(&m.marker).rsplit('-').next().unwrap_or("install")
                    ));
                }
            }
        } else {
            Vec::new()
        };
        let all = m.names.iter().all(|n| have.contains(n));
        if all && !(m.need_names && m.names.is_empty()) && bundle.is_file() {
            write_atomic(Path::new(&m.marker), marker_text(bundle)?.as_bytes())?;
            return Ok(m.yes.clone());
        }
        if Path::new(&m.marker).is_file() {
            fs::remove_file(&m.marker).map_err(|e| format!("{}: {e}", m.marker))?;
        }
        Ok(m.no.clone())
    }

    // ------------------------------------------------------------------------------------------- verify

    #[allow(clippy::too_many_lines, reason = "wake_install.verify's checks, in its order")]
    fn verify(&self, level: &Level, report: &mut Vec<String>) -> bool {
        let p = self.plan;
        let v = &p.verify;
        let mut ok = true;
        for kind in &level.kinds {
            let b = join(&level.pkg, kind);
            match fbrb::entries_path(&b) {
                Ok(h) => report.push(format!(
                    "verify {kind}-00.fbrb: opens, {} entries",
                    h.entries.iter().filter(|e| !e.is_deleted()).count()
                )),
                Err(e) => {
                    ok = false;
                    report.push(format!("verify {kind}-00.fbrb: FAILED to open ({e})"));
                }
            }
        }
        let text = read_latin1(&p.mft).unwrap_or_default();
        let lines: Vec<String> = read_latin1(&p.bm)
            .unwrap_or_default()
            .split('\n')
            .map(str::to_owned)
            .collect();
        for kind in &level.kinds {
            let a = text
                .matches(&format!("BundleSource game:levels/{}/{kind}-00.fbrb", level.lv))
                .count();
            let line = bm_line(&level.lv, kind);
            let c = lines.iter().filter(|l| **l == line).count();
            ok &= a == 1 && c == 1;
            report.push(format!(
                "verify {kind}: package.mft blocks {a}, bundleManifest lines {c}"
            ));
        }
        let entries = |b: &str| fbrb::entries_path(b).map_or_else(|_| Vec::new(), |h| h.entries);
        let st_all = entries(&v.startup);
        let st: Vec<String> = st_all
            .iter()
            .filter(|e| !e.is_deleted())
            .map(|e| e.name.to_lowercase())
            .collect();
        let desc = format!("levels/{}_description.dbx", level.name).to_lowercase();
        let d = st.iter().filter(|n| **n == desc).count();
        let sc = st.iter().filter(|n| *n == "menu/shared_config.dbx").count();
        let size2 = format!("{}/size2/", v.pictures).to_lowercase();
        let s2 = st.iter().filter(|n| n.starts_with(&size2)).count();
        ok &= d == 1 && sc == 1 && s2 > 0;
        report.push(format!(
            "verify startup: {d} description, {sc} shared_config, {s2} size2 picture assets"
        ));
        let xml_of = |name: &str| -> Option<String> {
            let data = bc2_gamefs::bundles::read_path(&v.startup, &[name])
                .ok()?
                .into_iter()
                .next()?;
            dbx::to_xml(&data).ok()
        };
        match st_all
            .iter()
            .find(|e| e.name.to_lowercase() == desc)
            .map(|e| e.name.clone())
        {
            None => {
                ok = false;
                report.push(format!(
                    "verify description: FAILED - Levels/{}_description.dbx is not in the startup bundle",
                    level.name
                ));
            }
            Some(dn) => {
                let x = xml_of(&dn).unwrap_or_default();
                let mut parts = Vec::new();
                let mut same = true;
                for (k, want) in &v.description_fields {
                    let got = Regex::new(&format!(r#"<field name="{}">([^<]*)</field>"#, regex::escape(k)))
                        .ok()
                        .and_then(|r| r.captures(&x).and_then(|c| c.get(1)).map(|m| m.as_str().to_owned()));
                    same &= got.as_deref() == Some(want.as_str());
                    parts.push(format!(
                        "{}: {}",
                        py_repr(k),
                        got.as_deref().map_or_else(|| "None".to_owned(), py_repr)
                    ));
                }
                ok &= same;
                report.push(format!("verify description: {{{}}}", parts.join(", ")));
            }
        }
        let ll = v.level_list.to_lowercase();
        let ln = st_all
            .iter()
            .find(|e| e.name.to_lowercase() == ll && !e.is_deleted())
            .map(|e| e.name.clone());
        let items: Vec<String> = ln.and_then(|n| xml_of(&n)).map_or_else(Vec::new, |x| {
            FIELD
                .captures_iter(&x)
                .filter_map(|c| c.get(1))
                .map(|m| m.as_str().to_owned())
                .collect()
        });
        let listed = items.contains(&format!("{}_description.dbx", level.lv));
        ok &= listed;
        report.push(format!(
            "verify level list: {} {}{}_description.dbx (the menus' level catalog)",
            v.level_list,
            if listed { "names " } else { "FAILED - does not name " },
            level.lv
        ));
        let od_all = entries(&v.ondemand);
        let pics = format!("{}/", v.pictures.to_lowercase());
        let od = od_all
            .iter()
            .filter(|e| e.name.to_lowercase().starts_with(&pics))
            .count();
        let lp_name = format!("awards/images/loadingscreens/{}.res", level.lv);
        let lp = od_all.iter().filter(|e| e.name.to_lowercase() == lp_name).count();
        ok &= lp == 1;
        report.push(format!(
            "verify loading picture: {lp} awards/images/loadingscreens/{}.res",
            level.lv
        ));
        let ov = entries(&v.overlay)
            .iter()
            .filter(|e| e.name.to_lowercase().starts_with("localization/bin/language_"))
            .count();
        ok &= od > 0;
        report.push(format!(
            "verify ondemand_awards: {od} WW2 pictures; overlay: {ov} language files"
        ));
        report.push(format!("verify: {}", if ok { "ok" } else { "FAILED" }));
        ok
    }

    // ------------------------------------------------------------------------------------------- install

    /// `install_all`: `write` false is the plan's tail (the guard, the registration's report, the reports, the staged
    /// record); true the apply. `staged`: installing a staged record's plan (`--apply-staged`).
    ///
    /// # Errors
    /// A stop in words (a refused file, an unreadable file); what was written before stays, each commit whole.
    #[allow(clippy::too_many_lines, reason = "install_all's steps, in its order")]
    pub fn install(&mut self, write: bool, staged: bool) -> Result<Code, Stop> {
        let p = self.plan;
        // an interrupted commit rolled back before any file is read (round OR1's kill test: the guard read a
        // half-written package.mft as WW2 content the ledger did not record, and refused)
        self.ready(&[])?;
        let mut level_reports: Vec<Vec<String>> = p.levels.iter().map(|l| l.report.clone()).collect();
        if staged {
            for r in &mut level_reports {
                r.push("(installed from the staged plan: --apply-staged)".to_owned());
            }
        }
        let mut ok = p.levels.iter().all(|l| l.ok);
        let mut shared_lines = p.shared_report.clone();
        let mut reg: Vec<String> = Vec::new();
        // the copies --revert (the opt-out) would put back: none may hold WW2 content
        let text_writes = self.text_writes()?;
        let mut guard_ok = true;
        let mut game_changed = false;
        for f in p.shared_files.iter().chain(&text_writes) {
            match self.guard.check(f) {
                Ok(()) => shared_lines.push(format!("backup guard {}: ok", basename(f))),
                Err(e) => {
                    shared_lines.push(format!("backup guard {}: REFUSED - {e}", basename(f)));
                    guard_ok = false;
                }
            }
        }
        if !ok || (write && !guard_ok) {
            ok = false;
            self.say(&format!(
                "WW2 levels: {} - a level's model groups do not fit their members (the level would crash at load), \
                 or a backup the opt-out relies on holds WW2 content (backup guard, below); nothing changed in the \
                 game copy",
                if write { "NOT installed" } else { "plan" }
            ));
        } else if write {
            let files: Vec<String> = p
                .shared
                .iter()
                .map(|s| s.file.clone())
                .chain(text_writes.iter().cloned())
                .collect();
            for f in &files {
                self.check_backup(f)?;
            }
            game_changed = true; // from here the game copy changes
            for level in &p.levels {
                fs::create_dir_all(&level.pkg).map_err(|e| format!("{}: {e}", level.pkg))?;
                for (kind, path) in &level.built {
                    let dst = PathBuf::from(join(&level.pkg, kind));
                    if let Some(d) = dst.parent() {
                        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
                    }
                    self.step(&format!("copy {}", dst.display()));
                    copy_atomic(Path::new(path), &dst)?;
                }
                let rec: Vec<(String, String)> = level
                    .kinds
                    .iter()
                    .map(|k| sha(&join(&level.pkg, k)).map(|s| (k.clone(), s)))
                    .collect::<Result<_, _>>()?;
                self.step(&format!("record {}", level.record));
                write_atomic(Path::new(&level.record), record_json(&rec).as_bytes())?;
            }
            fs::create_dir_all(&p.common.folder).map_err(|e| format!("{}: {e}", p.common.folder))?;
            for (kind, path) in &p.common.built {
                self.step(&format!("copy {}", join(&p.common.folder, kind)));
                copy_atomic(Path::new(path), Path::new(&join(&p.common.folder, kind)))?;
            }
            let rec: Vec<(String, String)> = p
                .common
                .built
                .iter()
                .map(|(k, _)| sha(&join(&p.common.folder, k)).map(|s| (k.clone(), s)))
                .collect::<Result<_, _>>()?;
            self.step(&format!("record {}", p.common.record));
            write_atomic(Path::new(&p.common.record), record_json(&rec).as_bytes())?;
            for m in &p.markers {
                self.step(&format!("marker {}", m.marker));
                let line = Self::marker(m)?;
                self.say(&format!("  {line}"));
            }
            for s in &p.shared {
                self.guard.check(&s.file)?;
                let data = fs::read(&s.built).map_err(|e| format!("{}: {e}", s.built))?;
                let owner = p.owner.clone();
                self.install_files(&owner, &[(s.file.as_str(), data)])?;
            }
        }
        if ok {
            self.set_ini(write, &mut reg)?;
            for level in &p.levels {
                reg.push(format!("{}:", level.name));
                self.register(level, write, &mut reg)?;
            }
            if write {
                for (level, rep) in p.levels.iter().zip(&mut level_reports) {
                    ok &= self.verify(level, rep);
                }
            }
        }
        for (level, rep) in p.levels.iter().zip(&level_reports) {
            let state = if !write {
                "plan (nothing changed)"
            } else if ok {
                "installed"
            } else {
                "FAILED"
            };
            self.say(&format!("{}: {state}", level.name));
            for line in rep {
                self.say(&format!("  {line}"));
            }
            fs::create_dir_all(&level.work).map_err(|e| format!("{}: {e}", level.work))?;
            let out = Path::new(&level.work).join("report.txt");
            fs::write(&out, text_mode(&(rep.join("\n") + "\n"))).map_err(|e| format!("{}: {e}", out.display()))?;
        }
        self.say("shared bundles and registration (one patch for every level):");
        for line in shared_lines.iter().chain(&reg) {
            self.say(&format!("  {line}"));
        }
        self.write_registration()?;
        if !write && !staged && ok && p.full {
            staged::save(p)?;
            self.say(&format!(
                "staged: {} (wake_install.py --apply-staged installs it while nothing it depends on changes)",
                p.staged.path
            ));
        } else if write && game_changed && Path::new(&p.staged.path).is_file() {
            fs::remove_file(&p.staged.path).map_err(|e| format!("{}: {e}", p.staged.path))?;
        }
        Ok(Code::from(!(ok && guard_ok)))
    }

    /// `write_registration_text`: every installed level's package.mft blocks and bundleManifest lines.
    fn write_registration(&self) -> Result<(), Stop> {
        let mut lines: Vec<String> = Vec::new();
        for r in &self.plan.registration {
            lines.push(format!(
                "# {}: game\\Package\\levels\\{}\\ ({})",
                r.title,
                r.lv,
                r.kinds.join(", ")
            ));
            lines.extend(r.mft_blocks.iter().map(|b| b.replace("\r\n", "\n") + "\n"));
            lines.extend(r.kinds.iter().map(|k| bm_line(&r.lv, k)));
        }
        let out = Path::new(&self.plan.registration_all);
        if let Some(d) = out.parent() {
            fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        fs::write(out, text_mode(&(lines.join("\n") + "\n"))).map_err(|e| format!("{}: {e}", out.display()))
    }

    // ------------------------------------------------------------------------------------------- revert

    fn opfor_rel(&self, folder: &str) -> String {
        crate::pytext::relpath(folder, &self.plan.opfor)
    }

    fn check_record(record: &str, folder: &str) -> Result<bool, Stop> {
        if !Path::new(record).is_file() {
            return Ok(false);
        }
        let d = fs::read(record).map_err(|e| format!("{record}: {e}"))?;
        let rec: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&d).map_err(|e| format!("{record}: {e}"))?;
        for (kind, want) in &rec {
            let b = join(folder, kind);
            if Path::new(&b).is_file() && Some(sha(&b)?.as_str()) != want.as_str() {
                return Err(format!("{b}: changed since --apply - nothing reverted"));
            }
        }
        Ok(true)
    }

    /// `revert_all` (the levels after the first: the other installed levels) and `wake_install.revert`: every check
    /// first (the level folders' records, each shared file's last writer), then the shared files back through the
    /// ledger, the bundleManifest repair kept as its own install, then the level folders removed.
    ///
    /// # Errors
    /// A stop in words: a level's bundles changed since the install, a refused revert (nothing written).
    pub fn revert(&mut self) -> Result<Code, Stop> {
        let p = self.plan;
        self.ready(&[])?; // an interrupted commit rolled back first
        let Some((first, rest)) = p.levels.split_first() else {
            return Err("the plan names no level".to_owned());
        };
        let mut others: Vec<String> = Vec::new();
        for level in rest {
            if Self::check_record(&level.record, &level.pkg)? {
                others.push(level.pkg.clone());
            }
        }
        for o in &p.others {
            if Path::new(&o.record).is_file() && !others.contains(&o.pkg) {
                others.push(o.pkg.clone());
            }
        }
        // wake_install.revert
        let mut remove: Vec<String> = Vec::new();
        for (rec, folder) in [(&first.record, &first.pkg), (&p.common.record, &p.common.folder)] {
            if Self::check_record(rec, folder)? {
                remove.push(folder.clone());
            }
        }
        let owner = p.owner.clone();
        let held = self.ledger().owner_files(&owner).map_err(|e| e.to_string())?;
        if !held.is_empty() {
            self.refresh_ini(&p.ini.file)?;
            let paths: Vec<String> = held
                .iter()
                .map(|r| self.ledger().path(r).to_string_lossy().into_owned())
                .collect();
            let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
            self.ready(&refs)?;
            match self.ledger().uninstall(&owner, true) {
                Ok(_) => {}
                Err(LedgerError::Refused(c)) => return Err(format!("{owner}: not reverted - {}", c.join("; "))),
                Err(e) => return Err(format!("{owner}: not reverted - revert of {owner} refused: {e}")),
            }
            match self.ledger().uninstall(&owner, false) {
                Ok(_) => {}
                Err(LedgerError::Refused(c)) => {
                    return Err(format!(
                        "revert of {owner} refused - nothing was written:\n{}",
                        c.iter().map(|x| format!("  {x}")).collect::<Vec<_>>().join("\n")
                    ));
                }
                Err(e) => return Err(format!("revert of {owner} refused: {e}")),
            }
            for rel in &held {
                self.say(&format!("restored {rel}"));
            }
        }
        // the manifest repair outside the install stays: its own install (AU4-SM-6)
        let bm = read_latin1(&p.bm)?;
        let lines: Vec<&str> = bm.split('\n').collect();
        if lines.iter().any(|l| manifest_line_broken(l)) {
            let fixed: Vec<String> = lines
                .iter()
                .map(|l| {
                    if manifest_line_broken(l) {
                        format!("{l}\t1")
                    } else {
                        (*l).to_owned()
                    }
                })
                .collect();
            let data = to_latin1(&fixed.join("\n")).ok_or("bundleManifest: a character outside latin-1")?;
            let fix = p.fix_owner.clone();
            self.install_files(&fix, &[(p.bm.as_str(), data)])?;
            self.say("kept the bundleManifest repair (levels/ons_mp_005gr/terrain's third field)");
        }
        for folder in remove.iter().chain(&others) {
            self.step(&format!("remove {folder}"));
            fs::remove_dir_all(folder).map_err(|e| format!("{folder}: {e}"))?;
            let rel = self.opfor_rel(folder);
            self.say(&format!("removed {rel}"));
        }
        Ok(0)
    }
}

/// A text file as Python's `open(path, 'w')` writes it on Windows (text mode: every line end CRLF) - the plan's
/// `report.txt` and `registration_all.txt` are in the install oracle (round OR1: written with LF they differed).
fn text_mode(text: &str) -> String {
    if cfg!(windows) {
        text.replace('\n', "\r\n")
    } else {
        text.to_owned()
    }
}

/// Counts a write step; the process ends (exit 9) at the n-th, before the step, as a kill would.
fn crash_step(steps: &Cell<u32>, at: u32, label: &str) {
    steps.set(steps.get() + 1);
    if steps.get() == at {
        eprintln!("BC2_INSTALL_CRASH_AT={at}: stopped before {label}");
        std::process::exit(9);
    }
}

/// The old protocol's markers beside a game file (`stages.legacy_markers`: not the DLL's own `.opfor-stings` /
/// `.opfor-tips` markers, temporary files or stamps).
#[must_use]
pub fn legacy_markers(path: &Path) -> Vec<String> {
    let (Some(dir), Some(base)) = (path.parent(), path.file_name()) else {
        return Vec::new();
    };
    let base = base.to_string_lossy().to_lowercase();
    let pre = format!("{base}.opfor-");
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = rd
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            let low = n.to_lowercase();
            low.starts_with(&pre) && n.get(base.len()..).is_some_and(|t| LEGACY.is_match(t))
        })
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_as_python_writes_them() {
        assert_eq!(record_json(&[]), "{}");
        assert_eq!(
            record_json(&[
                ("level".to_owned(), "ab".to_owned()),
                ("vo/en".to_owned(), "cd".to_owned())
            ]),
            "{\n \"level\": \"ab\",\n \"vo/en\": \"cd\"\n}"
        );
    }

    #[test]
    fn legacy_pattern() {
        for m in [
            ".opfor-orig",
            ".opfor-wake-orig",
            ".opfor-last",
            ".opfor-stockmp-applied",
            ".opfor-x-prevlast",
        ] {
            assert!(LEGACY.is_match(m), "{m}");
        }
        for m in [".opfor-stings", ".opfor-tips", ".opfor-pacific-music", ".opfor-tmp-12"] {
            assert!(!LEGACY.is_match(m), "{m}");
        }
    }
}
