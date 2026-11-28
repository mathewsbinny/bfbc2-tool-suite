//! `bfbc2 doctor` and `bfbc2 settings` (round OR15, IN1): the users' own check of their setup, in plain words with
//! the fix for each problem, and the game-folder setting the installs use.
//!
//! ```text
//! bfbc2 doctor [--game <folder>] [--opfor <folder>]   the setup, versions, fingerprints, install state, fixes
//!                                                     (exit 0: nothing to fix; 1: something to fix)
//! bfbc2 settings                                      the settings and where they are kept
//! bfbc2 settings game-folder <folder>                 the game copy installs write into (checked first)
//! bfbc2 settings game-folder --clear                  back to Project OPFOR BFBC2's game folder
//! ```
//!
//! The report is anonymised (`bc2_gamefs::privacy`): no PC name, no user name, the profile folder as
//! `%USERPROFILE%`, so it can be pasted into a bug report as it is.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bc2_gamefs::game::{Beside, Game, find_opfor};
use bc2_gamefs::ledger::Ledger;
use bc2_gamefs::privacy::anonymise;
use bc2_gamefs::settings::{self, Settings, not_a_game};

fn say(line: &str) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{}", anonymise(line));
}

const DOCTOR_USAGE: &str = "usage: bfbc2 doctor [--game <folder>] [--opfor <folder>]\n\
                            The setup checked: versions, fingerprints, the game folder, the install state; a fix for \
                            each problem. Nothing is written.\n";
const SETTINGS_USAGE: &str = "usage: bfbc2 settings\n       bfbc2 settings game-folder <folder> | --clear\n\
                              The suite's settings (the game folder the installs write into).\n";

fn opt(args: &[String], name: &str) -> Result<Option<String>, ()> {
    match args.iter().position(|a| a == name) {
        None => Ok(None),
        Some(i) => args.get(i + 1).cloned().map(Some).ok_or(()),
    }
}

/// The first 16 hex digits of a file's sha256 and its size.
fn fingerprint(p: &Path) -> Option<(String, u64)> {
    let size = std::fs::metadata(p).ok()?.len();
    let sha = bc2_gamefs::sha256::file(&p.to_string_lossy()).ok()?;
    Some((sha.get(..16).unwrap_or(&sha).to_owned(), size))
}

/// Runs `bfbc2 doctor <args>`.
#[allow(clippy::too_many_lines, reason = "the report's sections in their order")]
pub fn doctor(args: &[String], version: &str, source_hash: &str) -> ExitCode {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        say(DOCTOR_USAGE.trim_end());
        return ExitCode::SUCCESS;
    }
    let (Ok(given), Ok(opfor_given)) = (opt(args, "--game"), opt(args, "--opfor")) else {
        eprint!("{DOCTOR_USAGE}");
        return ExitCode::from(2);
    };
    let mut fixes: Vec<String> = Vec::new();
    say("BFBC2 Tool Suite - doctor (nothing is written; paste this report into a bug report as it is)");
    say("");
    say("Suite");
    say(&format!("  version            {}", version.trim()));
    say(&format!(
        "  bfbc2.exe sources  {}",
        if source_hash.is_empty() {
            "unknown (built by cargo alone)"
        } else {
            source_hash.get(..12).unwrap_or(source_hash)
        }
    ));
    let exe = std::env::current_exe().ok();
    let bin = exe.as_deref().and_then(Path::parent).map(Path::to_path_buf);
    if let Some(bin) = &bin {
        // optional: a command-line user needs neither (their absence is said, not a fix)
        for (name, what) in [
            ("bfbc2-editor.exe", "the map editor"),
            ("mapexport.exe", "the map export"),
        ] {
            let here = bin.join(name).is_file();
            say(&format!(
                "  {name:18} {}",
                if here {
                    "there".to_owned()
                } else {
                    format!("not there ({what}: build it with --editor, or the release)")
                }
            ));
        }
    }
    say("");
    say("Settings");
    let file = settings::path();
    say(&format!(
        "  file               {}",
        file.as_ref()
            .map_or_else(|| "none (APPDATA is not set)".to_owned(), |p| p.display().to_string())
    ));
    let set = match Settings::load() {
        Ok(s) => s,
        Err(e) => {
            fixes.push(e.clone());
            say(&format!("  damaged            {e}"));
            Settings::default()
        }
    };
    say(&format!(
        "  game folder        {}",
        set.game_folder
            .as_ref()
            .map_or_else(|| "not set".to_owned(), |p| p.display().to_string())
    ));
    let suite_dir = bin
        .as_deref()
        .and_then(Path::parent)
        .map_or_else(PathBuf::new, Path::to_path_buf);
    let starts = [suite_dir.as_path()];
    let opfor = find_opfor(opfor_given.as_deref(), true, Beside::Above(&starts)).ok();
    say(&format!(
        "  Project OPFOR      {}",
        opfor.as_ref().map_or_else(
            || "not found (not needed to install levels)".to_owned(),
            |p| p.display().to_string()
        )
    ));
    say("");
    say("Game");
    let game = match settings::game_folder(given.as_deref(), opfor.as_deref()) {
        Ok((g, src)) => {
            say(&format!("  folder             {} ({})", g.display(), src.words()));
            Some(g)
        }
        Err(e) => {
            say(&format!("  folder             none: {e}"));
            fixes.push(e);
            None
        }
    };
    if let Some(g) = &game {
        if let Some(why) = not_a_game(g) {
            say(&format!("  copy               {why}"));
            fixes.push(format!("{why}; then: bfbc2 settings game-folder <the game's folder>"));
        } else {
            let exe = g.join(Game::Bfbc2.executable());
            match fingerprint(&exe) {
                Some((sha, size)) => say(&format!(
                    "  {:18} sha256 {sha}..., {size} bytes",
                    Game::Bfbc2.executable()
                )),
                None => say(&format!("  {:18} cannot be read", Game::Bfbc2.executable())),
            }
            for sub in ["Package", "Dist", "Output"] {
                if !g.join(sub).is_dir() {
                    say(&format!("  {sub:18} missing"));
                    fixes.push(format!(
                        "{} has no {sub} folder: the game copy is incomplete - repair or reinstall the game",
                        g.display()
                    ));
                }
            }
            let running = bc2_gamefs::liveness::game_running();
            say(&format!("  running            {}", if running { "yes" } else { "no" }));
            if running {
                fixes.push(
                    "the game is running: close it before installing or reverting (the files it holds cannot \
                            be written)"
                        .to_owned(),
                );
            }
            install_state(g, &mut fixes);
        }
    }
    say("");
    if fixes.is_empty() {
        say("Nothing to fix.");
        return ExitCode::SUCCESS;
    }
    say("To fix");
    for f in &fixes {
        say(&format!("  - {f}"));
    }
    ExitCode::from(1)
}

