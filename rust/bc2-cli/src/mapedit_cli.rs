//! `bfbc2 install mapedit`: the map export's install (round OR6, [`bc2_install::mapedit`]; tool group `mods`) -
//! what Project OPFOR's `mapedit_install.py` did in Python, which now runs this.
//!
//! ```text
//! bfbc2 install mapedit plan   <stage> --game <folder> [--check-game <folder>] [--work <folder>]
//!                                    check the stage, build the shared patches, print every change; write nothing
//! bfbc2 install mapedit apply  <stage> --game <folder> [--check-game <folder>] [--work <folder>]
//!                                    install it (every check first, the registration last)
//! bfbc2 install mapedit revert <LEVEL>... | --all --game <folder> [--work <folder>]
//!                                    take exported levels out (every shared file back byte for byte)
//! bfbc2 install mapedit status [--game <folder>]
//!                                    the exported levels installed
//!   --game        the game copy (round OR15: default the game-folder setting, bfbc2 settings game-folder)
//!   --check-game  the game copy the stage is checked against (only read; default --game)
//!   --work        where the shared patches are built (default <game>\..\work\mapedit for Project OPFOR's
//!                 <opfor>\game, else %LOCALAPPDATA%\bfbc2-tool-suite\work\mapedit; removed when the run ends)
//! ```
//!
//! No Python and no Project OPFOR are needed (round OR15, IN1): a game folder and an exported stage are enough.
//!
//! Exit codes: 0 done; 1 refused or failed (the reason on stderr; nothing half written); 2 usage.
//! `BC2_INSTALL_CRASH_AT=<n>` (tests) ends the run before its n-th write step, as a kill would.

use std::io::Write as _;
use std::process::ExitCode;

use bc2_install::mapedit::{Game, Installer};

const USAGE: &str = "usage: bfbc2 install mapedit plan|apply <stage> --game <folder> [--check-game <folder>] \
                     [--work <folder>]\n       \
                     bfbc2 install mapedit revert <LEVEL>... | --all --game <folder> [--work <folder>]\n       \
                     bfbc2 install mapedit status --game <folder>\n\
                     The map export's install (Project OPFOR's mapedit_install.py) on the install ledger.\n";

fn out(line: &str) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{line}");
    let _ = o.flush();
}

fn usage() -> ExitCode {
    eprint!("{USAGE}");
    ExitCode::from(2)
}

/// Runs `bfbc2 install mapedit <args>`.
pub fn main(args: &[String]) -> ExitCode {
    let mut opts: [(&str, Option<String>); 3] = [("--game", None), ("--check-game", None), ("--work", None)];
    let mut all = false;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while let Some(a) = args.get(i) {
        if let Some(slot) = opts.iter_mut().find(|(k, _)| *k == a.as_str()) {
            let Some(v) = args.get(i + 1) else {
                return usage();
            };
            slot.1 = Some(v.clone());
            i += 2;
            continue;
        }
        if a == "--all" {
            all = true;
        } else {
            rest.push(a.clone());
        }
        i += 1;
    }
    let [(_, game), (_, check), (_, work)] = opts;
    // round OR15 (IN1): without --game, the game-folder setting (bfbc2 settings game-folder)
    let game = match bc2_gamefs::settings::game_folder(game.as_deref(), None) {
        Ok((g, _)) => g.to_string_lossy().into_owned(),
        Err(e) => {
            eprintln!("bfbc2 install mapedit: {e}");
            return usage();
        }
    };
    let work = work.unwrap_or_else(|| {
        let up = std::path::Path::new(&game)
            .parent()
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned());
        let opfor_layout = std::path::Path::new(&game)
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("game"));
        default_work(&up, opfor_layout)
    });
    let Some((cmd, names)) = rest.split_first() else {
        return usage();
    };
    let mut log = out;
    let patches = bc2_levelwrite::installpatch::LevelWrite;
    let mut inst = Installer::new(Game::new(&game, check.as_deref()), &work, &patches, &mut log);
    let r = match (cmd.as_str(), names) {
        ("plan", [stage]) => inst.plan_only(stage),
        ("apply", [stage]) => inst.apply(stage),
        ("revert", []) if all => inst.revert(&[]),
        ("revert", levels) if !all && !levels.is_empty() => inst.revert(levels),
        ("status", []) => inst.status(),
        _ => return usage(),
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let _ = std::io::stdout().flush();
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

/// `<folder>\work\mapedit` beside Project OPFOR's game copy (`<opfor>\game`: `opfor_layout`); for any other game copy
/// (round OR15, IN1: a plain install of the game) `%LOCALAPPDATA%\bfbc2-tool-suite\work\mapedit`, never a folder of
/// the suite's next to the game's own.
fn default_work(up: &str, opfor_layout: bool) -> String {
    let base = if up.is_empty() { "." } else { up };
    if !opfor_layout && let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()) {
        return std::path::PathBuf::from(local)
            .join("bfbc2-tool-suite")
            .join("work")
            .join("mapedit")
            .to_string_lossy()
            .into_owned();
    }
    format!("{}\\work\\mapedit", base.trim_end_matches(['\\', '/']))
}
