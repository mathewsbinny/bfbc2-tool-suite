//! `mapexport`: a map project exported as a NEW multiplayer level, staged; never writes the game copy
//! (`EDITOR_ARCHITECTURE.md` 3.12). The install of a stage is `bfbc2 install mapedit` (`bc2_install::mapedit`, round
//! OR6), which builds the shared bundles with the same library calls as `startup`, `overlay` and `awards` here.
//!
//! ```text
//! mapexport build   <project.mapedit.json> <export.json> --game <game folder> --out <stage folder>
//! mapexport check   <stage folder> --game <game folder>
//! mapexport validate <project.mapedit.json> --game <game folder>
//! mapexport rules
//! mapexport names   <export.json> --game <game folder> [--project <project.mapedit.json>]
//! mapexport startup <startup-00.fbrb> --out <file> --work <temp folder> [--add <LEVEL> <description.dbx>]...
//!                   [--remove <LEVEL>]...
//! mapexport overlay <overlay-00.fbrb> --out <file> --work <temp folder> [--add <stage folder>]... [--remove <LEVEL>]...
//! mapexport awards  <ondemand_awards-00.fbrb> --out <file> --work <temp folder> [--add <stage folder>]...
//!                   [--remove <LEVEL>]...
//! mapexport --source-hash   the Rust sources' fingerprint it was built from (bfbc2core.native, round QW1)
//! ```
//!
//! `overlay` puts each added stage's texts (its own name and description: `stage.json`'s `strings`) in every
//! language's localization and takes the removed levels' keys out; `awards` does the same with the stages' pictures
//! (`awards`) in the on-demand awards bundle (round E3b); a language with its own texts (`strings_by_language`, round
//! EX1) gets those, in its own encoding.
//!
//! `names` checks an export file's names (round EX1, `bc2_levelwrite::names`): the id, its folder, the server-browser
//! name and the on-screen names per language, against the game copy (stock levels, other projects' exports, the
//! localisation's keys and letters); `build` refuses names that fail and `check` repeats the rules on the stage.
//!
//! `validate` checks a project as the editor's Problems panel does (round V1, `bc2_levelwrite::validate`): the
//! export's rules of the edit phase and DICE's placement tendencies (round L1), each finding with its rule's code and
//! severity; exit code 1 when one is an error. `rules` lists the rule registry (`bc2_levelwrite::rules`): code,
//! severity, where it is checked, title, specification rules, round. Every `check` line names its rule's code.
//!
//! Exit code 0, or 1 with the reason on standard error (a failed check: 1, the checks printed).

use std::path::PathBuf;
use std::process::ExitCode;

use bc2_levelwrite::MapProject;
use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::installpatch;
use bc2_levelwrite::rules::Severity;

fn value(args: &[String], key: &str) -> Option<PathBuf> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from)
}