fn install_state(game: &Path, fixes: &mut Vec<String>) {
    say("");
    say("Install state (the install ledger)");
    let led = Ledger::new(game);
    match led.check() {
        Ok(r) => {
            for (o, n) in &r.owners {
                say(&format!("  {o:28} {n} file(s)"));
            }
            for l in r.lines() {
                say(&format!("  {l}"));
            }
            if !r.damaged.is_empty() {
                for (f, why) in &r.damaged {
                    fixes.push(format!(
                        "the ledger record {f} is damaged ({why}): restore it from a backup, or drop it with bfbc2 \
                         gameledger forget <game> {} (no game file is touched)",
                        f.strip_suffix(".json").unwrap_or(f).replace('~', ":")
                    ));
                }
            }
            if r.interrupted.is_some() {
                fixes.push(format!(
                    "an install or revert was interrupted: bfbc2 gameledger recover \"{}\"",
                    game.display()
                ));
            }
            for b in &r.blobs {
                fixes.push(format!(
                    "{b}: reinstall the game file from the game's own installer before reverting that owner"
                ));
            }
        }
        Err(e) => {
            say(&format!("  cannot be read: {e}"));
            fixes.push(e.to_string());
        }
    }
}

/// Runs `bfbc2 settings <args>`.
pub fn settings_main(args: &[String]) -> ExitCode {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let show = |s: &Settings| {
        say(&format!(
            "settings file: {}",
            settings::path().map_or_else(|| "none (APPDATA is not set)".to_owned(), |p| p.display().to_string())
        ));
        say(&format!(
            "game folder:   {}",
            s.game_folder.as_ref().map_or_else(
                || "not set (installs use Project OPFOR BFBC2's game folder)".to_owned(),
                |p| p.display().to_string()
            )
        ));
    };
    let current = match Settings::load() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("settings: {e}");
            return ExitCode::from(1);
        }
    };
    match a.as_slice() {
        [] => {
            show(&current);
            ExitCode::SUCCESS
        }
        ["game-folder", "--clear"] => {
            let s = Settings { game_folder: None };
            match s.save() {
                Ok(()) => {
                    show(&s);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("settings: {e}");
                    ExitCode::from(1)
                }
            }
        }
        ["game-folder", dir] => {
            let p = std::path::absolute(dir).unwrap_or_else(|_| PathBuf::from(dir));
            if let Some(why) = not_a_game(&p) {
                eprintln!("settings: {why} (nothing changed)");
                return ExitCode::from(1);
            }
            let s = Settings { game_folder: Some(p) };
            match s.save() {
                Ok(()) => {
                    show(&s);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("settings: {e}");
                    ExitCode::from(1)
                }
            }
        }
        _ => {
            eprint!("{SETTINGS_USAGE}");
            ExitCode::from(2)
        }
    }
}
