//! `bfbc2 mods`: Project OPFOR's mod manager (round MM1, [`bc2_install::mods`]) - each mod enabled, disabled,
//! installed or uninstalled on the install ledger without a rebuild, DICE's files put back per file or for the whole
//! game, every game file verified, and the one-time migration of the markers from before the ledger.
//!
//! ```text
//! bfbc2 mods list      [--game <g>] [--json]           every mod and its state (enabled / disabled / partly)
//! bfbc2 mods status    [--game <g>] [<mod> ...]        each mod's installs and kept copies, what it needs
//! bfbc2 mods disable   [--game <g>] <mod> ... [--yes]  out, kept for enable (dependents with --yes)
//! bfbc2 mods enable    [--game <g>] <mod> ... [--only] [--yes]   back from the kept copy (with the mods disabled
//!                                                      with it, unless --only; the mods it needs with --yes)
//! bfbc2 mods install   [--game <g>] <mod> [--staged <staged.json>] [-- <console_port.py arguments>]
//! bfbc2 mods uninstall [--game <g>] <mod> ... --yes    out for good (the kept copy dropped)
//! bfbc2 mods restore-original [--game <g>] <file> ... | --all  [--yes]   DICE's files back (the installs kept)
//! bfbc2 mods verify    [--game <g>] [--json]           every game file against the ledger, DICE's originals, kept copies
//! bfbc2 mods baseline  [--game <g>]                    the sha256 of every other game file, for verify
//! bfbc2 mods migrate   [--game <g>] [--apply]          the markers of before the ledger: the plan, then --apply
//! bfbc2 mods resume    [--game <g>]                    an interrupted operation carried on
//! ```
//!
//! `<mod>`: `ww2`, `pacific-menus`, `vietnam`, `onslaught` (every level's) or `onslaught:<level>`, `mapedit`,
//! `navkit:<level>` / `stockmp:<level>` (or `navkit` / `stockmp`: every level's), `base`. The game copy is
//! `--game`, else Project OPFOR's `game\` (`OPFOR_ROOT`, or `project-opfor-bfbc2` beside the suite).
//!
//! Exit codes: 0 done; 1 refused or failed (the reason on stderr, nothing written unless it says so); 2 usage, or a
//! confirmation is needed (run again with `--yes`); 3 another operation holds the lock.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bc2_install::mods::{Manager, ModError, migrate};

const USAGE: &str = "usage: bfbc2 mods list|status|verify|baseline|resume [--game <game>] [--json]\n       \
                     bfbc2 mods disable|uninstall <mod> ... [--game <game>] [--yes]\n       \
                     bfbc2 mods enable <mod> ... [--game <game>] [--only] [--yes]\n       \
                     bfbc2 mods install <mod> [--game <game>] [--staged <staged.json>] [-- <console_port.py args>]\n       \
                     bfbc2 mods restore-original <file> ... | --all [--game <game>] [--yes]\n       \
                     bfbc2 mods migrate [--game <game>] [--apply]\n\
                     Project OPFOR's mod manager on the install ledger: ww2, pacific-menus, vietnam, onslaught[:<level>], \
                     mapedit, navkit:<level>, stockmp:<level>, base.\n";

fn out(line: &str) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{line}");
}

fn usage() -> ExitCode {
    eprint!("{USAGE}");
    ExitCode::from(2)
}

/// Project OPFOR's folder: `OPFOR_ROOT`, else `project-opfor-bfbc2` beside the suite.
fn opfor_root() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("OPFOR_ROOT")
        && !v.is_empty()
    {
        return Some(PathBuf::from(v));
    }
    let suite = bc2_pyrt::py::suite_root();
    let c = suite.parent()?.join("project-opfor-bfbc2");
    c.join("game").is_dir().then_some(c)
}

fn fail(e: &ModError) -> ExitCode {
    let _ = std::io::stdout().flush();
    match e {
        ModError::Confirm(lines) => {
            for l in lines {
                eprintln!("{l}");
            }
            ExitCode::from(2)
        }
        ModError::Locked(m) => {
            eprintln!("{m}");
            ExitCode::from(3)
        }
        ModError::Refused(lines) => {
            eprintln!("refused - nothing was written:");
            for l in lines {
                eprintln!("  {l}");
            }
            ExitCode::from(1)
        }
        e => {
            eprintln!("{e}");
            eprintln!("(bfbc2 mods resume carries an interrupted operation on)");
            ExitCode::from(1)
        }
    }
}

