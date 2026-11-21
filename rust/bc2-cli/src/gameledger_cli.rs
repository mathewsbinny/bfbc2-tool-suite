//! `bfbc2 gameledger`: the game copy's install ledger (round DF2, [`bc2_formats::gameledger`]) seen from the command
//! line - what each tool installed, DICE's original of a file, and the recovery of an interrupted install.
//!
//! ```text
//! bfbc2 gameledger status   <game>          one line per installed file: <owner> <file> <state>
//! bfbc2 gameledger holders  <game> <file>   the owners holding a file whole, bottom first
//! bfbc2 gameledger original <game> <file>   the path holding what was there before any install (exit 1: none)
//! bfbc2 gameledger recover  <game> [--force]   an interrupted install / revert rolled back
//! ```
//!
//! `<game>` is the game copy (the folder holding `Package`, `Dist`, `Output`); `<file>` is relative to it.

use std::path::Path;
use std::process::ExitCode;

use bc2_formats::gameledger::Ledger;

/// A line on stdout (a failed write - a closed pipe - is not an error of the command).
macro_rules! say {
    ($($t:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), $($t)*);
    }};
}

const USAGE: &str = "usage: bfbc2 gameledger status <game>\n       \
                     bfbc2 gameledger holders <game> <file>\n       \
                     bfbc2 gameledger original <game> <file>\n       \
                     bfbc2 gameledger recover <game> [--force]\n\
                     The game copy's install ledger (<game>\\opfor\\ledger): what each tool installed, the file from \
                     before any install, and the recovery of an interrupted install or revert.\n";

fn fail(msg: &str) -> ExitCode {
    eprintln!("gameledger: {msg}");
    ExitCode::from(1)
}

/// Runs `bfbc2 gameledger <args>`.
pub fn main(args: &[String]) -> ExitCode {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let (cmd, game, file) = (a.first().copied(), a.get(1).copied(), a.get(2).copied());
    let Some(game) = game.filter(|_| cmd.is_some()) else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let mut led = Ledger::new(Path::new(game));
    match (cmd.unwrap_or_default(), file) {
        ("status", None) => match led.status() {
            Ok(lines) => {
                if lines.is_empty() {
                    say!("nothing installed through the ledger in {game}");
                }
                for l in lines {
                    say!("{l}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e.to_string()),
        },
        ("holders", Some(f)) => match led.holders(f) {
            Ok(h) => {
                for o in h {
                    say!("{o}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e.to_string()),
        },
        ("original", Some(f)) => match led.original(f) {
            Ok(Some(p)) => {
                say!("{}", p.display());
                ExitCode::SUCCESS
            }
            Ok(None) => {
                eprintln!("{f}: none - an install put the file where there was none (or it is missing)");
                ExitCode::from(1)
            }
            Err(e) => fail(&e.to_string()),
        },
        ("recover", f) if f.is_none() || f == Some("--force") => match led.recover(f.is_some()) {
            Ok(notes) => {
                if notes.is_empty() {
                    say!("nothing to recover");
                }
                for n in notes {
                    say!("{n}");
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e.to_string()),
        },
        _ => {
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
