//! `bfbc2 install`: the game-copy installer (round OR1, "OPFOR in Rust" stage 1, [`bc2_install`]) - Project OPFOR's
//! WW2 install after its build staged the bundles and wrote the install plan.
//!
//! ```text
//! bfbc2 install plan   <plan.json>                 the plan's tail: the backup guard, the registration's report,
//!                                                  each level's report.txt, registration_all.txt, staged.json
//! bfbc2 install apply  <plan.json>                 install the staged bundles (the gates and the guard first)
//! bfbc2 install apply  --staged <staged.json>      install the last good plan's staging while nothing it depends
//!                                                  on changed (the input fingerprint, every staged file's sha256)
//! bfbc2 install revert <plan.json>                 the Pacific War opt-out (less the menu mode: menu_install.py)
//! bfbc2 install marks  <plan.json> <file> ...      the install's own content in each file, one JSON list a line
//! bfbc2 install mapedit plan|apply|revert|status ...
//!                                                  round OR6: the map export's install (crate::mapedit_cli)
//!   --lock-token <t>   run under the rebuild lock OPFOR's driver took for this run (else the command takes it)
//! ```
//!
//! Exit codes: 0 done; 1 a gate, the guard or the verification failed, or the run stopped (the reason on stderr); 2
//! usage; 3 another WW2 rebuild or install holds the rebuild lock (`work\ww2\_rebuild\rebuild.lock`).

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;

use bc2_install::lock;
use bc2_install::plan::Plan;
use bc2_install::run::Run;
use bc2_install::staged;

const USAGE: &str = "usage: bfbc2 install plan <plan.json> [--lock-token <t>]\n       \
                     bfbc2 install apply <plan.json> | --staged <staged.json> [--lock-token <t>]\n       \
                     bfbc2 install revert <plan.json> [--lock-token <t>]\n       \
                     bfbc2 install marks <plan.json> <file> ...\n       \
                     bfbc2 install mapedit plan|apply|revert|status ... (the map export's install)\n\
                     The game-copy installer (Project OPFOR's WW2 install, on the install ledger): the plan is the one \
                     OPFOR's build wrote (wake_install.py).\n";

fn out(line: &str) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{line}");
    let _ = o.flush();
}

fn stop(msg: &str) -> ExitCode {
    let _ = std::io::stdout().flush();
    eprintln!("{msg}");
    ExitCode::from(1)
}

/// `bfbc2 install mapedit ...` (round OR6: the map export's install, in the tool group `mods`).
fn mapedit(args: &[String]) -> ExitCode {
    #[cfg(feature = "mods")]
    return crate::mapedit_cli::main(args);
    #[cfg(not(feature = "mods"))]
    {
        let _ = args;
        eprintln!("bfbc2 install mapedit: this bfbc2 was built without the tool group mods");
        ExitCode::from(2)
    }
}

/// Runs `bfbc2 install <args>`.
pub fn main(args: &[String]) -> ExitCode {
    if args.first().map(String::as_str) == Some("mapedit") {
        return mapedit(args.get(1..).unwrap_or_default());
    }
    let mut a: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut token = None;
    if let Some(i) = a.iter().position(|x| *x == "--lock-token") {
        token = a.get(i + 1).map(|t| (*t).to_owned());
        if token.is_none() {
            eprint!("{USAGE}");
            return ExitCode::from(2);
        }
        a.drain(i..i + 2);
    }
    let staged_mode = a.get(1) == Some(&"--staged");
    if staged_mode {
        a.remove(1);
    }
    let (Some(cmd), Some(file)) = (a.first().copied(), a.get(1).copied()) else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    if !matches!(cmd, "plan" | "apply" | "revert" | "marks") || (staged_mode && cmd != "apply") {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }
    // the plan (a staged record holds it)
    let plan = if staged_mode {
        match std::fs::read(file)
            .ok()
            .and_then(|d| serde_json::from_slice::<staged::Record>(&d).ok())
        {
            Some(r) => r.plan,
            None => {
                return stop(&format!(
                    "--apply-staged: the staged plan cannot be read ({file}) - run wake_install.py --plan again"
                ));
            }
        }
    } else {
        match Plan::read(Path::new(file)) {
            Ok(p) => p,
            Err(e) => return stop(&e),
        }
    };
    if cmd == "marks" {
        let mut log = |_: &str| {};
        let run = match Run::new(&plan, &mut log) {
            Ok(r) => r,
            Err(e) => return stop(&e),
        };
        for f in a.get(2..).unwrap_or_default() {
            match run.marks(f) {
                Ok(m) => out(&serde_json::Value::from(m).to_string()),
                Err(e) => return stop(&e),
            }
        }
        return ExitCode::SUCCESS;
    }
    let held = match lock::take(
        Path::new(&plan.lock),
        token.as_deref(),
        &plan.opfor,
        &format!("bfbc2 install {cmd}"),
    ) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(3);
        }
    };
    if let Some(n) = &held.note {
        out(n);
    }
    let plan = if staged_mode {
        match staged::load(Path::new(file)) {
            Ok(Ok(r)) => r.plan,
            Ok(Err(why)) => {
                out(&why);
                return ExitCode::from(1);
            }
            Err(e) => return stop(&e),
        }
    } else {
        plan
    };
    let mut log = out;
    let mut run = match Run::new(&plan, &mut log) {
        Ok(r) => r,
        Err(e) => return stop(&e),
    };
    let r = match cmd {
        "plan" => run.install(false, false),
        "apply" => run.install(true, staged_mode),
        _ => run.revert(),
    };
    drop(held);
    match r {
        Ok(code) => ExitCode::from(code),
        Err(e) => stop(&e),
    }
}
