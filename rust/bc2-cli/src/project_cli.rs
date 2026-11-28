//! `bfbc2 project share` (round OR15, IN1): a map project copied into a folder of its own, ready to give to someone
//! else - no PC name, no user name, no absolute path (`bc2_levelwrite::share`); tool group `mods`.
//!
//! ```text
//! bfbc2 project share <project.mapedit.json> <empty folder>
//! ```
//!
//! Exit codes: 0 written; 1 refused (in words: the copy would still name a place or a person, the folder is not empty,
//! the project does not read); 2 usage.

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: bfbc2 project share <project.mapedit.json> <empty folder>\n\
                     A copy of the map project to give to others: the edits' authors numbered, paths cut to file names, \
                     no PC or user name; its kept files and stroke tiles beside it.\n";

/// Runs `bfbc2 project <args>`.
pub fn main(args: &[String]) -> ExitCode {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let ["share", project, out] = a.as_slice() else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    match bc2_levelwrite::share::share(Path::new(project), Path::new(out)) {
        Ok(s) => {
            let mut o = std::io::stdout().lock();
            let _ = writeln!(o, "shared: {}", s.project.display());
            for f in &s.files {
                let _ = writeln!(o, "  with {f}");
            }
            for c in &s.changes {
                let _ = writeln!(o, "  {c}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("bfbc2 project share: {e}");
            ExitCode::from(1)
        }
    }
}
