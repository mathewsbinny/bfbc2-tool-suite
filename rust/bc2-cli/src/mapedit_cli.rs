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
//! bfbc2 install mapedit status --game <folder>
//!                                    the exported levels installed
//!   --check-game  the game copy the stage is checked against (only read; default --game)
//!   --work        where the shared patches are built (default <game>\..\work\mapedit; removed when the run ends)
//! ```
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
    let Some(game) = game else {
        return usage();
    };
    let work = work.unwrap_or_else(|| {
        let up = std::path::Path::new(&game)
            .parent()
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned());
        default_work(&up)
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

/// `<folder>\work\mapedit`.
fn default_work(up: &str) -> String {
    let base = if up.is_empty() { "." } else { up };
    format!("{}\\work\\mapedit", base.trim_end_matches(['\\', '/']))
}