fn done(r: Result<Vec<String>, ModError>) -> ExitCode {
    match r {
        Ok(lines) => {
            for l in lines {
                out(&l);
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

/// Runs `bfbc2 mods <args>`.
#[allow(clippy::too_many_lines, reason = "one arm per command")]
pub fn main(args: &[String]) -> ExitCode {
    let mut a: Vec<String> = args.to_vec();
    let passthrough: Vec<String> = match a.iter().position(|x| x == "--") {
        Some(i) => {
            let rest = a.split_off(i);
            rest.into_iter().skip(1).collect()
        }
        None => Vec::new(),
    };
    let mut take = |name: &str| -> Option<Option<String>> {
        let i = a.iter().position(|x| x == name)?;
        let v = a.get(i + 1).cloned();
        a.drain(i..(i + 2).min(a.len()));
        Some(v)
    };
    let game = match take("--game") {
        Some(Some(g)) => Some(PathBuf::from(g)),
        Some(None) => return usage(),
        None => None,
    };
    let staged = match take("--staged") {
        Some(Some(s)) => Some(s),
        Some(None) => return usage(),
        None => None,
    };
    let flag = |a: &mut Vec<String>, name: &str| -> bool {
        let n = a.len();
        a.retain(|x| x != name);
        a.len() != n
    };
    let (yes, only, json, apply, all) = (
        flag(&mut a, "--yes"),
        flag(&mut a, "--only"),
        flag(&mut a, "--json"),
        flag(&mut a, "--apply"),
        flag(&mut a, "--all"),
    );
    let Some((cmd, rest)) = a.split_first() else {
        return usage();
    };
    if rest.iter().any(|x| x.starts_with("--")) {
        return usage();
    }
    let opfor = opfor_root();
    let Some(game) = game.or_else(|| opfor.as_ref().map(|o| o.join("game"))) else {
        eprintln!(
            "mods: no game copy: give --game <folder>, set OPFOR_ROOT, or put the suite beside project-opfor-bfbc2"
        );
        return ExitCode::from(2);
    };
    if !game.is_dir() {
        eprintln!("mods: {}: not a folder", game.display());
        return ExitCode::from(2);
    }
    let writes = matches!(
        cmd.as_str(),
        "enable" | "disable" | "install" | "uninstall" | "restore-original" | "resume"
    ) || (cmd == "migrate" && apply);
    if writes && bc2_gamefs::liveness::game_running() {
        eprintln!("the game is running - close it first (nothing was changed)");
        return ExitCode::from(1);
    }
    let m = Manager::new(&game);
    match cmd.as_str() {
        "list" => match m.list() {
            Ok(l) if json => {
                let v = serde_json::json!({ "game": game.display().to_string(), "interrupted": m.interrupted(),
                                            "mods": l });
                out(&serde_json::to_string_pretty(&v).unwrap_or_default());
                ExitCode::SUCCESS
            }
            Ok(l) => {
                if m.interrupted() {
                    out("an operation was interrupted: bfbc2 mods resume carries it on");
                }
                for c in l {
                    out(&format!(
                        "{:<24} {:<9} {}",
                        c.id,
                        format!("{:?}", c.state).to_lowercase(),
                        c.title
                    ));
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        "status" => match m.list() {
            Ok(l) => {
                let want: Vec<String> = match m.select(rest) {
                    Ok(w) => w,
                    Err(e) if !rest.is_empty() => return fail(&e),
                    Err(_) => Vec::new(),
                };
                for c in l.iter().filter(|c| rest.is_empty() || want.contains(&c.id)) {
                    out(&format!("{} - {} ({:?})", c.id, c.title, c.state));
                    for u in &c.installed {
                        out(&format!("  in the game copy: {u}"));
                    }
                    for u in &c.kept {
                        out(&format!("  kept aside:       {u}"));
                    }
                    if !c.requires.is_empty() {
                        out(&format!("  needs:            {}", c.requires.join(", ")));
                    }
                }
                let old = migrate::scan(&game).len();
                if old > 0 {
                    out(&format!(
                        "{old} files still have install markers from before the ledger: bfbc2 mods migrate"
                    ));
                }
                if m.interrupted() {
                    out("an operation was interrupted: bfbc2 mods resume carries it on");
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        "disable" if !rest.is_empty() => done(m.disable(rest, yes)),
        "enable" if !rest.is_empty() => done(m.enable(rest, only, yes)),
        "uninstall" if !rest.is_empty() => done(m.uninstall(rest, yes)),
        "restore-original" if all == rest.is_empty() => done(m.restore_original(rest, all, yes)),
        "resume" => match m.resume() {
            Ok(l) if l.is_empty() => {
                out("nothing to resume");
                ExitCode::SUCCESS
            }
            r => done(r),
        },
        "install" => match rest {
            [name] => install(&m, &game, opfor.as_deref(), name, staged.as_deref(), &passthrough, yes),
            _ => usage(),
        },
        "verify" => match m.verify() {
            Ok(r) => {
                if json {
                    let v = serde_json::json!({ "files": r.files, "problems": r.problems, "lines": r.lines });
                    out(&serde_json::to_string_pretty(&v).unwrap_or_default());
                } else {
                    for l in &r.lines {
                        out(l);
                    }
                    out(&format!("{} checked, {} problem(s)", r.files, r.problems));
                }
                ExitCode::from(u8::from(r.problems > 0))
            }
            Err(e) => fail(&e),
        },
        "baseline" => match m.baseline() {
            Ok(n) => {
                out(&format!(
                    "baseline: {n} game files no install holds ({})",
                    m.root.join("baseline.json").display()
                ));
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        "migrate" if apply => match migrate::apply(&m) {
            Ok((moved, refused, lines)) => {
                for l in lines {
                    out(&l);
                }
                out(&format!("{moved} file(s) migrated, {refused} refused"));
                ExitCode::from(u8::from(refused > 0))
            }
            Err(e) => fail(&e),
        },
        "migrate" => match migrate::plan(&game) {
            Ok(fps) => {
                for l in migrate::show(&fps) {
                    out(&l);
                }
                let n_ref = fps.iter().filter(|f| !f.refused.is_empty()).count();
                out(&format!(
                    "{} file(s) with old markers: {} can be migrated, {n_ref} refused (plan only: --apply migrates)",
                    fps.len(),
                    fps.len() - n_ref
                ));
                ExitCode::from(u8::from(n_ref > 0))
            }
            Err(e) => fail(&e),
        },
        _ => usage(),
    }
}

/// `mods install`: a kept mod enabled; otherwise the mod's own installer - the WW2 install's staged plan
/// (`bfbc2 install apply --staged`), an Onslaught console port (OPFOR's `console_port.py`) - or where to build it.
fn install(
    m: &Manager,
    game: &Path,
    opfor: Option<&Path>,
    name: &str,
    staged: Option<&str>,
    passthrough: &[String],
    yes: bool,
) -> ExitCode {
    let ids = m.select(&[name.to_owned()]).unwrap_or_default();
    let kept = m
        .list()
        .is_ok_and(|l| l.into_iter().any(|c| ids.contains(&c.id) && !c.kept.is_empty()));
    if kept {
        return done(m.enable(&[name.to_owned()], false, yes));
    }
    let head = name.split(':').next().unwrap_or(name);
    let opfor_of_game = game.parent().map(Path::to_path_buf);
    match head {
        "ww2" => {
            let file = staged.map(PathBuf::from).or_else(|| {
                opfor_of_game
                    .as_ref()
                    .map(|o| o.join("work").join("ww2").join("staged.json"))
            });
            let Some(file) = file.filter(|f| f.is_file()) else {
                eprintln!(
                    "install ww2: no staged WW2 install (work\\ww2\\staged.json): build it with Project OPFOR's \
                     src\\tools\\ww2\\rebuild_all.py (or wake_install.py --plan), or give --staged <staged.json>"
                );
                return ExitCode::from(1);
            };
            // the staged plan must be this game copy's
            let plan_game = std::fs::read(&file)
                .ok()
                .and_then(|d| serde_json::from_slice::<bc2_install::staged::Record>(&d).ok())
                .map(|r| PathBuf::from(r.plan.game));
            let same = plan_game.as_ref().is_some_and(|p| {
                p.to_string_lossy()
                    .trim_end_matches(['\\', '/'])
                    .eq_ignore_ascii_case(game.to_string_lossy().trim_end_matches(['\\', '/']))
            });
            if !same {
                eprintln!(
                    "install ww2: {} is a plan for {} - not this game copy ({})",
                    file.display(),
                    plan_game.map_or_else(|| "?".to_owned(), |p| p.display().to_string()),
                    game.display()
                );
                return ExitCode::from(1);
            }
            crate::install_cli::main(&["apply".to_owned(), "--staged".to_owned(), file.display().to_string()])
        }
        "onslaught" => {
            let Some(lv) = name.split_once(':').map(|x| x.1) else {
                eprintln!(
                    "install onslaught: name the level (onslaught:ons_mp_002) and give console_port.py's \
                           arguments after --"
                );
                return ExitCode::from(2);
            };
            let Some(o) = opfor else {
                eprintln!("install onslaught: Project OPFOR not found (OPFOR_ROOT)");
                return ExitCode::from(1);
            };
            if !o.join("game").as_os_str().eq_ignore_ascii_case(game.as_os_str()) {
                eprintln!(
                    "install onslaught: console_port.py installs into Project OPFOR's own game copy ({}), not {}",
                    o.join("game").display(),
                    game.display()
                );
                return ExitCode::from(1);
            }
            let script = o.join("src").join("tools").join("onslaught").join("console_port.py");
            let mut c = std::process::Command::new("python");
            c.arg("-I").arg(&script).arg(lv).args(passthrough);
            match c.status() {
                Ok(s) => ExitCode::from(u8::try_from(s.code().unwrap_or(1) & 0xFF).unwrap_or(1)),
                Err(e) => {
                    eprintln!("install onslaught: {}: {e}", script.display());
                    ExitCode::from(1)
                }
            }
        }
        "vietnam" => done(m.enable(&[name.to_owned()], false, yes)),
        _ => {
            let how = match head {
                "pacific-menus" => "Project OPFOR's build_menus.bat (menu_install.py)",
                "mapedit" => "the map editor's Install (bfbc2 install mapedit apply <stage> --game <game>)",
                "navkit" => "bfbc2 navkit install <level>",
                "stockmp" => "bfbc2 stockmp install <level> --apply",
                "base" => "Project OPFOR's import (build.bat)",
                _ => "the tool that made it",
            };
            eprintln!("install {name}: nothing of it is kept aside here - it is installed by {how}");
            ExitCode::from(2)
        }
    }
}