#[allow(clippy::too_many_lines, reason = "one arm per command, each its arguments in order")]
fn run(args: &[String]) -> Result<bool, String> {
    let game = || -> Result<Game, String> {
        let g = value(args, "--game").ok_or("--game <game folder> is needed")?;
        Game::open(&g).map_err(|e| e.to_string())
    };
    let mut out = std::io::stdout().lock();
    let mut say = |s: &str| {
        use std::io::Write as _;
        let _ = writeln!(out, "{s}");
    };
    match args.first().map(String::as_str) {
        Some("build") => {
            let (Some(p), Some(s)) = (args.get(1), args.get(2)) else {
                return Err("build <project> <export.json> --game <folder> --out <stage>".into());
            };
            let project = MapProject::load(&PathBuf::from(p)).map_err(|e| format!("{p}: {e}"))?;
            let text = std::fs::read_to_string(s).map_err(|e| format!("{s}: {e}"))?;
            let spec = ExportSpec::from_json(&text).map_err(|e| e.to_string())?;
            let stage = value(args, "--out").ok_or("--out <stage folder> is needed")?;
            let t = std::time::Instant::now();
            let rec = export::build(&game()?, &project, p, &spec, &stage).map_err(|e| e.to_string())?;
            for l in &rec.report {
                say(l);
            }
            say(&format!(
                "staged {} in {} ({:.1} s)",
                rec.level,
                stage.display(),
                t.elapsed().as_secs_f64()
            ));
            Ok(true)
        }
        Some("names") => {
            // round EX1: the export file's names checked before a build (the same rules as build and check)
            let s = args
                .get(1)
                .ok_or("names <export.json> --game <folder> [--project <project>]")?;
            let text = std::fs::read_to_string(s).map_err(|e| format!("{s}: {e}"))?;
            let spec = ExportSpec::from_json(&text).map_err(|e| e.to_string())?;
            let g = game()?;
            let project = match value(args, "--project") {
                Some(p) => Some(MapProject::load(&p).map_err(|e| format!("{}: {e}", p.display()))?.id),
                None => None,
            };
            let mode = export::mode_title(&bc2_levelwrite::modelayer::mode_key(&spec.mode));
            let problems = bc2_levelwrite::names::check(
                &spec.names(),
                project.as_deref(),
                &mode,
                Some(&bc2_levelwrite::names::Taken::read(&g)),
                Some(&bc2_levelwrite::names::Languages::read(&g)),
            );
            for p in &problems {
                say(&format!("FAILED names: {}", p.message));
            }
            if problems.is_empty() {
                say(&format!(
                    "ok     names: {} ({:?}) follow the rules",
                    spec.names().id(),
                    spec.title
                ));
            }
            Ok(problems.is_empty())
        }
        Some("check") => {
            let stage = PathBuf::from(args.get(1).ok_or("check <stage folder> --game <folder>")?);
            let r = export::check(&game()?, &stage).map_err(|e| e.to_string())?;
            for (code, ok, what) in r.lines() {
                say(&export::CheckReport::line(code, ok, what));
            }
            say(if r.ok() { "check: ok" } else { "check: FAILED" });
            Ok(r.ok())
        }
        Some("validate") => {
            let p = args.get(1).ok_or("validate <project> --game <folder>")?;
            let project = MapProject::load(&PathBuf::from(p)).map_err(|e| format!("{p}: {e}"))?;
            let found = bc2_levelwrite::validate::project(&game()?, &project).map_err(|e| e.to_string())?;
            for f in &found {
                say(&f.line());
            }
            let n = |s: Severity| found.iter().filter(|f| f.severity == s).count();
            say(&format!(
                "validate: {} error(s), {} warning(s), {} note(s)",
                n(Severity::Error),
                n(Severity::Warning),
                n(Severity::Note)
            ));
            Ok(n(Severity::Error) == 0)
        }
        Some("rules") => {
            for l in bc2_levelwrite::rules::table() {
                say(&l);
            }
            let [n, w, e] = bc2_levelwrite::rules::counts();
            say(&format!("{} rules: {e} errors, {w} warnings, {n} notes", n + w + e));
            Ok(true)
        }
        Some("startup") => {
            let src = PathBuf::from(
                args.get(1)
                    .ok_or("startup <startup-00.fbrb> --out <file> --work <folder>")?,
            );
            let out_p = value(args, "--out").ok_or("--out <file> is needed")?;
            let work = value(args, "--work").ok_or("--work <folder> is needed")?;
            let mut add = Vec::new();
            let mut remove = Vec::new();
            let mut i = 0;
            while let Some(a) = args.get(i) {
                match a.as_str() {
                    "--add" => {
                        let (Some(l), Some(f)) = (args.get(i + 1), args.get(i + 2)) else {
                            return Err("--add <LEVEL> <description.dbx>".into());
                        };
                        add.push((l.clone(), std::fs::read(f).map_err(|e| format!("{f}: {e}"))?));
                        i += 3;
                    }
                    "--remove" => {
                        remove.push(args.get(i + 1).ok_or("--remove <LEVEL>")?.clone());
                        i += 2;
                    }
                    _ => i += 1,
                }
            }
            let items = export::patch_startup(&src, &add, &remove, &work, &out_p).map_err(|e| e.to_string())?;
            say(&format!("{}: level list of {} items", out_p.display(), items.len()));
            Ok(true)
        }
        Some(cmd @ ("overlay" | "awards")) => {
            let src = PathBuf::from(
                args.get(1)
                    .ok_or("overlay | awards <bundle> --out <file> --work <folder>")?,
            );
            let out_p = value(args, "--out").ok_or("--out <file> is needed")?;
            let work = value(args, "--work").ok_or("--work <folder> is needed")?;
            let (mut stages, mut remove) = (Vec::new(), Vec::new());
            let mut i = 0;
            while let Some(a) = args.get(i) {
                match a.as_str() {
                    "--add" => {
                        stages.push(PathBuf::from(args.get(i + 1).ok_or("--add <stage folder>")?));
                        i += 2;
                    }
                    "--remove" => {
                        remove.push(args.get(i + 1).ok_or("--remove <LEVEL>")?.clone());
                        i += 2;
                    }
                    _ => i += 1,
                }
            }
            // round OR6: the stages' texts and pictures read by the library the installer uses
            if cmd == "overlay" {
                let (langs, n) =
                    installpatch::overlay(&src, &stages, &remove, &work, &out_p).map_err(|e| e.to_string())?;
                say(&format!(
                    "{}: {n} texts in {} languages ({})",
                    out_p.display(),
                    langs.len(),
                    langs.join(", ")
                ));
            } else {
                let (added, n) =
                    installpatch::awards(&src, &stages, &remove, &work, &out_p).map_err(|e| e.to_string())?;
                say(&format!("{}: {added} pictures added, {n} entries", out_p.display()));
            }
            Ok(true)
        }
        _ => {
            Err(
                "mapexport build | check | validate | rules | names | startup | overlay | awards (see the source's documentation)"
                    .into(),
            )
        }
    }
}

fn main() -> ExitCode {
    // the one worker pool (BFBC2_WORKERS)
    bc2_gamefs::workers::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && args.first().map(String::as_str) == Some("--source-hash") {
        // `build_native.py --editor` passes the fingerprint as BC2_SOURCE_HASH (empty when built by cargo alone)
        use std::io::Write as _;
        let ok = writeln!(std::io::stdout(), "{}", option_env!("BC2_SOURCE_HASH").unwrap_or("")).is_ok();
        return if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE };
    }
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            use std::io::Write as _;
            let _ = writeln!(std::io::stderr(), "mapexport: {e}");
            ExitCode::from(1)
        }
    }
}
